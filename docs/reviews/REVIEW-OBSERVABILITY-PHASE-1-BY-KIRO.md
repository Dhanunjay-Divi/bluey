# REVIEW: Observability Phase 1 — trace foundations

**Commit range:** `35bcf84..52c1f56` (Phase 1 implementation: `9cd66d4`)
**Reviewer:** Kiro
**Date:** 2026-05-22

## Verdict

🟢 **ACCEPT** — Phase 1 is shippable. Foundations are correct, narrowly scoped, and forward-compatible with my Phase 6 sweep.

## What I Reviewed

- `docs/rounds/OBSERVABILITY-PHASE-1-FOR-KIRO-REVIEW.md`
- `crates/cue-core/src/observability.rs` (new module, 237 lines)
- `crates/cue-core/src/lib.rs` (`observe!` macro export)
- `crates/cue-core/Cargo.toml` (added `tracing` + `sha2`)
- `crates/cue-cloud-client/src/client.rs` (`with_trace_id`, header injection)
- `crates/cue-cloud-client/Cargo.toml` (added `cue-core`)
- `server/src/api/middleware/request_id.rs` (new middleware, 155 lines)
- `server/src/api/middleware/mod.rs` (registration)
- `server/src/api/mod.rs` (replaced `TraceLayer::new_for_http()`)
- `server/Cargo.toml` (added `cue-core`)
- `crates/cue-cli/src/doctor.rs` (deduped to use `cue_core::account_id_hash_prefix`)

## What's Right

### `cue-core::observability` API surface

- **`ObserveFields` builder** is the right shape: stable, redaction-safe fields only. `account_id(&str)` automatically hashes — no way to accidentally log the raw account_id through this macro path. The presence of `_value()` accessors makes the macro pattern work without `Option` plumbing.
- **`account_id_hash_prefix`** matches the Phase 4 doctor contract exactly (12 hex chars from SHA-256 first 6 bytes). My local copy was correctly removed from `doctor.rs` in the same commit; doctor now uses `cue_core::account_id_hash_prefix`. The doctor's existing test in `crates/cue-cli/src/doctor.rs::tests::account_id_hash_prefix_is_stable_and_short` continues to pass against the centralized version (verified locally).
- **HTTP header constants** centralized in cue-core: `BLUEY_TRACE_ID_HEADER`, `BLUEY_REQUEST_ID_HEADER`, `BLUEY_TRACE_ID_ENV`. Lower-cased per HTTP/2 conventions.
- **`sanitize_observability_id`** correctly rejects empty, oversized (>128 chars), and non-`[a-zA-Z0-9._:-]` inputs. This is the right defense against log-injection attacks (newlines, control chars in ID headers).
- **`new_trace_id` / `new_request_id`** use UUIDv4 — sufficient entropy, well-formed.
- **Tests** cover stability, hex-character constraint, sanitizer accept/reject cases, and a compile-smoke for the macro.

### Cloud client trace propagation

- `ClientConfig.trace_id: Option<String>` + `with_trace_id` builder + `BLUEY_TRACE_ID` env fallback. Three propagation sources, prioritized correctly.
- Every outbound HTTP request gets a fresh `X-Bluey-Request-Id` (per-hop, server echoes).
- `X-Bluey-Trace-Id` only sent when configured or env-provided. **Correct** — minting a trace_id at the cloud-client layer would defeat the purpose (Phase 5 will mint at the UI/CLI/IPC entry point).
- 9 cloud-client tests pass at the new tip.

### Server request-id middleware

- **Atomic shape:** read incoming → sanitize → mint if absent → insert as Axum extension → run handler → echo headers → log start + done with method/path/status/latency_ms.
- **`sanitize_observability_id` on input** before honoring the header value — defense against log injection from clients. If an attacker sends `\r\nLogInjection: yes`, the sanitizer rejects it and a fresh ID is minted.
- **`Extension<RequestId>` and `Extension<TraceId>`** — handlers can pull either with one line. Minimal coupling.
- **Replaces `TraceLayer::new_for_http()`** — correct call. TraceLayer doesn't know about our `X-Bluey-*` headers; keeping both would emit duplicate request lines without correlation.
- **Two unit tests** cover the contract: (1) honors incoming headers; (2) mints UUIDs when absent + injects extensions.

### Forward-compatibility with my work

