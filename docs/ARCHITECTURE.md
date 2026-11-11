# Architecture

## System Overview

**Remote-browser** streams one shared Chromium instance to multiple clients over LAN. The server manages tab lifecycle, input handling, and on-demand video streaming.

## Components

### Server (Node.js)

```
┌─────────────────────────────────────────┐
│  HTTP Server (Express)                  │
│  - Static files (public/)               │
│  - Health check (/healthz)              │
└─────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────┐
│  Socket.IO Server                       │
│  - Rooms: tab:<id>                      │
│  - Binary frames (JPEG)                 │
│  - Auth: optional TOKEN                 │
└─────────────────────────────────────────┘
                 │
      ┌──────────┴──────────┐
      ▼                     ▼
┌──────────────┐    ┌──────────────┐
│  Presence    │    │   Browser    │
│  - Viewers   │    │   - Tabs     │
│  - Prefs     │    │   - Pages    │
└──────────────┘    └──────────────┘
                          │
                          ▼
                  ┌──────────────┐
                  │  Streamer    │
                  │  - CDP       │
                  │  - Quality   │
                  └──────────────┘
```

### Modules

| Module | Responsibility | Key Operations |
|--------|----------------|----------------|
| `config.js` | Env validation | Parse PORT, VIEWPORT, MAX_TABS |
| `browser.js` | Chromium lifecycle | Launch, new tab, navigate, freeze |
| `streamer.js` | CDP screencast | Start/stop stream, quality presets |
| `presence.js` | Client tracking | Join, leave, viewer count |
| `input.js` | User input | Click (clamped), scroll, zoom, type |
| `sockets.js` | Socket.IO protocol | 16 events: join, nav, input:*, tabs:* |

### Client (Browser)

**HTML/CSS:** (`public/index.html`, `public/style.css`)
- Dark card UI: header + sidebar tabs + video viewer
- Responsive: desktop and mobile layouts
- Mobile: overlay sidebar with slide transition

**JavaScript:** (`public/client.js`)
- Socket.IO client, auto-reconnect
- Gesture recognition: tap, scroll, pinch-zoom
- Coordinate transform: DOM → page coords
- Frame rendering via Blob URLs (revoke on new frame)
- Stale stream detection: show badge if no frame for 3s

## Data Flow

### Stream Start

```
1. Client: join {name, quality: "med"}
2. Server: presence.join() → emit joined {name, viewport}
3. Server: emit tabs:sync [...]
4. Client: watchTab(tabId)
5. Client → Server: tab:watch {tabId}
6. Server: join room tab:<id>
7. Server: refreshStream(tabId)
   - Get viewer prefs (quality, paused)
   - If not paused: streamer.start()
8. Server: sendShot() → emit shot {tabId, dataUrl}
9. Streamer: CDP Page.startScreencast
10. CDP → Streamer: screencastFrame event
11. Streamer: decode base64 → Buffer
12. Streamer: ack frame (triggers next)
13. Streamer → Socket.IO: emit frame (binary) to room
14. Client: receive frame → Blob URL → <img>
```

### Input Flow

```
1. Client: touch/click at (clientX, clientY)
2. Client: toPageCoords() → (x, y) scaled to viewport
3. Client → Server: input:click {tabId, x, y} + ack
4. Server: clamp (x, y) to [0, viewport.width] × [0, viewport.height]
5. Server: page.mouse.click(x, y) [wait 3s for DOM]
6. Server → Client: ack {ok: true}
7. Client: show click dot animation, update latency badge
```

### Tab Freeze

```
When all viewers paused OR no viewers:
1. Server: browser.freeze(tabId, true)
2. CDP: Page.setWebLifecycleState {state: "frozen"}
3. CDP: Fetch.enable → block media/font routes
4. Page eval: pause all <video>/<audio>
5. Streamer: stop(tabId)

When first active viewer arrives:
1. Server: browser.freeze(tabId, false)
2. CDP: Page.setWebLifecycleState {state: "active"}
3. Page: unrouteAll()
4. Streamer: start(tabId, quality)
```

## Protocols

### Socket.IO Events

**Client → Server:**
- `join {name, quality?, paused?}` — authenticate, get viewport
- `rename {name}` — change display name
- `tab:watch {tabId}` — switch active tab, join room
- `tab:create {url?}` — spawn new page
- `tab:close {tabId}` — close page
- `nav {tabId, url}` — navigate
- `nav:back {tabId}` — history.back
- `nav:reload {tabId}` — reload
- `tab:reload {tabId}` — reload crashed tab
- `input:click {tabId, x, y}` — mouse click (ack callback)
- `input:scroll {tabId, dx, dy}` — mouse wheel
- `input:zoom {tabId, dy}` — Ctrl+wheel
- `input:type {tabId, text}` — keyboard insert (500 char max)
- `input:key {tabId, key}` — keyboard press (allow-list)
- `stream:prefs {quality, paused}` — change quality/pause
- `shot:refresh {tabId}` — request screenshot

**Server → Client:**
- `joined {name, viewport}` — auth success
- `tabs:sync [{id, title, url, creator, ...}]` — full tab list
- `tabs:update [...]` — incremental update
- `tab:created {tabId}` — notify creator
- `frame (tabId, Buffer)` — binary JPEG to room tab:<id>
- `shot {tabId, dataUrl}` — one-shot screenshot
- `error {message}` — toast notification

