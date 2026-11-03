'use strict';

const ALLOWED_KEYS = new Set([
  'Enter', 'Backspace', 'Tab', 'Escape', 'Delete',
  'ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End',
]);

const clamp = (n, min, max) => Math.min(max, Math.max(min, n));

function createInput({ getPage, viewport }) {
  function pageOf(tabId) {
    const page = getPage(tabId);
    if (!page) {
      const err = new Error('Tab khong ton tai.');
      err.code = 'NO_TAB';
      throw err;
    }
    return page;
  }

  async function click(tabId, x, y) {
    const page = pageOf(tabId);
    const cx = clamp(Math.round(Number(x)), 0, viewport.width);
    const cy = clamp(Math.round(Number(y)), 0, viewport.height);
    if (!Number.isFinite(cx) || !Number.isFinite(cy)) throw new Error('Toa do khong hop le.');
    try {
      await page.waitForLoadState('domcontentloaded', { timeout: 3000 });
    } catch {
      // Trang van dang tai — click thu, loi se bao cho client qua ack.
    }
    await page.mouse.click(cx, cy);
  }

  async function scroll(tabId, dx, dy) {
    const page = pageOf(tabId);
    await page.mouse.wheel(clamp(Number(dx) || 0, -2000, 2000), clamp(Number(dy) || 0, -2000, 2000));
  }

  async function type(tabId, text) {
    const page = pageOf(tabId);
    const slice = String(text || '').slice(0, 500);
    if (!slice) return;
    try {
      await page.keyboard.insertText(slice);
    } catch {
      await page.keyboard.type(slice);
    }
  }

  async function key(tabId, key) {
    const page = pageOf(tabId);
    if (!ALLOWED_KEYS.has(key)) throw new Error('Phim khong ho tro.');
    await page.keyboard.press(key);
  }

  async function back(tabId) {
    await pageOf(tabId).goBack({ timeout: 10000 }).catch(() => {});
  }
  async function reload(tabId) {
    await pageOf(tabId).reload({ timeout: 15000 }).catch(() => {});
  }

  async function zoom(tabId, dy) {
    const page = pageOf(tabId);
    await page.keyboard.down('Control').catch(() => {});
    try {
      await page.mouse.wheel(0, clamp(Number(dy) || 0, -500, 500));
    } finally {
      await page.keyboard.up('Control').catch(() => {});
    }
  }

  return { click, scroll, type, key, back, reload, zoom };
}

module.exports = { createInput, ALLOWED_KEYS };
