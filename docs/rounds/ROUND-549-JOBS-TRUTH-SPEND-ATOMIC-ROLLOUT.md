# Round 549 — Jobs truth/spend atomic rollout

Status: implementation and verification in progress; not deployed.

## Deployment boundary

The usage-origin cutover and durable provider-attempt holds are one coordinated
server release. They are not safe as a Jobs-only migration beside an older main
API binary, and an older paid-dispatch binary is not a safe rollback target while
provider routes remain enabled.

`BLUEY_UPSTREAM_SPEND_LIMIT_CENTS` is a mandatory safety boundary for every
managed paid dispatch, not a temporary live-test option. Unset, invalid, or `0`
produces no configured guard and must fail closed before provider I/O; it never
means unlimited spend. `BLUEY_UPSTREAM_SPEND_WINDOW_HOURS` defaults to 24 only
after a positive limit exists. Provider-dashboard caps remain an independent
second boundary.

After migration 008, an insert from an older binary that omits `origin` receives
`legacy_unverified`. That is the privacy-safe default, but authoritative spend
queries intentionally exclude it. Older binaries also dispatch without the new
pre-dispatch provider hold. Consequently, a mixed-version process can spend at a
provider without contributing authoritative exposure to the new global cap.
`old_binary_insert_is_legacy_and_proves_paid_rollout_must_be_atomic` preserves
this boundary as an executable regression test.

There is no database-only generation gate that can reliably stop a process that
has already loaded provider credentials: the legacy binary does not check such a
gate before dispatch. Database triggers can reject or relabel its later write,
but cannot undo the network side effect.

## Conservative cutover baseline

Relabelling existing usage rows as `legacy_unverified` would otherwise make the
new authoritative rolling-cap query appear to start at zero. Before that
relabel, migration 008 snapshots every positive pre-cutover Bluey cost into
`usage_cutover_spend_baseline`, clamping each row at the authoritative event
ceiling. The snapshot contains only occurrence time and cost: no account,
request, provider, model, or other user identity. A migration marker makes the
snapshot replay-safe, and the cutover table lock closes the insert race between
snapshot and relabel. A completed replay does not retake that table lock.

Both ordinary usage reservations and durable provider-attempt holds include the
active baseline in the same global admission cap. The baseline is deliberately
conservative, survives account deletion, is never used for customer billing or
product analytics, and stops contributing after the configured rolling window
(clamped to 1 through 720 hours). Physical deletion is independent of the
current configured window: baseline rows are retained for a fixed 31 days, the
maximum 720-hour window plus a 24-hour cleanup grace. Shortening the configured
window therefore cannot destroy evidence needed if operators later restore a
longer window.

Databases that crossed the authority marker with an earlier build but lack the
baseline marker receive a conservative compatibility repair: every positive
historical cost is snapshotted. This may temporarily double count exposure, but
it cannot reset the cap or silently undercount provider spend.

## Usage trust and projections

Every durable provider hold records one of three usage provenances:

- `exact`: a complete, trusted provider usage report for the route that was
  actually selected;
- `estimated`: a partial or explicitly estimated provider report; or
- `missing`: omitted usage, a zero-only report, an ambiguous live-audio send,
  or a provider/model route mismatch.

Only `exact` usage may shrink a durable hold below its immutable pre-dispatch
projection. `estimated` and `missing` settlements retain the greater of the
projection and any reported cost. An exact report above the projection is first
persisted as the actual exposure, then the request fails closed so the
under-projection cannot be hidden.

Projections are deliberately upper bounds rather than average-token guesses:
text uses UTF-8 byte length plus protocol and per-part envelopes, output uses
the route's effective maximum, vision uses a fixed decoded-image token ceiling,
web search uses a fixed maximum, and STT uses verified audio duration. These
same rules apply to main LLM streaming/non-streaming, answer planning,
embeddings, upload transcription, live STT, web search, and managed Jobs resume
generation.

## Ingress and recovery boundaries

Upload transcription accepts only structurally valid RIFF/WAV PCM, validates
declared sizes, byte rate, block alignment, and complete frames, then projects
from the verified duration before any provider call. The request body is capped
at 32 MiB and verified audio at 15 minutes. Live STT accepts only binary
16-kHz, mono, PCM16 frames, rejects odd-length PCM and client text/control
frames, caps confirmed audio at 32,000 bytes per second for at most 20 minutes,
and treats a cancelled or ambiguous upstream send as `missing` usage.

Vision validates decoded PNG, JPEG, WebP, or GIF containers rather than trusting
the data-URL label. Animated images, MIME/container mismatches, malformed
containers, duplicate or missing WebP payloads, and WebP canvas/payload
dimension mismatches are rejected. Per-image and aggregate byte, dimension,
pixel, and image-count ceilings are enforced before provider dispatch; base64
transport text is not double-counted as prompt text.

Reservation and hold expiry decisions use database transaction time. Startup
and periodic bounded janitors run in both server processes. Expired-reservation
release is fenced by account, request id, attempt number, and exact expiry in
the same refund transaction, so a stale janitor or delayed task cannot refund a
replacement attempt that reused the request id.

## Required rollout

1. Stop every old binary, then fail closed every paid dispatcher (main LLM streaming/non-streaming, answer
   planning, web search, embeddings, upload transcription, live STT, and managed
   Jobs resume generation), or remove its provider credentials, and verify no
   provider dispatch remains in flight.
2. Apply migrations 008 and 010 (migration 009 remains reserved for the Jobs
   discovery board-owner boundary) and deploy the matching main API and Jobs
   binaries in the same maintenance boundary.
3. Verify server-origin attempt rows, opaque provider holds, global-cap denial,
   account-deletion retention, and customer-root settlement before re-enabling
   paid routes.
4. Re-enable paid dispatch only after all server instances report the new build.

Rollback follows the same rule in reverse: disable paid routes first, drain
in-flight work, then roll back both binaries. Never start an older paid binary
against the cutover schema merely because it passes a boot check.

## Verification gates

- All-target Rust build, clippy, and full tests must pass.
- SQLite tests must prove the anonymous baseline blocks both global admission
  paths, clamps hostile values, survives account deletion, expires and cleans
  up only after the fixed 31-day retention, and is not duplicated on migration
  replay.
- SQL-shape tests must preserve the marker-guarded PostgreSQL snapshot and its
  conditional cutover lock; exact PostgreSQL runtime behavior is a separate
  frozen-commit drill.
- Route-family tests must prove that route mismatch persists as `missing` and
  cannot shrink a hold, while an exact over-projection report persists actual
  exposure before returning an error. They must cover all managed main API
  route families and the Jobs dispatcher.
- Ingress tests must reject malformed/compressed/oversized audio and malformed,
  animated, MIME-mismatched, or canvas-mismatched images before provider I/O.
- Deterministic janitor tests must prove a stale expiry candidate cannot refund
  a replacement attempt with the same request id.
- PostgreSQL migration/replay and account-deletion spend-retention drills must
  pass against the exact frozen commit.
- Windows and macOS packaging checks remain separate release gates; this server
  branch alone authorizes no client or production deployment.
