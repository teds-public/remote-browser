# remote-browser

Shared remote browser: one Playwright Chromium on a desktop, phones control it over LAN via Socket.IO.

## Stack

- Runtime: Node.js 20+, Windows desktop. Packages: `express`, `socket.io`, `playwright`, `dotenv`.
- Browser: `chromium.launchPersistentContext(PROFILE_DIR, { viewport, headless })`. One shared context (shared cookies/logins). One tab = one Page.
- Stream: CDP `Page.screencastFrame` (JPEG) per watched tab only, binary over rooms `tab:<id>`.

## Layout

- `run.bat` — entry point for users (checks Node, seeds `.env`, installs, starts).
- `scripts/monitor.js` — soak-test pinger, logs latency to stdout.
- `src/index.js` — bootstrap only: load config (fail-fast), static + `/healthz`, Socket.IO auth, graceful shutdown.
- `src/config.js` — env parsing + validation. All config loads here, nothing reads `process.env` elsewhere.
- `src/browser.js` — owns all Playwright state. Source of truth for `tabs[]`.
- `src/streamer.js` — owns CDP screencast sessions. Start on first viewer, stop at zero.
- `src/presence.js` — owns `socketId -> {name, tabId}`.
- `src/input.js` — owns all page interaction, validates bounds at the boundary.
- `src/sockets.js` — owns the Socket.IO protocol, wires browser/streamer/presence/input.
- `public/` — dark card UI (header + sidebar tabs + viewer), gestures, reconnect resync.

## Conventions

- Validate at boundaries: env in `config.js`, URLs in `browser.js` (http/https only), coordinates/keys in `input.js`.
- Server never trusts client-scaled coordinates; client sends raw page coords, server clamps to `VIEWPORT`.
- Errors: socket handlers reply via ack or `error` event; streamer failures log to console (never swallowed).
- No `page.screenshot()` polling for stream. No per-tab BrowserContext. No secrets in bundles — `TOKEN` stays server-side.
- UI language: English. Comments: none by default, one line only for non-obvious `why`.
