# REVIEW: Server Stage 8 — `bluey usage` and `bluey credits`

**Commit:** `92467dc feat(cli): bluey usage + bluey credits commands (Stage 8)`  
**Reviewer:** Codex  
**Date:** 2026-05-19

## Per-Task Review

### Stage 8 — Customer CLI Surface

| Field | Value |
|-------|-------|
| Files | `crates/cue-cli/src/app.rs`, `crates/cue-cli/src/bluey_cmds.rs`, `crates/cue-cloud-client/src/client.rs`, `crates/cue-cloud-client/src/tokens.rs` |
| Verdict | 🔴 blocker |

**Findings:**

- 🔴 `crates/cue-cli/src/app.rs:730` / `crates/cue-cli/src/app.rs:2263` — the visible `bluey login` command writes legacy `AccountConfig`, while `bluey usage` and `bluey credits` only read `cue-cloud-client` keyring tokens. A normal customer can run `bluey login` successfully and still get “not logged in” from the new commands. Bridge login to `CloudClient::save_tokens()`, or add a separate explicit command name for the new device/keyring login flow and update the messages.
- 🔴 `crates/cue-cli/src/bluey_cmds.rs:76` — `bluey credits` is documented as “Show credit-batch expiration info” in `crates/cue-cli/src/app.rs:57`, but only prints a placeholder because there is no `/account/credits` endpoint. That is acceptable as a deferred feature only if the CLI help is softened; as written, the command promises data it cannot show.
- 🟡 `crates/cue-cli/src/bluey_cmds.rs:52` — the tier comparison table hard-codes pricing projections from `docs/PRICING-MODEL.md`. Given Stage 4 currently bills whole cents, those projections are already inconsistent. Prefer a `/pricing/tiers` or `/account/usage` field that the server owns.
- 🟡 `crates/cue-cloud-client/src/client.rs:217` — non-402 server error bodies are preserved verbatim in `Error::Server` and printed by CLI callers. Sanitize production server messages before surfacing them to customers.

## Cross-Task Findings

- Stage 8 cannot be evaluated independently of Stage 4/7 pricing and usage semantics because it displays those numbers directly to customers.

## Build & Test Verification

```bash
cargo test -p cue-cloud-client --lib   # ✅ 4 passed
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Customer login/token storage and command promises must be aligned before these commands are usable in alpha.

## Follow-ups for Next Batch

- Add a CLI integration test with an in-memory/mock cloud server proving `bluey login` populates the same token store consumed by `bluey usage`.
