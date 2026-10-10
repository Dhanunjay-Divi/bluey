# Phase 626 reference adaptation map

Preflight: load `$bluey-ops`, `$pinky-ops` and `$pinky-bluey-integration-ops`.

Status: narrow handoff for the Pinky+Bluey feature worktree. This records reuse
decisions from the existing source-reference audit; it is not a fresh DMG/EXE
audit and does not claim runtime or platform acceptance.

| Evidence-backed pattern | Audit evidence | Current integration status | Smallest safe next step / acceptance |
| --- | --- | --- | --- |
| Compact context hierarchy for a native AI pill | `docs/research/source-reference-audit/OVERLAY-UI-AND-INTERACTION.md:131-145`; Littlebird coverage and disposition at `INDEX.md:108` | Source implemented: compact default/short/STAR native pill. Physical acceptance is separate. Bluey already has typed overlay/context concepts. | Keep the pill bounded and show only current ask/live state plus explicit source state. Accept when long text cannot crop controls and source/staleness remains visible. |
| Owned stream lifecycle with reopen/Stop/recovery | `OVERLAY-UI-AND-INTERACTION.md:33-50`; Bluey generation fencing at `crates/cue-daemon/src/app.rs:819-854,1027-1143`; recovery comparison at `FULL-SYSTEM-MAP.md:64,75` | Source implemented: owned status recovery, reopen and Stop fences. Helper-restart replay is not implemented. | Add focused stale-generation, cancel, reopen, and repeated-helper-restart acceptance. Do not replace the typed overlay protocol. |
| Explicit media boundary | Integration invariants in `~/.codex/skills/pinky-bluey-integration-ops/SKILL.md`; audit privacy boundary at `FULL-SYSTEM-MAP.md:76` | Implemented safety boundary: all media unavailable in this text slice. Revocable media consent is pending, not implemented. | Preserve explicit, revocable source consent bound to accepted generation/session; accept only with negative tests for viewer audio, stale queued input, and revoked consent. |
| Read-only management surface | Bluey account/usage foundations at `INDEX.md:130-151`; existing Pinky ownership boundary in `pinky-bluey-integration-ops/SKILL.md` | Source implemented: read-only web balance/activity view. No billing mutation is implied; live acceptance is separate. | Keep balance/activity queries read-only and account-scoped; accept with authorization, empty/error, and no-mutation evidence. Real billing remains pending. |
| Stable low-latency stream presentation | Natively frame batching at `_refs/natively-cluely-ai-assistant-main/src/components/NativelyInterface.tsx:884`; Bluey immediate deltas at `BACKEND-AUDIO-AND-RECOVERY.md:155-169` | Pending. The reference supports a presentation optimization, not a new renderer or permission model. | Add 16–33 ms/bounded-byte display coalescing with synchronous final/error/cancel flush; accept with exact final text and stale-stream suppression tests. |
| Helper/audio reliability | Littlebird/Cluely recovery evidence at `BACKEND-AUDIO-AND-RECOVERY.md:171-182`; Pluely overflow evidence at `:81-93`; Bluey gap map at `FULL-SYSTEM-MAP.md:68,75` | Pending; no claim of voice-KB/audio, real billing, or physical Windows completion. | Port bounded queues and explicit reconnect/drop health behind Bluey-owned contracts; accept only after focused offline tests plus physical Mac/Windows evidence. |

## Coverage boundary

The audit corpus is evidence for isolated patterns, not a merge candidate. Existing
audit disposition is `docs/research/source-reference-audit/INDEX.md:107-117`.
Tonight’s UI/lifecycle work adapts behavior into the existing Pinky/Bluey seams;
it does not copy a full Electron/Tauri product, replace Bluey’s backend, or claim
fresh recovered-artifact verification. Parent-owned tests and live receipts remain
separate acceptance evidence.

## Open evidence

Still unavailable from this handoff: real latency percentiles, physical Mac/Windows
capture behavior, voice-KB/audio completion, and real billing. The audit records
these as runtime/open gates at `INDEX.md:197-202` and the integration skill’s
evidence section. No conclusion here should be read as production promotion.
