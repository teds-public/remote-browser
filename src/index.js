'use strict';

require('dotenv').config();
const crypto = require('node:crypto');
const path = require('node:path');
const express = require('express');
const { Server } = require('socket.io');
const { loadConfig } = require('./config');
const { createBrowserManager } = require('./browser');
const { createPresence } = require('./presence');
const { createStreamer } = require('./streamer');
const { createInput } = require('./input');
const { registerSockets } = require('./sockets');

function tokenGuard(expected) {
  return (req, res, next) => {
    if (req.path === '/healthz') return next();
    const got = req.query.token || req.get('x-remote-token') || '';
    if (got && expected && crypto.timingSafeEqual(Buffer.from(String(got)), Buffer.from(expected))) return next();
    res.status(401).json({ ok: false, message: 'Thieu hoac sai token.' });
  };
}

async function main() {
  const config = loadConfig();
  const browser = createBrowserManager(config);
  const presence = createPresence();
  const input = createInput({ getPage: (id) => browser.getPage(id), viewport: config.viewport });

  await browser.launch();

  const app = express();
  if (config.token) app.use(tokenGuard(config.token));
  app.use(express.static(path.join(__dirname, '..', 'public')));
  app.get('/healthz', (req, res) => {
    res.json({ ok: true, tabs: browser.listTabs().length });
  });

  const server = app.listen(config.port, '0.0.0.0', () => {
    console.log(`remote-browser dang chay: http://localhost:${config.port}`);
  });
  const io = new Server(server);
  if (config.token) {
    io.use((socket, next) => {
      const got = String(socket.handshake.auth?.token || '');
      if (got && crypto.timingSafeEqual(Buffer.from(got), Buffer.from(config.token))) return next();
      next(new Error('Sai token.'));
    });
  }

  const streamer = createStreamer({
    getPage: (id) => browser.getPage(id),
    emitToRoom: (tabId, buf) => {
      io.to(`tab:${tabId}`).emit('frame', tabId, buf);
    },
    onError: (tabId, message) => {
      io.to(`tab:${tabId}`).emit('error', { message });
    },
  });

  registerSockets(io, { browser, presence, streamer, input });

  for (const signal of ['SIGINT', 'SIGTERM']) {
    process.on(signal, async () => {
      await streamer.shutdown();
      await browser.shutdown();
      server.close(() => process.exit(0));
    });
  }
}

main().catch((err) => {
  console.error(err.message);
  process.exit(1);
});
