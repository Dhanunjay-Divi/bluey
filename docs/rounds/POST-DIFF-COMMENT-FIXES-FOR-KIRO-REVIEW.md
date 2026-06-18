# Post-Diff Comment Fixes for Kiro Review

> Branch: `codex/bluey-ai-site`  
> Scope: post-review reliability, billing/finality, live captions, account, and release-safety fixes  
> Author: Codex  
> Date: 2026-06-17

## 1. Verdict Request

Please review the current fix batch against the large PR comment set.

Requested verdict:

- 🟢 ACCEPT — the fixed/verified items are production-safe enough for the next alpha smoke
- 🟡 ACCEPT WITH NITS — remaining non-blocking follow-ups can move to the next round
- 🔴 REQUEST CHANGES — a fixed item still has a correctness, billing, auth, or release-safety blocker

## 2. What Changed

### Browser account session and web safety

- Browser account calls now use the saved refresh token once on `401` before clearing local session state.
- The account/reload pages no longer accept raw bearer tokens from URL parameters.
- `bluey-site.js` SRI in `web/index.html` was recomputed after the script change.

### Account profile / API URL preservation

- New `AccountFileStore` profiles seed `api_url` from `BLUEY_API_BASE_URL`, `BLUEY_CLOUD_API_URL`, or `CUE_CLOUD_API_URL` before falling back to `https://bluey.sh`.
- This keeps staging/local deep-link and dashboard flows from silently pinning new profiles to production.
- CLI logout now notifies the daemon with `CloudLogout`, so in-memory cloud clients and balance polling stop instead of re-saving old refresh tokens.

### Signed update hardening

- Verified updates no longer inherit `BLUEY_SKIP_CHECKSUM`; the signed manifest's artifact hash remains enforced.
- Manual site deploy preserves `latest.json.sig` so a web-only deploy cannot delete the live update signature before release publishing succeeds.
- Release workflow builds now require `BLUEY_UPDATE_PUBKEY`, preventing production artifacts from shipping without an embedded updater public key.

### Managed stream finality and billing correctness

- Managed client streams now reject `[DONE]` / `finished` chunks that arrive before final billing metadata.
- Server streaming requests now fail the stream if the final balance deduction loses a concurrent low-balance race, rather than marking the idempotency key complete and caching an unpaid answer.
- Capacity-busy state is preserved across unconfigured fallback routes, so clients keep the intended retry/capacity behavior.
- Deep lane pricing keeps the deep 150% markup even when the current Sonnet model shares a balanced pricing entry.
- Deep/thinking routes use a longer first-output deadline to avoid misclassifying healthy thinking streams as stalls.

### Live audio and caption reliability

- Managed live STT uses the websocket relay only when all resolved sources support native-helper streaming; ffmpeg/AVFoundation-only capture falls back to chunked `/router/transcribe`.
- Chunked STT processes each source as it finishes instead of waiting for every source, so a slow system/mic lane does not block the healthy lane.
- Interim STT hypotheses are transient overlay/live-preview events only; they are not saved into sessions, RAG, or later answer context.
- Final STT segments emit a final transcript overlay event and then follow the normal persistence/indexing path.
- Relay completion and idle auto-stop both send the overlay back to `Paused`, so the UI cannot remain stuck as listening when capture has ended.
- Continuous system-audio capture is allowed to create/update a meeting instead of being dropped by the session-id guard.

### Session deletion and RAG safety

- Deleting the active session now stops both audio and screen capture before clearing the meeting.
- The daemon emits `ListeningStateChanged(Paused)` after active-session deletion.
- Transcript/artifact indexing and full session reindex check that the session still exists under the RAG index lock before writing vectors.
- Drag-and-drop file attachments are accepted from the idle overlay state as well as the explicit attach-open state.

## 3. Files Changed

- `.github/workflows/release.yml`
- `crates/cue-cli/src/app.rs`
- `crates/cue-cli/src/update.rs`
- `crates/cue-cloud-client/src/tokens.rs`
- `crates/cue-core/src/ipc.rs`
- `crates/cue-core/src/overlay.rs`
- `crates/cue-core/src/overlay_ipc.rs`
- `crates/cue-daemon/src/app.rs`
- `crates/cue-llm/src/bluey_managed.rs`
- `scripts/deploy-bluey-sh-manual.sh`
- `server/src/api/router.rs`
- `web/assets/bluey-site.js`
- `web/index.html`

`bluey-dev.db` remains local/untracked and untouched.

## 4. Verified / Already Closed in Current Branch

These comments were reviewed against the current branch and did not need new code in this batch because they are already closed or superseded:

- Reload pricing CTA points to the reload surface, not the login deep-link path.
- Short landing-page viewports can scroll.
- Provider stream completion/finality in server adapters has terminal-event tests.
- Stale daemon cleanup no longer blindly kills arbitrary reused PIDs.
- Dashboard sign-out clears the new account-file path and legacy keyring fallback where configured.
- Dashboard cloud clients and deep-link exchange use saved account API URLs.
- Square environment-specific webhook handling rejects sandbox crediting in production.
- Usage rows escape/render labels safely.
- OpenAI route token-limit compatibility is covered in the current router/model path.
- macOS smoke no longer hard-depends on Pillow.
- Privacy text no longer claims default desktop tokens live only in the OS keychain.

## 5. Superseded / Deferred Items

### R-1 — Dual-source STT reservation is closed in follow-up

This was still open when this handoff was first written, but it is no longer open at the current branch tip.

Follow-up commit `b93882b fix(server): reserve STT relay billing upfront` added server-side reservation/settlement for paid live STT. The server now reserves worst-case session cost/trial seconds before issuing a relay token, refunds unused reserved credit on close, records actual usage after settlement, and rejects a second mic/system relay when the account cannot cover it. See `docs/rounds/STT-RESERVATION-AND-DECOUPLING-FOR-KIRO-REVIEW.md`.

### R-2 — Auto-renew relay sessions is future polish

This batch chooses a safe stop-and-paused state when relay sessions finish. That prevents false "Listening" UI. Auto-renewing before the server limit is better UX, but needs careful cost caps and reconnect telemetry.

### R-3 — Provider-frame activity for thinking streams can be more precise

Deep/thinking now uses a longer deadline, which fixes the immediate false-stall risk. A future refinement should expose provider-neutral "activity" events for non-text Anthropic thinking/usage frames so the router can distinguish an active thinking stream from a quiet one without relying only on a larger timeout.

## 6. Verification

Checks run during implementation:

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

Result: all passed on uno.

## 7. Areas Most Likely Wrong

1. The deep/thinking first-output deadline is set to 30 seconds by default. That is intentionally safer for quality, but live telemetry may show a lower value works.
2. RAG tombstone checks depend on `MeetingStore::load_by_id` being the canonical deletion source. If cloud/session sync introduces another deletion ledger, the check should include it.
3. `BLUEY_UPDATE_PUBKEY` must be set in release build environments. The workflow now fails closed, but operators must create the repository variable before the next signed release.
4. This doc originally marked STT reservation unresolved; that statement is superseded by `b93882b` and `docs/rounds/STT-RESERVATION-AND-DECOUPLING-FOR-KIRO-REVIEW.md`.
