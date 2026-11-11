use crate::types::{Frame, RoomId, StreamQuality, TabId};
use base64::Engine;
use chromiumoxide::cdp::browser_protocol::page::{
    ScreencastFrameEvent, ScreencastFrameAckParams, StartScreencastParams,
    StopScreencastParams, CaptureScreenshotFormat,
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
    quality: StreamQuality,
    cancel: CancellationToken,
}

pub struct Streamer {
    streams: Arc<RwLock<HashMap<TabId, ActiveStream>>>,
    frame_tx: mpsc::UnboundedSender<Frame>,
}

impl Streamer {
    pub fn new(frame_tx: mpsc::UnboundedSender<Frame>) -> Self {
        Self {
            streams: Arc::new(RwLock::new(HashMap::new())),
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
                info!(%tab_id, "Stream already running at requested quality");
                return Ok(());
            }

            info!(%tab_id, "Restarting stream with new quality");
            existing.cancel.cancel();
            streams.remove(&tab_id);
        }

        let cancel = CancellationToken::new();
        let stream = ActiveStream {
            quality,
            cancel: cancel.clone(),
        };

        streams.insert(tab_id, stream);
        drop(streams);

        let frame_tx = self.frame_tx.clone();
        let streams = Arc::clone(&self.streams);

        tokio::spawn(async move {
            if let Err(e) = run_stream(tab_id, page, quality, frame_tx, cancel.clone()).await {
                error!(%tab_id, "Stream error: {}", e);
            }

            streams.write().remove(&tab_id);
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

    pub fn is_streaming(&self, tab_id: TabId) -> bool {
        self.streams.read().contains_key(&tab_id)
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
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let params = StartScreencastParams {
        format: Some(CaptureScreenshotFormat::Jpeg),
        quality: Some(quality.quality() as i64),
        every_nth_frame: Some(quality.every_nth_frame() as i64),
        max_width: Some(quality.max_width() as i64),
        max_height: Some(quality.max_height() as i64),
    };

    page.execute(params).await?;

    let mut events = page.event_listener::<ScreencastFrameEvent>().await?;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                break;
            }
            event = events.next() => {
                match event {
                    Some(event) => {
                        if let Err(e) = handle_frame(tab_id, &page, event, &frame_tx).await {
                            warn!(%tab_id, "Frame handling error: {}", e);
                        }
                    }
                    None => {
                        warn!(%tab_id, "Frame stream ended");
                        break;
                    }
                }
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
    event: ScreencastFrameEvent,
    frame_tx: &mpsc::UnboundedSender<Frame>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let jpeg = base64::prelude::BASE64_STANDARD.decode(&event.data)?;

    let ack_params = ScreencastFrameAckParams {
        session_id: event.session_id,
    };
    page.execute(ack_params).await?;

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
