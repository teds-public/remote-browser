'use strict';

const path = require('node:path');

function parseViewport(raw) {
  const m = /^(\d+)x(\d+)$/.exec(String(raw || '').trim());
  if (!m) return null;
  const width = Number(m[1]);
  const height = Number(m[2]);
  if (width < 800 || height < 600 || width > 7680 || height > 4320) return null;
  return { width, height };
}

function parsePort(raw) {
  const port = Number(String(raw ?? '3000').trim());
  if (!Number.isInteger(port) || port < 1 || port > 65535) return null;
  return port;
}

function parseMaxTabs(raw) {
  const n = Number(String(raw ?? '8').trim());
  if (!Number.isInteger(n) || n < 1 || n > 20) return null;
  return n;
}

function parseHeadless(raw) {
  const v = String(raw ?? 'true').trim().toLowerCase();
  if (['1', 'true', 'yes', 'on'].includes(v)) return true;
  if (['0', 'false', 'no', 'off'].includes(v)) return false;
  return null;
}

function loadConfig(env = process.env) {
  const viewport = parseViewport(env.VIEWPORT || '1920x1080');
  if (!viewport) throw new Error('VIEWPORT sai format, dung WxH (vd 1920x1080).');

  const port = parsePort(env.PORT);
  if (port === null) throw new Error('PORT phai la so nguyen 1-65535.');

  const maxTabs = parseMaxTabs(env.MAX_TABS);
  if (maxTabs === null) throw new Error('MAX_TABS phai la so nguyen 1-20.');

  const profileDir = path.resolve(String(env.PROFILE_DIR || './profile').trim() || './profile');

  const token = String(env.TOKEN || '').trim() || null;

  const headless = parseHeadless(env.HEADLESS);
  if (headless === null) throw new Error('HEADLESS phai la true/false.');

  return { port, viewport, maxTabs, profileDir, token, headless };
}

module.exports = { loadConfig, parseViewport, parsePort, parseMaxTabs, parseHeadless };
