# Round 613 — Jobs Canonical Taxonomy And Career Track Policy Authority

> **Codex preflight:** Load `$bluey-ops` before implementation or review. Use the current
> repository and local replacement handoff as authority. Use the SSD archive only when one
> specifically identified historical fact is missing.

**Status:** LOCAL SOURCE GREEN; RELEASE CONDITIONAL 🟡; FLAGS REMAIN `0` WITHOUT PRODUCTION
READ-BACK; NO DEPLOYMENT AUTHORITY

## Goal

Replace scattered role, skill, and location substring logic with one immutable taxonomy and bind
every executable Career Track to an exact reviewed policy revision. Matching may preserve raw
evidence and allow a human review path, but queueing, Auto-submit, and final execution must fail
closed unless current role, geography, identity, resume, preferences, and taxonomy authority are
proven together.

This round reconciles the useful behavior identified by Round 612 into the current Phase 612B
successor. It does not merge the stale Round 574 branch or copy a competitor implementation.

## Product And Safety Boundary

Round 613:

- adds one checked-in, digest-bound role, skill, country, subdivision, metro, and city registry;
- provides server-owned target-role resolution, posting classification, token-safe skill matching,
  and typed geography decisions;
- projects the same registry into the portal and requires an authenticated exact-registry
  handshake before Career Track writes;
- activates the exact taxonomy/canonicalizer tuple through one global monotonic event chain;
- records policy-relevant account and Track semantics through separate tenant-scoped monotonic
  transition chains before creating a policy revision;
- creates immutable, tenant-scoped Career Track policy revisions, review receipts, and policy-head
  transitions with compare-and-swap current heads;
- freezes the exact current policy authority into prepared packets and application receipts;
- invalidates readiness, queueing, Auto-submit, and execution after taxonomy, Track, identity,
  source-resume, or Jobs-preference drift;
- serializes policy-input writes against PostgreSQL execution authorization so a concurrent change
  cannot cross the final effect boundary;
- serializes PostgreSQL Auto-submit authorize/revoke against final Auto-submit execution in one
  account-and-Track lock namespace;
- preserves raw posting and Track values for explanation and review; and
- regenerates the checked-in Jobs portal bundle from the reviewed sources.

Round 613 does **not**:

- enable a discovery, generation, workflow, runner, communication, or submission flag;
- deploy, push, merge, retarget, publish, sign, or activate a release;
- grant Gmail, LinkedIn, MCP, or other external-account access;
- contact a job source, employer, recruiter, ATS, registry, hosted database, or production service;
- submit an application, send a message, publish a post, or infer a provider receipt;
- make custom roles executable without a later explicit review authority;
- broaden an exact city into its metro or guess an ambiguous role/location; or
- use the SSD archive. No current fact required historical fallback.

### Late Correctness Closure

The final Phase 613 review also closed FIX-706 through FIX-711:

- resume reservation/publication now takes object/account lifecycle fences before policy-child
  locks, matching account deletion's parent-before-child order;
- arbitrary timestamp-looking keys inside `CareerFact.value` remain semantic, while actual
  relational transport timestamps stay excluded;
- PostgreSQL migration 034 now mirrors SQLite positive/non-negative safe-integer CHECK bounds and
  has real PostgreSQL rejection coverage;
- Auto-submit authorize/revoke and Track deletion participate in the portal's shared mutation epoch
  and authoritative read-back fence;
- public SmartRecruiters/Workday search uses a bounded cursor-v2 continuation with ordered-prefix
  validation, provider overlap/total checks, and exact dedupe/cross-list history; and
- the public `many-matches` preview preserves every application parent and related reference while
  expanding deterministically to 125 matches.

## Canonical Registry Contract

The immutable schema-one registry is:

```text
version  bluey-jobs-taxonomy-v1-2026-08-25
SHA-256 facdb3593457b6585ea83c9f735c42616e7cf03caa6e0369be9154f542dd7254
roles   49
skills  44
countries 4
subdivisions 64
metros  38
cities  44
```

The server embeds the exact raw bytes and verifies their SHA-256 before parsing. The portal imports
the same raw bytes, validates an exact-key schema and cross-references, computes the digest with the
browser cryptography API, and compares the complete authenticated server registry before sending a
Career Track write. A missing, malformed, changed, or unsupported registry fails closed with a
refresh-and-review response.

### Role Semantics

- Target-role resolution is distinct from posting classification.
- Deterministic aliases such as `SWE`, `SDE`, `CRA`, and `CRC` resolve to one role.
- Ambiguous aliases such as `PM` and `TPM` return their bounded candidate roles and require a
  choice; they are never guessed from surrounding text.
- Seniority prefixes and bounded level suffixes may be removed for exact role resolution.
- Unknown custom targets retain their user-visible label and a stable custom identifier, but stay
  `needs_review` and cannot receive executable policy authority in this round.
