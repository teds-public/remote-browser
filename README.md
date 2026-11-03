# Remote Browser

One Chromium on a desktop, 3–5 phones view and control it over LAN.
Each tab is a Playwright Page; every viewer joins a Socket.IO room per tab
and receives a binary JPEG screencast (~10–15fps). The server is the source
of truth for tabs and presence.

## Run

```bash
copy .env.example .env
npm install
npx playwright install chromium
npm start
rem or double-click run.bat
```

Open on phones on the same Wi-Fi: `http://<desktop-ip>:5001`.
Health check: `http://<desktop-ip>:5001/healthz`.

## Project structure

```
remote-browser/
  run.bat            # double-click starter: checks Node, copies .env, installs, starts
  .env.example       # documented env template (never commit a real .env)
  package.json       # express, socket.io, playwright, dotenv
  scripts/
    monitor.js       # pings /healthz every 60s, logs latency for soak tests
  src/
    index.js         # bootstrap: env fail-fast, express + socket.io + playwright
    config.js        # reads PORT, VIEWPORT, MAX_TABS, PROFILE_DIR, TOKEN, HEADLESS
    browser.js       # persistent context, tab CRUD, popup guard, crash flag
    streamer.js      # CDP screencast per watched tab, q60/q40 adaptive, stops at 0 viewers
    presence.js      # socketId -> {name, tabId}, viewers per tab
    input.js         # click/scroll/zoom/key/back/reload with bounds validation
    sockets.js       # all Socket.IO events: join, tab:*, nav*, input:*, shot:refresh
  public/
    index.html       # dark card UI: header, sidebar tabs, viewer, modals
    client.js        # socket.io client: stream render, gestures, reconnect resync
```

## Environment (.env.example)

| Variable | Default | Notes |
|---|---|---|
| `PORT` | 5001 | HTTP + Socket.IO port |
| `VIEWPORT` | 1920x1080 | tab size, parsed at startup, crashes loudly if invalid |
| `MAX_TABS` | 8 | over limit: tab creation refused, popups auto-closed |
| `PROFILE_DIR` | ./profile | cookies/logins persist here, never wiped on restart |
| `HEADLESS` | true | `true` = background (stable when screen locked); `false` = visible window for debugging |
| `TOKEN` | (empty) | only set when exposing beyond LAN; client adds `?token=...` to URL |

LAN-only by default. Beyond LAN: Tailscale (recommended) or `TOKEN` + tunnel.
Never expose the port directly to the internet.

## Socket.IO protocol

Rooms: one room per tab (`tab:<id>`), so each viewer only gets their tab's frames.

Client → server: `join {name}` · `rename {name}` · `tab:create {url?}` ·
`tab:close {tabId}` · `tab:watch {tabId}` · `tab:reload {tabId}` ·
`nav {tabId, url}` · `nav:back` / `nav:reload {tabId}` ·
`input:click {tabId, x, y}` (ack with timing) · `input:scroll {tabId, dx, dy}` ·
`input:zoom {tabId, dy}` · `input:type {tabId, text}` · `input:key {tabId, key}` ·
`shot:refresh {tabId}`.

Server → client: `joined {name, viewport}` · `tabs:sync` / `tabs:update`
(id, title, url, creator, viewers, viewerCount) · `frame <tabId, jpeg buffer>` ·
`shot {tabId, dataUrl}` · `tab:crashed` · `error {message}`.

## Soak test

```bash
node scripts/monitor.js >> monitor.log
```

Pings `/healthz` every 60s and logs latency. Check RAM/CPU in Task Manager
after 1h with 8 heavy tabs and after 24h. `FAIL` in the log means the server
went down.
