'use strict';

function createStreamer({ getPage, emitToRoom, onError }) {
  const sessions = new Map();

  const QUALITY_PARAMS = {
    low: { quality: 30, everyNthFrame: 6, maxWidth: 854, maxHeight: 480 },
    med: { quality: 45, everyNthFrame: 4, maxWidth: 1280, maxHeight: 720 },
    high: { quality: 60, everyNthFrame: 2, maxWidth: 1920, maxHeight: 1080 },
  };

  async function start(tabId, prefs) {
    const page = getPage(tabId);
    if (!page) return;
    const { quality, everyNthFrame, maxWidth, maxHeight } =
      QUALITY_PARAMS[prefs.quality] || QUALITY_PARAMS.med;
    const existing = sessions.get(tabId);
    if (existing) {
      if (
        existing.quality === quality &&
        existing.everyNthFrame === everyNthFrame &&
        existing.maxWidth === maxWidth
      ) return;
      await stop(tabId);
    }
    const cdp = await page.context().newCDPSession(page);
    const session = { cdp, quality, everyNthFrame, maxWidth };
    sessions.set(tabId, session);
    cdp.on('Page.screencastFrame', ({ data, sessionId }) => {
      if (sessions.get(tabId) !== session) return;
      emitToRoom(tabId, Buffer.from(data, 'base64'));
      cdp.send('Page.screencastFrameAck', { sessionId }).catch(() => {});
    });
    await cdp.send('Page.startScreencast', {
      format: 'jpeg',
      quality,
      everyNthFrame,
      maxWidth,
      maxHeight,
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

  async function setViewers(tabId, prefs) {
    if (!prefs || prefs.paused) await stop(tabId);
    else await start(tabId, prefs).catch(() => {});
  }

  async function shutdown() {
    for (const tabId of [...sessions.keys()]) await stop(tabId);
  }

  return { setViewers, stop, shutdown };
}

module.exports = { createStreamer };
