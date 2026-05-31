#!/usr/bin/env bash
#
# test-agent-bridge.sh — end-to-end smoke test for the Bluey agent-context-bridge.
#
# WHAT THIS DOES
#   Walks you through testing Bluey's "coding agent" bridge: discovering installed
#   agents, attaching one, inspecting its connectors/sessions, asking a question that
#   routes through the attached agent, then detaching.
#
#   It exercises this sequence of CLI commands:
#       bluey on                 (start daemon + overlay — a BACKGROUND process)
#       bluey status             (poll until the daemon answers)
#       bluey agent list
#       bluey agent attach <kind>
#       bluey agent status
#       bluey agent connectors <kind>
#       bluey agent sessions <kind>
#       bluey ask "..."
#       bluey agent detach
#       bluey off                (stop everything)
#
# IMPORTANT — BACKGROUND PROCESSES
#   `bluey on` starts the Bluey daemon (and, on supported platforms, a native
#   overlay) as detached background processes. This script ALWAYS stops them again
#   at the end via `bluey off`, and installs an EXIT trap so cleanup runs even if a
#   step fails or you press Ctrl-C. You should not be left with a running daemon.
#
# SAFETY
#   - Read-only inspection plus one short `ask`. No files are written by this script
#     other than Bluey's own state under its app-data dir (created by `bluey on`).
#   - If no drivable coding agent CLI is installed, the attach / ask steps are
#     skipped with a clear note (the rest still runs).
#
# USAGE
#   scripts/test-agent-bridge.sh
#
# This script lives in the Bluey repo and locates (or builds) target/release/bluey.

set -euo pipefail

# ---------------------------------------------------------------------------
# Output helpers (no exotic deps; colors degrade to plain text if unsupported).
# ---------------------------------------------------------------------------
if [[ -t 1 ]] && command -v tput >/dev/null 2>&1 && [[ "$(tput colors 2>/dev/null || echo 0)" -ge 8 ]]; then
  C_RESET="$(tput sgr0)"
  C_BOLD="$(tput bold)"
  C_GREEN="$(tput setaf 2)"
  C_BLUE="$(tput setaf 4)"
  C_YELLOW="$(tput setaf 3)"
  C_RED="$(tput setaf 1)"
else
  C_RESET="" C_BOLD="" C_GREEN="" C_BLUE="" C_YELLOW="" C_RED=""
fi

# Markers. ASCII fallbacks are unnecessary on a UTF-8 terminal, but the glyphs are
# plain enough to render everywhere.
M_OK="${C_GREEN}✓${C_RESET}"
M_INFO="${C_BLUE}ℹ${C_RESET}"
M_WARN="${C_YELLOW}!${C_RESET}"
M_FAIL="${C_RED}✗${C_RESET}"

ok()   { printf '%s %s\n' "$M_OK" "$*"; }
info() { printf '%s %s\n' "$M_INFO" "$*"; }
warn() { printf '%s %s\n' "$M_WARN" "$*"; }
fail() { printf '%s %s\n' "$M_FAIL" "$*"; }
hr()   { printf '%s\n' "------------------------------------------------------------"; }
section() {
  printf '\n%s%s== %s ==%s\n' "$C_BOLD" "$C_BLUE" "$*" "$C_RESET"
}

# Run a bluey subcommand, echoing the command first and its output indented.
# Returns the command's own exit status. Never aborts the script (callers decide
# how to react), so a single failing inspection does not kill the whole run.
run_bluey() {
  printf '%s$ bluey %s%s\n' "$C_BOLD" "$*" "$C_RESET"
  local status=0
  # Indent output so it is visually distinct from the script's own lines.
  "$BLUEY_BIN" "$@" 2>&1 | sed 's/^/    /' || status=${PIPESTATUS[0]}
  return "$status"
}

# Capture a bluey subcommand's combined output into a variable (for parsing),
# without the indentation/echo that run_bluey adds. Returns the exit status.
capture_bluey() {
  "$BLUEY_BIN" "$@" 2>&1
}

