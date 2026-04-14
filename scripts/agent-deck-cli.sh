#!/usr/bin/env bash
# agent-deck shell CLI
# Sourced by ~/.zshrc or ~/.bash_profile via the line added by install.sh

agent-deck() {
  local AGENT_DECK_HOME="${AGENT_DECK_HOME:-$HOME/.agent-deck}"
  local PID_FILE="$AGENT_DECK_HOME/agent-deck.pid"
  local LOG_FILE="$AGENT_DECK_HOME/server.log"
  local RUN_SCRIPT="$AGENT_DECK_HOME/run.sh"

  case "${1:-help}" in
    start)
      if [ -f "$PID_FILE" ] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
        echo "agent-deck is already running (PID $(cat "$PID_FILE"))"
      else
        bash "$RUN_SCRIPT"
      fi
      ;;
    stop)
      if [ -f "$PID_FILE" ]; then
        local PID
        PID=$(cat "$PID_FILE")
        if kill -0 "$PID" 2>/dev/null; then
          kill "$PID"
          rm -f "$PID_FILE"
          echo "agent-deck stopped (PID $PID)"
        else
          echo "agent-deck is not running (stale PID file removed)"
          rm -f "$PID_FILE"
        fi
      else
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
      open "http://localhost:${AGENT_DECK_PORT:-7474}"
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
