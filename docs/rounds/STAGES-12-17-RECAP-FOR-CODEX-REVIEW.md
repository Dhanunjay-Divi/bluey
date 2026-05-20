# Stages 12-17 End-to-End Recap

> **Branch:** `feat/phase-3-round-12`
> **Base stage range:** `05ca56d..c0ea5a8`
> **Status:** v0.2 managed-server loop is now much closer to production: embed, transcribe, account compliance, metrics, CLI account actions, and daemon balance polling are implemented. Codex follow-up on top wires balance into the UI/overlay and replaces the SMTP TODO with a real SMTP path.

## What landed in the Stage 12-17 push

```text
c0ea5a8 feat(daemon): Stage 17 daemon-side balance polling module
1511791 feat(cli):    Stage 16 bluey logout/portal/export/delete-account commands
42fd022 feat(server): Stage 15 /admin/metrics Prometheus exporter
c2cfde6 feat(server): Stage 14 GDPR delete + export + Stripe Customer Portal
d451852 feat(server): Stage 13 email verification + password reset endpoints
7882182 feat(server): Stage 12b real /router/transcribe (Deepgram nova-3)
241fa53 feat(server): Stage 12a real /router/embed (OpenAI text-embedding-3-small)
```

## Codex follow-up work on top

| Area | Status | Reviewer notes |
|---|---|---|
| Live wallet balance in overlay | ✅ implemented | `BalanceWatch` snapshots are bridged to native overlay `SetBalance`; manual stop and auto-stop refresh the balance. |
| Dashboard balance pill | ✅ implemented | Tauri command `get_balance_snapshot` plus React `BalanceIndicator` in the dashboard shell. |
| SMTP transactional email | ✅ implemented | `BLUEY_SMTP_*` config + `lettre` delivery for verification/reset; unconfigured dev mode still logs local URLs. |
| 5-minute no-transcript stop | ✅ already implemented | `real_audio_loop` auto-stops after `DEFAULT_AUDIO_IDLE_STOP_SECS = 300` and refreshes final balance. Codex verified the path and refreshed manual stop as well. |

## Prior blocker closure carried into this chain

| Finding | Status |
|---|---|
| S9 managed-only `request_cue` rejected pre-dispatch | ✅ closed |
| S9 managed registry fell back through arbitrary direct providers | ✅ closed |
| S9 speculative draft/final shared request id | ✅ closed |
| S9 all-lanes-failed returned empty success | ✅ closed |
| S11 untrusted `X-Forwarded-For` accepted | ✅ closed |
| S10 duplicate-email string match | ✅ closed with typed error |
| S4 `mark_complete` failure observability | ✅ closed via `/admin/metrics` estimate |

## Per-stage reviewer asks

### Stage 12a — `/router/embed`

- Confirm OpenAI `text-embedding-3-small` request shape and response parsing.
- Check cost estimation / deduction path and request id idempotency.
- Verify provider errors are sanitized before returning to clients.

### Stage 12b — `/router/transcribe`

- Confirm Deepgram `nova-3` request shape, auth, and content-type handling.
- Review raw audio byte limits and rough duration estimate for entry checks.
- Verify server usage events record real duration/cost after completion.

### Stage 13 — email verification + password reset

- Confirm tokens are single-use, hashed at rest, and expiry-bound.
- Confirm password reset start remains enumeration-resistant.
- Review new SMTP delivery path: secrets are env-only, no token is logged when SMTP is configured, and dev logging only happens when SMTP is absent.

### Stage 14 — GDPR export/delete + Stripe Customer Portal

- Check export completeness and secret redaction.
- Verify delete cascades all customer-owned data without deleting global pricing/config.
- Review Stripe Portal session creation and failure handling.

### Stage 15 — `/admin/metrics`

- Decide whether SQL-derived counters are acceptable for alpha.
- Check admin auth gating and Prometheus text formatting.
- Confirm `bluey_mark_complete_failures_estimated` is clearly labeled as estimated.

### Stage 16 — CLI account commands

- Review keyring token clearing in `bluey logout`.
- Check destructive confirmation for `bluey delete-account`.
- Verify portal/export commands handle auth failure and server errors cleanly.

### Stage 17 — balance polling

- Review watch-channel semantics, poll interval, and low-balance threshold.
- Confirm manual refresh publishes to the same watch channel as background polling.
- Confirm UI consumers do not show stale balance as authoritative after errors.

## Verification requested

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets --release
cargo test --all-targets
(cd server && cargo test)
(cd crates/cue-dashboard/ui && npm test && npm run build)
swift build -c release --package-path native/macos/cue-overlay
swift build -c release --package-path native/macos/cue-whisper
git -P diff --check main..HEAD
```

## Known pending work after this review

- `/router/complete/stream` SSE proxy through `bluey-server`.
- Per-card cost labels in the native overlay and dashboard. Server cost fields exist, but `cue-llm`/`cue_response` metadata propagation is still a separate data-contract change.
- Onboarding web pages on `bluey.sh`.
- SMTP provider production smoke with real credentials.
- Wiremock harness for Stripe, OpenAI, Anthropic, and Deepgram.
- SQLite backup rotation and deployment automation.
- Wider platform matrix validation.

## Suggested Codex/Kiro verdict shape

Write `docs/reviews/STAGES-12-17-CODEX-REVIEW.md` using `docs/work/TEMPLATE-REVIEW.md`.

Verdict options:

- 🟢 **ACCEPT** — merge and start streaming proxy + cost-label contract.
- 🟡 **ACCEPT WITH NITS** — merge and fold nits into the next managed-server/UI round.
- 🔴 **REQUEST CHANGES** — block on any billing, auth, SMTP-token, or balance-staleness issue.
