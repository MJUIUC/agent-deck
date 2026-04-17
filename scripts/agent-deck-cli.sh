#!/usr/bin/env bash
# agent-deck shell CLI
# Sourced by ~/.zshrc or ~/.bash_profile via the line added by install.sh

agent-deck() {
  local AGENT_DECK_HOME="${AGENT_DECK_HOME:-$HOME/.agent-deck}"
  local PROCESS_DIR="$AGENT_DECK_HOME/.process"
  local PID_FILE="$PROCESS_DIR/agent-deck.pid"
  local LOG_FILE="$PROCESS_DIR/server.log"
  local RUN_SCRIPT="$PROCESS_DIR/run.sh"

  case "${1:-help}" in
    start)
      if [ -f "$PID_FILE" ] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
        echo "agent-deck is already running (PID $(cat "$PID_FILE"))"
      else
        bash "$RUN_SCRIPT"
      fi
      ;;
    stop)
      local PORT="${AGENT_DECK_PORT:-7474}"
      local stopped=false
      if [ -f "$PID_FILE" ]; then
        local PID
        PID=$(cat "$PID_FILE")
        if kill -0 "$PID" 2>/dev/null; then
          kill "$PID"
          echo "agent-deck stopped (PID $PID)"
          stopped=true
        fi
        rm -f "$PID_FILE"
      fi
      # Fallback: kill anything still holding the port (e.g. started via cargo run)
      local PORT_PIDS
      PORT_PIDS=$(lsof -ti ":$PORT" 2>/dev/null || true)
      if [ -n "$PORT_PIDS" ]; then
        echo "$PORT_PIDS" | xargs kill 2>/dev/null || true
        stopped=true
      fi
      if [ "$stopped" = false ]; then
        echo "agent-deck is not running"
      fi
      ;;
    status)
      if [ -f "$PID_FILE" ] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
        echo "agent-deck is running (PID $(cat "$PID_FILE"))"
      else
        echo "agent-deck is not running"
      fi
      if command -v tailscale &>/dev/null; then
        local TS_STATE
        TS_STATE=$(tailscale status --json 2>/dev/null | grep -o '"BackendState":"[^"]*"' | cut -d'"' -f4)
        if [ "$TS_STATE" = "Running" ]; then
          echo "Tailscale: connected"
        else
          echo "Tailscale: not connected (state: ${TS_STATE:-unknown})"
        fi
      fi
      ;;
    logs)
      if [ -f "$LOG_FILE" ]; then
        tail -f "$LOG_FILE"
      else
        echo "No log file found at $LOG_FILE"
      fi
      ;;
    open)
      if [[ "$(uname)" == "Darwin" ]]; then
        open "http://localhost:${AGENT_DECK_PORT:-7474}"
      else
        xdg-open "http://localhost:${AGENT_DECK_PORT:-7474}"
      fi
      ;;
    help|--help|-h)
      echo "Usage: agent-deck <command>"
      echo ""
      echo "Commands:"
      echo "  start   Start the agent-deck server"
      echo "  stop    Stop the agent-deck server"
      echo "  status  Show server and Tailscale status"
      echo "  logs    Tail the server log"
      echo "  open    Open agent-deck in the default browser"
      echo "  help    Show this help message"
      ;;
    *)
      echo "Unknown command: $1"
      echo "Run 'agent-deck help' for usage."
      return 1
      ;;
  esac
}
