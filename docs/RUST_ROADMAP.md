# Remote Browser - Rust Version Roadmap

## Status: complete

All phases are implemented and verified on chromiumoxide 0.9, socketioxide 0.18,
Chrome 157 and Windows 10. See [RUST_STATUS.md](RUST_STATUS.md) for evidence and
known gaps.

### Phase 1: Input Module — done

- [x] `page.click(Point)` replaces `page.mouse.click`
- [x] `Input.insertText` for text (no public `page.keyboard` in 0.9)
- [x] `Input.dispatchKeyEvent` for the allow-listed keys
- [x] `Input.dispatchMouseEvent` (`mouseWheel`) for scroll and Ctrl+wheel zoom
- [x] Coordinate clamping unchanged, now rejects non-finite values
- [x] UTF-8-safe text truncation at 500 chars

### Phase 2: CDP Type Imports — done

- [x] `SetWebLifecycleStateState` replaces `WebLifecycleState`
- [x] `EventScreencastFrame` replaces `ScreencastFrameEvent`
- [x] `StartScreencastFormat` replaces `CaptureScreenshotFormat` for screencast
- [x] No custom CDP wrappers needed; the generated types are public

### Phase 3: Browser Module — done

- [x] No `page.mouse` / `page.keyboard` references remain
- [x] Tabs close through `Page.execute(CloseParams)` because `Page::close` consumes `self`
- [x] Freeze/unfreeze through `Page.setWebLifecycleState` plus media pause
- [x] Browser relaunch on next `tab:create` after the process dies
- [x] Persistent profile path made absolute (Chrome rejects relative `--user-data-dir`)
- [x] New headless mode (Chrome 132+)

### Phase 4: Streamer — done

- [x] `EventScreencastFrame` listener registered before `startScreencast`
- [x] Frame ack before emit
- [x] Base64 decode through `Binary::as_ref::<str>()`
- [x] Quality switching restarts the stream behind a per-tab async gate, so
      `startScreencast` never races the previous `stopScreencast`
- [x] Stream generations prevent an old task from removing a newer stream entry
- [x] First-paint nudge for static tabs (backgrounded tabs emit no frame)

### Phase 5: Integration Testing — done

- [x] `cargo build --release` succeeds
- [x] Server starts, `healthz` and static UI respond
- [x] Socket.IO join / tab create / navigate / watch / screenshot
- [x] Click, scroll, zoom, type and key accepted without server errors
- [x] Tab freeze, unfreeze and stream pause/resume
- [x] Stream quality switching (low/med/high)
- [x] Multiple tabs; tab close and room cleanup

### Phase 6: Documentation — done

- [x] `RUST_STATUS.md` rewritten
- [x] `README.md` updated for both versions
- [x] Deployment notes: system Chrome/Edge detection, `CHROME` override
- [x] `examples/monitor.rs` replaces the Node soak pinger

## Historical notes

The original blocker was chromiumoxide 0.7, which exposed no input or screencast
event APIs. 0.9 exposes the generated CDP types and page-level click/move_mouse,
which unblocked the port. [`RUST_BLOCKED.md`](RUST_BLOCKED.md) is kept for history.
