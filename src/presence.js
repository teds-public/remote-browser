'use strict';

function sanitizeName(raw) {
  return String(raw || '').trim().slice(0, 20);
}

const STREAM_QUALITIES = new Set(['low', 'med', 'high']);

function createPresence() {
  const peers = new Map();

  function join(socketId, name) {
    const clean = sanitizeName(name) || 'Khach';
    peers.set(socketId, { name: clean, tabId: null, quality: 'med', paused: false });
    return clean;
  }

  function rename(socketId, name) {
    const peer = peers.get(socketId);
    if (!peer) return null;
    peer.name = sanitizeName(name) || peer.name;
    return peer.name;
  }

  function watch(socketId, tabId) {
    const peer = peers.get(socketId);
    if (peer) peer.tabId = tabId;
  }

  function leave(socketId) {
    peers.delete(socketId);
  }

  function setStream(socketId, quality, paused) {
    const peer = peers.get(socketId);
    if (!peer) return;
    if (STREAM_QUALITIES.has(quality)) peer.quality = quality;
    peer.paused = paused === true;
  }

  function streamPrefsFor(tabId) {
    const watching = [...peers.values()].filter((p) => p.tabId === tabId);
    if (!watching.length) return null;
    if (watching.every((p) => p.paused)) return { paused: true, count: watching.length };
    const rank = { low: 0, med: 1, high: 2 };
    let quality = 'low';
    for (const p of watching) {
      if (!p.paused && rank[p.quality] > rank[quality]) quality = p.quality;
    }
    return { paused: false, count: watching.length, quality };
  }

  function viewersByTab(tabs) {
    const result = {};
    for (const t of tabs) result[t.id] = [];
    for (const peer of peers.values()) {
      if (peer.tabId && result[peer.tabId]) result[peer.tabId].push(peer.name);
    }
    return result;
  }

  return { join, rename, watch, leave, setStream, streamPrefsFor, viewersByTab };
}

module.exports = { createPresence, sanitizeName };
