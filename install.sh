#!/usr/bin/env bash
set -euo pipefail

AGENT_DECK_HOME="${AGENT_DECK_HOME:-$HOME/.agent-deck}"
PORT="${AGENT_DECK_PORT:-7474}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

print_step() { echo "  → $1"; }
print_done() { echo "  ✓ $1"; }

echo ""
echo "  ╔══════════════════════════════════════╗"
echo "  ║         agent-deck installer         ║"
echo "  ╚══════════════════════════════════════╝"
echo ""

# ── 1. OS check ────────────────────────────────────────────────────────────────
OS="$(uname)"
if [[ "$OS" == "Darwin" ]]; then
  print_done "macOS detected"
elif [[ "$OS" == "Linux" ]]; then
  print_done "Linux detected"
else
  echo "Error: agent-deck supports macOS and Linux only (detected: $OS)."
  exit 1
fi

# ── 2. Detect install mode ─────────────────────────────────────────────────────
SOURCE_MODE=false
if [ -f "$SCRIPT_DIR/Cargo.toml" ]; then
  SOURCE_MODE=true
  print_done "Source clone detected — will build from source"
else
  print_done "Tarball mode — using bundled binary"
fi

# ── 3. Homebrew (macOS only) ──────────────────────────────────────────────────
if [[ "$OS" == "Darwin" ]]; then
  print_step "Checking Homebrew..."
  if ! command -v brew &>/dev/null; then
    print_step "Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    if [ -f "/opt/homebrew/bin/brew" ]; then
      eval "$(/opt/homebrew/bin/brew shellenv)"
    else
      eval "$(/usr/local/bin/brew shellenv)"
    fi
  fi
  print_done "Homebrew ready"
fi

# ── 4. Rust toolchain (source mode only) ──────────────────────────────────────
if [ "$SOURCE_MODE" = true ]; then
  print_step "Checking Rust toolchain..."
  if ! command -v cargo &>/dev/null; then
    print_step "Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
  fi
  print_done "Rust toolchain ready"
fi

# ── 5. Node.js via nvm (source mode only) ─────────────────────────────────────
if [ "$SOURCE_MODE" = true ]; then
  print_step "Checking nvm..."
  if [ ! -f "$HOME/.nvm/nvm.sh" ]; then
    print_step "Installing nvm..."
    curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
  fi
  # shellcheck source=/dev/null
  source "$HOME/.nvm/nvm.sh"
  print_done "nvm ready ($(nvm --version))"

  NODE_VERSION="$(cat "$SCRIPT_DIR/web/.nvmrc")"
  print_step "Installing Node.js $NODE_VERSION via nvm..."
  nvm install "$NODE_VERSION"
  nvm use "$NODE_VERSION"
  print_done "Node.js ready ($(node --version))"
fi

# ── 6. Tailscale ───────────────────────────────────────────────────────────────
print_step "Checking Tailscale..."
if ! command -v tailscale &>/dev/null; then
  if [[ "$OS" == "Darwin" ]]; then
    print_step "Installing Tailscale via Homebrew..."
    brew install --cask tailscale
    brew install tailscale
    echo ""
    echo "  ⚠ Tailscale installed. Open the app once to approve the network"
    echo "  extension, then it will run headlessly in the background:"
    echo "    open /Applications/Tailscale.app"
    echo ""
  elif [[ "$OS" == "Linux" ]]; then
    print_step "Installing Tailscale..."
    curl -fsSL https://tailscale.com/install.sh | sh
    print_step "Starting Tailscale service..."
    sudo systemctl enable --now tailscaled
    print_done "Tailscale service started"
  fi
else
  print_done "Tailscale ready"
fi

# ── 7. Build (source mode only) ───────────────────────────────────────────────
if [ "$SOURCE_MODE" = true ]; then
  print_step "Building agent-deck server..."
  (cd "$SCRIPT_DIR" && cargo build --release)
  print_done "Server built"

  print_step "Building web application..."
  # Source nvm if present so the correct Node version is available.
  # nvm is a shell function — it cannot be detected with command -v.
  if [ -f "$HOME/.nvm/nvm.sh" ]; then
    # shellcheck source=/dev/null
    source "$HOME/.nvm/nvm.sh"
    nvm use "$(cat "$SCRIPT_DIR/web/.nvmrc")"
  fi
  cd "$SCRIPT_DIR/web"
  npm ci
  npm run build
  cd "$SCRIPT_DIR"
  print_done "Web application built"
fi

# ── 7b. Migrate old layout to .process/ if needed ─────────────────────────────
if [ -d "$AGENT_DECK_HOME/bin" ] && [ ! -d "$AGENT_DECK_HOME/.process/bin" ]; then
  print_step "Migrating binary to .process/bin..."
  mkdir -p "$AGENT_DECK_HOME/.process/bin"
  mv "$AGENT_DECK_HOME/bin/agent-deck" "$AGENT_DECK_HOME/.process/bin/agent-deck" 2>/dev/null || true
  rmdir "$AGENT_DECK_HOME/bin" 2>/dev/null || true
  print_done "Binary migrated"
fi

if [ -d "$AGENT_DECK_HOME/public" ] && [ ! -d "$AGENT_DECK_HOME/.process/public" ]; then
  print_step "Migrating public assets to .process/public..."
  mkdir -p "$AGENT_DECK_HOME/.process"
  mv "$AGENT_DECK_HOME/public" "$AGENT_DECK_HOME/.process/public"
  print_done "Assets migrated"
fi

