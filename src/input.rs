use crate::error::InputError;
use crate::types::{TabId, Viewport};
use chromiumoxide::Page;
use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::time::Instant;
use tracing::{instrument, Span};

static ALLOWED_KEYS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut set = HashSet::new();
    set.insert("Enter");
    set.insert("Backspace");
    set.insert("Tab");
    set.insert("Escape");
    set.insert("Delete");
    set.insert("ArrowLeft");
    set.insert("ArrowRight");
    set.insert("ArrowUp");
    set.insert("ArrowDown");
    set.insert("Home");
    set.insert("End");
    set
});

const MAX_TEXT_LENGTH: usize = 500;
const MAX_SCROLL_DELTA: i32 = 2000;
const MAX_ZOOM_DELTA: i32 = 500;
const CLICK_DOM_WAIT_MS: u64 = 3000;

pub fn clamp_coords(x: f64, y: f64, viewport: Viewport) -> Result<(i32, i32), InputError> {
    if x < 0.0 || y < 0.0 || x > viewport.width as f64 || y > viewport.height as f64 {
        return Err(InputError::InvalidCoordinates(x, y));
    }

    let cx = x.clamp(0.0, viewport.width as f64).round() as i32;
    let cy = y.clamp(0.0, viewport.height as f64).round() as i32;

    Ok((cx, cy))
}

#[instrument(skip(page), fields(latency_ms))]
pub async fn click(
    page: &Page,
    x: f64,
    y: f64,
    viewport: Viewport,
) -> Result<(), InputError> {
    let start = Instant::now();
    let (cx, cy) = clamp_coords(x, y, viewport)?;

    tokio::time::sleep(tokio::time::Duration::from_millis(CLICK_DOM_WAIT_MS)).await;

    page.mouse
        .click(cx as f64, cy as f64)
        .await?;

    Span::current().record("latency_ms", start.elapsed().as_millis());
    Ok(())
}

#[instrument(skip(page))]
pub async fn scroll(
    page: &Page,
    dx: i32,
    dy: i32,
) -> Result<(), InputError> {
    let dx = dx.clamp(-MAX_SCROLL_DELTA, MAX_SCROLL_DELTA);
    let dy = dy.clamp(-MAX_SCROLL_DELTA, MAX_SCROLL_DELTA);

    page.mouse
        .wheel(dx as f64, dy as f64)
        .await?;

    Ok(())
}

#[instrument(skip(page))]
pub async fn zoom(
    page: &Page,
    dy: i32,
) -> Result<(), InputError> {
    let dy = dy.clamp(-MAX_ZOOM_DELTA, MAX_ZOOM_DELTA);

    page.keyboard.down("Control").await?;
    page.mouse.wheel(0.0, dy as f64).await?;
    page.keyboard.up("Control").await?;

    Ok(())
}

#[instrument(skip(page))]
pub async fn type_text(
    page: &Page,
    text: &str,
) -> Result<(), InputError> {
    if text.len() > MAX_TEXT_LENGTH {
        return Err(InputError::TextTooLong(text.len()));
    }

    let truncated = &text[..MAX_TEXT_LENGTH.min(text.len())];
    page.keyboard.type_str(truncated).await?;

    Ok(())
}

#[instrument(skip(page))]
pub async fn press_key(
    page: &Page,
    key: &str,
) -> Result<(), InputError> {
    if !ALLOWED_KEYS.contains(key) {
        return Err(InputError::DisallowedKey(key.to_string()));
    }

    page.keyboard.press(key).await?;

    Ok(())
}
