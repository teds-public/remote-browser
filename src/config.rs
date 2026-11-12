use crate::error::ConfigError;
use crate::types::{MaxTabs, Port, Viewport};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct RawConfig {
    #[serde(default = "default_port")]
    port: String,
    #[serde(default = "default_viewport")]
    viewport: String,
    #[serde(default = "default_max_tabs")]
    max_tabs: String,
    #[serde(default = "default_profile_dir")]
    profile_dir: String,
    #[serde(default)]
    headless: String,
    #[serde(default)]
    no_gpu: String,
    token: Option<String>,
}

fn default_port() -> String {
    "5001".to_string()
}
fn default_viewport() -> String {
    "1920x1080".to_string()
}
fn default_max_tabs() -> String {
    "8".to_string()
}
fn default_profile_dir() -> String {
    "./profile".to_string()
}

#[derive(Debug, Clone)]
pub struct Config {
    pub port: Port,
    pub viewport: Viewport,
    pub max_tabs: MaxTabs,
    pub profile_dir: PathBuf,
    pub headless: bool,
    pub no_gpu: bool,
    pub token: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        let raw: RawConfig = envy::from_env()?;

        let port: Port = raw.port.parse()?;
        let viewport: Viewport = raw.viewport.parse()?;
        let max_tabs: MaxTabs = raw.max_tabs.parse()?;
        let profile_dir = PathBuf::from(raw.profile_dir);

        let headless = raw.headless.is_empty() || raw.headless.eq_ignore_ascii_case("true");
        let no_gpu = raw.no_gpu.eq_ignore_ascii_case("true");

        let config = Self {
            port,
            viewport,
            max_tabs,
            profile_dir,
            headless,
            no_gpu,
            token: raw.token.filter(|t| !t.is_empty()),
        };

        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.viewport.width < 800 || self.viewport.height < 600 {
            return Err(ConfigError::ViewportTooSmall);
        }
        if self.viewport.width > 7680 || self.viewport.height > 4320 {
            return Err(ConfigError::ViewportTooLarge);
        }
        Ok(())
    }
}