- Posting titles classify independently into known, ambiguous, or unknown evidence. An unproven
  posting role may be reviewed/prepared, but cannot enter a runner.

### Skill Semantics

- Canonical matching recognizes only a registered skill ID, label, or alias.
- Word and symbol boundaries prevent `Go` from matching `Django`, `R` from matching `Rust`, and
  `C`, `C#`, and `C++` from collapsing into one another. Bare `Go`, `R`, and `C` prose tokens are
  not positive skill evidence; contextual aliases such as `Golang`, `R programming`, and
  `C language` remain recognized, while exact selected-skill resolution remains available.
- The portal's document preview has a separate, explicitly non-authoritative literal matcher for
  unknown free-form profile skills. It preserves useful previews without creating canonical
  classification or policy authority.

### Geography Semantics

- Geography is typed as workplace, country, subdivision, metro, and exact city.
- An explicit metro matches only its registered member cities.
- An exact city remains exact even when another city shares its metro.
- Country/subdivision conflicts, city/jurisdiction collisions, multiple candidates, unknown cities,
  and unknown metros return bounded ambiguity or review results instead of substring guesses.
- Canonical operational-hold scopes are added beside the preserved raw location, including exact
  country, subdivision, metro, and city IDs where classification succeeds.

## Career Track Policy Authority

A reviewed Track policy binds:

- taxonomy version and SHA-256;
- canonical target role and family;
- raw Track locations and canonical location IDs;
- remote, employment, engagement, work-authorization, and relevant-employment policy;
- exact verified application identity ID and canonical identity digest;
- exact source-resume asset ID and content SHA-256;
- the complete server-normalized Jobs preferences and their digest; and
- Track activation state.

The canonical JSON receives its own SHA-256. SQLite migration 056 and PostgreSQL migration 034 add
the following ten-table authority, with the same constraints and relationships in both databases:

1. `jobs_track_policy_taxonomy_activation_events` is the immutable global history of activated
   taxonomy-version/digest and canonicalizer-schema/digest tuples.
2. `jobs_track_policy_taxonomy_activation_head` is the singleton current activation head.
3. `jobs_track_policy_account_input_transitions` is the immutable per-account history of
   policy-relevant profile, preference, fact, identity, and source-resume semantics.
4. `jobs_track_policy_account_input_heads` is the compare-and-swap current account-input head.
5. `jobs_track_policy_track_input_transitions` is the immutable per-Track semantic history.
6. `jobs_track_policy_track_input_heads` is the compare-and-swap current Track-input head.
7. `jobs_track_policy_revisions` stores immutable encrypted canonical policies with exact
   predecessor identity.
8. `jobs_track_policy_review_receipts` stores immutable encrypted approval evidence for one exact
   revision and all of its generation bindings.
9. `jobs_track_policy_head_transitions` is the immutable history of reviewed current-head changes.
10. `jobs_track_policy_heads` is the compare-and-swap current approved head for each Track.

The server intentionally replays post-Jobs PostgreSQL migrations for idempotent repair. The
predecessor managed-cloud migration 033 therefore guards all sixteen column additions and all six
named constraints by exact table identity; a second startup cannot stop on a duplicate before
migration 034 is reached.

Startup activation is idempotent for the same taxonomy/canonicalizer tuple. Any changed tuple,
including a later return to old bytes, advances the global activation epoch instead of reviving an
old authority. Account and Track inputs work the same way: transport-only changes are no-ops, while
every policy-relevant semantic change advances its own generation and transition digest. A Track
write advances its semantic generation once before the canonical policy is built, even when the
relational and JSON projections both need updating.

The canonical policy, revision, review receipt, head transition, portal projection, prepared
packet, and application receipt bind the activation epoch plus the current account and Track input
generations, semantic digests, and transition digests. Revision and head generations are monotonic.
A true no-op verifies and reuses the complete current ledger. A reviewed `A -> B -> A` reversion
therefore creates new input transitions, a new revision, a new receipt, and a new head transition;
matching an earlier content digest cannot restore its authority.

SQLite uses one serialized write transaction for these transitions. PostgreSQL policy-relevant
writers take the account's exclusive transaction lock, while execution validation holds the shared
account lock through the effect-authorizing transaction. Auto-submit authorization is read and
written under the same stable account snapshot. Authorize/revoke takes an exclusive per-Track
Auto-submit fence and final Auto-submit execution holds its shared counterpart. Direct
immutable-history mutation/deletion,
cross-tenant reparenting, a skipped predecessor, a stale compare-and-swap update, or an equal-hash
row with incomplete/corrupt ledger evidence fails closed. Owner export verifies and returns the
global activation evidence, account and Track transition chains, revisions, receipts, head
transitions, and current heads. It requires every revision to have exactly one matching approved
receipt and immutable head transition, and every transition history to have one terminal current
head. Authorized Track/account deletion retains the reviewed cascade semantics; global activation
history is not tenant data.

