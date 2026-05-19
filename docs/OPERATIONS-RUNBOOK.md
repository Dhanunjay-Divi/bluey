# Bluey Operations Runbook

> **Secrets, server layout, smoke tests, daily ops.**
> Mirrors Pinky's `OPERATIONS-RUNBOOK.md` shape but Bluey-only.
>
> Last updated: 2026-05-19.

This is the ops playbook. If something is on fire, this is the doc to
reach for.

---

## 1. Production state

| Layer | Status | Where |
|---|---|---|
| Layer 1 (local client) | shipping v0.1.0 | uno (dev), end-user laptops |
| Layer 2 (distribution) | not provisioned | TBD, see `SERVER-REFERENCE.md` |
| Layer 3 (product server) | not built | future, see `ARCHITECTURE.md` Section 5 |

When Layer 2/3 are stood up, fill in their details below + in
`SERVER-REFERENCE.md`.

---

## 2. Secrets inventory (target — none in production yet)

| Secret | Where it lives | Used by |
|---|---|---|
| User's Anthropic API key | macOS keyring under `llm_anthropic` | local daemon, `cue-llm` |
| User's OpenAI API key | macOS keyring under `llm_openai` | local daemon, `cue-llm` |
| `BLUEY_OVERLAY_SESSION_TOKEN` | env var, generated per-spawn | overlay IPC handshake |
| `BLUEY_LOCAL_ONLY` | env var, opt-in | force Auto Router to Local lane |
| `BLUEY_SPECULATIVE_ROUTING` | env var, default ON | toggle speculative draft+final |
| Stripe API keys (future, Layer 3) | server env file `/opt/bluey-api/env` | bluey-server |
| `BLUEY_JWT_SECRET` (future, Layer 3) | server env file | bluey-server auth |

**Rule:** never commit secret values to this repo. The `secrets`
module reads from keyring for client-side, env files for server-side.
Only the names of secrets appear in code or docs.

---

## 3. Smoke tests

### Client (run on uno or any test Mac)

```bash
cd /Users/uno/Downloads/cue
bash scripts/smoke-test.sh
# expected: "Bluey smoke test passed: daemon, overlay, transcript,
#            instructions, context, memory, audio scaffold, AI routing
#            scaffold, cloud scaffold, ask, action-items, recap, and
#            archive all worked."
```

### Installed-path (after `scripts/install.sh`)

```bash
tmp=$(mktemp -d)
BLUEY_ARCHIVE=dist/bluey-0.1.0-darwin-universal.tar.gz \
  BLUEY_INSTALL_DIR="$tmp/bluey" \
  BLUEY_BIN_DIR="$tmp/bin" \
  bash scripts/install.sh
"$tmp/bin/bluey" on
sleep 2
pgrep -fl "bluey-daemon|bluey-overlay" | head
"$tmp/bin/bluey" off
sleep 1
pgrep -fl "bluey-daemon|bluey-overlay" || echo "(none — clean stop)"
rm -rf "$tmp"
```

### Cue request end-to-end (with provider keys configured)

```bash
bluey on --title "smoke"
bluey listen --speaker user "what is the meaning of life?"
bluey ask "answer my question"
# expect a streaming response in the overlay + a cue_response_chunk event
bluey off
```

---

## 4. Restart / inspect / kill

### Local daemon stuck

```bash
# Find:
pgrep -fl "bluey-daemon|bluey-overlay"

# Stop cleanly:
bluey off

# Force-kill if `bluey off` hangs:
pkill -f bluey-daemon
pkill -f bluey-overlay-macos

# Inspect state file:
cat ~/.local/share/bluey/state.json   # macOS
# OR
cat ~/Library/Application\ Support/bluey/state.json
```

### Overlay shows but pill is unresponsive

1. Check `pgrep -fl bluey-overlay-macos` — if there are multiple, kill
   the stale ones.
2. `bluey off; bluey on` resets state.
3. If pill never appears after `bluey on`: check the daemon log under
   `~/Library/Application Support/bluey/logs/`. The daemon emits
   `discover_overlay_bin` errors clearly.

### Distribution server stuck (when one exists)

```bash
ssh <distribution-host>
sudo systemctl status nginx          # Path C
sudo systemctl status bluey-server   # Path A (Rust)
sudo journalctl -u bluey-server --since "5 min ago" --no-pager
# rollback by swapping the latest symlink (see DELIVERY-LIFECYCLE.md §5)
```

---

## 5. Common failure modes

| Symptom | Likely cause | Fix |
|---|---|---|
| `bluey on` hangs | overlay binary missing from install dir | reinstall via `scripts/install.sh` |
| `cue_response` never arrives | no LLM provider configured | `bluey set-stt-api-key openai sk-...` (or env var) |
| Pill appears but feed never opens on click | overlay process old, stale build | `bluey off`, rebuild, reinstall |
| `Refused to spawn overlay` in logs | binary verification failed | check `BLUEY_OVERLAY_BIN` env, see `crates/cue-daemon/src/overlay.rs::verify_overlay_binary` |
| Speculative dispatch never fires | env var disables it OR no providers configured | `BLUEY_SPECULATIVE_ROUTING=1` (default) + at least one provider key |

---

## 6. Building from source on a fresh Mac

```bash
# 1. Clone
git clone <repo-url> bluey
cd bluey

# 2. Toolchain
xcode-select --install   # if not present
brew install rustup-init && rustup-init -y
rustup target add aarch64-apple-darwin x86_64-apple-darwin
brew install node       # for the dashboard UI

# 3. Build
make package-darwin-arm64       # arm64-only
make package-darwin-universal   # arm64 + x86_64 lipo

# 4. Smoke
bash scripts/smoke-test.sh
```

First build is slow (~3-5 min). Subsequent builds use the cargo cache.

---

## 7. Where to look when something is wrong

1. `docs/PRODUCTION-READINESS.md` — what's actually shipping.
2. `DECISIONS.md` — historical decisions; the bug may be deliberate.
3. `docs/rounds/PHASE-3-ROUND-N-PLAN.md` — current-round work in progress.
4. `~/Library/Application Support/bluey/logs/` — daemon logs.
5. `docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md` — codex's last-known view.