### CDP Screencast

**Start:**
```javascript
Page.startScreencast({
  format: "jpeg",
  quality: 30 | 45 | 60,  // low | med | high
  everyNthFrame: 6 | 4 | 2,
  maxWidth: 854 | 1280 | 1920,
  maxHeight: 480 | 720 | 1080,
})
```

**Frame loop:**
```javascript
Page.screencastFrame event → {data: base64, sessionId}
  ↓
decode base64 → Buffer
  ↓
emit to room tab:<id>
  ↓
Page.screencastFrameAck {sessionId}  // Must ack to get next frame
  ↓
(repeat)
```

**Stop:**
```javascript
Page.stopScreencast()
```

## Quality Presets

| Quality | JPEG | Frame skip | Max resolution | Bitrate est. | FPS |
|---------|------|------------|----------------|--------------|-----|
| Low     | 30   | every 6th  | 854×480       | ~50 KB/frame | ~2-3 |
| Med     | 45   | every 4th  | 1280×720      | ~80 KB/frame | ~4 |
| High    | 60   | every 2nd  | 1920×1080     | ~150 KB/frame | ~8 |

CDP base rate: ~16 fps when everyNthFrame=1.

## Security

- **Optional token auth:** Compare `TOKEN` env with client `?token=` query param via `crypto.timingSafeEqual`
- **URL validation:** Only http/https allowed, reject `file://`, `javascript:`, etc.
- **Coordinate clamping:** Server clamps client coords to viewport bounds
- **Input allow-list:** Only 10 keys permitted (Enter, Backspace, arrows, etc.)
- **Text length cap:** `input:type` truncated to 500 chars
- **Tab limit:** Block new tabs when `>= MAX_TABS`
- **No client trust:** All validation server-side

## Performance

**CPU:**
- Headless Chromium: ~10-15% idle, ~30-50% when streaming
- Freeze unused tabs: drops to ~5% when no viewers
- GPU disabled (`NO_GPU=true`): uses software raster

**Memory:**
- Base: ~200 MB (Chromium + Node.js)
- Per tab: ~50-100 MB
- Frame buffers: ~1 MB rotating (3 frames × 150 KB)

**Network:**
- Low: ~50 KB/frame × 3 fps = ~150 KB/s
- Med: ~80 KB/frame × 4 fps = ~320 KB/s
- High: ~150 KB/frame × 8 fps = ~1.2 MB/s

**Latency:**
- Frame: 60-120ms (screencast → ack → emit)
- Click: 50-100ms roundtrip (client → server → CDP → ack)

## Edge Cases

**Tab crash:**
- CDP `Inspector.targetCrashed` event → set `crashed: true`
- Client sees "Tab crashed" in UI
- `tab:reload` → `page.reload()` → `crashed: false`

**Stream stall:**
- Client detects no frame for 3s → show "Stream lost" badge
- Click badge → `shot:refresh` → server sends screenshot via `shot` event
- User can also toggle pause/unpause to restart stream

**Browser disconnect:**
- `context.on('close')` → auto-relaunch browser
- All tabs lost, clients reconnect with empty tab list

**Socket disconnect:**
- `presence.leave()` → remove from viewers
- `refreshStream()` on watched tab → may freeze if last viewer
- Client auto-reconnects, re-sends `join`

**Popup blocker:**
- `context.on('page')` → if `tabs.length >= MAX_TABS`, close new page
- No error to client (silent block)

## Rust Port

See `docs/RUST_STATUS.md` for conversion progress. Key differences:
- **Typed errors:** `BrowserError::TabNotFound(id)` vs strings
- **Channels:** `mpsc`/`broadcast` replace event emitters
- **Newtypes:** `TabId(usize)`, `Port(u16)` prevent primitive obsession
- **Tracing:** Structured logs with spans, latency metrics
- **No async runtime mixing:** Pure Tokio, no callback bridges

## Testing

**Manual:**
1. Start server: `npm start`
2. Open client: `http://localhost:5001`
3. Create tab, navigate to a site
4. Click, scroll, zoom
5. Open second client, verify tab list syncs
6. Close all clients, verify tab freezes (check logs)

**Load:**
- 4 clients @ high quality → ~5 MB/s total, ~60% CPU
- 8 tabs open → ~800 MB RAM

**Latency:**
```bash
node scripts/monitor.js
```
Pings server every 1s, logs roundtrip time.

## Deployment

**Docker:**
```bash
docker build -t remote-browser .
docker run -p 5001:5001 -v $(pwd)/profile:/app/profile remote-browser
```

**PM2:**
```bash
pm2 start src/index.js --name remote-browser
pm2 save
```

**Systemd:**
```ini
[Unit]
Description=Remote Browser
After=network.target

[Service]
Type=simple
User=youruser
WorkingDirectory=/path/to/remote-browser
ExecStart=/usr/bin/node src/index.js
Restart=always

[Install]
WantedBy=multi-user.target
```

## Limitations

- Single server instance (no horizontal scale)
- LAN only (no TURN/STUN for NAT traversal)
- JPEG stream (no WebRTC, H.264)
- Desktop-first (mobile viewport support but not optimized)
- No recording/screenshots via client UI
- No multi-user input conflict resolution (last-write-wins)
