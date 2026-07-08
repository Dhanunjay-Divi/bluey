# ROUND-417-CI-CLIPPY-CLEANUP

Date: 2026-07-07
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Keep the Bluey 0.1.92 sign-out/listen fail-closed release clean in CI after GitHub's strict clippy pass rejected style-only lints.

## What Changed

- Cleaned the daemon answer parsing helpers to satisfy newer clippy suggestions without changing behavior.
- Cleaned STT relay logging counters to use the current integer multiple helper.
- Added an explicit `too_many_arguments` allowance to the managed stream recovery helper because it intentionally carries provider, stream, request, timing, and failure context for recovery logging.
- Cleaned duplicate internal-disclosure guard trimming logic in the LLM answer helper.
- Added the missing Ubuntu ALSA development package to CI, observability, and release Linux dependency setup so audio crates can build on hosted runners.
- Moved the Bluey CLI billing tests below production items to satisfy the Rust 1.96 `items_after_test_module` lint in the full workspace clippy pass.
- Tightened platform-specific daemon `cfg` boundaries so Linux/Windows/macOS clippy do not see impossible native-helper, overlay-socket, or paste-helper branches as unused or unreachable.
- Marked the ffmpeg runtime path as intentionally unused on Linux because chunked desktop audio capture only reads it on macOS and Windows.
- Gated the CLI macOS permission-probe timer import and made the screen-capture preview argument platform-neutral so Ubuntu clippy does not see Mac/Windows-only code as unused.
- Corrected dashboard privacy-settings tests so Linux asserts the unsupported path while macOS and Windows continue to verify their launch commands.
- Split daemon audio imports so Linux test builds can use `AudioBackend` without importing Mac/Windows-only `AudioDeviceRole`.
- Gated the daemon keyring delete-missing test to desktop keyring platforms so headless Ubuntu CI does not fail on a missing keyring backend.
- Marked CPAL real-device resolution tests as ignored by default, matching the existing module contract that real audio-device tests are developer-machine checks. Hosted Windows passed clippy/build but crashed below Rust with a CPAL `STATUS_ACCESS_VIOLATION` during device enumeration, so CI now keeps pure audio logic tests active and leaves physical-device probing to `cargo test --ignored` on real machines.
- Aligned CI's Windows native-helper build step with the release workflow by wrapping each helper script in `Push-Location`/`Pop-Location`. The overlay helper intentionally changes to its script directory, so invoking the next helper through a repo-root relative path was fragile in PowerShell.
- Cleaned the server observability clippy gate after the hosted policy workflow started checking `server/Cargo.toml --all-targets`: grouped answer ops audit metadata into `AnswerOpsEvent`, grouped Square direct-payment inputs into `SquarePaymentRequest`, grouped balance credit inputs into `CreditWithSourceInput`, simplified duplicated AnswerPlan branches, and fixed small transcript/STT/source-count lint suggestions.
- Fixed the final observability policy blocker by hashing the cross-account artifact warning fields. The warning now emits `account_id_hash` and `object_key_hash` instead of raw account ids or R2 object keys.

## Verification

- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml signed_out_state_stops_active_audio_capture --quiet`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml listen_auth_gate --quiet`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml --lib audio::capture::tests --quiet`
- `cargo check -p cue-daemon -p cue-cli`
- `cargo check --manifest-path server/Cargo.toml --bin bluey-server`
- `cargo clippy -p cue-daemon --all-targets -- -D warnings`
- `cargo clippy --all-targets -- -D warnings`
- `cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings`
- `python3 scripts/analyze-tracing-calls.py --check-only`
- `cargo test --manifest-path server/Cargo.toml balance_ledger_records_credit_debit_and_request_evidence --quiet`
- `cargo test --manifest-path server/Cargo.toml credit_idempotent_on_same_processor_payment_id --quiet`
- `git diff --check`
- Hosted CI run `28913153344` passed macOS, Ubuntu, and Windows after the PowerShell path fix.
- Observability Policy run `28915521195` exposed one remaining raw `account_id` tracing field in the artifact-object cross-account warning. Local tracing policy and server clippy are now clean and the next pushed run should exercise the full policy gate itself.

## Notes

This round does not require a new desktop release by itself. The behavior-bearing signed-out listening fix is already published as 0.1.92 for macOS arm64 and Windows x86_64.
