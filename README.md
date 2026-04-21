# agent-deck

A self-hosted, highly configurable personal AI agent platform. Runs on a Mac mini, accessible privately over Tailscale, with a browser UI and Android mobile app.

---

## What it is

Agent-Deck lets you run your own AI assistant on hardware you own. Everything stays local — conversations, memories, API keys — with no third-party platform in the loop. You configure it once and use it from any device on your Tailscale network.

**Core features:**
- Multiple agent personas with distinct personalities and system prompts
- Persistent memory per persona (cross-thread, searchable)
- Scheduled routines (cron-based prompts that run automatically)
- MCP server integration
- Android mobile app with push notifications
- GitHub Copilot support via `copilot-api` proxy

---

## Requirements

- **Rust** (latest stable via `rustup`)
- **Node.js** 24+ (also provides `npm` — required to build and run `copilot-api`)
- **SQLx CLI**: `cargo install sqlx-cli --features sqlite`
- **Android Studio** + JDK 17 + Android SDK (only for mobile app)

---

## Quick start

### 1. Clone and set up the server

```bash
git clone <repo-url> agent-deck
cd agent-deck

# Set up the server environment
cd server
cp .env.example .env
# Edit .env if you want to change PORT or DATABASE_URL

# Create the database and run migrations
sqlx database create
sqlx migrate run

# Start the server
cargo run
```

The server starts on port **7474**. On first run it:
1. Creates the SQLite database at `./data/agent-deck.db`
2. Generates a random 64-character auth token and prints it to the terminal
3. Serves the placeholder UI at `http://localhost:7474`

The auth token is required to access the API from any non-localhost device. Copy it from the terminal output.

---

### 2. Build the web UI

In a separate terminal:

```bash
cd web
npm install
npm run dev       # dev server at http://localhost:5173, proxies /api to :7474
# or
npm run build     # production build → server/public/ (served by Rust server)
```

---

### 3. Set up the mobile app (optional)

```bash
cd mobile/BotRelayApp
npm install
npx react-native run-android
```

Pair the mobile app with your server by scanning the QR code from the mobile pairing screen in Settings.

---

### 4. Set up copilot-api (optional — only needed for GitHub Copilot)

```bash
git submodule update --init --recursive
```

The Rust server manages the `copilot-api` process automatically — no manual build step required. On first startup it will:

1. Run `npm install` inside `vendor/copilot-api/` to install dependencies
2. Run `./node_modules/.bin/tsdown` to bundle the TypeScript source into `dist/main.js`
3. Spawn `node dist/main.js start` and keep it supervised

Configure a Copilot provider in Settings to trigger the GitHub device auth flow.

> **Note:** Node.js 24+ must be on your PATH (or installed via nvm). No other runtime is required.

---

## Authentication

**Localhost:** The browser on the same machine as the server never needs a token. Auth is bypassed for `127.0.0.1` and `::1`.

**Remote devices (Tailscale):** On first visit from a remote browser, a token entry screen is shown. Paste the token printed to the server terminal. Once entered, it's stored as an `httpOnly` cookie — you won't need to re-enter it.

**Mobile app:** Pairing is done via QR code in Settings → Mobile Pairing. The app stores the token in MMKV and sends it as a `Bearer` header on every request.

---

## Project structure

```
agent-deck/
├── server/          # Rust server (Axum + SQLite)
│   ├── src/
│   │   ├── main.rs
│   │   ├── config.rs
│   │   ├── db/
│   │   │   └── migrations/
│   │   ├── routes/
│   │   ├── services/
│   │   ├── models/
│   │   └── error.rs
│   ├── public/      # React SPA build output (gitignored except placeholder)
│   └── .env.example
├── web/             # React SPA (Vite + TypeScript + Tailwind)
├── mobile/          # React Native Android app
│   └── BotRelayApp/
├── vendor/
│   └── copilot-api/ # Git submodule
├── mockups/         # Static HTML mockups (visual reference)
└── PLAN.md          # Full project specification
```

---

## API

All endpoints are under `/api`. See `PLAN.md` §6 for the full API contract.

Quick reference:

| Endpoint | Description |
|---|---|
| `GET /health` | Health check (public) |
| `GET /api/setup/status` | First-run setup status (public) |
| `POST /api/auth/token` | Exchange token for session cookie |
| `GET /api/providers` | List model providers |
| `GET /api/personas` | List agent personas |
| `GET /api/threads` | List chat threads |
| `POST /api/threads/:id/messages` | Send a message |
| `GET /api/threads/:id/stream` | SSE stream for LLM token streaming |
| `GET /api/events` | Global SSE event stream |

---

## Development

### Running tests

```bash
# All tests
cargo test

# With output
cargo test -- --nocapture
```

### SQLx offline mode

SQLx checks queries at compile time against a live database. If you're building without a running DB (e.g. CI):

```bash
# After any schema change, regenerate the query metadata:
cargo sqlx prepare

# Then commit the .sqlx/ directory.
# In CI, set:
SQLX_OFFLINE=true cargo build
```

### Environment variables

| Variable | Default | Description |
|---|---|---|
| `PORT` | `7474` | Server listen port |
| `DATABASE_URL` | `sqlite:./data/agent-deck.db` | SQLite database path |
| `RUST_LOG` | `info` | Log level |
| `FCM_SERVICE_ACCOUNT_JSON` | *(unset)* | Firebase service account for push notifications |
| `PUBLIC_DIR` | `./public` | Directory to serve the React SPA from |

---

## Phased implementation

The project is built in phases. See `PLAN.md` §10 for the full execution plan.

| Phase | Status | Description |
|---|---|---|
| 1 — Skeleton | ✅ Complete | Rust server, SQLite schema, React SPA shell, auth, all CRUD |
| 2 — First Chat | ✅ Complete | Agent run-loop, LLM streaming via SSE, chat UI |
| 3 — Config UI | ⏳ Pending | Setup wizard, provider/persona/thread management UI |
| 4 — Memory & Routines | ⏳ Pending | Persistent memory, cron scheduler |
| 5 — Mobile App | ⏳ Pending | React Native screens |
| 6 — Push Notifications | ⏳ Pending | FCM integration |
| 7 — MCP Depth | ⏳ Pending | Tool inspector, local process management |
| 8 — Polish | ⏳ Pending | Error handling, empty states, hardening |