'use strict';

const roomOf = (tabId) => `tab:${tabId}`;

function broadcastTabs(io, browser, presence) {
  const tabs = browser.listTabs();
  const viewers = presence.viewersByTab(tabs);
  io.emit(
    'tabs:update',
    tabs.map((t) => ({ ...t, viewers: viewers[t.id] || [], viewerCount: (viewers[t.id] || []).length }))
  );
}

function viewerCountOf(browser, presence, tabId) {
  const viewers = presence.viewersByTab(browser.listTabs());
  return (viewers[tabId] || []).length;
}

async function refreshStream(browser, presence, streamer, tabId) {
  if (!tabId || !browser.has(tabId)) return;
  await streamer.setViewers(tabId, viewerCountOf(browser, presence, tabId));
}

async function sendShot(socket, browser, tabId) {
  if (!tabId || !browser.has(tabId)) {
    socket.emit('shot', { tabId: null, dataUrl: null });
    return;
  }
  const buf = await browser.screenshot(tabId);
  if (buf) socket.emit('shot', { tabId, dataUrl: `data:image/jpeg;base64,${buf.toString('base64')}` });
  else socket.emit('shot', { tabId, dataUrl: null });
}

function registerSockets(io, { browser, presence, streamer, input }) {
  browser.onUpdate(() => broadcastTabs(io, browser, presence));
  browser.onPopupBlocked(() => {
    io.emit('error', { message: 'Popup bi chan: da dat gioi han tab.' });
  });

  io.on('connection', (socket) => {
    socket.on('join', async ({ name } = {}) => {
      const clean = presence.join(socket.id, name);
      socket.data.name = clean;
      socket.data.tabId = null;
      socket.emit('joined', { name: clean, viewport: browser.viewport });
      socket.emit('tabs:sync', browser.listTabs());
      broadcastTabs(io, browser, presence);
    });

    socket.on('rename', ({ name } = {}) => {
      const clean = presence.rename(socket.id, name);
      if (clean) {
        socket.data.name = clean;
        socket.emit('joined', { name: clean });
        broadcastTabs(io, browser, presence);
      }
    });

    socket.on('tab:watch', async ({ tabId } = {}) => {
      if (!browser.has(tabId)) {
        socket.emit('error', { message: 'Tab khong ton tai.' });
        return;
      }
      const prev = socket.data.tabId;
      if (prev === tabId) {
        await sendShot(socket, browser, tabId);
        return;
      }
      if (prev) socket.leave(roomOf(prev));
      socket.data.tabId = tabId;
      presence.watch(socket.id, tabId);
      socket.join(roomOf(tabId));
      broadcastTabs(io, browser, presence);
      await refreshStream(browser, presence, streamer, prev);
      await refreshStream(browser, presence, streamer, tabId);
      await sendShot(socket, browser, tabId);
    });

    socket.on('tab:create', async ({ url } = {}) => {
      try {
        const tab = await browser.newTab(url || 'about:blank', socket.data.name || 'Khach');
        socket.emit('tab:created', { tabId: tab.id });
      } catch (err) {
        socket.emit('error', { message: err.message });
      }
    });

    socket.on('tab:close', async ({ tabId } = {}) => {
      const ok = await browser.closeTab(tabId).catch(() => false);
      if (!ok) {
        socket.emit('error', { message: 'Could not close tab.' });
        return;
      }
      await streamer.stop(tabId);
      if (socket.data.tabId === tabId) {
        socket.leave(roomOf(tabId));
        socket.data.tabId = null;
      }
      await sendShot(socket, browser, socket.data.tabId);
    });

    socket.on('nav', async ({ tabId, url } = {}) => {
      try {
        await browser.navigate(tabId, url);
      } catch (err) {
        socket.emit('error', { message: err.message });
      }
    });

    socket.on('nav:back', ({ tabId } = {}) => input.back(tabId || socket.data.tabId).catch(() => {}));
    socket.on('nav:reload', ({ tabId } = {}) => input.reload(tabId || socket.data.tabId).catch(() => {}));
    socket.on('tab:reload', async ({ tabId } = {}) => {
      const ok = await browser.reloadTab(tabId).catch(() => false);
      if (!ok) socket.emit('error', { message: 'Khong tai lai duoc tab.' });
    });

    socket.on('input:click', async ({ tabId, x, y } = {}, ack) => {
      try {
        await input.click(tabId, x, y);
        if (typeof ack === 'function') ack({ ok: true });
      } catch (err) {
        if (typeof ack === 'function') ack({ ok: false, message: err.message });
        else socket.emit('error', { message: err.message });
      }
    });

    socket.on('input:scroll', ({ tabId, dx, dy } = {}) => {
      input.scroll(tabId, dx, dy).catch(() => {});
    });

    socket.on('input:zoom', ({ tabId, dy } = {}) => {
      input.zoom(tabId, dy).catch(() => {});
    });

    socket.on('input:type', ({ tabId, text } = {}) => {
      input.type(tabId, text).catch((err) => socket.emit('error', { message: err.message }));
    });

    socket.on('input:key', ({ tabId, key } = {}) => {
      input.key(tabId, key).catch((err) => socket.emit('error', { message: err.message }));
    });

    socket.on('shot:refresh', async ({ tabId } = {}) => {
      const id = tabId || socket.data.tabId;
      if (id) await sendShot(socket, browser, id);
    });

    socket.on('disconnect', async () => {
      const watched = socket.data.tabId;
      presence.leave(socket.id);
      broadcastTabs(io, browser, presence);
      await refreshStream(browser, presence, streamer, watched);
    });
  });
}

module.exports = { registerSockets, broadcastTabs };
