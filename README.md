# remote-browser

Shared remote browser: one Chromium on desktop, multiple phones control it via LAN.

## Quick Start

**Node.js:**
```bash
npm install
npm start
```

**Rust:**
```bash
cargo run --release
```

The Rust version uses an installed Chrome or Edge (auto-detected). Set
`CHROME=/path/to/browser` to override. On Windows, double-click `run.bat`.

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
- **[Rust Port](docs/RUST_STATUS.md)** - Status, API notes, known gaps
- **[Rust Roadmap](docs/RUST_ROADMAP.md)** - Completed migration phases

## Development

```bash
npm run dev              # Node watch mode
node scripts/monitor.js  # Node latency test
cargo run --example monitor  # Rust latency test
cargo check              # Rust compile check
```

## Troubleshooting

**Chromium not found (Rust):** install Chrome/Edge, or set `CHROME` to its executable.
The Node version needs `npx playwright install chromium`.

**Black screen:** Set `NO_GPU=true`  
**High CPU:** Lower quality to 480p  
**Input lag:** Check `VIEWPORT` matches display

## License

MIT
