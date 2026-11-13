use crate::error::BrowserError;
use crate::types::{Tab, TabEvent, TabId, TabMetadata, Viewport};
use chromiumoxide::browser::{Browser as ChromeBrowser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, CloseParams, SetWebLifecycleStateParams, SetWebLifecycleStateState,
};
use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::page::ScreenshotParams;
use futures::StreamExt;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info, instrument, warn};

struct LaunchOptions {
    profile_dir: PathBuf,
    viewport: Viewport,
    headless: bool,
    no_gpu: bool,
    max_tabs: usize,
}

pub struct Browser {
    browser: RwLock<Option<Arc<ChromeBrowser>>>,
    launch: LaunchOptions,
    tabs: Arc<RwLock<HashMap<TabId, Arc<Tab>>>>,
    event_tx: broadcast::Sender<TabEvent>,
    relaunch_lock: tokio::sync::Mutex<()>,
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
        // No Chrome process here: it spawns lazily on the first tab so the
        // server always reaches listening even if Chrome is slow or stuck.
        let launch = LaunchOptions {
            profile_dir: profile_dir.as_ref().to_path_buf(),
            viewport,
            headless,
            no_gpu,
            max_tabs,
        };

        let (event_tx, _) = broadcast::channel(100);

        Ok(Arc::new(Self {
            browser: RwLock::new(None),
            launch,
            tabs: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            relaunch_lock: tokio::sync::Mutex::new(()),
        }))
    }

    fn watch(this: Arc<Self>, mut handler: chromiumoxide::Handler) {
        tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(e) = event {
                    error!("Browser event error: {:?}", e);
                }
            }
            warn!("Browser disconnected");
            this.handle_disconnect();
        });
    }

    fn handle_disconnect(&self) {
        let ids: Vec<TabId> = {
            let mut tabs = self.tabs.write();
            let ids = tabs.keys().copied().collect();
            tabs.clear();
            ids
        };

        *self.browser.write() = None;

        let count = ids.len();
        for _ in ids {
            let _ = self.event_tx.send(TabEvent::Closed);
        }

        warn!("Cleared {} tab(s) after browser disconnect", count);
    }

    async fn ensure_browser(self: &Arc<Self>) -> Result<Arc<ChromeBrowser>, BrowserError> {
        if let Some(browser) = self.browser.read().clone() {
            return Ok(browser);
        }

        let _guard = self.relaunch_lock.lock().await;

        if let Some(browser) = self.browser.read().clone() {
            return Ok(browser);
        }

        info!("Spawning browser on demand");
        let spawn = tokio::time::timeout(
            std::time::Duration::from_secs(45),
            spawn_chrome(&self.launch),
        )
        .await
        .map_err(|_| {
            BrowserError::NavigationFailed("timed out waiting for Chrome to start".to_string())
        })?;
        let (chrome, handler) = spawn.map_err(|e| {
            warn!("Chrome failed to start (is another chrome.exe holding the profile?): {}", e);
            e
        })?;
        let chrome = Arc::new(chrome);

        // Adopt the window's initial tab so driving the desktop address bar
        // navigates a streamed tab instead of an untracked one.
        match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            chrome.pages(),
        )
        .await
        {
            Ok(Ok(pages)) => {
                for page in pages {
                    let page = Arc::new(page);
                    if let Err(e) = page
                        .execute(SetDeviceMetricsOverrideParams::new(
                            self.launch.viewport.width as i64,
                            self.launch.viewport.height as i64,
                            1.0,
                            false,
                        ))
                        .await
                    {
                        warn!("Failed to pin viewport: {}", e);
                    }
                    let id = TabId::next();
                    self.tabs.write().insert(
                        id,
                        Arc::new(Tab::new(id, page, "System".to_string())),
                    );
                    info!(%id, "Adopted startup tab");
                }
            }
            Ok(Err(e)) => warn!("Failed to list initial tabs: {}", e),
            Err(_) => warn!("Timed out listing initial tabs"),
        }

        *self.browser.write() = Some(chrome.clone());
        Self::watch(self.clone(), handler);

        Ok(chrome)
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<TabEvent> {
        self.event_tx.subscribe()
    }

    pub fn shutdown_if_idle(&self) {
        if self.tabs.read().is_empty() {
            *self.browser.write() = None;
            info!("No tabs left, shut down browser");
        }
    }

    pub fn shutdown(&self) {
        self.tabs.write().clear();
        *self.browser.write() = None;
        info!("Browser shut down");
    }

    #[instrument(skip(self))]
    pub async fn new_tab(
        self: &Arc<Self>,
        url: Option<&str>,
        creator: String,
    ) -> Result<TabId, BrowserError> {
        {
            let tabs = self.tabs.read();
            if tabs.len() >= self.launch.max_tabs {
                return Err(BrowserError::TabLimitReached(self.launch.max_tabs));
            }
        }

        let browser = self.ensure_browser().await?;
        let page = browser.new_page("about:blank").await?;

        // Headful windows rarely match the configured size, so pin the page
        // viewport explicitly like Playwright does for the Node version.
        if let Err(e) = page
            .execute(SetDeviceMetricsOverrideParams::new(
                self.launch.viewport.width as i64,
                self.launch.viewport.height as i64,
                1.0,
                false,
            ))
            .await
        {
            warn!("Failed to pin viewport: {}", e);
        }

        let id = TabId::next();

        let tab = Arc::new(Tab::new(id, Arc::new(page), creator.clone()));

        self.tabs.write().insert(id, tab);

        info!(%id, "Created tab");

        let _ = self.event_tx.send(TabEvent::Created);

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

        if let Err(e) = tab.page.execute(CloseParams::default()).await {
            warn!(%id, "Failed to close tab page: {}", e);
        }

        info!(%id, "Closed tab");
        let _ = self.event_tx.send(TabEvent::Closed);

        Ok(())
    }

    #[instrument(skip(self), fields(tab_id = %id))]
    pub async fn navigate(&self, id: TabId, url: &str) -> Result<(), BrowserError> {
        let url = normalize_url(url)?;
        info!(%url, "Navigating");

        let tab = {
            let tabs = self.tabs.read();
            tabs.get(&id).cloned()
        }
        .ok_or_else(|| BrowserError::TabNotFound(id.to_string()))?;

        tab.page
            .goto(url.as_str())
            .await
            .map_err(|e| BrowserError::NavigationFailed(e.to_string()))?;

        let new_url = tab.page.url().await.ok().flatten().unwrap_or(url);
        let new_title = tab.page.get_title().await.ok().flatten().unwrap_or_default();

        *tab.url.write() = new_url.clone();
        *tab.title.write() = new_title.clone();

        let _ = self.event_tx.send(TabEvent::Updated);

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn reload_tab(&self, id: TabId) -> Result<(), BrowserError> {
        let tab = {
            let tabs = self.tabs.read();
            tabs.get(&id).cloned()
        }
        .ok_or_else(|| BrowserError::TabNotFound(id.to_string()))?;

        tab.page.reload().await?;
        *tab.crashed.write() = false;

        info!(%id, "Reloaded tab");
        Ok(())
    }

    pub fn get_tab(&self, id: TabId) -> Option<Arc<Tab>> {
        self.tabs.read().get(&id).cloned()
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.read().len()
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
        let tab = {
            let tabs = self.tabs.read();
            tabs.get(&id).cloned()
        }
        .ok_or_else(|| BrowserError::TabNotFound(id.to_string()))?;

        let state = if freeze {
            SetWebLifecycleStateState::Frozen
        } else {
            SetWebLifecycleStateState::Active
        };

        if let Err(e) = tab.page.execute(SetWebLifecycleStateParams { state }).await {
            warn!(%id, "Failed to set lifecycle state: {}", e);
        }

        if freeze {
            let _ = tab.page.evaluate(
                "document.querySelectorAll('video,audio').forEach(e => e.pause())"
            ).await;
        }

        *tab.frozen.write() = freeze;
        info!(%id, freeze, "Tab freeze state changed");

        Ok(())
    }

    pub async fn screenshot(&self, id: TabId) -> Result<Vec<u8>, BrowserError> {
        let tab = {
            let tabs = self.tabs.read();
            tabs.get(&id).cloned()
        }
        .ok_or_else(|| BrowserError::TabNotFound(id.to_string()))?;

        if *tab.crashed.read() {
            return Err(BrowserError::TabCrashed);
        }

        let bytes = tab.page
            .screenshot(ScreenshotParams::builder()
                .format(CaptureScreenshotFormat::Jpeg)
                .quality(50)
                .build())
            .await?;

        Ok(bytes)
    }

    pub fn viewport(&self) -> Viewport {
        self.launch.viewport
    }
}

