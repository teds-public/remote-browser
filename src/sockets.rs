use crate::browser::Browser;
use crate::error::SocketError;
use crate::presence::PresenceManager;
use crate::streamer::Streamer;
use crate::types::{StreamQuality, TabId, ViewerPrefs};
use crate::input;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::json;
use socketioxide::extract::{AckSender, Data, SocketRef};
use socketioxide::SocketIo;
use std::sync::Arc;
use tracing::{error, info, instrument};

#[derive(Debug, Deserialize)]
pub(crate) struct JoinData {
    name: String,
    #[serde(default)]
    quality: Option<String>,
    #[serde(default)]
    paused: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RenameData {
    name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WatchData {
    #[serde(rename = "tabId")]
    tab_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateTabData {
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CloseTabData {
    #[serde(rename = "tabId")]
    tab_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct NavData {
    #[serde(rename = "tabId")]
    tab_id: String,
    url: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TabActionData {
    #[serde(rename = "tabId")]
    tab_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClickData {
    #[serde(rename = "tabId")]
    tab_id: String,
    x: f64,
    y: f64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ScrollData {
    #[serde(rename = "tabId")]
    tab_id: String,
    dx: i32,
    dy: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ZoomData {
    #[serde(rename = "tabId")]
    tab_id: String,
    dy: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TypeData {
    #[serde(rename = "tabId")]
    tab_id: String,
    text: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct KeyData {
    #[serde(rename = "tabId")]
    tab_id: String,
    key: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StreamPrefsData {
    quality: String,
    paused: bool,
}

#[derive(Debug, Serialize)]
struct JoinedResponse {
    name: String,
    viewport: crate::types::Viewport,
}

#[derive(Debug, Serialize)]
struct TabCreatedResponse {
    #[serde(rename = "tabId")]
    tab_id: String,
}

pub struct AppState {
    pub browser: Arc<Browser>,
    pub presence: Arc<PresenceManager>,
    pub streamer: Arc<Streamer>,
    pub io: SocketIo,
}

fn parse_tab_id(id_str: &str) -> Result<TabId, SocketError> {
    id_str.strip_prefix('t')
        .and_then(|s| s.parse::<usize>().ok())
        .map(TabId::from)
        .ok_or_else(|| SocketError::MissingField("tabId"))
}

impl From<usize> for TabId {
    fn from(n: usize) -> Self {
        TabId::from_raw(n)
    }
}

fn parse_quality(s: &str) -> Result<StreamQuality, SocketError> {
    match s {
        "low" => Ok(StreamQuality::Low),
        "med" => Ok(StreamQuality::Med),
        "high" => Ok(StreamQuality::High),
        _ => Err(SocketError::InvalidQuality(s.to_string())),
    }
}

fn get_state(socket: &SocketRef) -> Option<Arc<AppState>> {
    socket.extensions.get::<Arc<AppState>>().map(|s| Arc::clone(&s))
}

#[instrument(skip(socket))]
pub async fn on_join(
    socket: SocketRef,
    Data(data): Data<JoinData>,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => {
            error!("Failed to get state from socket extensions");
            return;
        }
    };

    info!("Client joining: {}", data.name);

    let quality = data.quality
        .as_ref()
        .and_then(|q| parse_quality(q).ok())
        .unwrap_or_default();

    let prefs = ViewerPrefs {
        quality,
        paused: data.paused.unwrap_or(false),
    };

    let presence = state.presence.join(
        socket.id.to_string(),
        data.name.clone(),
        prefs,
    );

    let response = JoinedResponse {
        name: presence.name.clone(),
        viewport: state.browser.viewport(),
    };

    let _ = socket.emit("joined", &response);

    send_tabs_sync(&socket, &state).await;
    broadcast_tabs(&state).await;
}

#[instrument(skip(socket))]
pub async fn on_rename(
    socket: SocketRef,
    Data(data): Data<RenameData>,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    state.presence.rename(&socket.id.to_string(), data.name);
    broadcast_tabs(&state).await;
}

#[instrument(skip(socket))]
pub async fn on_tab_watch(
    socket: SocketRef,
    Data(data): Data<WatchData>,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    let tab_id = match parse_tab_id(&data.tab_id) {
        Ok(id) => id,
        Err(_) => {
            error!("Invalid tab ID: {}", data.tab_id);
            return;
        }
    };

    if state.browser.get_tab(tab_id).is_none() {
        let _ = socket.emit("error", &json!({"message": "Tab not found"}));
        return;
    }

    let old_tab_id = state.presence.get_watching_tab(&socket.id.to_string());

    if let Some(old_id) = old_tab_id {
        let room = crate::types::RoomId::for_tab(old_id);
        socket.leave(room.to_string());
    }

    state.presence.set_watching_tab(&socket.id.to_string(), Some(tab_id));

    let room = crate::types::RoomId::for_tab(tab_id);
    socket.join(room.to_string());

    if let Some(old_id) = old_tab_id {
        refresh_stream(old_id, &state).await;
    }

    refresh_stream(tab_id, &state).await;
    send_shot(&socket, &state, tab_id).await;
    broadcast_tabs(&state).await;
}

#[instrument(skip(socket))]
pub async fn on_tab_create(
    socket: SocketRef,
    Data(data): Data<CreateTabData>,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    let creator = state.presence
        .name_of(&socket.id.to_string())
        .unwrap_or_else(|| "System".to_string());

    let _ = socket.emit("tab:creating", &json!({}));

    match state.browser.new_tab(data.url.as_deref(), creator).await {
        Ok(tab_id) => {
            let response = TabCreatedResponse {
                tab_id: tab_id.to_string(),
            };
            let _ = socket.emit("tab:created", &response);
        }
        Err(e) => {
            error!("Failed to create tab: {}", e);
            let _ = socket.emit("error", &json!({"message": e.to_string()}));
        }
    }
}

#[instrument(skip(socket))]
pub async fn on_tab_close(
    socket: SocketRef,
    Data(data): Data<CloseTabData>,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    let tab_id = match parse_tab_id(&data.tab_id) {
        Ok(id) => id,
        Err(_) => return,
    };

    state.streamer.forget(tab_id);

    if let Err(e) = state.browser.close_tab(tab_id).await {
        error!("Failed to close tab: {}", e);
    }

    let room = crate::types::RoomId::for_tab(tab_id);
    socket.leave(room.to_string());

    state.browser.shutdown_if_idle();
    broadcast_tabs(&state).await;
}

#[instrument(skip(socket))]
pub async fn on_nav(
    socket: SocketRef,
    Data(data): Data<NavData>,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    let tab_id = match parse_tab_id(&data.tab_id) {
        Ok(id) => id,
        Err(_) => return,
    };

    if let Err(e) = state.browser.navigate(tab_id, &data.url).await {
        error!("Navigation failed: {}", e);
        let _ = socket.emit("error", &json!({"message": e.to_string()}));
    }
}

#[instrument(skip(socket))]
pub async fn on_nav_back(
    socket: SocketRef,
    Data(data): Data<TabActionData>,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    let tab_id = match parse_tab_id(&data.tab_id) {
        Ok(id) => id,
        Err(_) => return,
    };

    if let Some(tab) = state.browser.get_tab(tab_id) {
        if let Err(e) = tab.page.evaluate("history.back()").await {
            error!("go_back failed: {}", e);
        }
    }
}

#[instrument(skip(socket))]
pub async fn on_nav_reload(
    socket: SocketRef,
    Data(data): Data<TabActionData>,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    let tab_id = match parse_tab_id(&data.tab_id) {
        Ok(id) => id,
        Err(_) => return,
    };

    if let Some(tab) = state.browser.get_tab(tab_id) {
        if let Err(e) = tab.page.reload().await {
            error!("reload failed: {}", e);
        }
    }
}

#[instrument(skip(socket))]
pub async fn on_tab_reload(
    socket: SocketRef,
    Data(data): Data<TabActionData>,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    let tab_id = match parse_tab_id(&data.tab_id) {
        Ok(id) => id,
        Err(_) => return,
    };

    if let Err(e) = state.browser.reload_tab(tab_id).await {
        error!("tab_reload failed: {}", e);
    }
}

#[instrument(skip(socket, ack))]
pub async fn on_input_click(
    socket: SocketRef,
    Data(data): Data<ClickData>,
    ack: AckSender,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    let tab_id = match parse_tab_id(&data.tab_id) {
        Ok(id) => id,
        Err(_) => {
            let _ = socket.emit("error", &json!({"message": "Invalid tab ID"}));
            return;
        }
    };

    let tab = match state.browser.get_tab(tab_id) {
        Some(t) => t,
        None => {
            let _ = socket.emit("error", &json!({"message": "Tab not found"}));
            return;
        }
    };

    let viewport = state.browser.viewport();

    match input::click(&tab.page, data.x, data.y, viewport).await {
        Ok(_) => {
            info!(%tab_id, x = data.x, y = data.y, "Click successful");
            let _ = ack.send(&json!({"ok": true}));
        }
        Err(e) => {
            error!(%tab_id, "Click failed: {}", e);
            let _ = ack.send(&json!({"ok": false, "message": e.to_string()}));
        }
    }
}

#[instrument(skip(socket))]
pub async fn on_input_scroll(
    socket: SocketRef,
    Data(data): Data<ScrollData>,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    let tab_id = match parse_tab_id(&data.tab_id) {
        Ok(id) => id,
        Err(_) => return,
    };

    if let Some(tab) = state.browser.get_tab(tab_id) {
        let _ = input::scroll(&tab.page, data.dx, data.dy, state.browser.viewport()).await;
    }
}

#[instrument(skip(socket))]
pub async fn on_input_zoom(
    socket: SocketRef,
    Data(data): Data<ZoomData>,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    let tab_id = match parse_tab_id(&data.tab_id) {
        Ok(id) => id,
        Err(_) => return,
    };

    if let Some(tab) = state.browser.get_tab(tab_id) {
        let _ = input::zoom(&tab.page, data.dy, state.browser.viewport()).await;
    }
}

#[instrument(skip(socket))]
pub async fn on_input_type(
    socket: SocketRef,
    Data(data): Data<TypeData>,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    let tab_id = match parse_tab_id(&data.tab_id) {
        Ok(id) => id,
        Err(_) => return,
    };

    if let Some(tab) = state.browser.get_tab(tab_id) {
        if let Err(e) = input::type_text(&tab.page, &data.text).await {
            error!("type_text failed: {}", e);
        }
    }
}

#[instrument(skip(socket))]
pub async fn on_input_key(
    socket: SocketRef,
    Data(data): Data<KeyData>,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    let tab_id = match parse_tab_id(&data.tab_id) {
        Ok(id) => id,
        Err(_) => return,
    };

    if let Some(tab) = state.browser.get_tab(tab_id) {
        if let Err(e) = input::press_key(&tab.page, &data.key).await {
            error!("press_key failed: {}", e);
            let _ = socket.emit("error", &json!({"message": e.to_string()}));
        }
    }
}

#[instrument(skip(socket))]
pub async fn on_stream_prefs(
    socket: SocketRef,
    Data(data): Data<StreamPrefsData>,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    let quality = match parse_quality(&data.quality) {
        Ok(q) => q,
        Err(_) => {
            error!("Invalid quality: {}", data.quality);
            return;
        }
    };

    let prefs = ViewerPrefs {
        quality,
        paused: data.paused,
    };

    state.presence.update_prefs(&socket.id.to_string(), prefs);

    if let Some(tab_id) = state.presence.get_watching_tab(&socket.id.to_string()) {
        refresh_stream(tab_id, &state).await;
        send_shot(&socket, &state, tab_id).await;
    }
}

#[instrument(skip(socket))]
pub async fn on_shot_refresh(
    socket: SocketRef,
    Data(data): Data<WatchData>,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    let tab_id = match parse_tab_id(&data.tab_id) {
        Ok(id) => id,
        Err(_) => return,
    };

    send_shot(&socket, &state, tab_id).await;
}

pub async fn on_disconnect(
    socket: SocketRef,
) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };

    info!("Client disconnected: {}", socket.id);

    if let Some(tab_id) = state.presence.get_watching_tab(&socket.id.to_string()) {
        let room = crate::types::RoomId::for_tab(tab_id);
        socket.leave(room.to_string());

        state.presence.leave(&socket.id.to_string());

        refresh_stream(tab_id, &state).await;
    } else {
        state.presence.leave(&socket.id.to_string());
    }

    broadcast_tabs(&state).await;
}

async fn send_tabs_sync(socket: &SocketRef, state: &AppState) {
    let viewers_by_tab = state.presence.viewers_by_tab();
    let tabs = state.browser.list_tabs(&viewers_by_tab);
    let _ = socket.emit("tabs:sync", &json!(tabs));
}

pub async fn broadcast_tabs(state: &AppState) {
    let viewers_by_tab = state.presence.viewers_by_tab();
    let tabs = state.browser.list_tabs(&viewers_by_tab);

    if let Err(e) = state.io.emit("tabs:update", &json!(tabs)).await {
        tracing::debug!("Broadcast skipped: {}", e);
    }
}

async fn refresh_stream(tab_id: TabId, state: &AppState) {
    let prefs = state.presence.aggregate_stream_prefs_for_tab(tab_id);

    match prefs {
        None => {
            state.streamer.stop_stream(tab_id);
            let _ = state.browser.freeze_tab(tab_id, true).await;
        }
        Some(p) if p.paused => {
            state.streamer.stop_stream(tab_id);
            let _ = state.browser.freeze_tab(tab_id, true).await;
        }
        Some(p) => {
            let _ = state.browser.freeze_tab(tab_id, false).await;

            if let Some(tab) = state.browser.get_tab(tab_id) {
                let _ = state.streamer.start_stream(tab_id, tab.page.clone(), p.quality).await;
            }
        }
    }
}

async fn send_shot(socket: &SocketRef, state: &AppState, tab_id: TabId) {
    match state.browser.screenshot(tab_id).await {
        Ok(bytes) => {
            let data_url = format!(
                "data:image/jpeg;base64,{}",
                base64::prelude::BASE64_STANDARD.encode(&bytes)
            );

            let _ = socket.emit("shot", &json!({
                "tabId": tab_id.to_string(),
                "dataUrl": data_url,
            }));
        }
        Err(e) => {
            error!(%tab_id, "Screenshot failed: {}", e);
        }
    }
}
