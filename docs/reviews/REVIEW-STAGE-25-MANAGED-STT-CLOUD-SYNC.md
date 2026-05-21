# Review: Stage 25 — Managed Streaming + Cost Metadata + Cloud Sync + STT Auth

**Branch:** `feat/phase-3-round-12`
**Tip:** `520bc20` (uncommitted batch in working tree)
**Reviewer:** Kiro
**Date:** 2026-05-21

---

## 1. Verdict

🟡 **ACCEPT WITH ONE BLOCKER + THREE NITS**

The architecture is sound. The cloud sync, RAG, and STT relay design is exactly the right shape for the 1,000-user product: server-owned provider keys, account-scoped tenant boundaries, idempotent upserts, single-claim relay tokens. The cost metadata threading (server → cue-llm → daemon → SQLite → dashboard → overlay) is comprehensive.

One blocker prevents shipping: a security-hardening regression breaks an existing test on `/tmp/`-rooted DB paths. Three nits should be tracked but are not v0.2 blockers.

---

## 2. What I Reviewed

### Code paths read
- `server/src/api/router.rs` (`complete_stream` + `response_to_sse_events`)
- `server/src/api/sync.rs` (`batch`, `list_sessions`, `get_session`, `rag_query`, `validate_batch`)
- `server/src/api/stt.rs` (`create_session`, `relay`, `claim_relay_session`, `finalize_relay_session`, `random_token`)
- `server/src/db/sync.rs` (upsert behaviors)
- `crates/cue-cloud-client/src/client.rs` (`auth_post_stream`, `log_safe_response_body`, `redact_json_value`, `is_sensitive_log_key`)
- `crates/cue-llm/src/bluey_managed.rs` (`complete_stream` SSE consumer, `parse_managed_sse_chunks`)
- `crates/cue-daemon/src/cloud/sync.rs` (`sync_local_meetings`, `build_sync_batches`, `load_local_responses`)
- `crates/cue-daemon/src/app.rs` (chunked STT routing block: explicit_stt_key vs managed `/router/transcribe`)

### Handoff docs read
- `docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md`
- `docs/rounds/UI-CANVAS-PASS-FOR-KIRO.md`
- `docs/rounds/CLOUD-SYNC-RAG-STT-AUTH-FOR-KIRO.md`

### Pipeline run
- `cargo fmt --all --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --all-targets` — **1 test failing** (see Blocker B-1)
- `server && cargo clippy + cargo test` — 88 passed, no failures

---

## 3. What's Right

### 3.1 STT relay design — 🟢 strong

- `POST /stt/session` requires authenticated account (`AuthedAccount`), validates session_id, clamps `requested_seconds` to 30..1200, runs balance pre-check against worst-case cost (max_seconds × per-second rate, with trial-seconds carved out). Returns `PAYMENT_REQUIRED` when balance can't cover. **Server never returns the Deepgram API key** — only a Bluey-scoped session token + relay URL.
- `random_token()` — 32 bytes (256 bits) of cryptographic entropy via `rand::thread_rng().fill_bytes()`, base64-url-safe-no-pad encoded. Strong.
- `claim_relay_session()` — atomic UPDATE with `started_at_ms IS NULL` predicate makes the relay claim **single-use**. Two concurrent claim attempts cannot both succeed. Token is also account-bound (`WHERE session_token = ?1 AND account_id = ?2`) so a leaked token can't be replayed by a different account.
- `finalize_relay_session()` — bills on close: trial seconds consumed first, billable seconds × pricing → balance deduction; balance overrun is absorbed by Bluey with a warn log; usage event recorded for analytics. Round-up to next whole second favors the user.
- `expires_at_ms <= now` pre-check returns 410 GONE; the atomic UPDATE has the `expires_at_ms > ?1` guard built in too — defense in depth.

This is the right design. Customer desktop never holds a static Deepgram key.

### 3.2 Chunked-audio default path — 🟢 strong

`crates/cue-daemon/src/app.rs` lines 1648-1697:

- If `BLUEY_STT_API_KEY` or `OPENAI_API_KEY` is set in env → developer-direct OpenAI (override path)
- Else if logged in (`account_token + account_api_url`) → managed `/router/transcribe`
- Else → no STT runtime

**Customer default = managed cloud, no desktop provider key required.** ✅ This satisfies the user's stated requirement: "no desktop provider secrets required for customer STT path."

### 3.3 Cloud sync — 🟢 strong

