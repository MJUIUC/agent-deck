#!/usr/bin/env bash
# =============================================================================
# agent-dev.sh — Build and run the agent-deck dev Docker instance
#
# Usage:
#   ./scripts/agent-dev.sh            # build + restart dev container
#   ./scripts/agent-dev.sh build      # build image only
#   ./scripts/agent-dev.sh start      # start existing container
#   ./scripts/agent-dev.sh stop       # stop dev container
#   ./scripts/agent-dev.sh logs       # tail container logs
#   ./scripts/agent-dev.sh status     # show container status
#   ./scripts/agent-dev.sh clean      # stop + remove container and image
#
# The dev instance runs on port 7475 with data isolated to ~/.agent-deck-dev/
# The live instance on port 7474 is never touched.
# =============================================================================

set -euo pipefail

CONTAINER_NAME="agent-deck-dev"
IMAGE_NAME="agent-deck-dev"
DEV_PORT=7475
DEV_DATA_DIR="$HOME/.agent-deck-dev"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# ── Colour helpers ────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()    { echo -e "${CYAN}[agent-dev]${NC} $*"; }
success() { echo -e "${GREEN}[agent-dev]${NC} $*"; }
warn()    { echo -e "${YELLOW}[agent-dev]${NC} $*"; }
error()   { echo -e "${RED}[agent-dev]${NC} $*" >&2; exit 1; }

# ── Guard: must be on the Mac mini ───────────────────────────────────────────
IDENTITY_FILE="$REPO_ROOT/.agent-machine"
if [[ ! -f "$IDENTITY_FILE" ]]; then
  error "No .agent-machine file found at $IDENTITY_FILE\nThis script may only run on the designated Mac mini.\nSee docs/AGENT_WORKFLOW.md §15 for setup instructions."
fi

MACHINE_ID="$(cat "$IDENTITY_FILE" | tr -d '[:space:]')"
if [[ "$MACHINE_ID" != "mac-mini-primary" ]]; then
  error ".agent-machine contains '$MACHINE_ID' — expected 'mac-mini-primary'.\nDev Docker builds are only permitted on the primary Mac mini."
fi

# ── Guard: Docker / Colima must be running ───────────────────────────────────
if ! docker info &>/dev/null; then
  warn "Docker is not responding. Attempting to start Colima…"
  colima start --foreground=false || error "Could not start Colima. Run 'colima start' manually."
  sleep 3
  docker info &>/dev/null || error "Docker still not responding after Colima start."
fi

# ── Subcommand dispatch ───────────────────────────────────────────────────────
CMD="${1:-rebuild}"

do_build() {
  info "Building Docker image '$IMAGE_NAME' from $REPO_ROOT …"
  docker build -t "$IMAGE_NAME" "$REPO_ROOT"
  success "Image built successfully."
}

do_stop() {
  if docker ps -q -f name="^${CONTAINER_NAME}$" | grep -q .; then
    info "Stopping container '$CONTAINER_NAME' …"
    docker stop "$CONTAINER_NAME" >/dev/null
    success "Container stopped."
  else
    warn "Container '$CONTAINER_NAME' is not running."
  fi

  if docker ps -aq -f name="^${CONTAINER_NAME}$" | grep -q .; then
    info "Removing container '$CONTAINER_NAME' …"
    docker rm "$CONTAINER_NAME" >/dev/null
  fi
}

do_start() {
  mkdir -p "$DEV_DATA_DIR"
  info "Starting dev instance on port $DEV_PORT …"
  info "  Data dir : $DEV_DATA_DIR"
  info "  Container: $CONTAINER_NAME"
  docker run -d \
    --name "$CONTAINER_NAME" \
    -p "${DEV_PORT}:${DEV_PORT}" \
    -e PORT="$DEV_PORT" \
    -e AGENT_DECK_DATA_DIR="/data" \
    -v "${DEV_DATA_DIR}:/data" \
    "$IMAGE_NAME"
  success "Dev instance running at http://localhost:$DEV_PORT"
}

case "$CMD" in
  build)
    do_build
    ;;
  start)
    do_start
    ;;
  stop)
    do_stop
    ;;
  logs)
    info "Tailing logs for '$CONTAINER_NAME' (Ctrl-C to stop) …"
    docker logs -f "$CONTAINER_NAME"
    ;;
  status)
    if docker ps -q -f name="^${CONTAINER_NAME}$" | grep -q .; then
      success "Container '$CONTAINER_NAME' is RUNNING on port $DEV_PORT"
      docker ps --filter "name=^${CONTAINER_NAME}$" --format "  ID: {{.ID}}  Image: {{.Image}}  Status: {{.Status}}"
    else
      warn "Container '$CONTAINER_NAME' is NOT running."
    fi
    ;;
  clean)
    do_stop
    if docker images -q "$IMAGE_NAME" | grep -q .; then
      info "Removing image '$IMAGE_NAME' …"
      docker rmi "$IMAGE_NAME" >/dev/null
      success "Image removed."
    fi
    ;;
  rebuild|*)
    do_stop
    do_build
    do_start
    ;;
esac
