mod browser;
mod config;
mod error;
mod input;
mod presence;
mod sockets;
mod streamer;
mod types;

use axum::{routing::get, Router};
use socketioxide::{SocketIo, extract::SocketRef};
use std::sync::Arc;
use tokio::sync::mpsc;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::{info, error};

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

    let state = Arc::new(AppState {
        browser: browser.clone(),
        presence: presence.clone(),
        streamer: streamer.clone(),
    });

    let (layer, io) = SocketIo::new_layer();

    io.ns("/", move |socket: SocketRef| {
        info!("Client connected: {}", socket.id);

        socket.extensions.insert(state.clone());

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
            if let Err(e) = io_clone.to(frame.room.as_str()).emit_binary("frame", frame.jpeg) {
                error!("Failed to emit frame: {}", e);
            }
        }
    });

    let app = Router::new()
        .route("/healthz", get(|| async { "OK" }))
        .nest_service("/", ServeDir::new("public"))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(layer),
        );

    let addr = format!("0.0.0.0:{}", config.port.0);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Server listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(streamer))
        .await?;

    Ok(())
}

async fn shutdown_signal(streamer: Arc<Streamer>) {
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
    }

    info!("Shutdown signal received");
    streamer.shutdown();
}