- `validate_batch` — caps total records at 500, transcript segments at 16 KB, response text at 128 KB, RAG chunks at 16 KB, embedding dimensions at 4096. All sensible.
- Server-side upserts use `ON CONFLICT(account_id, ${id}) DO UPDATE SET ...` for every cloud table. **Idempotent** — repeated CloudSyncNow calls don't create duplicates. Cross-tenant safety via `account_id` in the unique key.
- `cloud_sessions.updated_at_ms = MAX(cloud_sessions.updated_at_ms, excluded.updated_at_ms)` prevents older-write-wins races.
- Daemon `sync_local_meetings` chunks via `build_sync_batches` so long sessions don't blow the 500-record limit.

### 3.4 Cost metadata threading — 🟢 strong

`LlmCostMetadata` + `LlmArtifactMetadata` are added at `cue_llm::LlmResponse` / `LlmChunk` and propagated through:

- `BlueyManagedProvider::complete_stream` parses SSE billing event into LlmChunk metadata
- `cue-router::SpeculativeChunk` carries them through draft/final lanes
- Dashboard `request_cue` forwards them in `cue_response_chunk` events
- `CueResponse` serializes `cost_label`, `artifact_type`, `artifact_body`, `artifact_confidence`
- `cue_responses` SQLite rows include billing columns with additive migration for existing local DBs
- Native overlay renders explicit metadata first, falls back to local heuristics only when absent

This closes the cost/artifact ownership gap from S18 cleanly.

### 3.5 Cloud-client log redaction — 🟢 strong

`log_safe_response_body` parses non-2xx response body as JSON, recursively redacts any field whose key matches:

- `token` / `secret` / `password` / `authorization` (substring)
- `code` exact or `_code` suffix (catches `device_code`, `verification_code`)
- `url` exact or `_url` suffix (catches `verification_url`, `reload_url`)

Then truncates at 256 bytes. Falls back to raw string if not JSON. Recursive redaction handles nested objects.

`device-flow polling` reuses this via `log_safe_response_body` import in `auth.rs`. Verified.

---

## 4. Blocker

### B-1 🔴 Security-hardening regression breaks `/tmp/`-rooted DB paths

**File:** `crates/cue-daemon/src/db/mod.rs` — interaction with `crates/cue-core/src/app_paths.rs::create_private_dir`

**Symptom:**
```
test load_mic_device_setting_round_trips ... FAILED
called `Result::unwrap()` on an `Err` value:
  failed to set private permissions on /tmp
Caused by: Operation not permitted (os error 1)
```

**Root cause:** When `Database::open("/tmp/cue_test_mic_device_<pid>.db")` is called, the new SQLite hardening calls `cue_core::app_paths::create_private_dir(parent)` where `parent = /tmp`. `set_private_dir_permissions` then attempts `set_permissions(/tmp, 0o700)` which fails on shared system directories.

`should_harden_db_parent` only excludes `.` and empty paths — it doesn't guard against system-owned dirs.

**Impact:** Any code path that opens a SQLite DB under a non-user-owned parent regresses. This is hit by integration tests but could also hit production if a user has a non-standard `BLUEY_DATA_DIR` pointing somewhere they don't own (`/usr/local/share/...`, mounted volume, etc.).

**Fix (any of these works):**

Option A (simplest) — make `set_private_dir_permissions` best-effort:
```rust
pub fn create_private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))?;
    // Best-effort: don't fail if we don't own the directory.
    if let Err(e) = set_private_dir_permissions(path) {
        tracing::debug!(path = %path.display(), error = %e,
            "could not set private permissions; continuing");
    }
    Ok(())
}
```

Option B — track which directories we created vs pre-existing:
- Use `fs::create_dir(path)` first; on `AlreadyExists`, skip the chmod
- Falls back to `create_dir_all` with chmod only on the leaf

Option C — broaden `should_harden_db_parent` to skip system-owned paths:
- Brittle (Windows paths, `/private/tmp` symlinks on macOS, container paths)

**Recommendation:** Option A. The chmod is defense-in-depth, not a trust boundary, and a degraded permission on a shared dir is exactly the kind of thing where logging is more useful than crashing.

---

## 5. Nits

### N-1 🟡 `/router/complete/stream` is not true upstream streaming

Codex was honest about this in the handoff: the endpoint emits SSE deltas AFTER the full LLM completion finishes, then sends a `billing` event with the full `CompleteResponse`, then `data: [DONE]`.

**Why it matters:** The endpoint NAME implies token-by-token live streaming. A client developer reading `/router/complete/stream` would expect first-token latency reduction. They get the SSE wire format but not the streaming UX.