if [ -f "$AGENT_DECK_HOME/.database/agent-deck.db" ] && [ ! -f "$AGENT_DECK_HOME/.process/.database/agent-deck.db" ]; then
  print_step "Migrating database to .process/.database/..."
  mkdir -p "$AGENT_DECK_HOME/.process/.database"
  cp "$AGENT_DECK_HOME/.database/agent-deck.db" "$AGENT_DECK_HOME/.process/.database/agent-deck.db"
  print_done "Database migrated"
fi

if [ -f "$AGENT_DECK_HOME/server.log" ] && [ ! -f "$AGENT_DECK_HOME/.process/server.log" ]; then
  mkdir -p "$AGENT_DECK_HOME/.process"
  mv "$AGENT_DECK_HOME/server.log" "$AGENT_DECK_HOME/.process/server.log" 2>/dev/null || true
fi

if [ -f "$AGENT_DECK_HOME/agent-deck.pid" ]; then
  rm -f "$AGENT_DECK_HOME/agent-deck.pid"
fi

if [ -f "$AGENT_DECK_HOME/run.sh" ]; then
  rm -f "$AGENT_DECK_HOME/run.sh"
fi

if [ -f "$AGENT_DECK_HOME/agent-deck-cli.sh" ]; then
  rm -f "$AGENT_DECK_HOME/agent-deck-cli.sh"
fi

# ── 8. Deploy ──────────────────────────────────────────────────────────────────
print_step "Deploying to $AGENT_DECK_HOME..."

mkdir -p "$AGENT_DECK_HOME/.process/bin"

if [ "$SOURCE_MODE" = true ]; then
  cp "$SCRIPT_DIR/target/release/server" "$AGENT_DECK_HOME/.process/bin/agent-deck"
  rm -rf "$AGENT_DECK_HOME/.process/public"
  cp -r "$SCRIPT_DIR/server/public" "$AGENT_DECK_HOME/.process/public"
else
  cp "$SCRIPT_DIR/server" "$AGENT_DECK_HOME/.process/bin/agent-deck"
  rm -rf "$AGENT_DECK_HOME/.process/public"
  cp -r "$SCRIPT_DIR/public" "$AGENT_DECK_HOME/.process/public"
fi

chmod +x "$AGENT_DECK_HOME/.process/bin/agent-deck"

cp "$SCRIPT_DIR/scripts/run.sh" "$AGENT_DECK_HOME/.process/run.sh"
chmod +x "$AGENT_DECK_HOME/.process/run.sh"

print_done "Deployed"

# ── 9. Install CLI ─────────────────────────────────────────────────────────────
print_step "Installing agent-deck CLI..."

cp "$SCRIPT_DIR/scripts/agent-deck-cli.sh" "$AGENT_DECK_HOME/.process/agent-deck-cli.sh"

SOURCE_LINE="source \"$AGENT_DECK_HOME/.process/agent-deck-cli.sh\""

if [[ "$OS" == "Darwin" ]]; then
  SHELL_PROFILES=("$HOME/.zshrc" "$HOME/.bash_profile")
else
  SHELL_PROFILES=("$HOME/.zshrc" "$HOME/.bashrc")
fi

for PROFILE in "${SHELL_PROFILES[@]}"; do
  touch "$PROFILE"
  if ! grep -qF "$SOURCE_LINE" "$PROFILE" 2>/dev/null; then
    echo "" >> "$PROFILE"
    echo "# agent-deck CLI" >> "$PROFILE"
    echo "$SOURCE_LINE" >> "$PROFILE"
    print_done "CLI added to $PROFILE"
  else
    print_done "CLI already in $PROFILE (skipped)"
  fi
done
unset SHELL_PROFILES

# ── 10. Stop any running instance ─────────────────────────────────────────────
PROCESS_PID_FILE="$AGENT_DECK_HOME/.process/agent-deck.pid"
if [ -f "$PROCESS_PID_FILE" ]; then
  OLD_PID=$(cat "$PROCESS_PID_FILE")
  if kill -0 "$OLD_PID" 2>/dev/null; then
    print_step "Stopping running agent-deck (PID $OLD_PID)..."
    kill "$OLD_PID"
    # Wait up to 10 seconds for graceful shutdown so MCP children have time to exit
    waited=0
    while kill -0 "$OLD_PID" 2>/dev/null && [ "$waited" -lt 10 ]; do
      sleep 1
      waited=$((waited + 1))
    done
    if kill -0 "$OLD_PID" 2>/dev/null; then
      kill -9 "$OLD_PID" 2>/dev/null || true
    fi
    print_done "Stopped"
  fi
  rm -f "$PROCESS_PID_FILE"
fi

# ── 11. Start service ──────────────────────────────────────────────────────────
print_step "Starting agent-deck..."
bash "$AGENT_DECK_HOME/.process/run.sh"

# ── 12. Summary ───────────────────────────────────────────────────────────────
echo ""
echo "  ══════════════════════════════════════════"
echo "  Installation complete!"
echo ""
echo "  Open agent-deck at: http://localhost:$PORT"

if command -v tailscale &>/dev/null; then
  TS_HOST=$(tailscale status --json 2>/dev/null | grep -o '"DNSName":"[^"]*"' | head -1 | cut -d'"' -f4 | sed 's/\.$//' || true)
  if [ -n "${TS_HOST:-}" ]; then
    echo "  Also available at:  http://$TS_HOST:$PORT"
  fi
fi

echo ""
echo "  To activate the agent-deck command in this terminal:"
if [[ "$OS" == "Darwin" ]]; then
  echo "    source ~/.zshrc"
else
  echo "    source ~/.bashrc  (or source ~/.zshrc if you use zsh)"
fi
echo "  (or open a new terminal)"
echo ""
echo "  Commands: agent-deck start | stop | status | logs | open"
echo "  ══════════════════════════════════════════"
echo ""
