# REVIEW: Production Pre-Launch Follow-ups

**Commit range:** `e064e46..dd87ef0`
**Current verified tip:** `9aeec02`
**Reviewer:** Codex
**Date:** 2026-05-21

## Per-Task Review

### Follow-up Closures

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/cloud/meeting_detect.rs`, `crates/cue-dashboard/src/commands.rs`, `server/tests/integration_e2e.rs` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 `lsappinfo` parsing was replaced with a direct `NSWorkspace.sharedWorkspace().frontmostApplication()` bundle identifier query on macOS.
- 🟢 `get_disguise` now defaults consistently to `activity`, matching `CueSettings::default().disguise_mode`.
- 🟢 Wiremock e2e coverage now includes Stripe Customer Portal and the no-payment-method auto-topup skip path.

---

### Operational Artifacts

| Field | Value |
|-------|-------|
| Files | `docs/PRODUCTION-DEPLOY-RUNBOOK.md`, `docs/PRELAUNCH-CHECKLIST.md`, `ops/Caddyfile.example`, `ops/bluey-api.service.example`, `ops/backup-bluey-db.sh` |
| Verdict | 🟢 accept after closure commits |

**Findings:**
- 🟢 The Caddyfile is structurally sane for single-host deployment: Caddy terminates TLS, rewrites `X-Forwarded-For` from `{client_ip}`, and the runbook pairs that with `BLUEY_TRUSTED_PROXIES=127.0.0.1,::1`.
- 🟢 The systemd unit has reasonable single-host hardening: unprivileged user, strict system protection, private tmp/devices, write paths narrowed to app/log/backup directories, and restart-on-failure.
- 🟢 The SQLite backup script uses `.backup`, which is the right online-backup API for a live SQLite database.
- 🟢 Follow-up commit `c998ad9` fixed the rollback bug I would have blocked on: previous-binary backup now happens before replacing `/usr/local/bin/bluey-server`, not in `ExecStartPre`.
- 🟢 Follow-up commit `c998ad9` also tightened the backup script: off-host backup failures are no longer silently ignored, `stat` is portable across Linux/macOS, and the runbook installs `jq`/`gnupg` before using them.
- 🟡 Local validation note: `caddy validate` and `systemd-analyze verify` were not run on this Mac because those tools are not installed here. The artifacts are ready for clean Ubuntu validation during the operator deploy pass.

---

### Current-Tip Dashboard Closures

| Field | Value |
|-------|-------|
| Files | `Cargo.lock`, `crates/cue-dashboard/Cargo.toml`, `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/src/lib.rs`, `crates/cue-dashboard/tauri.conf.json` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 The current branch tip has additional UI/invisibility commits after the requested `dd87ef0` handoff. I included them in verification because they are now part of the branch being considered.
- 🟢 `tauri-build`/`tauri-utils` were updated and the Tauri `image-png` feature is enabled, closing the build-script and tray-icon decode failures that surfaced during `cargo test --all-targets` and `cargo clippy --all-targets`.
- 🟢 The tray icon swap path now compiles against the pinned Tauri image API.

## Cross-Task Findings

- The code side is in good shape for the next gate. Remaining risk is operational, not code: clean Ubuntu deploy, production DNS/TLS, Stripe live-mode, SMTP, signed/notarized macOS app, web pages, and monitoring.
- The extra commits after `dd87ef0` changed the actual branch under review. Future handoffs should include the current branch tip at the moment Codex starts review to avoid range drift.

## Build & Test Verification

```bash
cargo fmt --all --check                                           # ✅
cargo clippy --all-targets -- -D warnings                         # ✅
cargo test --all-targets                                          # ✅
cargo build --all-targets --release                               # ✅
cd server && cargo test                                           # ✅ 64 unit + 11 integration/doc tests
cd server && cargo build --release                                # ✅
cd crates/cue-dashboard/ui && npm test                            # ✅ 15 passed
cd crates/cue-dashboard/ui && npm run build                       # ✅
swift build -c release --package-path native/macos/cue-overlay    # ✅
swift build -c release --package-path native/macos/cue-whisper    # ✅
bash -n ops/backup-bluey-db.sh                                    # ✅
sqlite3 + ops/backup-bluey-db.sh local backup smoke               # ✅
git diff --check                                                  # ✅
```

## Overall Verdict

🟢 **ACCEPT** — code and repo artifacts are ready for the operator pre-launch gate.

## Follow-ups for Next Batch

- Run `caddy validate --config /etc/caddy/Caddyfile` on the target Ubuntu host after replacing the placeholder domain.
- Run `systemd-analyze verify /etc/systemd/system/bluey-api.service` on the target Ubuntu host.
- Execute the full `docs/PRELAUNCH-CHECKLIST.md` against real production DNS, Stripe, SMTP, signing, web pages, and monitoring.
