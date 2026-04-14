#!/usr/bin/env bash
set -euo pipefail

AGENT_DECK_HOME="${AGENT_DECK_HOME:-$HOME/.agent-deck}"
LOG_FILE="$AGENT_DECK_HOME/server.log"
PID_FILE="$AGENT_DECK_HOME/agent-deck.pid"
BINARY="$AGENT_DECK_HOME/bin/agent-deck"
PORT="${AGENT_DECK_PORT:-7474}"

if [ ! -f "$BINARY" ]; then
  echo "Error: agent-deck binary not found at $BINARY"
  echo "Run ./install.sh to build and install agent-deck."
  exit 1
fi

mkdir -p "$AGENT_DECK_HOME"

nohup "$BINARY" >> "$LOG_FILE" 2>&1 &

echo $! > "$PID_FILE"

echo ""
echo "  agent-deck is running (PID $(cat "$PID_FILE"))"
echo ""
echo "  Open:  http://localhost:$PORT"

if command -v tailscale &>/dev/null; then
  TS_HOST=$(tailscale status --json 2>/dev/null | grep -o '"DNSName":"[^"]*"' | head -1 | cut -d'"' -f4 | sed 's/\.$//')
  if [ -n "$TS_HOST" ]; then
    echo "         http://$TS_HOST:$PORT"
  fi
fi

echo ""
echo "  Logs:  $LOG_FILE"
echo "         (run 'agent-deck logs' to tail)"
echo ""
