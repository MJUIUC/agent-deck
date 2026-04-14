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
if [[ "$(uname)" != "Darwin" ]]; then
  echo "Error: agent-deck currently only supports macOS."
  exit 1
fi
print_done "macOS detected"

# ── 2. Detect install mode ─────────────────────────────────────────────────────
SOURCE_MODE=false
if [ -f "$SCRIPT_DIR/Cargo.toml" ]; then
  SOURCE_MODE=true
  print_done "Source clone detected — will build from source"
else
  print_done "Tarball mode — using bundled binary"
fi

# ── 3. Homebrew ────────────────────────────────────────────────────────────────
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

# ── 5. Node.js (source mode only) ─────────────────────────────────────────────
if [ "$SOURCE_MODE" = true ]; then
  print_step "Checking Node.js..."
  if ! command -v node &>/dev/null || ! node -e "process.exit(parseInt(process.version.slice(1)) >= 20 ? 0 : 1)" 2>/dev/null; then
    print_step "Installing Node.js via Homebrew..."
    brew install node
  fi
  print_done "Node.js ready ($(node --version))"
fi

# ── 6. Tailscale ───────────────────────────────────────────────────────────────
print_step "Checking Tailscale..."
if ! command -v tailscale &>/dev/null; then
  print_step "Installing Tailscale via Homebrew..."
  brew install tailscale
  echo ""
  echo "  ⚠ Tailscale installed. To start it, run:"
  echo "    brew services start tailscale"
  echo "  Then open System Settings → Tailscale and sign in."
  echo ""
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
    nvm use 24 2>/dev/null || nvm use --lts 2>/dev/null || true
  fi
  cd "$SCRIPT_DIR/web"
  npm ci
  npm run build
  cd "$SCRIPT_DIR"
  print_done "Web application built"
fi

# ── 8. Deploy ──────────────────────────────────────────────────────────────────
print_step "Deploying to $AGENT_DECK_HOME..."

mkdir -p "$AGENT_DECK_HOME/bin"

if [ "$SOURCE_MODE" = true ]; then
  cp "$SCRIPT_DIR/target/release/server" "$AGENT_DECK_HOME/bin/agent-deck"
  rm -rf "$AGENT_DECK_HOME/public"
  cp -r "$SCRIPT_DIR/web/dist" "$AGENT_DECK_HOME/public"
else
  cp "$SCRIPT_DIR/server" "$AGENT_DECK_HOME/bin/agent-deck"
  rm -rf "$AGENT_DECK_HOME/public"
  cp -r "$SCRIPT_DIR/public" "$AGENT_DECK_HOME/public"
fi

chmod +x "$AGENT_DECK_HOME/bin/agent-deck"

cp "$SCRIPT_DIR/scripts/run.sh" "$AGENT_DECK_HOME/run.sh"
chmod +x "$AGENT_DECK_HOME/run.sh"

print_done "Deployed"

# ── 9. Install CLI ─────────────────────────────────────────────────────────────
print_step "Installing agent-deck CLI..."

cp "$SCRIPT_DIR/scripts/agent-deck-cli.sh" "$AGENT_DECK_HOME/agent-deck-cli.sh"

SOURCE_LINE="source \"$AGENT_DECK_HOME/agent-deck-cli.sh\""

for PROFILE in "$HOME/.zshrc" "$HOME/.bash_profile"; do
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

# ── 10. Start service ──────────────────────────────────────────────────────────
print_step "Starting agent-deck..."
bash "$AGENT_DECK_HOME/run.sh"

# ── 11. Summary ───────────────────────────────────────────────────────────────
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
echo "    source ~/.zshrc"
echo "  (or open a new terminal)"
echo ""
echo "  Commands: agent-deck start | stop | status | logs | open"
echo "  ══════════════════════════════════════════"
echo ""
