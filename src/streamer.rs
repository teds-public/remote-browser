use crate::types::{Frame, RoomId, StreamQuality, TabId};
use base64::Engine;
use chromiumoxide::cdp::browser_protocol::page::{
    EventScreencastFrame, ScreencastFrameAckParams, StartScreencastFormat,
    StartScreencastParams, StopScreencastParams,
};
use chromiumoxide::Page;
use futures::StreamExt;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument, warn};

struct ActiveStream {
    id: u64,
    quality: StreamQuality,
    cancel: CancellationToken,
}

pub struct Streamer {
    streams: Arc<RwLock<HashMap<TabId, ActiveStream>>>,
    gates: Arc<RwLock<HashMap<TabId, Arc<tokio::sync::Mutex<()>>>>>,
    next_id: Arc<std::sync::atomic::AtomicU64>,
    frame_tx: mpsc::UnboundedSender<Frame>,
}

impl Streamer {
    pub fn new(frame_tx: mpsc::UnboundedSender<Frame>) -> Self {
        Self {
            streams: Arc::new(RwLock::new(HashMap::new())),
            gates: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            frame_tx,
        }
    }

    #[instrument(skip(self, page))]
    pub async fn start_stream(
        &self,
        tab_id: TabId,
        page: Arc<Page>,
        quality: StreamQuality,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut streams = self.streams.write();

        if let Some(existing) = streams.get(&tab_id) {
            if existing.quality == quality {
                tracing::debug!(%tab_id, "Stream already running at requested quality");
                return Ok(());
            }

            info!(%tab_id, "Restarting stream with new quality");
            existing.cancel.cancel();
            streams.remove(&tab_id);
        }

        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let cancel = CancellationToken::new();
        let stream = ActiveStream {
            id,
            quality,
            cancel: cancel.clone(),
        };

        streams.insert(tab_id, stream);
        drop(streams);

        let gate = self
            .gates
            .write()
            .entry(tab_id)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();

        let frame_tx = self.frame_tx.clone();
        let streams = Arc::clone(&self.streams);

        tokio::spawn(async move {
            if let Err(e) = run_stream(tab_id, page, quality, frame_tx, cancel.clone(), gate).await {
                error!(%tab_id, "Stream error: {}", e);
            }

            let mut active = streams.write();
            if active.get(&tab_id).map(|s| s.id) == Some(id) {
                active.remove(&tab_id);
            }
            drop(active);
            info!(%tab_id, "Stream stopped");
        });

        info!(%tab_id, ?quality, "Started stream");
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn stop_stream(&self, tab_id: TabId) {
        if let Some(stream) = self.streams.write().remove(&tab_id) {
            stream.cancel.cancel();
            info!(%tab_id, "Stopping stream");
        }
    }

    pub fn forget(&self, tab_id: TabId) {
        self.stop_stream(tab_id);
        self.gates.write().remove(&tab_id);
    }

    pub fn shutdown(&self) {
        let streams = self.streams.write();
        for stream in streams.values() {
            stream.cancel.cancel();
        }
        info!("All streams stopped");
    }
}

async fn run_stream(
    tab_id: TabId,
    page: Arc<Page>,
    quality: StreamQuality,
    frame_tx: mpsc::UnboundedSender<Frame>,
    cancel: CancellationToken,
    gate: Arc<tokio::sync::Mutex<()>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _gate = tokio::time::timeout(std::time::Duration::from_secs(5), gate.lock())
        .await
        .map_err(|_| "timed out waiting for the previous screencast to stop")?;

    let params = StartScreencastParams::builder()
        .format(StartScreencastFormat::Jpeg)
        .quality(quality.quality() as i64)
        .every_nth_frame(quality.every_nth_frame() as i64)
        .max_width(quality.max_width() as i64)
        .max_height(quality.max_height() as i64)
        .build();

    let mut events = page.event_listener::<EventScreencastFrame>().await?;
    tracing::debug!(%tab_id, "Screencast listener ready");

    // Headless Chrome only emits a frame once the tab is foregrounded and repainted.
    page.bring_to_front().await?;
    page.execute(params).await?;

    let _ = page
        .evaluate(
            "(() => { if (!document.body) return; \
             const prev = document.body.style.opacity; let n = 0; \
             const timer = setInterval(() => { \
               document.body.style.opacity = n % 2 ? '0.999' : '1'; \
               if (++n < 4) return; \
               clearInterval(timer); \
               document.body.style.opacity = prev; \
             }, 400); })()",
        )
        .await;

    info!(%tab_id, "Screencast started");

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                break;
            }
            Some(event) = events.next() => {
                if let Err(e) = handle_frame(tab_id, &page, event, &frame_tx).await {
                    warn!(%tab_id, "Frame handling error: {}", e);
                }
            }
            else => {
                warn!(%tab_id, "Frame stream ended");
                break;
            }
        }
    }

    if let Err(e) = page.execute(StopScreencastParams::default()).await {
        warn!(%tab_id, "Failed to stop screencast: {}", e);
    }

    Ok(())
}

async fn handle_frame(
    tab_id: TabId,
    page: &Page,
    event: Arc<EventScreencastFrame>,
    frame_tx: &mpsc::UnboundedSender<Frame>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let encoded: &str = event.data.as_ref();
    let jpeg = base64::prelude::BASE64_STANDARD.decode(encoded)?;
    tracing::debug!(%tab_id, bytes = jpeg.len(), "Screencast frame");

    page.execute(ScreencastFrameAckParams::new(event.session_id)).await?;

    let frame = Frame {
        tab_id,
        room: RoomId::for_tab(tab_id),
        jpeg,
    };

    if let Err(e) = frame_tx.send(frame) {
        error!(%tab_id, "Failed to send frame: {}", e);
    }

    Ok(())
}
