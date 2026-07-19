# Round 549 — Jobs truth/spend atomic rollout

Status: exact code/artifact seal deployed and public desktop release complete.

Code/artifact seal: commit
`53c258cf843599f595a1e250d1872d191638d57a`, tree
`65dabc483bea3ea3694a28fc168d5b1e9dd247d0`, epoch `1784475153`.
This evidence document is intentionally updated in a later docs-only commit;
that descendant does not replace the seal embedded in the deployed binaries
and release artifacts.

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

## Executed rollout

1. Full public ingress was taken out of service, both old binaries were stopped,
   paid dispatch was held fail-closed, and every measured in-flight/work
   aggregate was verified at `0`.
2. Migrations 008, 009, and 010 were applied in the same maintenance boundary
   as the matching main API and Jobs API binaries. Migration 009 owns the Jobs
   discovery board-owner boundary; migration 010 owns provider usage
   provenance.
3. The four replay-safe markers were verified:
   `usage-cutover-spend-baseline-v1`,
   `usage-origin-authority-cutover-v1`,
   `usage-origin-taxonomy-repair-v1`, and
   `usage-reservations-settled-at-repair-v1`.
4. Both APIs reported the exact seal before the temporary hold was removed.
   The configured global cap returned to 1,000 cents per 24 hours while Jobs
   model generation, local Browser distribution, and cloud Browser
   distribution remained `0`.

Rollback follows the same full-outage rule in reverse. Before migration 008
commits, both old binaries/configurations may be restored together with the
original positive cap. After migration 008 commits, never start an older paid
binary against the cutover schema merely because it passes a boot check: a true
old-version rollback requires restoring the verified pre-cutover PostgreSQL
dump and both old binaries/configurations while ingress and both APIs remain
stopped. Once ingress reopens, database rollback would discard new writes and
the normal recovery path is fix-forward.

## Verification gates

The exact seal passed the independent diff/security/release audit, all-target
Rust builds, formatting, clippy with warnings denied, the 788-test server full
gate (including 706 library tests), all Jobs package gates, physical Windows,
macOS, Linux-server, and the 12 targeted PostgreSQL runtime tests.

The executed suites proved:

- SQLite tests prove the anonymous baseline blocks both global admission
  paths, clamps hostile values, survives account deletion, expires and cleans
  up only after the fixed 31-day retention, and is not duplicated on migration
  replay.
- SQL-shape tests preserve the marker-guarded PostgreSQL snapshot and its
  conditional cutover lock; the exact PostgreSQL 18.4 + pgvector 0.8.3 runtime
  drill also passed fresh, restore, operator, replay, uniqueness, retention,
  and local-submit authority paths.
- Route-family tests prove that route mismatch persists as `missing` and
  cannot shrink a hold, while an exact over-projection report persists actual
  exposure before returning an error. They cover all managed main API
  route families and the Jobs dispatcher.
- Ingress tests reject malformed/compressed/oversized audio and malformed,
  animated, MIME-mismatched, or canvas-mismatched images before provider I/O.
- Deterministic janitor tests prove a stale expiry candidate cannot refund
  a replacement attempt with the same request id.
- PostgreSQL migration/replay and account-deletion spend-retention drills pass
  against the exact frozen commit.
- Separate exact macOS and physical Windows packaging/runtime checks passed;
  Bluey Browser remains test-only and is not publicly distributed.

GitHub Actions executed no job step. Every zero-step job reported, "The job was
not started because an Actions budget is preventing further use." This is an
explicit infrastructure waiver, neither a green check nor a product-test
failure; equivalent or stronger exact platform gates above provide the release
evidence.

## Production evidence

The pre-mutation backup
`bluey-postgres-20260719T163508Z.pgdump` is 24,845,155 bytes with SHA-256
`6cd7272a947a88e377cdf477ba182b29ddadde277b92f77c7a92840106b7022a`.
Its remote copy and transferred macOS copy were both verified before rollout.
R2 replication remains degraded because the current credential returns
`AccessDenied`; the verified remote and macOS copies are the present recovery
inputs.

After the full-ingress maintenance window:

- main API PID `2217136` runs sealed binary SHA-256
  `7704fb8d279d1d64af15646f19d14a657ed36d5144fcf89d98ac0e8bad593888`;
- Jobs API PID `2217137` runs sealed binary SHA-256
  `687fc4834a08a53465bde64cb55336f931ae26acf3819139e221edb30a07bb2e`;
- Caddy PID is `2217438`; the deployed portal `index.html` SHA-256 is
  `309c555e40ed568a84c79e681e93756758392d3cf0762e9fc10e32fc1cb08ca3`;
- both services are active with `NRestarts=0`, public health reports exact
  commit `53c258cf843599f595a1e250d1872d191638d57a`, the cap is 1,000 cents per
  24 hours, all three Jobs flags are `0`, and all measured aggregates remain
  `0`.

Deployment and public desktop publication are complete. The signed manifest
advertises `0.1.104`, all four public platform artifacts match the seal, and the
website terminal fallback was updated only after installer and updater checks.

The signed publication packet is live: `latest.json`
SHA-256 `b97a6090d93d69c14455d63c9ff354de997f7a2abb99c8417cc81a3cf4f4f716`,
detached signature SHA-256
`9b70ccbf7894979d818e53fb20087682d234ef83b2a895929d8dc7dcc9f1c3b9`,
installer SHA-256
`ced25c22f7d8cf58439bcd08e0563cb6f81d83a6ec93a3b443fdecaff1b74e57`,
and Windows installer SHA-256
`74de690c5eebf6a03cac6aafb2e00ab96b410fab9db919ecfc55ef1e111481b3`.
The Ed25519 signature verifies live. Both installers and all four public
artifacts downloaded byte-identically. An isolated no-sudo macOS smoke with
optional local tools skipped installed `0.1.104`, and an equivalently isolated
`0.1.103` installation updated through the signed public manifest to `0.1.104`;
full helper/runtime coverage remains the earlier exact-artifact gate. The
PowerShell installer was hash/content-type verified, while Windows execution
evidence remains the earlier exact-artifact physical Windows 11 gate. The
subsequently published website origin `index.html` has SHA-256
`80c2d1136960bfd341806a1179a25bd325da6a15d63295a391faeaa60134bf70` and
contains the `0.1.104` fallback.

The immutable release directory deliberately retains its sealed pre-publication
`RELEASE.md` with SHA-256
`76e25d76075cbeae26fdc450deebffb43a61a66f54cff78dd6955e7f7d43006a`.
The signed manifest still links to that note, so the public link retains stale
status language. This post-publication round records the final state without
mutating the published audit packet or altering the link.
