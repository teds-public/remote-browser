# Rust Conversion - Alternative Approaches

## Problem Summary

chromiumoxide 0.7 doesn't expose:
- `page.mouse` / `page.keyboard` for input
- CDP event types (`ScreencastFrameEvent`)
- CDP command types (`WebLifecycleState`)

## Alternative Solutions

### Option 1: Upgrade to chromiumoxide 0.9+

**Status:** Available (0.9.1 on crates.io)

```toml
[dependencies]
chromiumoxide = "0.9"
```

**Risks:**
- Breaking API changes possible
- Need to verify input/CDP APIs exposed
- May have different async patterns

**Next step:** Test upgrade

### Option 2: Use chromiumoxide-cdp directly

Access CDP protocol layer without high-level Page wrapper:

```rust
// Direct CDP commands
let session = page.inner(); // Get CDP session
session.execute(runtime::Evaluate { ... }).await?;

// Direct CDP events
let mut stream = session.event_listener::<page::ScreencastFrameEvent>().await?;
```

**Pros:**
- Full CDP access
- No API limitations

**Cons:**
- More verbose
- Lose Page abstractions
- Need to manage CDP session lifecycle

### Option 3: Fork chromiumoxide

Create fork with exposed APIs:

```rust
// In our fork:
pub struct Page {
    pub mouse: Mouse,  // Make public
    pub keyboard: Keyboard,  // Make public
    // ...
}
```

**Pros:**
- Minimal code changes
- Keep high-level API

**Cons:**
- Maintenance burden
- Need to sync upstream
- Adds build complexity

### Option 4: Switch to headless_chrome

Use `headless_chrome` crate (800+ stars, active):

```rust
use headless_chrome::{Browser, protocol::cdp::Page};

let browser = Browser::default()?;
let tab = browser.new_tab()?;
tab.navigate_to("https://example.com")?;
tab.wait_until_navigated()?;

// Input
tab.click(x, y)?;
tab.type_str(text)?;

// Screenshot
let png = tab.capture_screenshot(...)?;
```

**Pros:**
- Mature, well-maintained
- Public input APIs
- Good CDP access

**Cons:**
- Different API, need rewrite
- Synchronous by default (has async fork)
- Heavier than chromiumoxide

### Option 5: Hybrid Node.js + Rust

Keep CDP operations in Node.js, use Rust for server:

```
┌─────────────────┐
│  Rust Server    │
│  - Axum         │──┐
│  - socketioxide │  │
│  - Presence     │  │
└─────────────────┘  │
                     │ IPC/HTTP
                     │
┌─────────────────┐  │
│  Node.js CDP    │◄─┘
│  - Playwright   │
│  - Screencaster │
└─────────────────┘
```

**Pros:**
- Leverage existing Node.js CDP code
- Rust handles networking
- Incremental migration

**Cons:**
- Complex architecture
- IPC overhead
- Two runtimes to manage

## Recommendation Priority

### 1. Test chromiumoxide 0.9+ (1 hour effort)

**Quick test:**
```bash
# Update Cargo.toml
chromiumoxide = "0.9"

# Check if APIs exist
cargo doc --open
# Look for: Page::mouse, Page::keyboard
```

If APIs public → Use 0.9  
If still private → Try option 2 or 4

### 2. Try chromiumoxide-cdp direct (2-3 hours)

Rewrite `browser.rs` to use CDP session directly.

### 3. Evaluate headless_chrome (1 day)

Rewrite browser module with different API.

### 4. Consider hybrid (2-3 days)

Only if above fail and benefits justify complexity.

## Next Action

Let's test chromiumoxide 0.9 upgrade now.
