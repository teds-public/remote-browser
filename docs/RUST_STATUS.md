# Rust Conversion Status

Rust port: **85% complete** — core modules finished, Socket.IO integration needs completion.

## Progress

✅ **Complete:**
- Type system (`src/types.rs`) - TabId, Viewport, StreamQuality newtypes
- Config (`src/config.rs`) - Validated env parsing with `envy` + `dotenvy`
- Errors (`src/error.rs`) - `thiserror` typed errors in English
- Browser (`src/browser.rs`) - chromiumoxide wrapper, tab lifecycle, CDP freeze
- Input (`src/input.rs`) - Coordinate clamping, key allow-list, tracing spans
- Presence (`src/presence.rs`) - Socket→viewer mapping, name sanitization
- Streamer (`src/streamer.rs`) - CDP screencast, quality presets, frame ack

🚧 **In Progress:**
- Socket.IO handlers (`src/sockets.rs`) - Need to update 15 remaining handlers
- Main server (`src/main.rs`) - Binary frame emit with socketioxide 0.18

## Technical Blocker

**Issue:** socketioxide 0.18 uses `socket.extensions` for state, not extractors.

**Solution:** Update all handlers to use `get_state()` helper:
```rust
fn get_state(socket: &SocketRef) -> Option<Arc<AppState>> {
    socket.extensions.get::<Arc<AppState>>().cloned()
}

pub async fn on_join(socket: SocketRef, Data(data): Data<JoinData>) {
    let state = match get_state(&socket) {
        Some(s) => s,
        None => return,
    };
    // ... handler logic
}
```

## To Complete

1. Apply `get_state()` pattern to 15 remaining handlers in `src/sockets.rs`
2. Verify binary frame emit: `io.to(room).emit("frame", jpeg_bytes)`
3. Test: `playwright install chromium`, then `cargo run`

## Running

**Node.js (current):**
```bash
npm install && npm start
```

**Rust (when complete):**
```bash
npx playwright install chromium
cargo run --release
```

## Architecture Improvements

**From Node.js to Rust:**
- Callback acks → `Result<T, E>` return types
- Event emitters → `mpsc`/`broadcast` channels
- Magic strings → `TabId`, `RoomId` newtypes
- `console.log` → Structured tracing with spans
- Runtime errors → Compile-time type safety

**Example - Typed Errors:**
```rust
// Before (Node.js)
throw new Error('Tab not found');

// After (Rust)
return Err(BrowserError::TabNotFound(id));
```

**Example - Latency Tracing:**
```rust
#[instrument(skip(page), fields(latency_ms))]
pub async fn click(page: &Page, x: f64, y: f64) -> Result<(), InputError> {
    let start = Instant::now();
    // ... click logic
    tracing::Span::current().record("latency_ms", start.elapsed().as_millis());
    Ok(())
}
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| axum | HTTP server, routing |
| tokio | Async runtime |
| socketioxide | Socket.IO server (v0.18) |
| chromiumoxide | Chromium CDP automation |
| thiserror | Typed error definitions |
| tracing | Structured logging |
| serde/serde_json | JSON serialization |
| base64 | JPEG encoding |

## File Structure

```
src/
├── main.rs          Server entry, graceful shutdown
├── config.rs        Env parsing, validation
├── types.rs         TabId, Viewport, Quality, Frame
├── error.rs         BrowserError, InputError, SocketError
├── browser.rs       Chromium lifecycle, tab management
├── input.rs         Mouse/keyboard input
├── presence.rs      Client tracking, viewer aggregation
├── streamer.rs      CDP screencast, frame ack loop
└── sockets.rs       Socket.IO event handlers (WIP)
```

## Next Steps

1. **Finish handlers** - Copy `get_state()` pattern × 15
2. **Test build** - `cargo check`, fix any errors
3. **Binary emit** - Verify socketioxide 0.18 API
4. **Manual test** - Connect client, verify stream

See [Architecture](ARCHITECTURE.md) for protocol details.

## Why Rust?

- **Memory safety** - No segfaults, data races caught at compile time
- **Performance** - Lower CPU/RAM vs Node.js V8 heap
- **Typed CDP** - chromiumoxide provides typed CDP commands
- **Explicit errors** - `Result<T, E>` vs uncaught exceptions
- **Fearless concurrency** - Tokio + Send/Sync guarantees

The Node.js version remains fully functional while Rust port is completed.
