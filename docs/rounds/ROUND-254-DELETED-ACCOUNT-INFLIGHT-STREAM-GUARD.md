# ROUND-254 Deleted Account Inflight Stream Guard

Date: 2026-06-30
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Fix the case where a user deletes an account while an answer or listen session is already running. New requests already fail after account deletion, but an already-accepted stream could keep producing deltas until the final billing step.

## Finding

The managed answer stream accepted an authenticated account snapshot at request start. If the account was hard-deleted after the request was accepted, the SSE stream could continue reading provider deltas because the stream loop did not re-check account liveness until the final billing/deduction path.

This did not bill another account. If the account row is gone, the final balance/usage write cannot charge that deleted account. The bad behavior was that Bluey could keep streaming and burn provider cost until the stream completed.

## Fixed

- Added live account state checks in managed chat routing:
  - before dispatch for `/router/complete/stream`
  - before every streamed SSE provider event
  - immediately before streaming billing
  - before dispatch for `/router/complete`
  - immediately before non-streaming billing
- If the account is deleted mid-stream, Bluey now emits:
  - `reason: account_deleted`
  - `This Bluey account was deleted. The answer was stopped and was not billed.`
- If the account is billing-restricted mid-stream, Bluey now stops before further output/billing.
- If Bluey cannot verify account liveness, it stops instead of risking a wrong charge.
- Hardened STT relay liveness too:
  - checks the account before forwarding client audio to Deepgram
  - checks the account before forwarding Deepgram transcript messages back to the client
  - skips STT settlement if the account was deleted while the relay was open

## Verification

Passed locally:

```bash
cargo fmt --manifest-path server/Cargo.toml
cargo check --manifest-path server/Cargo.toml --bin bluey-server
cargo test --manifest-path server/Cargo.toml stt -- --nocapture
cargo test --manifest-path server/Cargo.toml account_delete_requires_typed_delete_and_credit_loss_consent -- --nocapture
cargo test --manifest-path server/Cargo.toml auth_device -- --nocapture
git diff --check
```

Deployed the updated Linux server binary to:

```text
/usr/local/bin/bluey-server
```

Live health:

```text
https://bluey.sh/health -> status=ok
```

Live stream/delete smoke:

```text
email=codex-stream-delete-1782837633@bluey.local
request_id=codex-stream-delete-1782837633
delete_status=200
curl_code=0
exists_after=0
```

Stream output ended with:

```text
event: error
data: {"error":"This Bluey account was deleted. The answer was stopped and was not billed.","reason":"account_deleted"}
```

Journal confirmed:

- request accepted
- route selected
- account deleted
- stream stopped because account is no longer active
- no `managed chat completed and billed` line for that request

## User-Facing Answer

If the user deletes an account while a response is already streaming, Bluey now stops that stream and does not emit the final billing event. Before this fix, the already-open stream could continue visually, but the deleted account could not be charged after its account row was gone.