# ---------------------------------------------------------------------------
# Resolve paths.
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BLUEY_BIN="$REPO_ROOT/target/release/bluey"

# Make sure cargo (and any agent CLIs installed via cargo) are on PATH.
export PATH="$HOME/.cargo/bin:$PATH"

printf '%s%sBluey agent-context-bridge smoke test%s\n' "$C_BOLD" "$C_BLUE" "$C_RESET"
hr
info "Repo root:   $REPO_ROOT"
info "Binary path: $BLUEY_BIN"
warn "This will start the Bluey daemon (a background process) and an overlay."
warn "It is stopped automatically at the end (and on Ctrl-C / errors)."
hr

# ---------------------------------------------------------------------------
# 1. Locate or build the binary.
# ---------------------------------------------------------------------------
section "Step 1: locate the bluey binary"
if [[ -x "$BLUEY_BIN" ]]; then
  ok "Found existing release binary."
else
  info "No release binary at target/release/bluey — building it now."
  info "Running: cargo build --release -p cue-cli --bin bluey"
  if ( cd "$REPO_ROOT" && cargo build --release -p cue-cli --bin bluey ); then
    ok "Build complete."
  else
    fail "Build failed. Fix the build, then re-run this script."
    exit 1
  fi
  if [[ ! -x "$BLUEY_BIN" ]]; then
    fail "Build reported success but $BLUEY_BIN is missing or not executable."
    exit 1
  fi
fi

# ---------------------------------------------------------------------------
# 2. Detect installed agent CLIs BEFORE starting the daemon.
#
#    Map of probe binary -> Bluey agent kind (the snake_case label that
#    `bluey agent list` prints in its KIND column and that `attach` expects).
#    These match cue-agent-bridge's registry binary_candidates.
# ---------------------------------------------------------------------------
section "Step 2: detect installed coding-agent CLIs"
info "Bluey discovers agents by their CLI on PATH (a 'drive'-capable agent)."
info "Checking which agent CLIs you have installed:"

# Parallel arrays: probe binary, the Bluey kind it implies, a friendly label.
PROBE_BINS=(claude   cursor-agent copilot gemini codex aider)
PROBE_KINDS=(claude_code cursor   copilot gemini codex aider)
PROBE_NAMES=("Claude Code" "Cursor" "GitHub Copilot" "Gemini" "Codex" "Aider")

# Collect the kinds that look drivable from this machine's PATH.
INSTALLED_KINDS=()
INSTALLED_ANY=0
for i in "${!PROBE_BINS[@]}"; do
  bin="${PROBE_BINS[$i]}"
  kind="${PROBE_KINDS[$i]}"
  name="${PROBE_NAMES[$i]}"
  if path="$(command -v "$bin" 2>/dev/null)"; then
    ok "$name — '$bin' found at $path  (kind: $kind)"
    INSTALLED_KINDS+=("$kind")
    INSTALLED_ANY=1
  else
    info "$name — '$bin' not installed"
  fi
done

if [[ "$INSTALLED_ANY" -eq 1 ]]; then
  info "→ 'bluey agent list' should show the above as 'drive'-capable agents."
else
  warn "No agent CLIs detected on PATH."
  warn "'bluey agent list' may show read-only agents (from config/session"
  warn "stores) but nothing drivable. Attach/ask steps will be skipped."
fi

# ---------------------------------------------------------------------------
# 3. Cleanup trap — guarantees `bluey off` runs no matter how we exit.
# ---------------------------------------------------------------------------
DAEMON_STARTED=0
cleanup() {
  local exit_code=$?
  # Restore default handlers so a second Ctrl-C during cleanup exits immediately.
  trap - EXIT INT TERM
  printf '\n'
  section "Cleanup: stopping Bluey"
  if [[ "$DAEMON_STARTED" -eq 1 ]]; then
    info "Running: bluey off"
    if "$BLUEY_BIN" off 2>&1 | sed 's/^/    /'; then
      ok "Bluey stopped."
    else
      fail "'bluey off' reported an error. Check 'bluey status' and stop it manually if needed."
    fi
  else
    info "Daemon was never started by this script; nothing to stop."
  fi
  exit "$exit_code"
}
trap cleanup EXIT INT TERM

