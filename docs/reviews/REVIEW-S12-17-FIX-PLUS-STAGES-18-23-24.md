# REVIEW: S12-17 Fix Wave + Stages 18, 23, 24

**Commit range:** `60ad3e4..ac37b1d` plus Codex fixes on top
**Reviewer:** Codex
**Date:** 2026-05-21

## Per-Task Review

### S12-17 Fix Wave

| Field | Value |
|-------|-------|
| Files | `crates/cue-router/src/speculative.rs`, `server/src/main.rs`, `server/src/api/account.rs`, `server/src/api/auth_routes.rs`, `server/src/pricing/mod.rs`, `crates/cue-daemon/src/cloud/balance.rs` |
| Verdict | 🟢 accept after Codex fix |

**Findings:**
- 🟢 The all-lanes-failed path now returns no answer instead of an empty success card.
- 🟢 `axum::serve` now installs `ConnectInfo<SocketAddr>` in the real production serve path.
- 🟢 Embed and Deepgram pricing unit conversions are corrected and covered by tests.
- 🟢 Password reset now validates/hashes the new password before consuming the reset token.
- 🔴 Found and fixed during review: `/account/delete` had the correct GDPR cleanup logic in tests, but the real handler used unquoted SQLite JSON paths and ignored the SQL error. The handler now uses quoted JSON paths and treats cleanup failure as a hard 500 before deleting the account.
- 🟢 BalanceWatch now has a shutdown-aware loop variant.

---

### Stage 18 — Deep-Link Onboarding, Invisibility, Disguise

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/src/lib.rs`, `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/ui/src/pages/Onboarding.tsx`, `crates/cue-dashboard/ui/src/pages/Settings.tsx` |
| Verdict | 🟢 accept after Codex fix |

**Findings:**
- 🔴 Found and fixed during review: `crates/cue-dashboard/src/lib.rs` had two chained Tauri `.setup(...)` calls. Tauri stores only the last setup callback, so the deep-link handler, tray invisible/disguise menu, and meeting watch from the first callback were dead at runtime. The setup paths are now merged into the single effective callback.
- 🔴 Found and fixed during review: Stage 24 Settings called `account_me`, `billing_portal_url`, `sign_out`, and `delete_account_now`, while Onboarding called `get_signin_url` and `complete_onboarding`; none were registered Tauri commands. These are now implemented and registered.
- 🔴 Found and fixed during review: Onboarding imported `@tauri-apps/plugin-opener`, but the package was not installed. The UI now uses `window.open`, matching the existing Settings portal behavior.
- 🔴 Found and fixed during review: sign-out/delete-account cleared tokens but left `onboarding_complete=true` and navigated to a non-hash `/onboarding` path. The commands now mark onboarding incomplete and the UI reloads through the existing first-run gate.
- 🔴 Found and fixed during review: auto-disguise accept/decline only updated in-memory atomics despite the commit claiming durable `CueSettings` persistence. Accept/decline now persist to `CueSettings`, and `set_disguise` keeps the core `disguise_mode` in sync best-effort.
- 🟡 Remaining nit: `meeting_detect.rs` still shells out to `lsappinfo` and string-parses output. It is acceptable for the MVP, but should move to a direct `NSWorkspace.shared.frontmostApplication.bundleIdentifier` path before relying on this as a production signal.
- 🟡 Remaining QA item: the `tauri-plugin-deep-link` config is present and its build script handles macOS Info.plist generation, but `bluey://` still needs a bundled-app smoke on a clean Mac.

---

### Stage 23 — Wiremock E2E Harness

| Field | Value |
|-------|-------|
| Files | `server/tests/integration_e2e.rs`, `server/src/routing/dispatcher.rs`, `server/src/api/billing.rs`, `server/src/billing/topup.rs` |
| Verdict | 🟢 accept after Codex fix |

**Findings:**
- 🟢 Existing OpenAI completion, idempotency replay, and link mint/exchange tests are useful and run through the Axum router.
- 🔴 Found and fixed during review: `BLUEY_TEST_DEEPGRAM_URL` was set in the harness but ignored by `deepgram_transcribe`, so transcribe tests would have hit the real Deepgram URL. The dispatcher now honors the override and has a mock-backed transcribe e2e test.
- 🔴 Found and fixed during review: `BLUEY_TEST_STRIPE_URL` was set but billing still used hard-coded Stripe URLs. Checkout, portal, payment-intent retrieve, and auto-topup payment-intent create now route through the same override helper. Checkout has a mock-backed e2e test.
- 🟡 Remaining nit: the e2e harness still depends on the server crate from within the same package. That is fine for now, but a dedicated integration-test crate would be cleaner once this grows.
- 🟡 Remaining coverage follow-up: add wiremock cases for Stripe portal, webhook PaymentIntent retrieval, and auto-topup. The URL seam exists now; only coverage is missing.

---

### Stage 24 — Persistent Disguise Preferences + Settings Polish

| Field | Value |
|-------|-------|
| Files | `crates/cue-core/src/config.rs`, `crates/cue-dashboard/src/lib.rs`, `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/ui/src/pages/Settings.tsx` |
| Verdict | 🟢 accept after Codex fix |

**Findings:**
- 🟢 `CueSettings` has backward-compatible defaults for the new disguise fields.
- 🟢 Settings no longer exposes BYOK/STT API-key prompts in the managed-product UI.
- 🟢 Account, billing portal, sign-out, and delete-account buttons now have backend command implementations.
- 🟡 Minor product mismatch to decide later: `CueSettings::default().disguise_mode` is `"activity"`, while the dashboard DB fallback for `get_disguise` is still `"none"`. I did not change that default behavior in this review because it affects product visibility semantics.

## Cross-Task Findings

- The largest issue was integration drift between frontend additions and Tauri command/setup wiring. Compile-time checks did not catch it because `invoke(...)` command names are strings and Tauri accepts multiple `.setup(...)` calls by overwriting the earlier one.
- The original Stage 23 mock harness was directionally right but not yet proving every claimed upstream seam. Deepgram and Stripe are now partially covered; continue adding one e2e for each new upstream path as it lands.
- No process naming/product wording changes were made.

## Build & Test Verification

```bash
cargo fmt --all --check                                           # ✅
cargo clippy --all-targets -- -D warnings                         # ✅
cargo build --all-targets                                         # ✅
cargo build --all-targets --release                               # ✅
cargo test --all-targets                                          # ✅
cd crates/cue-dashboard/ui && npm test                            # ✅ 15 passed
cd crates/cue-dashboard/ui && npm run build                       # ✅
cd server && cargo fmt -- --check                                 # ✅
cd server && cargo clippy --all-targets -- -D warnings            # ✅
cd server && cargo build --release                                # ✅
cd server && cargo test                                           # ✅
swift build -c release --package-path native/macos/cue-overlay    # ✅
swift build -c release --package-path native/macos/cue-whisper    # ✅
git diff --check                                                  # ✅
```

## Overall Verdict

🟢 **ACCEPT** — after the Codex fixes in this review, the S12-17 fix wave plus Stages 18, 23, and 24 are mergeable.

## Follow-ups for Next Batch

- Clean-Mac bundled-app smoke for `bluey://` registration and the visible overlay/pill flow.
- Replace `lsappinfo` parsing with a direct NSWorkspace/AppKit frontmost-app query.
- Add wiremock coverage for Stripe portal, Stripe webhook PaymentIntent retrieval, and auto-topup.
- Decide product default for first-run disguise mode: core default `"activity"` vs dashboard fallback `"none"`.
