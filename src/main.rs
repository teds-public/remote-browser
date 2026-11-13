#![windows_subsystem = "windows"]

mod browser;
mod config;
mod error;
mod input;
mod presence;
mod sockets;
mod streamer;
mod tray;
mod types;

use axum::response::Html;
use axum::{routing::get, Router};
use socketioxide::{SocketIo, extract::SocketRef};
use std::sync::Arc;
use tokio::sync::mpsc;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
// Web UI is baked into the exe so one file runs on another machine.
const INDEX_HTML: &str = include_str!("../public/index.html");
const CLIENT_JS: &str = include_str!("../public/client.js");
const SOCKETIO_JS: &str = include_str!("../public/socket.io/socket.io.js");

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn client_js() -> impl axum::response::IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "application/javascript")],
        CLIENT_JS,
    )
}

async fn socketio_js() -> impl axum::response::IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "application/javascript")],
        SOCKETIO_JS,
    )
}
use tracing::{error, info, warn};

use browser::Browser;
use presence::PresenceManager;
use sockets::AppState;
use streamer::Streamer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let config = config::Config::load()?;

    info!("Starting remote-browser");
    info!("Port: {}", config.port.0);
    info!("Viewport: {}", config.viewport);
    info!("Max tabs: {}", config.max_tabs.0);
    info!("Headless: {}", config.headless);

    let browser = Browser::launch(
        &config.profile_dir,
        config.viewport,
        config.headless,
        config.no_gpu,
        config.max_tabs.0,
    )
    .await?;

    let (frame_tx, mut frame_rx) = mpsc::unbounded_channel();
    let streamer = Arc::new(Streamer::new(frame_tx));
    let presence = Arc::new(PresenceManager::new());

    let (layer, io) = SocketIo::builder().req_path("/live").build_layer();

    let (quit_tx, quit_rx) = tokio::sync::oneshot::channel();
    tray::spawn(config.port.0, quit_tx);

    let state = Arc::new(AppState {
        browser: browser.clone(),
        presence: presence.clone(),
        streamer: streamer.clone(),
        io: io.clone(),
    });

    let mut tab_events = browser.subscribe_events();
    let event_state = state.clone();
    tokio::spawn(async move {
        loop {
            match tab_events.recv().await {
                Ok(_) => sockets::broadcast_tabs(&event_state).await,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!("Tab event subscriber lagged, skipped {}", skipped);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let ns_state = state.clone();
    io.ns("/", move |socket: SocketRef| async move {
        info!("Client connected: {}", socket.id);

        socket.extensions.insert(ns_state.clone());

        socket.on("join", sockets::on_join);
        socket.on("rename", sockets::on_rename);
        socket.on("tab:watch", sockets::on_tab_watch);
        socket.on("tab:create", sockets::on_tab_create);
        socket.on("tab:close", sockets::on_tab_close);
        socket.on("nav", sockets::on_nav);
        socket.on("nav:back", sockets::on_nav_back);
        socket.on("nav:reload", sockets::on_nav_reload);
        socket.on("tab:reload", sockets::on_tab_reload);
        socket.on("input:click", sockets::on_input_click);
        socket.on("input:scroll", sockets::on_input_scroll);
        socket.on("input:zoom", sockets::on_input_zoom);
        socket.on("input:type", sockets::on_input_type);
        socket.on("input:key", sockets::on_input_key);
        socket.on("stream:prefs", sockets::on_stream_prefs);
        socket.on("shot:refresh", sockets::on_shot_refresh);
        socket.on_disconnect(sockets::on_disconnect);
    });

    let io_clone = io.clone();
    tokio::spawn(async move {
        while let Some(frame) = frame_rx.recv().await {
            let tab_id_str = frame.tab_id.to_string();

            if let Err(e) = io_clone
                .to(frame.room.as_str().to_string())
                .emit("frame", &(tab_id_str, bytes::Bytes::from(frame.jpeg)))
                .await
            {
                error!("Failed to emit frame: {}", e);
            }
        }
    });

    let health_state = state.clone();
    let app = Router::new()
        .route(
            "/healthz",
            get(move || {
                let state = health_state.clone();
                async move {
                    axum::Json(serde_json::json!({
                        "ok": true,
                        "tabs": state.browser.tab_count()
                    }))
                }
            }),
        )
        .route("/", get(index))
        .route("/client.js", get(client_js))
        .route("/socket.io/socket.io.js", get(socketio_js))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(layer),
        );

    let addr = format!("0.0.0.0:{}", config.port.0);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Server listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(
            streamer.clone(),
            browser.clone(),
            quit_rx,
        ))
        .await?;

    Ok(())
}

async fn shutdown_signal(
    streamer: Arc<Streamer>,
    browser: Arc<Browser>,
    quit_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
        _ = quit_rx => {}
    }

    info!("Shutdown signal received");
    streamer.shutdown();
    browser.shutdown();
}