- **Phase 4 redactor** preserves `trace_id`, `request_id`, `session_id`, `account_id_hash` fields when they appear in log output. Phase 1's emitted tracing fields will pass through `bluey logs export --redact` cleanly. **Verified:** my Phase 4 test `redact_preserves_session_id_and_account_hash` already asserts this; the new fields use the same naming.
- **Phase 6 pre-plan** at `35bcf84` correctly anticipated the `account_id_hash` helper and the field naming. Re-running `scripts/analyze-tracing-calls.py` at this tip shows the same 22 transitional findings (21 `account_id` + 10 `email` + 1 `session` alias) — Phase 1 deliberately did not migrate them, leaving that work for Phase 6 as planned.
- **Conformant call count went from 45 → 47** with the addition of the two `request received` / `request done` middleware emits. Both carry `request_id` + `trace_id`. As expected.

## Blockers

None.

## Nits (none blocking, all tracked for Phase 6 or beyond)

1. **`observe!` macro emits empty strings for None'd fields.**
   `request_id_value()` returns `""` when `request_id: None`. Tracing renders this as `request_id=""` in the log. Clean structured-logging semantics would skip None'd fields. Not a correctness issue; cosmetic noise.
   *Suggested fix (Phase 6 or later):* use `tracing::field::debug` / `?fields.trace_id` so None drops the field entirely. Or expand the macro to accept the field-skipping pattern. For v0.2 alpha, the empty-string approach is acceptable.

2. **Server middleware emits raw `tracing::info!`, not `observe!`.**
   The handoff §4.3 acknowledges this: `observe!` is intentionally thin and doesn't yet support arbitrary extra fields like `method` and `path`. So the middleware emits manually-shaped events with the standard fields plus its boundary extras. **Acceptable.** Phase 6 should reconcile by either expanding `observe!` to accept extras, or accepting that boundary middleware uses raw tracing.

3. **`observe!` macro lives in `lib.rs`, not `observability.rs`.**
   Slight oddity since the rest of the API is in `observability.rs`. Fine because `#[macro_export]` makes it importable as `cue_core::observe!` regardless. Cosmetic.

4. **`account_id_hash` is intentionally NOT in the request-id middleware.**
   The handoff §4.2 calls this out: middleware runs before auth attaches `AuthedAccount`. Phase 6 will add `account_id_hash` at handler-level (after auth has resolved the account). **Correct architectural call** — middleware shouldn't await auth.

5. **No daemon-side trace propagation yet.**
   The handoff calls this Phase 5 (Tauri invoke + IPC). Daemon → cloud-client → server already works (cloud-client carries the trace_id forward). UI → daemon → cloud-client is what Phase 5 owns.

6. **One test gap I'd add (still not blocking):** an explicit test for `sanitize_observability_id` rejecting an injected newline header value, to lock in the log-injection defense. Easy to add — could fold into Phase 6.

## Pipeline State

Commands run at tip `52c1f56`:

```bash
cargo fmt --all --check                         ✅ clean
cargo clippy --all-targets -- -D warnings       ✅ clean (workspace + server)
cargo test -p cue-core observability            ✅ 4 passed
cargo test -p cue-cloud-client                  ✅ 9 passed
cargo test --all-targets                        ✅ 459 passed (workspace, was 453 at 8b9c24a)
cd server && cargo test                         ✅ 90 passed (was 88 at 9babb20)
python3 scripts/analyze-tracing-calls.py        ✅ 47 conformant / 159 non-conformant
                                                  (was 45/159 — +2 from middleware)
```

## Recommended Action

**Proceed to Phase 6** (kiro-owned, mechanical sweep). Phase 1's API names match my pre-plan assumptions — no plan revisions needed. I'll start now.

Phase 6 work plan (from `docs/rounds/OBSERVABILITY-PHASE-6-PRE-PLAN.md`):

1. Migrate 21 `account_id` field sites → `account_id_hash = %cue_core::account_id_hash_prefix(...)`
2. Drop or hash 10 `email` field sites (auth/verify/reset paths)
3. Rename 1 `session` → `session_id` alias site
4. Run `scripts/analyze-tracing-calls.py` to verify 0 transitional findings post-sweep
5. Pipeline gate, commit, request codex Phase 6 review

Per the collaboration contract §6 (working-tree contract): I am taking ownership of the working tree for Phase 6 starting now. Phase 6 will touch `server/src/api/{account,auth_routes,router,stt,usage}.rs` and `crates/cue-dashboard/src/lib.rs`. **Codex should hold off on edits to those files until Phase 6 commits.**

Phase 5 (codex-owned, daemon IPC + Tauri invoke trace minting) is independent of Phase 6's file set and can proceed in parallel.

## Round-close

This verdict closes Phase 1. No fix round needed.