# ---------------------------------------------------------------------------
# 4. Start the daemon and wait until it responds.
#
#    `bluey on` spawns the daemon (and overlay) detached and returns promptly —
#    it does NOT block — so we call it directly, then poll `bluey status`.
# ---------------------------------------------------------------------------
section "Step 3: start the Bluey daemon"
info "Running: bluey on   (starts daemon + overlay in the background)"
if "$BLUEY_BIN" on 2>&1 | sed 's/^/    /'; then
  DAEMON_STARTED=1
  ok "'bluey on' returned."
else
  # `bluey on` can return non-zero if the overlay isn't reachable yet while the
  # daemon itself came up. Treat the daemon as possibly-started and let the
  # status poll below decide.
  DAEMON_STARTED=1
  warn "'bluey on' returned a non-zero status (overlay may be unavailable on this host)."
  warn "Continuing to poll daemon status anyway."
fi

info "Waiting for the daemon to answer 'bluey status' (timeout ~15s)..."
DAEMON_READY=0
for attempt in $(seq 1 15); do
  if "$BLUEY_BIN" status >/dev/null 2>&1; then
    DAEMON_READY=1
    ok "Daemon is up (after ${attempt}s)."
    break
  fi
  printf '    .'
  sleep 1
done
printf '\n'

if [[ "$DAEMON_READY" -ne 1 ]]; then
  fail "Daemon did not become ready within 15s."
  fail "Aborting the agent test sequence. (Cleanup will still run 'bluey off'.)"
  exit 1
fi

# ---------------------------------------------------------------------------
# 5. Agent test sequence.
# ---------------------------------------------------------------------------
section "Step 4: agent bridge test sequence"

# 5a. List discovered agents.
info "Listing discovered agents."
AGENT_LIST_OUTPUT=""
if AGENT_LIST_OUTPUT="$(capture_bluey agent list)"; then
  printf '%s$ bluey agent list%s\n' "$C_BOLD" "$C_RESET"
  printf '%s\n' "$AGENT_LIST_OUTPUT" | sed 's/^/    /'
  ok "agent list succeeded."
else
  printf '%s$ bluey agent list%s\n' "$C_BOLD" "$C_RESET"
  printf '%s\n' "$AGENT_LIST_OUTPUT" | sed 's/^/    /'
  fail "agent list failed (see output above). Continuing."
fi

# 5b. Choose an agent to attach.
#     Preference order: claude_code, then gemini, then any other installed kind.
#     We only attach a 'drive'-capable kind that BOTH (a) we detected on PATH and
#     (b) the live `agent list` reports with capability 'drive'.
choose_agent() {
  # Build the set of kinds the daemon reports as drivable, parsed from the table.
  # Each data row looks like:  "<kind>[*]  <capability>  <conns>  <sessions>"
  # We strip the trailing '*' attached-marker before comparing.
  local drivable_from_list=()
  if [[ -n "$AGENT_LIST_OUTPUT" ]]; then
    while IFS= read -r line; do
      # Skip the header row, blank lines, and the legend line (which starts
      # with the '*' attached-marker), keeping only agent data rows.
      case "$line" in
        KIND*|''|\**) continue ;;
      esac
      # Field 1 = kind (maybe with trailing '*'), field 2 = capability.
      local k cap
      k="$(awk '{print $1}' <<<"$line")"
      cap="$(awk '{print $2}' <<<"$line")"
      k="${k%\*}"
      if [[ "$cap" == "drive" && -n "$k" ]]; then
        drivable_from_list+=("$k")
      fi
    done <<<"$AGENT_LIST_OUTPUT"
  fi

  # Helper: is $1 present in the drivable-from-list set?
  list_has() {
    local needle="$1" item
    for item in "${drivable_from_list[@]:-}"; do
      [[ "$item" == "$needle" ]] && return 0
    done
    return 1
  }

  # Preferred order, intersected with what we detected on PATH.
  local preferred=(claude_code gemini cursor codex copilot aider)
  local kind
  for kind in "${preferred[@]}"; do
    # Must be installed on PATH (Step 2) ...
    local installed=0 ik
    for ik in "${INSTALLED_KINDS[@]:-}"; do
      [[ "$ik" == "$kind" ]] && installed=1 && break
    done
    [[ "$installed" -eq 1 ]] || continue
    # ... and, if the live list was parseable, drivable there too.
    if [[ "${#drivable_from_list[@]}" -gt 0 ]]; then
      list_has "$kind" || continue
    fi
    printf '%s' "$kind"
    return 0
  done

  # Fallback: nothing matched our preference list, but the live table may still
  # report something drivable we did not probe for. Use the first such kind.
  if [[ "${#drivable_from_list[@]}" -gt 0 ]]; then
    printf '%s' "${drivable_from_list[0]}"
    return 0
  fi

  return 1
}