An exact source-resume publication replay also validates the current account semantic head and
recomputes the complete semantic digest before taking its idempotent fast path. Unapproved Tracks
durably retain their exact review reasons. Candidate-evidence hashing excludes only Track
timestamps and derived match counts, so transport-only retries cannot revoke prepared work while
every semantic Track and reviewed-authority field remains bound.

Legacy Tracks remain readable as `legacy_unreviewed`. A current approved Track is projected back
to `needs_review` when the activation epoch, canonicalizer, account-input generation, Track-input
generation, verified identity, source resume, Jobs preferences, or revision/head binding changes.
The projection does not rewrite immutable history.

## Enforcement Matrix

| Boundary | Required behavior |
| --- | --- |
| Discovery acquisition | Positive raw role, location, and workplace filters are hints only; neither provider query text nor worker substrings may permanently discard candidates before server classification. Explicit company/title exclusions remain worker-safe. Cursor v2 binds the ordered source/query/window, validates each bounded provider prefix resumably, checks trailing overlap and advertised totals, and preserves exact dedupe/cross-list history. Capped ordinary feeds retain explicit partial results; closure-authoritative snapshots fail closed. |
| Match and presentation | Canonical role/family, token-safe skills, and typed geography determine scoring and explanations. Unknown or ambiguous values retain review state. |
| Preparation | Proven hard role/location conflicts are blocked. Reviewable ambiguity may create a packet for human review but cannot queue. |
| Approval and queue | Exact current Track policy, proven role family, proven Track geography, source authority, ATS authority, and all existing safety checks are required. |
| Auto-submit | The authorization fingerprint includes the reviewed Track authority, complete Jobs preferences, taxonomy activation, source resume, identity, profile, and confirmed facts; authorization is minted from one stable account snapshot. |
| Final execution | The application receipt's frozen Track policy authority must equal the current validated ledger and server projection inside the effect-authorizing transaction. Drift denies new effects while preserving receipt/reconciliation paths. |
| Operational holds | Raw location plus every proven canonical region scope participates in exact hold evaluation. |

Prepared, approved, queued, submitted, and side-effect-unknown remain distinct. No taxonomy result
can manufacture an employer submission receipt.

## Portal Contract

- Role suggestions derive from the committed registry instead of a second hard-coded list.
- Experience presentation uses canonical role-family assessment and labels unknown/custom targets
  review-required.
- Settings shows the exact approved policy revision or a review-required state.
- Portal readiness requires the complete account-input and Track-input semantic digests in
  addition to their positive generations and transition digests.
- Portal authority reconciliation uses a monotonic request epoch so a pre-mutation refresh or an
  older overlapping read-back cannot reinstall a stale approved/active projection.
- Auto-submit authorize/revoke and Track deletion use that same epoch/read-back path; deletion also
  removes dependent local authorization and match references while waiting for current server truth.
- Auto-submit cannot be enabled from the portal until the current server Track projection is
  approved.
- The Command Center reports Track readiness only for approved current authority; legacy or stale
  Tracks are actionable, not optimistic `ready` state.
- The packaged `/jobs` assets are rebuilt from the final source and contain no source maps.
- Public preview scenarios preserve application/job/evidence/session/intervention/event references;
  this is presentation-fixture integrity, not authenticated workspace authority.

## Acceptance Criteria

1. The embedded server bytes and portal raw import resolve to the exact frozen registry digest.
2. Registry validation rejects unknown keys, broken references, duplicate IDs/aliases, and invalid
   metro/city scope.
3. Deterministic and ambiguous role aliases have exhaustive boundary tests.
4. `Go`/`Golang`, `R`/`Rust`, and `C`/`C#`/`C++` regressions pass without substring or terse-prose
   broadening.
5. Country, subdivision, metro, city, remote, ambiguity, conflict, and unknown geography tests pass.
6. Career Track create/update normalizes server-side; legacy/stale Track readback visibly requires
   review.
7. SQLite 056 and PostgreSQL 034 contain the same ten-table activation/input/policy authority and
   preserve exact immutability, predecessor, tenant, compare-and-swap, and cascade behavior;
   PostgreSQL migration 033 is safe under the runtime's mandatory replay.
8. Same-value startup and semantic no-ops reuse current generations, while taxonomy, account, or
   Track `A -> B -> A` changes advance their immutable histories and cannot revive old authority;
   exact resume replay rejects a stale account semantic head.
9. Resume, identity, preference, taxonomy/canonicalizer activation, or Track-policy drift
   invalidates queue, Auto-submit, and execution authority.