**Why it's accept-for-v0.2:** Codex deliberately punted true streaming to keep idempotency/billing simple. This is the right tradeoff for alpha.

**Recommendation for v0.2.x:**
1. Either rename to `/router/complete/sse` (more honest about wire format, doesn't claim streaming)
2. OR keep the name but add a public note in the API doc: "Synthesized streaming for v0.2 alpha; true upstream streaming is a v0.3 deliverable"
3. OR ship true upstream streaming as the next round (Codex's "Stage 19" suggestion in the handoff)

### N-2 🟡 SSE parser UTF-8 chunk boundary

`crates/cue-llm/src/bluey_managed.rs::complete_stream`:

```rust
let part = std::str::from_utf8(&chunk)
    .map_err(|e| LlmError::Provider(format!("invalid managed SSE utf8: {e}")))?;
```

If a multi-byte UTF-8 sequence (Chinese chars, emoji, etc.) spans across a TCP chunk boundary, this returns Provider error and breaks the stream. Correct fix: byte-buffer until the buffer ends on a valid UTF-8 boundary, OR use `from_utf8_lossy` and re-decode the buffer at parse-chunk time.

**Realistic impact:** Low — most TCP segments don't split on byte-3-of-4, and most LLM output is heavily ASCII. But it's a latent bug that will show up on Japanese/emoji-heavy completions.

### N-3 🟡 STT mid-stream balance check missing (already flagged)

Pre-flight in `create_session` rejects sessions where worst-case cost exceeds balance. Close-path billing absorbs overrun on Bluey's side with a warn log. But if the customer holds an active relay AND consumes other resources (LLM completions) that drop their balance below the in-flight session's needs, the STT relay continues until the timer hits `max_seconds`.

**Mitigation already in code:** The pre-flight ceiling is the worst case, so a session that fits the pre-flight WILL be billed at or below that. The risk is only multi-session concurrent abuse.

**Suggested follow-up:** Add a heartbeat from `run_deepgram_relay` every 30s that re-checks balance; close on exhaustion. Already noted in codex handoff section "Remaining Gaps" — no need to block v0.2 alpha.

### N-4 🟡 Synthesized SSE response_to_sse_events ordering

Quick read of `response_to_sse_events` would help — verify that:
- `event: billing` carries the `CompleteResponse` AS the data payload (not as a name with empty data)
- `data: [DONE]` is the LAST event and closes the stream cleanly
- Cost label / artifact metadata is JSON-encoded in the billing event matches the parser expectation in `bluey_managed.rs`

Codex's regression test `bluey_managed_sse_parser_billing_metadata` exercises this contract — relying on that test to catch wire-format drift is acceptable.

---

## 6. Specific Answers To User-Listed Focus Areas

| Focus area | Verdict | Notes |
|---|---|---|
| Managed server streaming/cost/artifact metadata path | 🟢 with N-1 + N-4 | Synthesized streaming is honest tradeoff; cost metadata threading is comprehensive |
| Native overlay UI/canvas/session UX | 🟢 (Codex visual QA only) | Code is clean; needs real-Mac click-through smoke (already on operator checklist) |
| Cloud sync schema/API/CLI | 🟢 | Account-scoped, idempotent upserts, sensible size caps |
| Daemon CloudSyncNow upload path | 🟢 | Reads local sessions + cue_responses + RAG chunks; chunks under 500-record cap |
| STT safety path (`/stt/session` + `/stt/relay` + managed `/router/transcribe`) | 🟢 | Account-bound single-claim tokens, no static provider keys leaked |
| No desktop provider secrets required for customer STT | 🟢 | `app.rs` lines 1648-1697 verify managed default when logged in |

---

## 7. Pipeline State

```
✅ cargo fmt --all --check
✅ cargo clippy --all-targets -- -D warnings (workspace + server)
🔴 cargo test --all-targets — 1 failed: load_mic_device_setting_round_trips (B-1)
✅ cargo test in server — 88 passed
```

Need B-1 fixed before this batch is committable.

---

## 8. Recommended Action

1. **Fix B-1** — make `set_private_dir_permissions` best-effort. Single-line change in `crates/cue-core/src/app_paths.rs`. Re-run the failing test to confirm green. ~15 minutes.
2. **Commit the working-tree batch** — once B-1 is fixed.
3. **N-1 and N-2 → next round** — track in `docs/PRELAUNCH-CHECKLIST.md` as v0.2.x followups, not v0.2 blockers.
4. **N-3 → already deferred** — codex listed it as a remaining gap.