CHOSEN_KIND=""
if CHOSEN_KIND="$(choose_agent)"; then
  ok "Selected agent to drive: ${C_BOLD}${CHOSEN_KIND}${C_RESET}"
else
  CHOSEN_KIND=""
fi

if [[ -z "$CHOSEN_KIND" ]]; then
  warn "No drivable coding agent found."
  warn "Install one of: claude (Claude Code), gemini, cursor-agent, codex, copilot, aider."
  warn "Skipping attach / status / connectors / sessions / ask."
else
  # 5c. Attach.
  info "Attaching '$CHOSEN_KIND'."
  if run_bluey agent attach "$CHOSEN_KIND"; then
    ok "Attached $CHOSEN_KIND."
  else
    fail "Attach failed. Skipping the agent-routed steps."
    CHOSEN_KIND=""
  fi
fi

if [[ -n "$CHOSEN_KIND" ]]; then
  # 5d. Agent status.
  info "Confirming attach state."
  if run_bluey agent status; then
    ok "agent status reported."
  else
    fail "agent status failed. Continuing."
  fi

  # 5e. Connectors (inherited MCP connectors — name/tier/ready only).
  info "Listing inherited MCP connectors for '$CHOSEN_KIND'."
  if run_bluey agent connectors "$CHOSEN_KIND"; then
    ok "agent connectors reported."
  else
    fail "agent connectors failed. Continuing."
  fi

  # 5f. Sessions (consent-gated).
  info "Listing prior sessions for '$CHOSEN_KIND'."
  warn "NOTE: sessions are gated behind the 'allow_agent_session_history' setting."
  warn "      If you have not opted in, this will print 'No sessions found' — that"
  warn "      is expected, not a failure."
  if run_bluey agent sessions "$CHOSEN_KIND"; then
    ok "agent sessions reported (may be empty due to the consent gate)."
  else
    fail "agent sessions failed. Continuing."
  fi

  # 5g. Ask — routes through the attached agent.
  info "Asking a question routed through the attached agent."
  warn "This invokes '$CHOSEN_KIND' headlessly and may take several seconds."
  if run_bluey ask "what is 2+2? reply with just the number"; then
    ok "ask returned an answer (shown above)."
  else
    fail "ask failed. The agent CLI may need auth, or driving may be unavailable."
  fi

  # 5h. Detach.
  info "Detaching the agent."
  if run_bluey agent detach; then
    ok "Detached. Answers would now use Bluey's normal providers."
  else
    fail "Detach failed. You can run 'bluey agent detach' manually."
  fi
fi

# ---------------------------------------------------------------------------
# 6. Final summary. (Cleanup / `bluey off` runs after this via the EXIT trap.)
# ---------------------------------------------------------------------------
section "Summary"
if [[ -n "$CHOSEN_KIND" ]]; then
  ok "Smoke test complete: discovered, attached '$CHOSEN_KIND', inspected, asked, detached."
else
  ok "Smoke test complete: daemon came up and 'agent list' ran."
  info "No drivable agent was available, so attach/ask were skipped — install a"
  info "coding-agent CLI (e.g. 'claude' or 'gemini') and re-run to test the full path."
fi
info "Stopping Bluey now (via cleanup trap)..."
