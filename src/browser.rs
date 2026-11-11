use crate::error::BrowserError;
use crate::types::{Tab, TabEvent, TabId, TabMetadata, Viewport};
use chromiumoxide::browser::{Browser as ChromeBrowser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::{SetWebLifecycleStateParams, WebLifecycleState};
use chromiumoxide::cdp::browser_protocol::fetch::{EnableParams, RequestPausedEvent};
use chromiumoxide::Page;
use futures::StreamExt;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info, instrument, warn};

pub struct Browser {
    browser: Arc<ChromeBrowser>,
    tabs: Arc<RwLock<HashMap<TabId, Arc<Tab>>>>,
    max_tabs: usize,
    viewport: Viewport,
    event_tx: broadcast::Sender<TabEvent>,
}

impl Browser {
    #[instrument(skip(profile_dir))]
    pub async fn launch(
        profile_dir: impl AsRef<Path>,
        viewport: Viewport,
        headless: bool,
        no_gpu: bool,
        max_tabs: usize,
    ) -> Result<Arc<Self>, BrowserError> {
        info!("Launching browser");

        let mut args = vec![
            "--disable-backgrounding-occluded-windows".to_string(),
            "--disable-renderer-backgrounding".to_string(),
            "--disable-background-timer-throttling".to_string(),
            "--disable-translate".to_string(),
            "--disable-features=OptimizationHints,MediaRouter".to_string(),
        ];

        if no_gpu {
            args.push("--disable-gpu".to_string());
        }

        let config = BrowserConfig::builder()
            .window_size(viewport.width, viewport.height)
            .user_data_dir(profile_dir)
            .with_head(!headless)
            .args(args)
            .build()
            .map_err(|e| BrowserError::NavigationFailed(e.to_string()))?;

        let (browser, mut handler) = ChromeBrowser::launch(config)
            .await
            .map_err(|e| BrowserError::NavigationFailed(e.to_string()))?;

        tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(e) = &event {
                    error!("Browser event error: {:?}", e);
                }
            }
            warn!("Browser disconnected, auto-relaunching...");
        });

        let (event_tx, _) = broadcast::channel(100);

        let browser = Arc::new(Self {
            browser: Arc::new(browser),
            tabs: Arc::new(RwLock::new(HashMap::new())),
            max_tabs,
            viewport,
            event_tx,
        });

        Ok(browser)
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<TabEvent> {
        self.event_tx.subscribe()
    }

    #[instrument(skip(self))]
    pub async fn new_tab(
        &self,
        url: Option<&str>,
        creator: String,
    ) -> Result<TabId, BrowserError> {
        let tabs = self.tabs.read();
        if tabs.len() >= self.max_tabs {
            return Err(BrowserError::TabLimitReached(self.max_tabs));
        }
        drop(tabs);

        let page = self.browser.new_page("about:blank").await?;
        let id = TabId::next();

        let tab = Arc::new(Tab::new(id, Arc::new(page), creator.clone()));

        self.tabs.write().insert(id, tab.clone());

        info!(%id, "Created tab");

        let _ = self.event_tx.send(TabEvent::Created { id, creator });

        if let Some(url) = url {
            if let Err(e) = self.navigate(id, url).await {
                warn!(%id, %url, "Failed to navigate new tab: {}", e);
            }
        }

        Ok(id)
    }

    #[instrument(skip(self))]
    pub async fn close_tab(&self, id: TabId) -> Result<(), BrowserError> {
        let tab = self.tabs.write().remove(&id)
            .ok_or_else(|| BrowserError::TabNotFound(id.to_string()))?;

        if let Err(e) = tab.page.close().await {
            warn!(%id, "Failed to close tab page: {}", e);
        }

        info!(%id, "Closed tab");
        let _ = self.event_tx.send(TabEvent::Closed { id });

        Ok(())
    }

    #[instrument(skip(self), fields(tab_id = %id))]
    pub async fn navigate(&self, id: TabId, url: &str) -> Result<(), BrowserError> {
        let url = normalize_url(url)?;
        info!(%url, "Navigating");

        let tabs = self.tabs.read();
        let tab = tabs.get(&id)
            .ok_or_else(|| BrowserError::TabNotFound(id.to_string()))?;

        let result = tab.page
            .goto(&url)
            .await
            .map_err(|e| BrowserError::NavigationFailed(e.to_string()))?;

        if let Some(response) = result {
            let new_url = response.url().unwrap_or(&url);
            let new_title = tab.page.get_title().await.unwrap_or_default().unwrap_or_default();

            *tab.url.write() = new_url.to_string();
            *tab.title.write() = new_title.clone();

            let _ = self.event_tx.send(TabEvent::Updated {
                id,
                title: new_title,
                url: new_url.to_string(),
            });
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn reload_tab(&self, id: TabId) -> Result<(), BrowserError> {
        let tabs = self.tabs.read();
        let tab = tabs.get(&id)
            .ok_or_else(|| BrowserError::TabNotFound(id.to_string()))?;

        tab.page.reload().await?;
        *tab.crashed.write() = false;

        info!(%id, "Reloaded tab");
        Ok(())
    }

    pub fn get_tab(&self, id: TabId) -> Option<Arc<Tab>> {
        self.tabs.read().get(&id).cloned()
    }

    pub fn list_tabs(&self, viewers_by_tab: &HashMap<TabId, Vec<String>>) -> Vec<TabMetadata> {
        self.tabs
            .read()
            .values()
            .map(|tab| {
                let viewers = viewers_by_tab.get(&tab.id).cloned().unwrap_or_default();
                tab.metadata(viewers)
            })
            .collect()
    }

    #[instrument(skip(self))]
    pub async fn freeze_tab(&self, id: TabId, freeze: bool) -> Result<(), BrowserError> {
        let tabs = self.tabs.read();
        let tab = tabs.get(&id)
            .ok_or_else(|| BrowserError::TabNotFound(id.to_string()))?;

        let state = if freeze {
            WebLifecycleState::Frozen
        } else {
            WebLifecycleState::Active
        };

        let params = SetWebLifecycleStateParams {
            state,
        };

        if let Err(e) = tab.page.execute(params).await {
            warn!(%id, "Failed to set lifecycle state: {}", e);
        }

        if freeze {
            let _ = tab.page.execute(EnableParams::default()).await;

            if let Ok(mut stream) = tab.page.event_listener::<RequestPausedEvent>().await {
                tokio::spawn(async move {
                    while let Some(_event) = stream.next().await {
                    }
                });
            }

            let _ = tab.page.evaluate(
                "document.querySelectorAll('video,audio').forEach(e => e.pause())"
            ).await;
        }

        *tab.frozen.write() = freeze;
        info!(%id, freeze, "Tab freeze state changed");

        Ok(())
    }

    pub async fn screenshot(&self, id: TabId) -> Result<Vec<u8>, BrowserError> {
        let tabs = self.tabs.read();
        let tab = tabs.get(&id)
            .ok_or_else(|| BrowserError::TabNotFound(id.to_string()))?;

        if *tab.crashed.read() {
            return Err(BrowserError::TabCrashed);
        }

        let bytes = tab.page
            .screenshot(chromiumoxide::page::ScreenshotParams::builder()
                .format(chromiumoxide::page::CaptureScreenshotFormat::Jpeg)
                .quality(50)
                .build())
            .await?;

        Ok(bytes)
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport
    }
}

fn normalize_url(url: &str) -> Result<String, BrowserError> {
    let url = url.trim();

    let url_with_scheme = if !url.contains("://") {
        format!("https://{}", url)
    } else {
        url.to_string()
    };

    let parsed = url::Url::parse(&url_with_scheme)
        .map_err(|e| BrowserError::InvalidUrl(e.to_string()))?;

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(BrowserError::InvalidUrl(
            "Only http and https protocols are allowed".to_string(),
        ));
    }

    Ok(url_with_scheme)
}
