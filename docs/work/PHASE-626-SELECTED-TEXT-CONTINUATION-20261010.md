# Phase 626: selected-text and native continuation

Preflight: `bluey-ops`, `pinky-ops`, `pinky-bluey-integration-ops`; current
AGENTS and `docs/ops/PINKY-BLUEY-INTEGRATION-RUNBOOK.md`.

Bluey branch: `feat/phase-626-pinky-integration` in the owned integration
worktree. Pinky branch: `codex/bluey-integration-runtime-20261009`.
Do not edit the canonical meeting-owned Cue checkout, Jobs, production or
another Pinky team's checkout. No new server deployment is recorded here.
Live integration remains Bluey `625b9131` and Pinky `b7de1545`.

## Boundary map for the next agent

Pinky keeps sign-in, native Remote/CC/AI UI, source consent, same-origin routes,
external-subject delegation, run-to-context-ID binding and daemon state. Its
full continuation is `docs/work/PINKY-BLUEY-CONTINUATION-20261010.md`; its new
Git-backed native skill is `docs/skills/pinky-bluey-overlay-ops/SKILL.md`.
Read those in the Pinky feature branch. Public availability must be verified;
GitHub transport has previously been unavailable without Keychain, which must
not be bypassed. Preserve both feature branches/private bundles.

Bluey reuses managed routing, provider streaming, reservation/settlement,
cancellation and external-account authority—not a copied backend in Pinky.
Standalone Bluey auth, wallet and normal completion remain independent.

This source adds exact delegated artifact create/list/delete routes and typed
selected Document context. Principal files:

- `server/src/pinky_integration/mod.rs`: default-off text-context flag, bounded
  create/selection validation, owned Ask fingerprint, dispatch admission.
- `server/src/pinky_integration/store.rs`: dual SQLite/PostgreSQL artifact
  metadata/text/hash storage, owned session checks, replay and deletion.
- `server/src/pinky_integration/delegation.rs`: exact artifact path/scope/body
  authority; no arbitrary upload or media delegation.
- `server/src/api/router/{streaming_completion,prompt_contracts,completion}.rs`:
  server-owned delegated-only JSON USER evidence projection. Selected documents
  cannot become system instructions/tool authority. Standalone behavior stays off.
- HTTP/store/router tests: ownership, replay, default-off, deletion and untrusted
  evidence regression. Run them; source assertions alone are not acceptance.

## Gates before dedicated preprod enablement

- Both sides agree on four selected IDs, 16 KiB aggregate UTF-8 text and 160-byte
  title; encoded JSON envelope handles worst-case escaping without weakening
  unrelated route limits. Native sends no local paths.
- Active storage count is bounded; old records are not silently hidden by a
  LIMIT without a matching quota. Same-owner delete retry is idempotent; foreign
  IDs reveal nothing. Exact content/hash replay cannot change a previous run.
- Ownership/revocation/deletion checked before dispatch; stale queued documents
  cannot sneak through a live-session-only fence. Zero document context preserves
  legacy Ask accounting. Document estimates cover actual escaped envelope.
- Run queued `scripts/run-pinky-integration-tests.sh focused`, projector regression,
  strict Clippy and the isolated real PostgreSQL harness. Run Pinky's runtime-race
  harness after final wire alignment. All owned temporary build/DB roots clean.
- Mac regular-file picker, upload, selected factual response, delete, retry and
  cross-user negative tests against the dedicated synthetic preprod. Windows
  selected-file UI and full physical acceptance remain separate open gates.

No AI listening, screen analysis, PDF/DOCX extraction, named saved conversation,
transcript knowledge base or real AI checkout is launched by this slice. Those
remain explicit next vertical slices with consent, retention, source fencing,
account isolation and platform testing. Do not add fake functioning shortcuts.

Update this receipt with actual commits/tests/hashes after completion. Publication,
source validation, live runtime and physical acceptance must be reported separately.

## 2026-10-10 checkpoint

Selected-text implementation: `8623a3d1`; SQLite lifetime repair: `edb2f089`;
default-off fixtures: `ba53a2c6`; strict-lint repair: `39880614`.
At `ba53a2c6`, 44 focused integration tests and one document projector regression
passed. Strict Clippy rejected an obfuscated conditional and a ten-field storage
transaction signature. The conditional is explicit and the storage method has
a narrowly scoped documented lint allowance in `39880614`. Two test-only clone
lints were corrected in `7a288472`. At that exact code revision, the final
queued gate passed 44 focused tests, one projector regression and strict
all-target Clippy (`-D warnings`); its owned temporary root was cleaned.
PostgreSQL is still unexecuted (the environment-gated test returned
without an ephemeral PG URL in the ordinary focused run).

Pinky source `3f99ea2c` has passing final Go runtime-race/Node/Python gates,
Mac arm64/x86_64 builds and five AppKit test groups. See its companion receipt
for artifact hashes, synthetic visual checks and physical Windows gaps.

GitHub documentation snapshot in both repos:
`docs/bluey-pinky-overlay-handoff-20261010`. It contains skills/handoffs only;
do not deploy it or infer implementation publication. The observed remote Bluey
implementation tip was `20991bdf781318280f773e0dda2937d63f3c4d17`, behind local
source. Normal Git authentication is unavailable without Keychain access, which
the owner rejected. Preserve verified private bundles; publish the normal feature
branches with approved Git credentials later, without recreating product history.

Maintain the Git-backed integration skill with material operating changes and
keep its local installed copy byte-identical. Each subsequent slice must record
source, executed tests, unchanged live pins, open gates and next-agent steps.

Companion GitHub entrypoint after verified publication:
[Pinky native handoff](https://github.com/Dhanunjay-Divi/pinky/blob/docs/bluey-pinky-overlay-handoff-20261010/docs/work/PINKY-BLUEY-CONTINUATION-20261010.md).
The same docs branch carries Pinky's `docs/skills/pinky-bluey-overlay-ops/SKILL.md`.

Skill checks: YAML/name/description validation passed with Ruby; local installed
skill matched the Git copy. The bundled Python quick-validator was attempted
but could not run because PyYAML is unavailable. This is not a claimed official
validator pass. Pinky's runbook-reference check passed.
