#!/usr/bin/env bash
# =============================================================================
# agent-deck — Phase 2.5 end-to-end integration test
#
# Tests the full chat flow:
#   1. Setup (create user)
#   2. Create provider
#   3. Create persona
#   4. Create thread
#   5. Open SSE stream
#   6. Send a message
#   7. Assert tokens streamed
#   8. Assert message_complete event received
#   9. Assert thread title auto-generated
#  10. Assert assistant message persisted to DB
#
# Usage:
#   ./scripts/test-phase2.sh [OPTIONS]
#
# Options:
#   --api-key   KEY   OpenAI-compatible API key (optional, not needed for copilot-api)
#   --base-url  URL   Provider base URL (default: http://localhost:4141/v1)
#   --model     ID    Model ID to use   (default: gpt-4o-mini)
#   --provider-kind   Kind: openai | custom | copilot (default: custom)
#   --port      N     Server port       (default: 7474)
#   --timeout   N     Seconds to wait for SSE events (default: 30)
#   --no-server       Skip building/starting the server (use an already-running one)
#   --no-docker       Skip starting the copilot-api Docker container
#   --help            Print this help
#
# Environment variables (alternative to flags):
#   OPENAI_API_KEY    API key (optional)
#   AGENT_DECK_PORT   Server port
#
# Prerequisites:
#   - Docker running with the copilot-api image built:
#       docker build -t copilot-api vendor/copilot-api
#   - GitHub Copilot authenticated:
#       npx copilot-api@latest auth
# =============================================================================

set -euo pipefail

# ── Defaults ──────────────────────────────────────────────────────────────────

PORT="${AGENT_DECK_PORT:-7575}"
BASE_URL="http://localhost:4141/v1"
MODEL="gpt-4o"
PROVIDER_KIND="custom"
API_KEY="${OPENAI_API_KEY:-}"
SSE_TIMEOUT=30
START_SERVER=true
START_DOCKER=true
SERVER_PID=""
DOCKER_CONTAINER="copilot-api-test"
DB_FILE=""
AUTH_TOKEN=""

# ── Colours ───────────────────────────────────────────────────────────────────

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

pass() { echo -e "${GREEN}  ✔ $*${RESET}"; }
fail() { echo -e "${RED}  ✘ $*${RESET}"; }
info() { echo -e "${CYAN}  → $*${RESET}"; }
header() { echo -e "\n${BOLD}${YELLOW}$*${RESET}"; }

# ── Arg parsing ───────────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
  case "$1" in
    --api-key)       API_KEY="$2";        shift 2 ;;
    --base-url)      BASE_URL="$2";       shift 2 ;;
    --model)         MODEL="$2";          shift 2 ;;
    --provider-kind) PROVIDER_KIND="$2";  shift 2 ;;
    --port)          PORT="$2";           shift 2 ;;
    --timeout)       SSE_TIMEOUT="$2";    shift 2 ;;
    --no-server)     START_SERVER=false;  shift   ;;
    --no-docker)     START_DOCKER=false;  shift   ;;
    --help)
      sed -n '/^# Usage:/,/^# =/p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

BASE="http://localhost:${PORT}"

# ── Dependency checks ─────────────────────────────────────────────────────────

