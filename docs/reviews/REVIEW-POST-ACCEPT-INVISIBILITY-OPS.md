# REVIEW: Post-Acceptance Invisibility + UI Wiring + Ops

**Commit range:** `e064e46..9aeec02`
**Reviewer:** Codex
**Date:** 2026-05-21

## Per-Task Review

### S18/S24 Follow-Up Closures

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/cloud/meeting_detect.rs`, `crates/cue-dashboard/src/commands.rs` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 `lsappinfo` shell parsing is gone. macOS meeting detection now uses `NSWorkspace.sharedWorkspace().frontmostApplication()` through `objc2-app-kit`.
- 🟢 `get_disguise` now defaults to `activity`, matching the product decision recorded in `CueSettings::default()`.

---

### Invisibility + Dashboard Window Behavior

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/Info.plist`, `crates/cue-dashboard/tauri.conf.json`, `crates/cue-dashboard/src/lib.rs`, `crates/cue-dashboard/src/macos.rs`, `crates/cue-dashboard/src/commands.rs` |
| Verdict | 🟢 accept after Codex fix |

**Findings:**
- 🟢 Dashboard windows are excluded from screen capture on startup with `NSWindowSharingNone`, matching the native overlay approach.
- 🟢 `LSUIElement=true` makes the app menu-bar-only as intended.
- 🔴 Found and fixed during review: startup still defaulted the applied disguise to `none`, and the tray icon was built with the Bluey logo even when the default/persisted mode was `activity`. The setup path now defaults to `activity` and reapplies the resolved disguise after the tray exists, so title + tray icon + persisted state line up at boot.
- 🟡 Found and fixed during review: with `LSUIElement=true`, `show()` + `set_focus()` can show a window without reliably making the app active. The dashboard show path now explicitly calls `NSApplication.activateIgnoringOtherApps(true)` on macOS before focusing the window.
- 🟢 Tray icon swapping uses embedded PNG bytes and Tauri's `image-png` feature, so it is independent of runtime bundle file paths.
- 🟡 Remaining QA item: verify the signed/unsigned app bundle on a clean Mac. Code builds locally, but actual Dock/Cmd-Tab/tray focus behavior still needs real desktop validation.

---

### UI Wiring

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/ui/src/App.tsx`, `crates/cue-dashboard/ui/src/components/AutoDisguiseToast.tsx`, `crates/cue-dashboard/ui/src/components/InvisibilityToast.tsx`, `native/macos/cue-overlay/Sources/cue-overlay/main.swift` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 `AutoDisguiseToast` subscribes to `auto_disguise_offer` and calls the persisted accept/decline commands added in the earlier fix wave.
- 🟢 `spawn_meeting_watch` checks the in-memory prompted/enabled flags loaded from `CueSettings`; after accept/decline it should not re-emit offers in the same process.
- 🟢 `InvisibilityToast` subscribes to `invisibility_changed` and provides brief state feedback.
- 🟢 The overlay pill now updates from `set_balance` instead of reserving balance text only for the expanded panel.
- 🟡 Minor UX note: ignored auto-disguise toasts auto-dismiss without persisting a decision, so a future distinct meeting-app detection may show it again. This is acceptable for alpha because explicit accept/decline is durable.

---

### Stage 23 Wiremock Coverage

| Field | Value |
|-------|-------|
| Files | `server/tests/integration_e2e.rs` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 `/billing/portal` now has a mock-backed Stripe e2e case for success and a 400 when no Stripe customer exists.
- 🟢 Auto-topup has a no-payment-method short-circuit e2e case with a strict `expect(0)` Stripe `payment_intents` mock.

---

### Operational Artifacts

| Field | Value |
|-------|-------|
| Files | `docs/PRODUCTION-DEPLOY-RUNBOOK.md`, `docs/PRELAUNCH-CHECKLIST.md`, `ops/Caddyfile.example`, `ops/bluey-api.service.example`, `ops/backup-bluey-db.sh` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 Caddyfile shape is appropriate for a single-host Caddy → bluey-server deployment and rewrites `X-Forwarded-For` consistently with `BLUEY_TRUSTED_PROXIES=127.0.0.1,::1`.
- 🟢 systemd hardening is reasonable for this single-binary service: unprivileged user, strict filesystem protection, narrowed write paths, private tmp/devices, and restart-on-failure.
- 🟢 The backup script uses SQLite `.backup`, rotates local snapshots, and fails loudly when configured off-site backup tooling is missing.
- 🟢 The runbook now backs up the old binary before replacement during rollout; it no longer relies on `ExecStartPre`, which would copy the already-replaced binary.
- 🟡 Local validation note: `caddy validate` and `systemd-analyze verify` were not run because those tools are not installed on this Mac. They remain target-host checks in the prelaunch gate.

## Cross-Task Findings

- The post-acceptance hardening work is code-complete for this branch. The only remaining gates are environment/operator gates: clean Mac bundle smoke, production DNS/TLS, Stripe live, SMTP, web pages, monitoring, and target-host Caddy/systemd validation.
- The branch had newer distribution commits above `9aeec02` when this review ran. They are not part of this requested range, except that the final verification ran against current branch state plus the Codex fix.

## Build & Test Verification

```bash
cargo fmt --all --check                                           # ✅
cargo clippy --all-targets -- -D warnings                         # ✅
cargo test --all-targets                                          # ✅
cargo build --all-targets --release                               # ✅
cd server && cargo test                                           # ✅
cd crates/cue-dashboard/ui && npm test                            # ✅ 15 passed
cd crates/cue-dashboard/ui && npm run build                       # ✅
swift build -c release --package-path native/macos/cue-overlay    # ✅
swift build -c release --package-path native/macos/cue-whisper    # ✅
bash -n ops/backup-bluey-db.sh                                    # ✅
sqlite3 + ops/backup-bluey-db.sh local backup smoke               # ✅
git diff --check                                                  # ✅
```

## Overall Verdict

🟢 **ACCEPT** — after the Codex startup/focus fix, the post-acceptance invisibility, UI wiring, and ops/prelaunch batch is ready for the operator gate.

## Follow-ups for Next Batch

- Run signed/unsigned app bundle smoke on a clean Mac: tray app focus, `bluey://` registration, dashboard capture exclusion, and tray icon boot state.
- Run `caddy validate` and `systemd-analyze verify` on the target Ubuntu host.
- Execute `docs/PRELAUNCH-CHECKLIST.md` against real production DNS, Stripe, SMTP, web pages, signing/distribution, and monitoring.
