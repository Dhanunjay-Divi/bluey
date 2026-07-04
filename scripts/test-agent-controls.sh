#!/usr/bin/env bash
# Headless end-to-end test of the agent-bridge controls landed on
# agent/agent-bridge-fixes: model listing, model override, session selection,
# resume, and speed. Exercises the LIVE daemon over the CLI — no overlay binary
# (honors the screen-share-invisibility rule).
#
# Usage:  ./scripts/test-agent-controls.sh [agent_kind]
#   agent_kind defaults to cursor (the one live-enumerable model source).
#
# Prereqs: `bluey on` (a daemon on the freshly built binary), and for the
# session/resume steps, session-history consent enabled once:
#   bluey settings   # (or the overlay toggle) — sets allow_agent_session_history
set -uo pipefail

BLUEY="${BLUEY_BIN:-target/aarch64-apple-darwin/debug/bluey}"
KIND="${1:-cursor}"
SETTINGS="${HOME}/Library/Application Support/bluey/settings.json"

hr() { printf '\n=== %s ===\n' "$1"; }
model_now() { python3 -c "import json;print(json.load(open('${SETTINGS}')).get('attached_model'))" 2>/dev/null; }
session_now() { python3 -c "import json;print(json.load(open('${SETTINGS}')).get('attached_session'))" 2>/dev/null; }

hr "daemon status"
"${BLUEY}" status >/dev/null 2>&1 || { echo "daemon not running — run 'bluey on' first"; exit 1; }
echo "daemon reachable"

hr "1. model list for ${KIND} (live scrape or curated fallback)"
"${BLUEY}" agent models "${KIND}" | head -12

hr "2. attach ${KIND} with a model override"
MODEL="$("${BLUEY}" agent models "${KIND}" | sed -n '2p')"   # first real id after "auto"
echo "picking model: ${MODEL}"
"${BLUEY}" agent attach "${KIND}" --model "${MODEL}"
echo "persisted attached_model = $(model_now)   (expect: ${MODEL})"

hr "3. plain re-attach clears the override"
"${BLUEY}" agent attach "${KIND}" >/dev/null
echo "persisted attached_model = $(model_now)   (expect: None)"

hr "4. session selection"
"${BLUEY}" agent sessions "${KIND}" | head -5 \
  || echo "(no sessions — enable session-history consent first)"

hr "5. resume + model together"
SID="$("${BLUEY}" agent sessions "${KIND}" 2>/dev/null \
  | grep -oE '[0-9a-f-]{36}' | head -1)"
if [ -n "${SID}" ]; then
  "${BLUEY}" agent attach "${KIND}" --session "${SID}" --model "${MODEL}"
  echo "persisted model=$(model_now) session=$(session_now)"
else
  echo "(no session id available — skipping resume step)"
fi

hr "6. ask with a speed tier (drives the real agent — spends quota)"
echo "run manually when ready:"
echo "  ${BLUEY} ask --speed fast \"what does this repo do in one line?\""
echo "  ${BLUEY} ask --speed deep \"explain the agent-bridge session ledger\""

hr "done"
echo "detach with: ${BLUEY} agent detach"