check_deps() {
  local missing=()
  for cmd in curl jq docker; do
    command -v "$cmd" &>/dev/null || missing+=("$cmd")
  done
  if [[ ${#missing[@]} -gt 0 ]]; then
    echo -e "${RED}Missing required tools: ${missing[*]}${RESET}"
    echo "Install them and re-run."
    exit 1
  fi
}

# ── Cleanup ───────────────────────────────────────────────────────────────────

cleanup() {
  if [[ -n "$SERVER_PID" ]]; then
    info "Stopping server (PID $SERVER_PID)…"
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  if [[ "$START_DOCKER" == true ]]; then
    info "Stopping copilot-api container…"
    docker stop "$DOCKER_CONTAINER" 2>/dev/null || true
    docker rm "$DOCKER_CONTAINER" 2>/dev/null || true
  fi
  # Clean up temp files
  rm -f /tmp/agent_deck_sse_$$.txt
  [[ -n "${DB_FILE:-}" ]] && rm -f "$DB_FILE" 2>/dev/null || true
}
trap cleanup EXIT

# ── Helpers ───────────────────────────────────────────────────────────────────

# POST and return body. Does NOT use -f so 4xx/5xx responses are still
# captured as output rather than triggering set -e.
api_post() {
  local path="$1"
  local data="$2"
  curl -s -X POST "${BASE}${path}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${AUTH_TOKEN}" \
    -d "$data"
}

api_get() {
  local path="$1"
  curl -s "${BASE}${path}" \
    -H "Authorization: Bearer ${AUTH_TOKEN}"
}

# Public GET — no auth header (for /health, /api/setup/*)
api_get_public() {
  local path="$1"
  curl -s "${BASE}${path}"
}

# Public POST — no auth header (for /api/setup/complete)
api_post_public() {
  local path="$1"
  local data="$2"
  curl -s -X POST "${BASE}${path}" \
    -H "Content-Type: application/json" \
    -d "$data"
}

# Extract .data.<field> from a JSON response
jq_data() {
  local field="$1"
  local json="$2"
  echo "$json" | jq -r ".data.${field} // empty"
}

# Wait for the server to become healthy
wait_for_server() {
  local attempts=0
  local max=30
  info "Waiting for server on port ${PORT}…"
  while ! curl -s "${BASE}/health" &>/dev/null; do
    attempts=$((attempts + 1))
    if [[ $attempts -ge $max ]]; then
      fail "Server did not start after ${max} seconds"
      exit 1
    fi
    sleep 1
  done
  pass "Server is up"
}

# Extract the auth token from the server log.
# The server prints it across two lines in tracing format, e.g.:
#   ...INFO server::routes: ║  Auth token: f76f201f95b74987dc86d02792b8cc0a  ║
#   ...INFO server::routes: ║             f91c379c87ebc6ba3a5ff23744e2e87a  ║
# We use awk to grab both hex chunks in a single pass over the file.
extract_auth_token() {
  local log_file="$1"
  local attempts=0
  info "Extracting auth token from server log…"
  while [[ $attempts -lt 15 ]]; do
    AUTH_TOKEN=$(awk '
      /Auth token: [0-9a-f]{32}/ {
        match($0, /[0-9a-f]{32}/)
        part1 = substr($0, RSTART, RLENGTH)
      }
      part1 != "" && !/Auth token/ && /[0-9a-f]{32}/ {
        match($0, /[0-9a-f]{32}/)
        part2 = substr($0, RSTART, RLENGTH)
        print part1 part2
        exit
      }
    ' "$log_file" 2>/dev/null)
    if [[ -n "$AUTH_TOKEN" && ${#AUTH_TOKEN} -eq 64 ]]; then
      pass "Auth token extracted (${AUTH_TOKEN:0:8}…)"
      return 0
    fi
    sleep 1
    attempts=$((attempts + 1))
  done
  fail "Could not extract auth token from server log"
  grep -i "auth\|token" "$log_file" | head -10
  exit 1
}

# ── Test counters ─────────────────────────────────────────────────────────────

TESTS_PASSED=0
TESTS_FAILED=0

assert_eq() {
  local label="$1"
  local expected="$2"
  local actual="$3"
  if [[ "$actual" == "$expected" ]]; then
    pass "$label"
    TESTS_PASSED=$((TESTS_PASSED + 1))
  else
    fail "$label (expected: '$expected', got: '$actual')"
    TESTS_FAILED=$((TESTS_FAILED + 1))
  fi
}

assert_not_empty() {
  local label="$1"
  local value="$2"
  if [[ -n "$value" ]]; then
    pass "$label"
    TESTS_PASSED=$((TESTS_PASSED + 1))
  else
    fail "$label (value was empty)"
    TESTS_FAILED=$((TESTS_FAILED + 1))
  fi
}

assert_contains() {
  local label="$1"
  local needle="$2"
  local haystack="$3"
  if echo "$haystack" | grep -qF "$needle"; then
    pass "$label"
    TESTS_PASSED=$((TESTS_PASSED + 1))
  else
    fail "$label (expected to contain: '$needle')"
    TESTS_FAILED=$((TESTS_FAILED + 1))
  fi
}

assert_not_contains() {
  local label="$1"
  local needle="$2"
  local haystack="$3"
  if ! echo "$haystack" | grep -qF "$needle"; then
    pass "$label"
    TESTS_PASSED=$((TESTS_PASSED + 1))
  else
    fail "$label (expected NOT to contain: '$needle')"
    TESTS_FAILED=$((TESTS_FAILED + 1))
  fi
}

# ── Main ──────────────────────────────────────────────────────────────────────

echo -e "${BOLD}"
echo "╔══════════════════════════════════════════════════════════╗"
echo "║     agent-deck — Phase 2.5 Integration Test             ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo -e "${RESET}"

check_deps

# ── Validate args ─────────────────────────────────────────────────────────────

# API key is optional when using copilot-api (it handles its own auth)
if [[ "$PROVIDER_KIND" == "openai" && -z "$API_KEY" ]]; then
  echo -e "${RED}Error: --api-key is required for provider kind 'openai'${RESET}"
  echo "Pass your key via --api-key or set \$OPENAI_API_KEY."
  exit 1
fi

# ── Start server ──────────────────────────────────────────────────────────────

# Find the repo root (parent of the scripts/ dir)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
# Use an absolute path with ?mode=rwc so SQLite creates the file regardless of
# the binary's working directory. /tmp is always writable.
DB_FILE="/tmp/agent_deck_test_$$.db"
DB_URL="sqlite://${DB_FILE}?mode=rwc"

# ── Start copilot-api Docker container ────────────────────────────────────────

if [[ "$START_DOCKER" == true ]]; then
  header "Starting copilot-api Docker container…"

  GH_TOKEN_FILE="$HOME/.local/share/copilot-api/github_token"
  if [[ ! -f "$GH_TOKEN_FILE" ]]; then
    echo -e "${RED}Error: GitHub token not found at $GH_TOKEN_FILE${RESET}"
    echo "Run: npx copilot-api@latest auth"
    exit 1
  fi
  GH_TOKEN="$(cat "$GH_TOKEN_FILE")"

  # Check the image exists
  if ! docker image inspect copilot-api &>/dev/null; then
    echo -e "${RED}Error: copilot-api Docker image not found${RESET}"
    echo "Run: docker build -t copilot-api vendor/copilot-api"
    exit 1
  fi

  # Stop any leftover container from a previous run (by name)
  docker stop "$DOCKER_CONTAINER" 2>/dev/null || true
  docker rm   "$DOCKER_CONTAINER" 2>/dev/null || true

  # Also stop any other container that has port 4141 bound, to avoid
  # "port is already allocated" failures from stale debug/manual containers.
  STALE_CONTAINERS=$(docker ps --filter "publish=4141" -q 2>/dev/null || true)
  if [[ -n "$STALE_CONTAINERS" ]]; then
    info "Stopping containers already bound to port 4141…"
    echo "$STALE_CONTAINERS" | xargs docker stop 2>/dev/null || true
    echo "$STALE_CONTAINERS" | xargs docker rm   2>/dev/null || true
  fi

  docker run -d \
    --name "$DOCKER_CONTAINER" \
    -p 4141:4141 \
    -e GH_TOKEN="$GH_TOKEN" \
    copilot-api

  info "Waiting for copilot-api to become healthy…"
  attempts=0
  until curl -sf http://localhost:4141/v1/models &>/dev/null; do
    attempts=$((attempts + 1))
    if [[ $attempts -ge 30 ]]; then
      fail "copilot-api did not become healthy after 30 seconds"
      docker logs "$DOCKER_CONTAINER" | tail -20
      exit 1
    fi
    sleep 1
  done
  pass "copilot-api is healthy on http://localhost:4141"
else
  header "Skipping Docker — assuming copilot-api is already running on port 4141"
  if ! curl -sf http://localhost:4141/v1/models &>/dev/null; then
    echo -e "${RED}Error: copilot-api is not reachable on http://localhost:4141${RESET}"
    exit 1
  fi
  pass "copilot-api reachable on http://localhost:4141"
fi

# ── Start agent-deck server ───────────────────────────────────────────────────

if [[ "$START_SERVER" == true ]]; then
  header "Building server…"
  (cd "$REPO_ROOT" && cargo build --bin server 2>/dev/null) || {
    # Re-run with stderr visible so the user can see what went wrong
    (cd "$REPO_ROOT" && cargo build --bin server)
    exit 1
  }
  pass "Build succeeded"

  header "Starting server with a clean test database…"

  # Kill any stale process already on the test port so we don't accidentally
  # talk to the wrong server (e.g. a leftover run from a previous test).
  STALE_PID=$(lsof -ti :"$PORT" 2>/dev/null || true)
  if [[ -n "$STALE_PID" ]]; then
    info "Killing stale process on port ${PORT} (PID ${STALE_PID})…"
    kill "$STALE_PID" 2>/dev/null || true
    sleep 1
  fi

  # Run from REPO_ROOT so vendor/copilot-api is resolvable by the binary.
  (cd "$REPO_ROOT" && \
    DATABASE_URL="$DB_URL" \
    PORT="$PORT" \
    PUBLIC_DIR="${REPO_ROOT}/server/public" \
      "${REPO_ROOT}/target/debug/server") &>/tmp/agent_deck_server_$$.log &
  SERVER_PID=$!
  info "Server PID: $SERVER_PID (log: /tmp/agent_deck_server_$$.log)"

  wait_for_server
  # Brief pause to ensure the token banner has been flushed to the log file
  sleep 1
  extract_auth_token "/tmp/agent_deck_server_$$.log"
else
  header "Using already-running server on port ${PORT}"
  wait_for_server
  # When --no-server is passed, the caller must supply the token via AUTH_TOKEN env var
  if [[ -z "$AUTH_TOKEN" ]]; then
    echo -e "${RED}Error: AUTH_TOKEN env var must be set when using --no-server${RESET}"
    echo "Export the token printed by the server on startup."
    exit 1
  fi
  pass "Using provided AUTH_TOKEN"
fi

# ── Step 1: Setup ─────────────────────────────────────────────────────────────

header "Step 1 — Setup"

SETUP_RESP=$(api_post_public "/api/setup/complete" '{"display_name":"Test User"}')
SETUP_COMPLETE=$(jq_data "complete" "$SETUP_RESP")

if [[ "$SETUP_COMPLETE" == "true" ]]; then
  assert_eq "Setup reports complete" "true" "$SETUP_COMPLETE"
else
  # May already be set up (fresh server pointed at existing DB, or --no-server)
  info "Setup endpoint returned non-complete — checking status"
  STATUS_RESP=$(api_get_public "/api/setup/status")
  SETUP_COMPLETE=$(jq_data "complete" "$STATUS_RESP")
  if [[ "$SETUP_COMPLETE" == "true" ]]; then
    pass "Setup already complete"
  else
    fail "Setup failed — status reports incomplete. Response: $SETUP_RESP"
    exit 1
  fi
fi

# ── Step 2: Create provider ───────────────────────────────────────────────────

header "Step 2 — Create provider"

PROVIDER_PAYLOAD=$(jq -n \
  --arg name "Test Provider" \
  --arg kind "$PROVIDER_KIND" \
  --arg base_url "$BASE_URL" \
  --arg api_key "$API_KEY" \
  '{name: $name, kind: $kind, base_url: $base_url, api_key: $api_key}')

PROVIDER_RESP=$(api_post "/api/providers" "$PROVIDER_PAYLOAD")
PROVIDER_ID=$(jq_data "id" "$PROVIDER_RESP")

assert_not_empty "Provider created, got ID" "$PROVIDER_ID"
info "Provider ID: $PROVIDER_ID"

PROVIDER_KIND_RESP=$(jq_data "kind" "$PROVIDER_RESP")
assert_eq "Provider kind matches" "$PROVIDER_KIND" "$PROVIDER_KIND_RESP"

# ── Step 2b: Sync models and pick a model ID ──────────────────────────────────

header "Step 2b — Sync models from provider"

SYNC_RESP=$(api_post "/api/providers/${PROVIDER_ID}/models" '{}')
info "Sync response: $(echo "$SYNC_RESP" | jq -c '.data | length' 2>/dev/null || echo "$SYNC_RESP")"

# Pick the model whose model_id exactly matches MODEL, then fall back to substring, then first.
# We need the UUID .id column (FK in threads/agent_personas), NOT .model_id (the string name).
MODELS_RESP=$(api_get "/api/providers/${PROVIDER_ID}/models")

# 1. Exact match
MODEL_ID=$(echo "$MODELS_RESP" \
  | jq -r --arg m "$MODEL" '
      .data[]
      | select(.model_id == $m)
      | .id
    ' 2>/dev/null | head -1)

# 2. Substring match (case-insensitive)
if [[ -z "$MODEL_ID" ]]; then
  info "No exact match for '$MODEL' — trying substring match"
  MODEL_ID=$(echo "$MODELS_RESP" \
    | jq -r --arg m "$MODEL" '
        .data[]
        | select(.model_id | test($m; "i"))
        | .id
      ' 2>/dev/null | head -1)
fi

# 3. First available
if [[ -z "$MODEL_ID" ]]; then
  info "Preferred model '$MODEL' not found — using first available model"
  MODEL_ID=$(echo "$MODELS_RESP" | jq -r '.data[0].id // empty' 2>/dev/null)
fi

assert_not_empty "Got a model ID from provider" "$MODEL_ID"
info "Model ID: $MODEL_ID"

# ── Step 3: Create persona ────────────────────────────────────────────────────

header "Step 3 — Create persona"

# default_model references models(id) FK — omit it and set it on the thread instead.
PERSONA_PAYLOAD=$(jq -n \
  --arg name "Test Persona" \
  --arg emoji "🤖" \
  --arg system_prompt "You are a concise test assistant. Keep all responses under 3 sentences." \
  '{name: $name, emoji: $emoji, system_prompt: $system_prompt}')

PERSONA_RESP=$(api_post "/api/personas" "$PERSONA_PAYLOAD")
PERSONA_ID=$(jq_data "id" "$PERSONA_RESP")

assert_not_empty "Persona created, got ID" "$PERSONA_ID"
info "Persona ID: $PERSONA_ID"

# ── Step 4: Create thread ─────────────────────────────────────────────────────

header "Step 4 — Create thread"

# Set active_model and active_provider on the thread — these reference DB rows.
THREAD_PAYLOAD=$(jq -n \
  --arg persona_id "$PERSONA_ID" \
  --arg active_model "$MODEL_ID" \
  --arg active_provider "$PROVIDER_ID" \
  '{persona_id: $persona_id, active_model: $active_model, active_provider: $active_provider}')
THREAD_RESP=$(api_post "/api/threads" "$THREAD_PAYLOAD")
THREAD_ID=$(jq_data "id" "$THREAD_RESP")

assert_not_empty "Thread created, got ID" "$THREAD_ID"
info "Thread ID: $THREAD_ID"

INITIAL_TITLE=$(jq_data "title" "$THREAD_RESP")
assert_eq "Thread starts with default title" "New Chat" "$INITIAL_TITLE"

# ── Step 5: Open SSE stream ───────────────────────────────────────────────────

header "Step 5 — Opening SSE stream"

SSE_OUTPUT="/tmp/agent_deck_sse_$$.txt"
> "$SSE_OUTPUT"

# curl -N follows the stream; we run it in the background and capture output
curl -sN "${BASE}/api/threads/${THREAD_ID}/stream" \
  -H "Authorization: Bearer ${AUTH_TOKEN}" > "$SSE_OUTPUT" &
SSE_PID=$!
info "SSE stream PID: $SSE_PID (output: $SSE_OUTPUT)"

# Give it a moment to connect
sleep 0.5

if kill -0 "$SSE_PID" 2>/dev/null; then
  pass "SSE stream connected"
else
  fail "SSE stream process exited immediately"
  TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# ── Step 6: Send message ──────────────────────────────────────────────────────

header "Step 6 — Sending message"

USER_MESSAGE="What is 2 + 2? Answer in exactly one sentence."

MSG_PAYLOAD=$(jq -n --arg content "$USER_MESSAGE" '{content: $content}')
MSG_RESP=$(api_post "/api/threads/${THREAD_ID}/messages" "$MSG_PAYLOAD")

MSG_ID=$(jq_data "id" "$MSG_RESP")
MSG_ROLE=$(jq_data "role" "$MSG_RESP")
MSG_CONTENT=$(jq_data "content" "$MSG_RESP")

assert_not_empty "User message returned immediately" "$MSG_ID"
assert_eq      "Returned message has role=user" "user" "$MSG_ROLE"
assert_eq      "Returned message content matches" "$USER_MESSAGE" "$MSG_CONTENT"
info "User message ID: $MSG_ID"

# ── Step 7: Wait for SSE events ───────────────────────────────────────────────

header "Step 7 — Waiting for SSE events (timeout: ${SSE_TIMEOUT}s)"

info "Waiting for message_complete event…"

elapsed=0
while [[ $elapsed -lt $SSE_TIMEOUT ]]; do
  if grep -q "message_complete" "$SSE_OUTPUT" 2>/dev/null; then
    break
  fi
  sleep 1
  elapsed=$((elapsed + 1))
done

# Kill the SSE stream now that we're done with it
kill "$SSE_PID" 2>/dev/null || true
wait "$SSE_PID" 2>/dev/null || true
SSE_CONTENT=$(cat "$SSE_OUTPUT")

# Assertions on the SSE output
if [[ $elapsed -ge $SSE_TIMEOUT ]]; then
  fail "Timed out waiting for message_complete after ${SSE_TIMEOUT}s"
  TESTS_FAILED=$((TESTS_FAILED + 1))
  echo -e "${YELLOW}  SSE output so far:${RESET}"
  cat "$SSE_OUTPUT" | head -40 | sed 's/^/    /'
else
  pass "message_complete event received within ${elapsed}s"
  TESTS_PASSED=$((TESTS_PASSED + 1))
fi

assert_contains     "SSE output contains 'token' events"           '"event":"token"'            "$SSE_CONTENT"
assert_contains     "SSE output contains 'message_complete' event" '"event":"message_complete"' "$SSE_CONTENT"
assert_not_contains "SSE output contains no 'error' events"        '"event":"error"'            "$SSE_CONTENT"

# Extract the assistant message ID from the message_complete event.
# SSE lines look like:
#   data: {"event":"message_complete","id":"<uuid>","content":"..."}
# Use jq for robust extraction — grep -o returns exit 1 on no-match which
# trips set -e, so we guard every extraction with "|| true".
COMPLETE_JSON=$(echo "$SSE_CONTENT" \
  | grep '"event":"message_complete"' \
  | head -1 \
  | sed 's/^data: *//' \
  || true)

ASSISTANT_MSG_ID=""
ASSISTANT_CONTENT=""
if [[ -n "$COMPLETE_JSON" ]]; then
  ASSISTANT_MSG_ID=$(echo "$COMPLETE_JSON" | jq -r '.id // empty' 2>/dev/null || true)
  ASSISTANT_CONTENT=$(echo "$COMPLETE_JSON" | jq -r '.content // empty' 2>/dev/null || true)
fi

assert_not_empty "message_complete event contains assistant message ID" "$ASSISTANT_MSG_ID"
assert_not_empty "message_complete event contains content"              "$ASSISTANT_CONTENT"
info "Assistant message ID: $ASSISTANT_MSG_ID"
info "Assistant content preview: ${ASSISTANT_CONTENT:0:80}…"

# ── Step 8: Assert thread title updated ───────────────────────────────────────

header "Step 8 — Verifying thread title auto-generation"

# Give the DB write a moment to land
sleep 0.5

THREAD_AFTER=$(api_get "/api/threads/${THREAD_ID}")
TITLE_AFTER=$(jq_data "title" "$THREAD_AFTER")

assert_not_empty "Thread title is set after first message" "$TITLE_AFTER"
assert_not_contains "Thread title is no longer 'New Chat'" "New Chat" "$TITLE_AFTER"
info "Thread title: $TITLE_AFTER"

# ── Step 9: Assert assistant message persisted ────────────────────────────────

header "Step 9 — Verifying assistant message persistence"

MESSAGES_RESP=$(api_get "/api/threads/${THREAD_ID}/messages")
MESSAGES_JSON=$(echo "$MESSAGES_RESP" | jq '.data')
MSG_COUNT=$(echo "$MESSAGES_JSON" | jq 'length')

assert_not_empty "Messages endpoint returned data"       "$MSG_COUNT"

# Should have exactly 2 visible messages: the user message and the assistant reply
assert_eq "Thread has exactly 2 visible messages" "2" "$MSG_COUNT"

LAST_ROLE=$(echo "$MESSAGES_JSON" | jq -r '.[-1].role')
LAST_CONTENT=$(echo "$MESSAGES_JSON" | jq -r '.[-1].content')
LAST_SOURCE=$(echo "$MESSAGES_JSON" | jq -r '.[-1].source')

assert_eq      "Last message has role=assistant"  "assistant" "$LAST_ROLE"
assert_eq      "Last message has source=chat"     "chat"      "$LAST_SOURCE"
assert_not_empty "Last message has non-empty content"  "$LAST_CONTENT"

info "Persisted assistant content preview: ${LAST_CONTENT:0:80}…"

# ── Results ───────────────────────────────────────────────────────────────────

echo ""
echo -e "${BOLD}══════════════════════════════════════════════════════════${RESET}"
TOTAL=$((TESTS_PASSED + TESTS_FAILED))

if [[ $TESTS_FAILED -eq 0 ]]; then
  echo -e "${GREEN}${BOLD}  All ${TOTAL} tests passed ✔${RESET}"
  echo -e "${BOLD}══════════════════════════════════════════════════════════${RESET}"
  exit 0
else
  echo -e "${RED}${BOLD}  ${TESTS_FAILED} of ${TOTAL} tests FAILED ✘${RESET}"
  echo -e "${BOLD}══════════════════════════════════════════════════════════${RESET}"
  echo ""
  echo -e "${YELLOW}Tip: check the server log at /tmp/agent_deck_server_$$.log${RESET}"
  exit 1
fi
