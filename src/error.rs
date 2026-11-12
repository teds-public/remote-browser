use thiserror::Error;

#[derive(Error, Debug)]
pub enum BrowserError {
    #[error("Tab {0} not found")]
    TabNotFound(String),
    #[error("Tab limit reached ({0} max)")]
    TabLimitReached(usize),
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("Navigation failed: {0}")]
    NavigationFailed(String),
    #[error("Tab crashed")]
    TabCrashed,
    #[error("CDP error: {0}")]
    Cdp(#[from] chromiumoxide::error::CdpError),
}

#[derive(Error, Debug)]
pub enum InputError {
    #[error("Invalid coordinates: ({0}, {1}) outside viewport")]
    InvalidCoordinates(f64, f64),
    #[error("Key '{0}' not in allow-list")]
    DisallowedKey(String),
    #[error("Text too long: {0} chars (max 500)")]
    TextTooLong(usize),
    #[error("CDP error: {0}")]
    Cdp(#[from] chromiumoxide::error::CdpError),
}

#[derive(Error, Debug)]
pub enum SocketError {
    #[error("Missing field: {0}")]
    MissingField(&'static str),
    #[error("Invalid quality: {0}")]
    InvalidQuality(String),
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Environment variable error: {0}")]
    Env(#[from] envy::Error),
    #[error("Port must be between 1-65535")]
    InvalidPort,
    #[error("Viewport must be at least 800x600")]
    ViewportTooSmall,
    #[error("Viewport cannot exceed 7680x4320")]
    ViewportTooLarge,
    #[error("Max tabs must be between 1-20")]
    InvalidMaxTabs,
    #[error("Invalid viewport format, expected WIDTHxHEIGHT")]
    InvalidViewportFormat,
}
