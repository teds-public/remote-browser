# remote-browser

Shared remote browser: one Chromium on desktop, multiple phones control it via LAN.

## Quick Start

**Node.js (stable):**
```bash
npm install
npm start
```

**Rust (in development - 85%):**
```bash
npx playwright install chromium
cargo run --release
```

Open `http://localhost:5001` on your phone.

## Features

- One shared Chromium, multiple clients control it
- Live stream at 480p/720p/1080p (adjustable)
- Click, scroll, zoom, keyboard input
- Tab management: create, close, switch
- Shared cookies/logins persist
- Auto-pause when no viewers (saves CPU)
- Crash recovery

## Config

Copy `.env.example` to `.env`:

```bash
PORT=5001
VIEWPORT=1920x1080
MAX_TABS=8
HEADLESS=true
NO_GPU=false
TOKEN=
```

## Stack

**Node.js:** Express, Socket.IO, Playwright, CDP screencast  
**Rust:** Tokio, Axum, socketioxide, chromiumoxide

## Documentation

- **[Architecture](docs/ARCHITECTURE.md)** - System design, protocols
- **[Rust Status](docs/RUST_STATUS.md)** - Rust port (85% complete)

## Development

```bash
npm run dev              # Watch mode
node scripts/monitor.js  # Latency test
cargo check              # Rust compile check
```

## Troubleshooting

**Chromium not found:**
```bash
npx playwright install chromium
```

**Black screen:** Set `NO_GPU=true`  
**High CPU:** Lower quality to 480p  
**Input lag:** Check `VIEWPORT` matches display

## License

MIT
