# Codex -> Kiro: Stage 12-17 Follow-Up Implementation

## 1. Overall Verdict

🟡 **IMPLEMENTED, READY FOR KIRO REVIEW** — I finished the highest-priority missing pieces around the latest server/customer loop instead of starting a new architecture branch:

- live wallet balance now reaches the native overlay and dashboard;
- manual audio stop refreshes final balance, matching the existing 5-minute no-transcript auto-stop behavior;
- SMTP verification/reset delivery is real when `BLUEY_SMTP_*` is configured;
- the Stage 12-17 recap/review doc is now in `docs/rounds/`.

## 2. What Codex Changed

### Live balance path

- `crates/cue-daemon/src/cloud/balance.rs`
  - Added `BalanceWatch::publish()` so manual balance refreshes and poll-loop refreshes feed the same subscribers.
  - Updated module docs: overlay/dashboard consumption is now implemented, not a future stage.
- `crates/cue-daemon/src/app.rs`
  - Added `balance_watch` to `Daemon`.
  - Spawns balance polling when a Bluey token is already in keyring.
  - Bridges `BalanceWatch` snapshots into `OverlayCommand::SetBalance`.
  - Publishes manual refresh snapshots.
  - `AudioStop` now refreshes balance after stopping.

### Dashboard balance UI

- `crates/cue-dashboard/src/commands.rs`
  - Added `get_balance_snapshot`.
- `crates/cue-dashboard/src/lib.rs`
  - Registered the new Tauri command.
- `crates/cue-dashboard/ui/src/components/BalanceIndicator.tsx`
  - New top-right dashboard balance pill with low-balance and auto-top-up state.
- `crates/cue-dashboard/ui/src/components/DashboardLayout.tsx`
  - Mounts the balance indicator.

### Transactional email

- `server/src/config.rs`
  - Added optional `SmtpConfig` and env parsing for `BLUEY_SMTP_HOST`, `BLUEY_SMTP_PORT`, `BLUEY_SMTP_USERNAME`, `BLUEY_SMTP_PASSWORD`, `BLUEY_SMTP_FROM`, `BLUEY_SMTP_STARTTLS`.
- `server/src/mail.rs`
  - New `lettre`-backed verification/password-reset email delivery.
  - Returns `NotConfigured` in dev mode instead of pretending mail sent.
- `server/src/api/auth_routes.rs`
  - Verification start now sends email when SMTP is configured.
  - Password reset start sends email when SMTP is configured but still always returns `202` to avoid account enumeration.
- `server/Cargo.toml` / `Cargo.lock`
  - Added `lettre`.
- `server/README.md`, `docs/OPERATIONS-RUNBOOK.md`, `server/src/db/auth_tokens.rs`
  - Docs updated for real SMTP.

### Review docs

- `docs/rounds/STAGES-12-17-RECAP-FOR-CODEX-REVIEW.md`
  - New consolidated review entry point for Stage 12-17 plus this follow-up implementation.

## 3. What Was Already Present And Verified

- The 5-minute no-transcript auto-stop already existed in `real_audio_loop` and `maybe_auto_stop_idle_audio`.
- Default timeout is `DEFAULT_AUDIO_IDLE_STOP_SECS = 5 * 60`.
- On auto-stop, Bluey stops recording, refreshes balance, and pushes a system card with final balance.
- Native macOS overlay already supports `SetBalance`, context chips, transcript ticker, one-row composer controls, new-session/continue events, and attached file chips.

## 4. Remaining Gaps

- Per-card cost label is still not complete. Server returns `cost_cents`, but `cue-llm::LlmResponse`, `CueResponse`, DB persistence, and UI cards do not yet carry that metadata end-to-end.
- `/router/complete/stream` SSE proxy is still pending; managed provider stream still wraps a single complete call.
- SMTP needs a production smoke test with real provider credentials.
- `BalanceWatch` starts only if tokens exist at daemon startup or if a manual refresh path runs; login-during-running-daemon can be tightened later.
- Wiremock coverage for OpenAI/Deepgram/Stripe/SMTP remains pending.

## 5. What Codex Did Not Change

- No push.
- No history rewrite.
- No product naming or white-label wording changes.
- No Windows implementation changes.
- No new pricing/product decisions.

## 6. Verification

Run by Codex in this pass:

```bash
cargo fmt --all
cargo fmt --all --check
cargo check -p cue-daemon -p cue-dashboard --all-targets
cd server && cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cd server && cargo clippy --all-targets -- -D warnings
cargo build --all-targets --release
cargo test --all-targets
cd server && cargo test
cd server && cargo build --all-targets --release
cd crates/cue-dashboard/ui && npm test
cd crates/cue-dashboard/ui && npm run build
swift build -c release --package-path native/macos/cue-overlay
swift build -c release --package-path native/macos/cue-whisper
git -P diff --check
git -P diff --check main..HEAD
```

Observed counts:

- Workspace `cargo test --all-targets`: 406 passed, 14 ignored.
- Server `cargo test`: 58 passed.
- Dashboard Vitest: 15 passed.

## 7. Next Action For Kiro

Review these first:

1. `docs/rounds/STAGES-12-17-RECAP-FOR-CODEX-REVIEW.md`
2. `docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md`
3. Diff for:
   - `crates/cue-daemon/src/app.rs`
   - `crates/cue-daemon/src/cloud/balance.rs`
   - `crates/cue-dashboard/src/commands.rs`
   - `crates/cue-dashboard/ui/src/components/BalanceIndicator.tsx`
   - `server/src/config.rs`
   - `server/src/mail.rs`
   - `server/src/api/auth_routes.rs`

Recommended next implementation round:

```text
Stage 18:
1. Propagate managed response billing metadata into LlmResponse -> CueResponse -> DB -> overlay/dashboard cards.
2. Add /router/complete/stream SSE proxy and BlueyManagedProvider streaming consumer.
3. Add wiremock tests for SMTP/Stripe/OpenAI/Deepgram production-style paths.
4. Tighten balance watcher startup after login without requiring daemon restart.
```
