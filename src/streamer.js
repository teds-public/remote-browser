'use strict';

function createStreamer({ getPage, emitToRoom, onError }) {
  const sessions = new Map();

  function qualityFor(viewerCount) {
    return viewerCount > 3 ? 40 : 60;
  }

  async function start(tabId, viewerCount) {
    const page = getPage(tabId);
    if (!page) return;
    const quality = qualityFor(viewerCount);
    const existing = sessions.get(tabId);
    if (existing) {
      if (existing.quality === quality) return;
      await stop(tabId);
    }
    const cdp = await page.context().newCDPSession(page);
    const session = { cdp, quality };
    sessions.set(tabId, session);
    cdp.on('Page.screencastFrame', ({ data, sessionId }) => {
      if (sessions.get(tabId) !== session) return;
      emitToRoom(tabId, Buffer.from(data, 'base64'));
      cdp.send('Page.screencastFrameAck', { sessionId }).catch(() => {});
    });
    await cdp.send('Page.startScreencast', {
      format: 'jpeg',
      quality,
      everyNthFrame: 2,
    }).catch((err) => {
      console.error(`screencast that bai (tab ${tabId}): ${err.message}`);
      sessions.delete(tabId);
      if (onError) onError(tabId, `Khong lay duoc hinh tab (screencast loi).`);
    });
  }

  async function stop(tabId) {
    const session = sessions.get(tabId);
    if (!session) return;
    sessions.delete(tabId);
    await session.cdp.send('Page.stopScreencast').catch(() => {});
    await session.cdp.detach().catch(() => {});
  }

  async function setViewers(tabId, viewerCount) {
    if (viewerCount <= 0) await stop(tabId);
    else await start(tabId, viewerCount).catch(() => {});
  }

  async function shutdown() {
    for (const tabId of [...sessions.keys()]) await stop(tabId);
  }

  return { setViewers, stop, shutdown };
}

module.exports = { createStreamer };
