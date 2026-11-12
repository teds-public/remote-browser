use crate::error::InputError;
use crate::types::Viewport;
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchKeyEventParams, DispatchKeyEventType, DispatchMouseEventParams,
    DispatchMouseEventType, InsertTextParams,
};
use chromiumoxide::keys::get_key_definition;
use chromiumoxide::layout::Point;
use chromiumoxide::Page;
use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tracing::instrument;

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
const DOM_READY_TIMEOUT_MS: u64 = 3000;
const CTRL_MODIFIER: i64 = 2;

pub fn clamp_coords(x: f64, y: f64, viewport: Viewport) -> Result<(i32, i32), InputError> {
    if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
        return Err(InputError::InvalidCoordinates(x, y));
    }

    let cx = x.clamp(0.0, viewport.width as f64).round() as i32;
    let cy = y.clamp(0.0, viewport.height as f64).round() as i32;

    Ok((cx, cy))
}

async fn wait_for_dom_ready(page: &Page) {
    let deadline = Instant::now() + Duration::from_millis(DOM_READY_TIMEOUT_MS);

    while Instant::now() < deadline {
        let ready = page
            .evaluate("document.readyState !== 'loading'")
            .await
            .ok()
            .and_then(|result| result.value().and_then(|value| value.as_bool()))
            .unwrap_or(true);

        if ready {
            return;
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[instrument(skip(page))]
pub async fn click(page: &Page, x: f64, y: f64, viewport: Viewport) -> Result<(), InputError> {
    let start = Instant::now();
    let (cx, cy) = clamp_coords(x, y, viewport)?;

    wait_for_dom_ready(page).await;

    page.click(Point::new(cx as f64, cy as f64)).await?;

    tracing::debug!(latency_ms = start.elapsed().as_millis(), "Click dispatched");
    Ok(())
}

fn wheel(viewport: Viewport, modifiers: Option<i64>, dx: i32, dy: i32) -> DispatchMouseEventParams {
    let center_x = (viewport.width / 2) as f64;
    let center_y = (viewport.height / 2) as f64;

    let mut params =
        DispatchMouseEventParams::new(DispatchMouseEventType::MouseWheel, center_x, center_y);
    params.modifiers = modifiers;
    params.delta_x = Some(dx as f64);
    params.delta_y = Some(dy as f64);
    params
}

#[instrument(skip(page))]
pub async fn scroll(page: &Page, dx: i32, dy: i32, viewport: Viewport) -> Result<(), InputError> {
    let dx = dx.clamp(-MAX_SCROLL_DELTA, MAX_SCROLL_DELTA);
    let dy = dy.clamp(-MAX_SCROLL_DELTA, MAX_SCROLL_DELTA);

    page.execute(wheel(viewport, None, dx, dy)).await?;

    Ok(())
}

#[instrument(skip(page))]
pub async fn zoom(page: &Page, dy: i32, viewport: Viewport) -> Result<(), InputError> {
    let dy = dy.clamp(-MAX_ZOOM_DELTA, MAX_ZOOM_DELTA);

    page.execute(wheel(viewport, Some(CTRL_MODIFIER), 0, dy)).await?;

    Ok(())
}

#[instrument(skip(page))]
pub async fn type_text(page: &Page, text: &str) -> Result<(), InputError> {
    if text.chars().count() > MAX_TEXT_LENGTH {
        return Err(InputError::TextTooLong(text.len()));
    }

    let text: String = text.chars().take(MAX_TEXT_LENGTH).collect();
    page.execute(InsertTextParams::new(text)).await?;

    Ok(())
}

#[instrument(skip(page))]
pub async fn press_key(page: &Page, key: &str) -> Result<(), InputError> {
    if !ALLOWED_KEYS.contains(key) {
        return Err(InputError::DisallowedKey(key.to_string()));
    }

    let definition = get_key_definition(key)
        .ok_or_else(|| InputError::DisallowedKey(key.to_string()))?;

    let mut down = DispatchKeyEventParams::new(DispatchKeyEventType::KeyDown);
    down.key = Some(definition.key.to_string());
    down.code = Some(definition.code.to_string());
    down.windows_virtual_key_code = Some(definition.key_code);
    down.native_virtual_key_code = Some(definition.key_code);
    down.text = definition.text.map(str::to_string);
    page.execute(down).await?;

    let mut up = DispatchKeyEventParams::new(DispatchKeyEventType::KeyUp);
    up.key = Some(definition.key.to_string());
    up.code = Some(definition.code.to_string());
    up.windows_virtual_key_code = Some(definition.key_code);
    up.native_virtual_key_code = Some(definition.key_code);
    page.execute(up).await?;

    Ok(())
}
