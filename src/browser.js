'use strict';

const { chromium } = require('playwright');

function normalizeUrl(raw) {
  let url = String(raw || '').trim();
  if (!url) return null;
  if (!/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(url)) url = `https://${url}`;
  let parsed;
  try {
    parsed = new URL(url);
  } catch {
    return null;
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return null;
  return parsed.toString();
}

function createBrowserManager({ viewport, maxTabs, profileDir, headless = true }) {
  let context = null;
  let counter = 0;
  const tabs = new Map();
  const knownPages = new Set();
  const listeners = new Set();
  let blockedListener = null;

  function emit() {
    const state = listTabs();
    for (const fn of listeners) fn(state);
  }

  function onUpdate(fn) {
    listeners.add(fn);
  }

  function onPopupBlocked(fn) {
    blockedListener = fn;
  }

  function listTabs() {
    return [...tabs.values()].map((t) => ({
      id: t.id,
      title: t.title,
      url: t.url,
      creator: t.creator,
      createdAt: t.createdAt,
      crashed: t.crashed,
    }));
  }

  function debugTabs() {
    return [...tabs.values()].map((t) => ({
      id: t.id,
      url: t.url,
      frozen: !!t.frozen,
    }));
  }

  function describe(tab) {
    return {
      id: tab.id,
      title: tab.title,
      url: tab.url,
      creator: tab.creator,
      createdAt: tab.createdAt,
      crashed: tab.crashed,
    };
  }

  async function launch() {
    context = await chromium.launchPersistentContext(profileDir, {
      viewport,
      headless,
      args: headless
        ? [
            '--disable-gpu',
            '--disable-background-timer-throttling',
            '--disable-backgrounding-occluded-windows',
            '--disable-renderer-backgrounding',
            '--disable-features=Translate,OptimizationHints,MediaRouter',
          ]
        : [],
    });
    context.on('page', (page) => {
      if (knownPages.has(page)) return;
      if (tabs.size >= maxTabs) {
        page.close().catch(() => {});
        if (blockedListener) blockedListener();
        return;
      }
      registerPage(page, 'he thong').catch(() => {});
    });
    const pages = context.pages();
    for (const page of pages) await page.close().catch(() => {});
  }

  async function registerPage(page, creator) {
    for (const t of tabs.values()) {
      if (t.page === page) return t;
    }
    knownPages.add(page);
    counter += 1;
    const tab = {
      id: `t${counter}`,
      page,
      title: '',
      url: page.url() || 'about:blank',
      creator,
      createdAt: Date.now(),
      crashed: false,
      lastNavEmit: 0,
    };
    tabs.set(tab.id, tab);

    page.on('crash', () => {
      tab.crashed = true;
      emit();
    });
    page.on('close', () => {
      if (!tabs.has(tab.id)) return;
      tabs.delete(tab.id);
      emit();
    });
    page.on('framenavigated', (frame) => {
      if (frame !== page.mainFrame()) return;
      const now = Date.now();
      if (now - tab.lastNavEmit < 500) return;
      tab.lastNavEmit = now;
      tab.url = page.url();
      page.title().then(
        (title) => {
          tab.title = title || tab.url;
          emit();
        },
        () => {
          tab.title = tab.url;
          emit();
        }
      );
    });

    try {
      tab.title = (await page.title()) || tab.url;
    } catch {
      tab.title = tab.url;
    }
    emit();
    return tab;
  }

  async function newTab(url, creator) {
    if (tabs.size >= maxTabs) {
      const err = new Error(`Da dat gioi han ${maxTabs} tab.`);
      err.code = 'MAX_TABS';
      throw err;
    }
    const page = await context.newPage();
    const tab = await registerPage(page, creator || 'he thong');
    if (url && url !== 'about:blank') await navigate(tab.id, url);
    return describe(tab);
  }

  async function closeTab(id) {
    const tab = tabs.get(id);
    if (!tab) return false;
    tabs.delete(id);
    await tab.page.close().catch(() => {});
    emit();
    return true;
  }

  async function closeAllTabs() {
    for (const tab of [...tabs.values()]) {
      tabs.delete(tab.id);
      await tab.page.close().catch(() => {});
    }
    emit();
  }

  async function navigate(id, rawUrl) {
    const tab = tabs.get(id);
    if (!tab) {
      const err = new Error('Tab khong ton tai.');
      err.code = 'NO_TAB';
      throw err;
    }
    if (rawUrl === 'about:blank') {
      await tab.page.goto('about:blank');
      return describe(tab);
    }
    const url = normalizeUrl(rawUrl);
    if (!url) {
      const err = new Error('URL khong hop le (chi http/https).');
      err.code = 'BAD_URL';
      throw err;
    }
    tab.crashed = false;
    await tab.page.goto(url, { timeout: 15000, waitUntil: 'domcontentloaded' });
    return describe(tab);
  }

  async function screenshot(id) {
    const tab = tabs.get(id);
    if (!tab || tab.crashed) return null;
    try {
      return await tab.page.screenshot({ type: 'jpeg', quality: 50 });
    } catch {
      return null;
    }
  }

  function has(id) {
    return tabs.has(id);
  }

  function getPage(id) {
    return tabs.get(id)?.page || null;
  }

  const freezeSessions = new Map();

  async function freeze(id, shouldFreeze) {
    const tab = tabs.get(id);
    if (!tab) return;
    if (tab.frozen === shouldFreeze) return;
    tab.frozen = shouldFreeze;
    try {
      let cdp = freezeSessions.get(id);
      if (!cdp) {
        cdp = await tab.page.context().newCDPSession(tab.page);
        freezeSessions.set(id, cdp);
      }
      await cdp.send('Page.setWebLifecycleState', {
        state: shouldFreeze ? 'frozen' : 'active',
      });
    } catch {
      // Best effort: tab van chay binh thuong neu lenh that bai.
    }
  }

  async function reloadTab(id) {
    const tab = tabs.get(id);
    if (!tab) return false;
    tab.crashed = false;
    await tab.page.reload({ timeout: 15000 }).catch(() => {});
    return true;
  }

  async function shutdown() {
    await context?.close().catch(() => {});
  }

  return { viewport, launch, onUpdate, onPopupBlocked, listTabs, debugTabs, newTab, closeTab, closeAllTabs, navigate, screenshot, freeze, has, getPage, reloadTab, shutdown };
}

module.exports = { createBrowserManager, normalizeUrl };
