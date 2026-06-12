# Kiro Line-by-Line Review — codex/bluey-ai-site (8ea42ac..a29d2f1)

**Branch:** `codex/bluey-ai-site`
**Range reviewed:** `8ea42ac..a29d2f1` (153 commits, 192 files, +24,485 / -3,348)
**Reviewer:** Kiro
**Date:** 2026-06-12
**Method:** Read on uno only; nothing pulled/run locally; no subagents.

---

## Verdict

🟢 **ACCEPT** — with **one elevated-risk posture item to flag** (auto-update
shipped without the signed release manifest that was the documented P0).
No blocking bugs found. Pipeline fully green. Money, auth, and streaming
paths are all sound on close reading.

---

## Pipeline baseline at HEAD (`a29d2f1`)

```
cargo fmt --all --check                          CLEAN
cargo clippy --all-targets -- -D warnings        CLEAN (workspace + server)
cargo test --all-targets                         519 passed, 0 failed
cd server && cargo test                          156 passed, 0 failed
scripts/analyze-tracing-calls.py --check-only     exit 0 (0 transitional, 0 PII)
scripts/observability-acceptance-smoke.sh         8/8 PASS
```

Test growth since I last saw the branch: workspace 483 → 519 (+36),
server 90 → 156 (+66). Real coverage growth, not padding.

Observability work survived intact: `doctor.rs`, `logs.rs`, `support.rs`,
`macos_perms.rs`, the analyzer, the CI workflow, and the acceptance smoke
are all present and passing.

---

## What I read line-by-line (highest risk first)

### 1. Payments — Square added alongside Stripe — 🟢 SOUND

`server/src/api/billing.rs` (+650), `7336be4`.

- Square is a **second** provider, not a Stripe migration. Both webhook
  paths (`stripe_webhook_impl`, `square_webhook_impl`) coexist.
- **Signature verification** (`verify_square_signature`): HMAC-SHA256 over
  `notification_url + body`, base64-STANDARD, constant-time compare via
  `subtle::ConstantTimeEq`, length-check before compare. Matches Square's
  documented webhook signature scheme. Correct.
- **Environment guard**: the impl tries each configured signing key
  (sandbox + production), determines which environment matched, then
  **rejects if the matched environment != active billing environment**.
  This blocks a sandbox webhook from crediting the production ledger.
  Strong defense.
- **Idempotency**: the real protection is `balance::credit(..., Some(charge_id))`
  with `charge_id = "square:{payment_id}"`, backed by migration 0008's
  partial `UNIQUE INDEX ON credit_batches(stripe_charge_id) WHERE NOT NULL`.
  Square ids share that column, so a concurrent double-fire fails the
  second INSERT → tx error → 500 → Square retries → second time the
  dedupe SELECT finds the row → `Ok(false)`. **Double-credit is impossible.**
- Credit extraction handles both `order.updated` (COMPLETED orders) and
  `payment.updated/created` (COMPLETED payments), pulls account id from
  `metadata.bluey_account_id` or `reference_id`, amount from metadata or
  `total_money/amount`. Account-id hashed in logs.

### 2. Real managed LLM streaming — 🟢 SOUND

`server/src/api/router.rs` (+2014), `crates/cue-llm/src/bluey_managed.rs`
(+568), `server/src/routing/dispatcher.rs` (+1246). `a56592b` + `411b01a`.

Replaces the earlier synthesized-SSE with true upstream token streaming.
The billing/idempotency lifecycle (which the synthesized approach was
chosen to protect) is preserved:

- **Bills after stream completes** on real token counts from the upstream
  usage trailer (`final_tokens`), not on synthesized estimates.
- **Incomplete stream → no charge**: if the provider ends without a
  terminal billing event, the customer is not billed; the
  `StreamingIdempotencyGuard` marks the reservation failed (if a delta
  was delivered) or releases it (if nothing delivered), so retry is clean.