async fn spawn_chrome(
    launch: &LaunchOptions,
) -> Result<(ChromeBrowser, chromiumoxide::Handler), BrowserError> {
    let mut args = vec![
        "--disable-backgrounding-occluded-windows".to_string(),
        "--disable-renderer-backgrounding".to_string(),
        "--disable-background-timer-throttling".to_string(),
        "--disable-translate".to_string(),
        "--disable-features=OptimizationHints,MediaRouter".to_string(),
    ];

    if launch.no_gpu {
        args.push("--disable-gpu".to_string());
    }

    if !launch.headless {
        args.push("--start-maximized".to_string());
        args.push("--hide-crash-restore-bubble".to_string());
    }

    let executable = chromiumoxide::detection::default_executable(Default::default())
        .map_err(BrowserError::NavigationFailed)?;
    info!(?executable, "Using browser executable");

    let profile_dir = absolute_profile_dir(&launch.profile_dir)?;

    // A killed Chrome leaves its Singleton locks behind and the next instance
    // then hangs or exits instead of starting. No live Chrome uses this
    // profile at spawn time, so stale locks are safe to drop.
    for name in ["SingletonSocket", "SingletonLock", "SingletonCookie"] {
        let lock = profile_dir.join(name);
        if lock.exists() {
            let _ = std::fs::remove_file(&lock);
        }
    }

    let mut builder = BrowserConfig::builder()
        .chrome_executable(&executable)
        .window_size(launch.viewport.width, launch.viewport.height)
        .user_data_dir(&profile_dir)
        .args(args);

    builder = if launch.headless {
        builder.new_headless_mode()
    } else {
        builder.with_head()
    };

    let config = builder
        .build()
        .map_err(BrowserError::NavigationFailed)?;

    ChromeBrowser::launch(config)
        .await
        .map_err(|e| BrowserError::NavigationFailed(e.to_string()))
}

fn absolute_profile_dir(profile_dir: &Path) -> Result<PathBuf, BrowserError> {
    if profile_dir.is_absolute() {
        return Ok(profile_dir.to_path_buf());
    }

    std::env::current_dir()
        .map(|cwd| cwd.join(profile_dir))
        .map_err(|e| BrowserError::NavigationFailed(e.to_string()))
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
