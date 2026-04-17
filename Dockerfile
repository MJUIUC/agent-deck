# =============================================================================
# agent-deck Dockerfile
#
# Multi-stage build:
#   Stage 1 (node-builder)  — builds the React/Vite frontend
#   Stage 2 (rust-builder)  — compiles the Rust server binary
#   Stage 3 (runtime)       — minimal Debian image with just the binary + assets
#
# Build:
#   docker build -t agent-deck .
#
# Run (dev instance on port 7475 with separate data dir):
#   docker run -d \
#     --name agent-deck-dev \
#     -p 7475:7475 \
#     -e PORT=7475 \
#     -v ~/.agent-deck-dev:/data \
#     agent-deck
#
# Run (production-style on port 7474):
#   docker run -d \
#     --name agent-deck \
#     -p 7474:7474 \
#     -v ~/.agent-deck:/data \
#     agent-deck
#
# Environment variables:
#   PORT                      — port to listen on (default: 7474)
#   AGENT_DECK_DATA_DIR       — absolute path inside the container (default: /data)
#   FCM_SERVICE_ACCOUNT_JSON  — optional path to FCM service account JSON
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: Build the frontend
# -----------------------------------------------------------------------------
FROM node:22-slim AS node-builder

WORKDIR /app/web

# Copy package files first for layer caching
COPY web/package.json web/package-lock.json ./
RUN npm ci

# Copy the rest of the frontend source
COPY web/ ./

# Build the Vite PWA
RUN npm run build

# -----------------------------------------------------------------------------
# Stage 2: Build the Rust binary
# -----------------------------------------------------------------------------
FROM rust:1.87-slim AS rust-builder

# Install build dependencies for SQLite (sqlx uses the bundled feature but
# still needs cc + the linker)
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy workspace manifests first so cargo can cache dependencies
COPY Cargo.toml Cargo.lock ./
COPY server/Cargo.toml ./server/

# Create a dummy main so `cargo build` can resolve and cache all deps
# without rebuilding them every time source changes
RUN mkdir -p server/src && echo 'fn main() {}' > server/src/main.rs
RUN cargo build --release --bin server 2>/dev/null || true

# Now copy the real source and build properly
COPY server/src ./server/src

# Touch main.rs so cargo knows it changed
RUN touch server/src/main.rs

RUN cargo build --release --bin server

# -----------------------------------------------------------------------------
# Stage 3: Runtime image
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# Install ca-certificates (needed for HTTPS calls to AI providers / MCP)
# and curl (healthcheck). Keep it minimal.
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -m -u 1000 -s /bin/sh agentdeck

WORKDIR /app

# Copy the compiled binary
COPY --from=rust-builder /app/target/release/server ./agent-deck

# Copy the built frontend assets into /app/public
# The Rust server serves these as static files (PUBLIC_DIR env var)
COPY --from=node-builder /app/web/dist ./public

# Copy the server's own static assets (manifest, icons, SW)
# These are the files baked into server/public/ at build time
COPY server/public ./public

# Data directory — mount a volume here to persist DB, MCP configs, etc.
RUN mkdir -p /data && chown agentdeck:agentdeck /data

# Switch to non-root
USER agentdeck

# Defaults — override with -e at runtime
ENV PORT=7474
ENV AGENT_DECK_DATA_DIR=/data
ENV PUBLIC_DIR=/app/public

EXPOSE 7474

# Healthcheck: ping the health endpoint
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:${PORT}/api/health || exit 1

ENTRYPOINT ["./agent-deck"]