- **`StreamingIdempotencyGuard`** with Drop semantics: releases the
  idempotency reservation if the stream task is dropped before billing,
  or preserves InProgress for manual reconciliation after billing.
  Both paths have unit tests (`streaming_idempotency_guard_releases_on_drop_before_billing`,
  `streaming_idempotency_guard_can_preserve_in_progress_after_billing`).
- **Overrun handling**: post-completion deduct failure logs "bluey absorbs
  overrun" rather than failing an already-delivered answer. Correct product
  call; per-request overrun is bounded to cents.
- **Pre-flight idempotency reserve** at request start: FreshReservation /
  CachedComplete (replays without re-billing) / InProgress.

### 3. Upstream spend guard — 🟢 SOUND

`release_and_upstream_spend_guard_check` in router.rs, `f0ab9f0`.

Operator circuit-breaker: before upstream dispatch, checks rolling-window
Bluey-cost spend (`usage::bluey_spend_cents_in_window`). If projected total
exceeds `BLUEY_UPSTREAM_SPEND_LIMIT_CENTS`, pauses managed dispatch with
503 + `retry_after_secs`. Fails closed on query error (500). Releases the
idempotency reservation when it blocks. Opt-in via env (None by default).
Right design for protecting against runaway cost during live tests.

### 4. Auth — OTP signup + legacy retirement — 🟢 SOUND

`server/src/api/auth_routes.rs` (+289), `244775c` + `3f59212`.

- `signup_start`: OTP hashed via HMAC keyed on `jwt_secret` over email+otp
  (never plaintext), TTL, attempts counter, `ON CONFLICT(email)` resets
  attempts, email hashed in logs, OTP only logged in dev-unconfigured mode,
  fails closed (503) if SMTP unconfigured in prod.
- `signup_confirm`: 6-digit format check, expiry+cleanup, attempts cap
  (`SIGNUP_OTP_MAX_ATTEMPTS` → 429), **constant-time hash compare**,
  attempts incremented on failure, account created only after OTP verified
  + marked `email_verified_at`, OTP row deleted (single-use), admin
  bootstrap via `is_admin_email`.
- Legacy `/auth/signup`: now a hard **410 GONE** stub redirecting to
  start/confirm. Route still mounted but no account-creation bypass.
- `login`: unchanged, non-enumerating ("invalid credentials" for both
  missing account and bad password).

### 5. Capture-visible release gate — 🟢 STRONGER THAN BEFORE

`native/macos/cue-overlay/Sources/cue-overlay/main.swift`, `a08e3a3`.

The capture-visible debug gate is now `#if DEBUG ... #else return false #endif`.
In a **release build, `captureVisibleForDebug` is compiled to always-false**
regardless of any env var or CLI flag. `sharingType` is therefore always
`.none` in shipped builds. Malware or a curious customer can no longer set
`BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1` to make the overlay screen-capturable.
This is a real improvement over the prior runtime gate — the screen-share
invisibility promise is now compile-time guaranteed in release.

### 6. Provider health + managed-capacity rate limiting — 🟢 SCALE-READY

`server/src/provider_health.rs` (+296 new), `server/src/rate_limit.rs` (+518).

- `provider_health`: 429-cooldown ledger so realtime paid requests route
  around a throttled provider/model/key instead of waiting. Redis-backed
  with local fallback + counters.
- `rate_limit`: per-account runaway-loop guardrails (LLM/embed/STT) +
  per-provider capacity limits, Redis-backed distributed with local
  fallback. Protects the shared managed provider allocation from a single
  abusive/buggy account.

---

## Elevated-risk item to flag (NOT a bug — a posture decision)

### A-1 ⚠️ Auto-update shipped without the signed release manifest (the documented P0)

`crates/cue-cli/src/update.rs` (+499 new), `c6a23c5`.

`maybe_update_before_on` runs on every `bluey on` by default (opt-out via
`BLUEY_SKIP_UPDATE`, skips dev builds), shows an ESC-to-skip window, then
auto-installs. The flow:

1. Fetch `https://bluey.sh/latest.json` over HTTPS.
2. Download `install.sh` from the manifest origin over HTTPS.
3. **Content sanity check only**: `text.starts_with("#!/")` AND
   `text.contains("Bluey one-line installer")` — trivially spoofable by
   anyone who controls the response; this is a "right kind of thing"
   check, NOT authentication.
4. `bash`-execute the downloaded script, passing the manifest-declared
   `BLUEY_ARTIFACT_SHA256`.
5. install.sh verifies the artifact against `BLUEY_ARTIFACT_SHA256`
   (or `SHA256SUMS.txt` fetched from the same host).

**Trust model = TLS + trust that `bluey.sh` hosting is not compromised.**
The sha256 provides ZERO protection against a compromised host because the
attacker who controls the host sets both the manifest sha256 AND the
artifact. It only protects against network MITM (TLS already does that) and
CDN corruption.

**Why this is worse than the manual `curl | bash` install:** it is
automatic, silent (brief ESC window), and runs on every launch — so a
compromised `bluey.sh` would push executed code to every customer
automatically.

This is exactly the gap I flagged at the Observability round-close as the
**P0 next-security-round item**: "Signed release manifest (ed25519) for safe
auto-update." Auto-update shipped before the signature landed.

**What's good in the implementation:** HTTPS enforced; dev-build guard;
`BLUEY_SKIP_UPDATE` opt-out; 10s timeout; relative artifact URL resolved
against manifest origin; best-effort stop-before-update; ESC-to-skip window.

**Recommendation:** prioritize the ed25519 signed-manifest work before any
wider/public alpha. Concretely:
- Sign `latest.json` (or a detached `latest.json.sig`) with an ed25519 key.
- Embed the public key in the CLI binary at build time.
- Verify the manifest signature in `check_for_update` before trusting any
  field; verify the artifact hash comes from the signed manifest.
- Until then, consider defaulting auto-update to **check-and-notify**
  (print "update available, run `bluey update`") rather than
  silent-auto-install, so code execution requires an explicit user action.

---

## Minor notes (non-blocking)

- **N-1** `router.rs` inner handlers log `request_id` (136 refs) but not
  `trace_id` (0 refs). Trace correlation works at the middleware boundary
  (`request received`/`request done` carry trace_id) and `request_id` is
  unique-per-request, so support can still correlate. This is the known
  deferred "broader call-site trace_id sweep" from Phase 5 close, not a
  regression.
- **N-2** Web site (`web/`, ~40 commits, +2443 CSS / +827 JS / index.html)
  is a static customer site (landing, account, install, link pages). I
  sanity-surveyed it but did not line-by-line review static HTML/CSS/JS —
  low security risk (no secrets, served static). Worth a separate visual
  QA pass, which is operator-side.
- **N-3** Square idempotency reuses the `stripe_webhook_events` table with
  a `square:` event-id prefix. Functionally correct (namespaced) but the
  table name is now a slight misnomer. Cosmetic; not worth a migration.

---

## What I did NOT do (per scope)

- Did not pull anything to local; all reads + builds were on uno.
- Did not run anything locally.
- Did not spawn subagents.
- Did not line-by-line review the ~40 web-site styling commits (surveyed
  only) or the ~13 docs commits (read titles + spot-checked).

---

## Bottom line

153 commits of high-quality work. The money path (Square + streaming
billing) is the part I scrutinized hardest and it is sound — idempotency
is enforced at the money-mutation layer with a DB unique-constraint
backstop, streaming bills on real token counts and never charges for
incomplete streams, and a Drop-guarded reservation keeps the lifecycle
consistent under failure. Auth OTP is properly built. The capture-visible
release gate is now compile-time guaranteed — stronger than before.

The one thing to surface to the human and to codex: **auto-update is live
and silent without the signed manifest that was the documented P0
security item.** That is the single highest-leverage thing to close before
a wider alpha. Everything else is green.
