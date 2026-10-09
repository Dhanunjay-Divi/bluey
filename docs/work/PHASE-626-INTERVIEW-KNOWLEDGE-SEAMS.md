# Private interview knowledge-base source seams

Date: 2026-10-09. Read-only source audit; no transcript ingestion or feature
activation. Preflight: `bluey-ops`, `pinky-ops`, integration ops. Read the
[voice/memory plan](PHASE-626-CONVERSATIONAL-VOICE-AND-INTERVIEW-MEMORY.md).

## Reusable code and material gaps

| Area | Existing source evidence | Required delta |
| --- | --- | --- |
| Account-owned sync/RAG | `server/src/api/mod.rs:292` routes; `api/sync.rs:98` authenticated account; `db/sync.rs:2381` account-keyed queries | new scoped delegated import/retrieval seam for Pinky; never browser-supplied account authority |
| Import | `crates/cue-core/src/ipc.rs:100`; daemon `app.rs:20959`; `doc_conversion.rs:80` | cloud-owned explicit transcript import, not local IPC assumed available in Pinky |
| Long input | daemon `cloud/sync.rs:35`, `:4085`, `:4278` uses a bounded preview and single chunk | complete bounded multi-chunk ingestion; detect/reject incomplete input rather than silently truncate |
| Speakers | `server/src/db/sync.rs:33`; daemon `db/speakers.rs:7`; `app.rs:16930` heuristic detection | structured speaker/timestamp parser plus owner attribution review |
| Retrieval | `server/src/db/sync.rs:2381`; `api/router/provider_runtime.rs:1041` | exact approved collection/source filter; session score boost is not an authorization filter |
| Evidence | `api/router/provider_runtime.rs:1076` untrusted source wrapping; `api/router.rs:1156` sources DTO | confirmed-fact IDs, source/timestamp citation DTO; existing returned sources are web sources, not proof of RAG citations |
| Deletion | `server/src/db/sync.rs:1962` session purge; `api/account.rs:802` account object purge; daemon `cloud/sync.rs:3064` artifact tombstones | direct source deletion, including global chunks, object/vector verification and anti-reanimation fence |
| Current Pinky text | `server/src/pinky_integration/mod.rs:239`, `:443` fixed styles, unavailable-history boundary | explicit separate transcript consent/authority; do not enable broad account memory implicitly |

Paths labeled daemon resolve under `crates/cue-daemon/src/`; all server paths
resolve from the Bluey repository root. Line references are this audit's
snapshot, not immutable Git blame. No general confirmed-fact schema was found;
free-form metadata is not verified experience authority.

## Smallest safe next slice

Import only owner-selected exports into an account-owned interview collection.
Preview speakers and mark the owner's answers; ambiguous attribution stays
unverified. Preserve source hashes and timestamps through complete chunking.
Keep factual stories separate from style preferences. Retrieve only approved
collection chunks, inject them as untrusted evidence and show private citations.
Delete source text, objects and vectors together with durable revocation.
Do not add a second humanizer call or turn interviewer examples into user facts.

Acceptance: tenant/consent/retention isolation; long-input completeness;
speaker/timestamp fidelity; verified versus proposed facts; source-filtered
retrieval and honest no-answer; prompt-injection resistance; independent style;
correct citations; source/session/account deletion and no resurrection; existing
Pinky cancellation/accounting remain unchanged. Raw audio stays off.

This is a handoff plan, **not implemented or deployed**. The owner was asked
for 2–3 representative exports and their speaker identity; unrelated files,
Messages and Otter accounts were not searched.
