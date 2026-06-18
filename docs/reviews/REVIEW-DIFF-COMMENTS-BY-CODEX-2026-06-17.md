# REVIEW: PR Diff Comment Set and Current Fix Batch

**Branch:** `codex/bluey-ai-site`  
**Reviewer:** Codex  
**Date:** 2026-06-17  
**Scope:** Review comments on web, account auth, updater, streaming billing, live captions, RAG, and overlay event handling.

## Verdict

🟡 **ACCEPT WITH ONE REQUIRED NEXT ARCHITECTURE ROUND**

Most of the comment set is now either already closed in the branch or fixed in this batch. The remaining real risk is dual-source managed STT billing reservation. I do not recommend a small opportunistic patch there; it needs a deliberate server-side reservation/settlement design before wider paid live-caption usage.

## Findings

### P1 — Dual-source STT relay can still outrun close-time billing

When Listen starts both mic and system lanes, each lane can create its own STT relay session. The server currently checks balance at session creation and deducts at close. If the account has enough credits for one stream but not both, the second stream can run until close and then fail deduction. That is not solved by UI state changes or client gating.

Required follow-up: add aggregate STT reservation or a single multi-source relay accounting session. This should be treated as a billing architecture round, not a local daemon patch.

### P2 — Deep thinking timeout is mitigated, not perfect

The router no longer uses the fast 6s first-token deadline for deep/thinking lanes; it uses a 30s default. That prevents common false stall fallback for valid thinking streams. A cleaner future design is to count provider activity frames even when they are not text deltas.

### P2 — Relay renewal is safe-stop for now

When relay sessions finish, Bluey now stops capture and marks the overlay paused. That is correct and safe. It is not the polished version where a long call seamlessly renews the session before expiry.

## What I Agree With

- Rejecting managed `[DONE]` without billing metadata is required.
- Final low-balance deduct failure must fail the stream/idempotency record, not cache an unpaid answer.
- Deep lane pricing must preserve deep markup even when it uses a shared Sonnet model entry.
- Browser account pages should refresh once before logging the user out.
- Raw bearer tokens in URL query params should not be accepted.
- Release builds must embed the update public key, and verified updates must not inherit checksum-skip.
- Interim STT partials should not be saved into session history or RAG.
- Active session deletion must stop audio/screen capture first.
- RAG writes after deletion need a tombstone/session-exists check.

## What I Disagree With / Narrowed

- I do not agree that live relay expiry must immediately auto-renew in this patch. Safe stop plus a visible paused state is the correct alpha-safe closure.
- I do not agree with a quick client-only STT precheck for dual-source billing. Billing correctness belongs on the server, where retries and concurrent sessions can be controlled.
- I treat the repeated/duplicate Anthropic finality comments as one issue. The important invariant is "no successful cached/billed stream without terminal usage/billing finality," and that is now enforced in the managed client path plus existing server adapter tests.

## Fix Coverage Map

| Area | Status |
|---|---|
| Web reload/scroll/session refresh/query-token safety | Closed |
| Usage-label XSS | Already closed in current web code |
| Account API URL preservation | Closed |
| Dashboard/logout token stale state | Closed |
| Signed update manifest trust | Closed |
| Release build embedded pubkey | Closed |
| Manual deploy preserving `latest.json.sig` | Closed |
| Managed client final billing metadata | Closed |
| Server final deduct race | Closed |
| Capacity-busy preservation | Closed |
| Deep markup / deep first-output deadline | Closed / mitigated |
| Drag-and-drop attach state | Closed |
| Active session delete stops audio + screen | Closed |
| RAG stale writes after delete | Closed |
| STT ffmpeg fallback + chunked concurrency | Closed |
| STT partial persistence noise | Closed |
| Relay finish / idle stop overlay state | Closed |
| Dual-source STT reservation | Open architecture round |
| Relay auto-renew | Deferred polish |

## Verification Expected Before Merge

Run:

```bash
cargo fmt --all --check
git diff --check
node --check web/assets/bluey-site.js
cargo test -p cue-core attach_files_allowed_from_idle_or_attach_open
cargo test -p cue-cloud-client account_file_store_save_seeds_api_url_from_environment
cargo test -p cue-llm complete_stream_errors_when_done_arrives_before_billing_final
cargo test -p cue-daemon --all-targets --no-run
cargo test --manifest-path server/Cargo.toml --lib
cargo clippy --all-targets -- -D warnings
```

Result: all passed on uno before handoff.

Recommended additional gates before live paid caption testing:

- Server reservation tests for dual-source STT once implemented.
- Real Deepgram live-caption smoke: mic-only, system-only, mic+system, idle stop, relay finish.
- Streaming disconnect/billing tests for client closes before final billing event.
- Manual release publish dry run with `BLUEY_UPDATE_PUBKEY` and `latest.json.sig`.
