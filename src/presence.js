'use strict';

function sanitizeName(raw) {
  return String(raw || '').trim().slice(0, 20);
}

function createPresence() {
  const peers = new Map();

  function join(socketId, name) {
    const clean = sanitizeName(name) || 'Khach';
    peers.set(socketId, { name: clean, tabId: null });
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

  function viewersByTab(tabs) {
    const result = {};
    for (const t of tabs) result[t.id] = [];
    for (const peer of peers.values()) {
      if (peer.tabId && result[peer.tabId]) result[peer.tabId].push(peer.name);
    }
    return result;
  }

  return { join, rename, watch, leave, viewersByTab };
}

module.exports = { createPresence, sanitizeName };