10. The prepared packet and application receipt freeze the exact policy authority, while
    transport-only Track timestamps and derived match counts do not manufacture a new candidate
    evidence revision.
11. The authenticated portal/server taxonomy handshake rejects missing or stale writes.
12. Public ATS discovery neither sends an unsafe positive Workday role filter nor treats raw
    positive role/location/workplace substrings as final authority. Cursor v2 rejects more than 24
    sources before fetch, binds query/window/source state, resumably validates the complete ordered
    prefix within logical `maxPages` operations, verifies provider overlap/total evidence, and
    preserves only post-filter exact dedupe/cross-list history within the 512-entry/192-KiB bounds.
13. PostgreSQL policy-input writes and Auto-submit revocation cannot race execution authorization
    across the irreversible effect boundary; resume reservation/publication follows the same
    parent-before-child deletion order, and SQLite preserves the equivalent serialized boundary.
14. Owner export verifies and includes the complete activation, input-transition, revision,
    receipt, head-transition, and current-head evidence with exact revision/receipt/transition
    bijection rather than only mutable projections.
15. Arbitrary Career Fact value keys remain semantic, and the paired authority migrations enforce
    the same positive/non-negative safe-integer CHECK contract.
16. Portal, automation, Rust, migration, schema, privacy, provenance, and generated-bundle gates
    pass locally, and the synthetic many-match preview retains exact referential integrity.

## Local Release Evidence — 2026-08-26

The final evidence for local implementation commit `03c595de` is local unless explicitly identified
as deployed read-only QA:

```text
PostgreSQL 17.10 + pgvector 0.8.3        PASS: 13 / 13; normal migrations twice
Portal                                  PASS: 349 / 349 across 28 files
  App                                   PASS: 20 / 20
  Preview                               PASS: 3 / 3
  App + preview                         PASS: 23 / 23
  FIX-697 focused six-file group        PASS: 70 / 70
Public ATS focused                      PASS: 39 / 39
Automation                              PASS: 680 / 1 skipped; 37 files / 1 skipped file
Jobs Vitest aggregate                   PASS: 1,847 / 1 skipped; 144 files / 1 skipped file
Rust server library                     PASS: 1,401
Rust all-target                         PASS: 1,517 / 1,517
Jobs workspace typecheck                PASS: all 5 workspaces
Jobs production builds                  PASS: all 5 workspaces
Portal production build                 PASS: 2,299 modules; existing >500-KiB advisory only
Schema parity                           PASS: 81 tables / 74 indexes per dialect
CI guard self-tests                     PASS
Privacy, whole-diff snapshot            PASS: 2,622 tracked paths / 2,347 text paths
Provenance                              PASS: 663 lock entries / 631 unique package versions /
                                              1 audited override / 14 pinned repositories
Native storage                          PASS: 14 / 14
Browser release                         PASS: 10 / 10
Managed release                         PASS: 16 / 16
Account-deletion browser guard          PASS: 3 / 3
```

The clean Rust gate used `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, and
`CARGO_PROFILE_TEST_DEBUG=0`; fmt, all-target check, and all-target Clippy with warnings denied were
green before the 1,517-test pass. An earlier all-target attempt ended because the local disk filled
and is environmental failure evidence, not a passing or failing source result.

Local current-source visual QA showed 125 Matches split 63/62 between the two Tracks, all four
applications resolving company and location after FIX-711, the correct Overview route, and no
horizontal overflow or console error at desktop or 390x844. Read-only QA of the deployed preview
from `/jobs/overview?preview=1` still produced a stale recursive `/matches/matches` route and
pre-fix unknown-company presentation. That deployed bundle mismatch is an external release blocker,
not evidence against the corrected local source.

The release verdict therefore remains conditional yellow. Exact-tip CI; the unavailable Docker
CLI and exact Linux managed-runner image/native smoke; hosted migration, network, canary, rollback,
and deployment proof; and production flag read-back remain pending. The configured release flags
remain `0`; no live flag read-back is claimed.

## External-Only Evidence And Successors

Local source and isolated database checks cannot prove a hosted full-stack migration, production
PostgreSQL/network behavior, registry artifact read-back, threshold signing, protected approvals,
live runner capacity, runtime image identity, ATS canaries, customer cohorts, kill switches, or a
rollback rehearsal. Those remain parked Phase 611 release gates.

Round 614 remains responsible for the original-source verification worker. Round 615 remains
responsible for the operated source control plane and freshness SLOs. C2C communication,
government-contract ingestion, MCP/Agent Connect, Gmail/LinkedIn-style integrations, and broader
autonomous workflows must each arrive as separately reviewed, consent- and receipt-bound batches;
this taxonomy foundation does not silently authorize them.
