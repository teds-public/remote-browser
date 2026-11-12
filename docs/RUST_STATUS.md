# Rust Port Status

Rust port: **complete and working** on chromiumoxide 0.9 / socketioxide 0.18.
The Node.js version remains the reference implementation.

## Verified

- `cargo build` and `cargo build --examples` pass (one warning: `TOKEN` is loaded but not enforced).
- Server starts, launches Chrome/Edge, serves `public/` and `GET /healthz` (`{"ok":true,"tabs":N}`).
- Socket.IO flow over Engine.IO polling: `join`, `tab:create`, `nav`, `tab:watch`,
  `input:click|scroll|zoom|type|key`, `stream:prefs`, `shot:refresh`, `tab:close`.
- CDP screencast: first frame after stream start, quality switch restarts the stream,
  pause stops frames, resume after unfreeze produces frames again.
- Tab freeze/unfreeze through `Page.setWebLifecycleState` plus media pause.
- Lazy browser relaunch: if the browser process dies, the next `tab:create` launches a
  new instance and clears stale tabs.
- `examples/monitor.rs` pings `/healthz` and prints latency.

## Modules

| Module | Status |
|--------|--------|
| `src/types.rs` | Complete: newtypes, events, quality profiles |
| `src/error.rs` | Complete: typed errors |
| `src/config.rs` | Complete: env parsing and validation |
| `src/presence.rs` | Complete: viewers, prefs, aggregation |
| `src/browser.rs` | Complete: launch, tabs, navigate, screenshot, freeze, relaunch |
| `src/input.rs` | Complete: CDP mouse wheel + key events, `Input.insertText` |
| `src/streamer.rs` | Complete: listener registration, frame ack, per-tab restart gate |
| `src/sockets.rs` | Complete: all 16 handlers, tab broadcasts |
| `src/main.rs` | Complete: bootstrap, health, frame pump, graceful shutdown |

## chromiumoxide 0.9 notes

- `page.mouse` / `page.keyboard` do not exist. Mouse wheel uses
  `Input.dispatchMouseEvent`, typing uses `Input.insertText`, keys use
  `Input.dispatchKeyEvent` with `chromiumoxide::keys`.
- Event types are named `Event*`: `EventScreencastFrame`, not `ScreencastFrameEvent`.
- `Page.setWebLifecycleState` takes `SetWebLifecycleStateState`, not `WebLifecycleState`.
- `Page::close` consumes the page; `CloseParams` via `Page::execute` is used instead.
- Chrome 132+ removed the old headless mode; the port uses `.new_headless_mode()`.
- Chrome rejects a relative `--user-data-dir`; `PROFILE_DIR` is resolved to an absolute path.

## Runtime quirks

- Headless Chrome emits no screencast frame for a static tab. The streamer foregrounds
  the tab and briefly toggles `body.style.opacity` between `1` and `0.999` after each
  start to force a paint. The style is restored.
- `everyNthFrame` (6 for low, 4 for med, 2 for high) means a static page at low quality
  produces very few frames by design. Frames flow when the page repaints.
- `Page.setWebLifecycleState` freezes the tab while nobody watches. Unfreezing is
  best-effort: if Chrome refuses, the stream is still restarted and the next repaint
  resumes frames.

## Not implemented

- `TOKEN` authentication. Config loads it, but neither HTTP nor the Socket.IO handshake
  enforces it. Run on a trusted LAN until this is added.
- Popup blocking and per-page crash listeners (the Node version has both).

## Run

```bash
cargo run --release
```

Chrome or Edge is detected automatically. Override with `CHROME=/path/to/browser`.

## Documentation

- [Architecture](ARCHITECTURE.md) - system design
- [Roadmap](RUST_ROADMAP.md) - completed phases and discovery notes
- [chromiumoxide 0.9 discovery](CHROMIUMOXIDE_09_DISCOVERY.md) - API mapping
- [Blocked details](RUST_BLOCKED.md) - historical 0.7 limitations
