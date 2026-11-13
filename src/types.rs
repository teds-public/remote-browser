use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

static TAB_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub struct TabId(usize);

impl TabId {
    pub fn next() -> Self {
        Self(TAB_COUNTER.fetch_add(1, Ordering::SeqCst) + 1)
    }

    pub fn from_raw(n: usize) -> Self {
        Self(n)
    }

    pub fn as_str(&self) -> String {
        format!("t{}", self.0)
    }
}

impl Serialize for TabId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_str())
    }
}

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "t{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RoomId(String);

impl RoomId {
    pub fn for_tab(id: TabId) -> Self {
        Self(format!("tab:{}", id.as_str()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RoomId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
        }
    }
}

impl FromStr for Viewport {
    type Err = crate::error::ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('x').collect();
        if parts.len() != 2 {
            return Err(crate::error::ConfigError::InvalidViewportFormat);
        }

        let width = parts[0].parse::<u32>()
            .map_err(|_| crate::error::ConfigError::InvalidViewportFormat)?;
        let height = parts[1].parse::<u32>()
            .map_err(|_| crate::error::ConfigError::InvalidViewportFormat)?;

        Ok(Self { width, height })
    }
}

impl fmt::Display for Viewport {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Port(pub u16);

impl Default for Port {
    fn default() -> Self {
        Self(5001)
    }
}

impl FromStr for Port {
    type Err = crate::error::ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let port = s.parse::<u16>()
            .map_err(|_| crate::error::ConfigError::InvalidPort)?;
        if port == 0 {
            return Err(crate::error::ConfigError::InvalidPort);
        }
        Ok(Self(port))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MaxTabs(pub usize);

impl Default for MaxTabs {
    fn default() -> Self {
        Self(8)
    }
}

impl FromStr for MaxTabs {
    type Err = crate::error::ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let n = s.parse::<usize>()
            .map_err(|_| crate::error::ConfigError::InvalidMaxTabs)?;
        if !(1..=20).contains(&n) {
            return Err(crate::error::ConfigError::InvalidMaxTabs);
        }
        Ok(Self(n))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamQuality {
    Low,
    Med,
    High,
}

impl StreamQuality {
    pub fn quality(&self) -> u8 {
        match self {
            Self::Low => 30,
            Self::Med => 45,
            Self::High => 60,
        }
    }

    pub fn every_nth_frame(&self) -> u32 {
        match self {
            Self::Low => 6,
            Self::Med => 4,
            Self::High => 2,
        }
    }

    pub fn max_width(&self) -> u32 {
        match self {
            Self::Low => 854,
            Self::Med => 1280,
            Self::High => 1920,
        }
    }

    pub fn max_height(&self) -> u32 {
        match self {
            Self::Low => 480,
            Self::Med => 720,
            Self::High => 1080,
        }
    }
}

impl Default for StreamQuality {
    fn default() -> Self {
        Self::Med
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewerPrefs {
    #[serde(default)]
    pub quality: StreamQuality,
    #[serde(default)]
    pub paused: bool,
}

impl Default for ViewerPrefs {
    fn default() -> Self {
        Self {
            quality: StreamQuality::default(),
            paused: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TabMetadata {
    pub id: TabId,
    pub title: String,
    pub url: String,
    pub creator: String,
    #[serde(rename = "createdAt")]
    pub created_at: u64,
    pub crashed: bool,
    pub viewers: Vec<String>,
    #[serde(rename = "viewerCount")]
    pub viewer_count: usize,
}

pub struct Tab {
    pub id: TabId,
    pub page: std::sync::Arc<chromiumoxide::Page>,
    pub title: parking_lot::RwLock<String>,
    pub url: parking_lot::RwLock<String>,
    pub creator: String,
    pub created_at: u64,
    pub crashed: parking_lot::RwLock<bool>,
    pub frozen: parking_lot::RwLock<bool>,
}

impl Tab {
    pub fn new(
        id: TabId,
        page: std::sync::Arc<chromiumoxide::Page>,
        creator: String,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            id,
            page,
            title: parking_lot::RwLock::new(String::new()),
            url: parking_lot::RwLock::new(String::new()),
            creator,
            created_at: now,
            crashed: parking_lot::RwLock::new(false),
            frozen: parking_lot::RwLock::new(false),
        }
    }

    pub fn metadata(&self, viewers: Vec<String>) -> TabMetadata {
        TabMetadata {
            id: self.id,
            title: self.title.read().clone(),
            url: self.url.read().clone(),
            creator: self.creator.clone(),
            created_at: self.created_at,
            crashed: *self.crashed.read(),
            viewer_count: viewers.len(),
            viewers,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Presence {
    pub socket_id: String,
    pub name: String,
    pub tab_id: Option<TabId>,
    pub prefs: ViewerPrefs,
}

impl Presence {
    pub fn new(socket_id: String, name: String, prefs: ViewerPrefs) -> Self {
        Self {
            socket_id,
            name,
            tab_id: None,
            prefs,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TabEvent {
    Created,
    Closed,
    Updated,
}

pub struct Frame {
    pub tab_id: TabId,
    pub room: RoomId,
    pub jpeg: Vec<u8>,
}
