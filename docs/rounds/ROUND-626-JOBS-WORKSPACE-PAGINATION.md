# Round 626 — Jobs Authenticated Workspace Pagination

> **Codex preflight:** Load `$bluey-ops` before implementation, verification, review, or release
> work. The current worktree and live read-only evidence are authoritative. The SSD archive is a
> fallback only for one specifically missing historical fact.

**Date:** 2026-08-31

**Branch:** `feat/phase-626-jobs-workspace-pagination`

**Base source:** `95696dd0`

**Status:** GREEN independent source/static review with P0/P1/P2/P3 all zero; exact-source
Rust/Cargo, portal, configured PostgreSQL, hosted CI, artifact promotion, deployment, and
production activation remain gated and are not claimed

After corrections for five earlier blockers, a subsequent review found one remaining packet-lock
P1. The correction received a bounded GREEN review, and the fresh independent full line-by-line
review of the frozen 50-path candidate then returned **GREEN for source/static review: P0: 0,
P1: 0, P2: 0, P3: 0**. This clears source/static review only. Exact-source Rust/Cargo, portal
typecheck/Vitest, configured PostgreSQL, and hosted CI evidence remain mandatory before promotion.

## Stop-The-Line Evidence

Read-only production diagnosis found live accounts with 3,954 and 5,478 posting rows. One
authenticated workspace request took 21.448 seconds while the portal aborts reads after 20 seconds.
The accepted source loaded up to 501 postings and failed above its 500-row representation envelope.
Before that failure it also composed current source, ATS, and signed-integrity authority for every
posting. The result was both a deterministic availability failure for large accounts and an
interactive query shape whose cost grew with total history.

## Bounded Interactive Contract

- `GET /api/jobs/workspace` keeps an additive wire shape, responds `private, no-store`, and now
  contains at most 50 interactive matches plus `matches_page` continuation metadata. This
  intentionally changes behavior from exhaustive/fail-above-500 semantics: an older client can
  decode the response but sees only the first page and cannot continue it.
- Authenticated `POST /api/jobs/matches/page` accepts deny-unknown JSON `{limit,cursor}` and returns
  `{ matches, page }` with `private, no-store`; limits from 1 through 100 are accepted. The opaque
  cursor is never placed in a URI, access log field, browser storage, or aggregate application log.
- The stable order is `match_score DESC, updated_at_ms DESC, id ASC`. The relational ID, score, and
  update time override encrypted legacy-document copies before filtering or cursor construction.
- `has_more=true` always carries a non-empty `next_cursor`; a terminal page always carries
  `has_more=false` and `next_cursor=null`.
- The cursor is AES-GCM authenticated with the Jobs data key, versioned, bound to the account and
  requested page size, and valid for 30 minutes. Tampering, malformed data, another account,
  changed page size, expiry, or data-key rotation produces the same privacy-safe 400 error. The
  portal refreshes current authority before asking the user to continue.
- Portal installation is conditional on both the cursor and the installed workspace authority
  epoch remaining current; any intervening refresh or authority-sensitive mutation rejects the
  response. Already loaded IDs and duplicate IDs within a response are de-duplicated.

Keyset pagination guarantees deterministic traversal without offset drift for one stable ordering
state. A posting whose score or update time changes between page requests is intentionally treated
as new current state; cursor expiry bounds how long a client may continue an old traversal.
Because posting availability is encrypted, an ordered candidate window that contains only expired
or otherwise non-interactive rows can return a short or empty `matches` list with `has_more=true`.
The cursor advances past that bounded window rather than scanning the account's complete history in
one request.

## Interactive Membership Semantics

The queue includes an open/unknown posting when any existing inclusion path is current:

1. it is untracked/manual and has no discovery membership;
2. it is linked to an application; or
3. it has an active/unknown membership from an active source whose health is waiting or healthy.

Closed or expired postings are not returned by the interactive queue. They remain reachable by
exact account-and-job-ID detail for application history and remain present in account export.

## Authority And Effect Boundary

One database snapshot loads a bounded page, its discovery memberships, mutable account inputs, and
one database time. The account-global managed-source release authority is resolved once for the
page. When that authority is inactive—the observed production state—the page returns fail-closed
source/ATS/integrity projections without per-posting authority SQL. Positive source bindings still
receive exact ATS composition under the established PostgreSQL lock order. ATS trust, heads,
canary membership, evidence, layouts, runtimes, quarantine, and circuits are read once per page.
Policy and policy-key revocation resolve as bounded global flags; non-global revocations are
limited to exact page subjects and keyed for bounded lookup. Page/target arrays and binding/child
result sets are explicitly capped; non-contiguous or over-envelope manifest children fail closed
before revocation lookup. Signed-integrity control, policy,
head, attestation/transition, and current-revocation reads are likewise page-batched under the
already-held publication fence; signatures, channel/scope identity, rollout enrollment,
canonical membership, and source matching are still evaluated per record.

Canary membership is looked up only for the exact bounded allowlist hashes referenced by the
page. Quarantine and circuit state is joined by exact page-derived primary-key identity, so
unrelated mutable history cannot make page cost grow. Scalar and page resolvers also compare the
signed manifest evidence, layout-observation, and runtime-target counts with the exact loaded
children; missing or extra children fail closed even when ordinals remain contiguous and within
the absolute envelope.

Pagination is representation only. Detail, application approval, queueing, finalization, runner
claim, and final-submit paths do not accept or trust the page cursor or page snapshot. They reload
the exact posting by account and relational ID, reload discovery evidence, and independently
recompose current source, ATS, integrity, account, policy, entitlement, and effect authority.
The first packet effect additionally reloads the current profile, safety-normalized preferences,
exact Career Track and policy ledger, application-aware reservations, discovery memberships and
sources, composed discovery evidence, and signed source destination inside the same transaction.
Exact meter replay returns before those current checks. A new denial precedes allowance, debit,
ledger, or meter effects. PostgreSQL takes global authority locks, the parent Account with
`FOR NO KEY UPDATE`, policy, the exact application/posting, and the entitlement with `FOR UPDATE`
before reading reservations. Database time and current composed authority are loaded only after
the entitlement lock. That one database timestamp governs posting/source freshness, the current
reservation period, packet metering, entitlement/allowance updates, and any overage ledger entry.
Reservation authority is limited to one active same-company row plus at most the clamped 50 active
current-period rows, excludes the current application, de-duplicates by reservation ID, and uses
the existing company/period indexes. The locked entitlement row is reused for metering. SQLite uses
the same bounded reservation and database-time shape inside its immediate transaction.

Account export retains its separate exhaustive ID pager, but each call into the fixed authority
resolver is capped at 100 rows. A 102-row positive fixture must traverse two chunks without loss.

Application history is not joined solely through ranked matches. The first workspace page includes
one strict display-only `application_jobs` entry for each distinct application job outside the page.
Frozen receipt labels are preferred and malformed legacy snapshots use bounded tenant-posting
fallback. The projection cannot authorize a URL or effect: handoff/interview loads exact current
detail, queueing does not reject solely for page absence, and the server independently revalidates.
Legacy `GET /api/jobs/matches` stays exhaustive through 500 and explicitly requires pagination
above that limit rather than silently truncating.

## Portal Truth

Search, sort, filters, Career Track counts, and summary values operate on loaded rows. While another
server page exists, the portal states that scope explicitly and offers the next bounded page; it
does not claim that zero loaded matches means zero matches in the account. Older servers without
`matches_page` remain readable as one terminal bounded workspace, and older clients may ignore the
new additive field.

## Storage And Diagnostics

The SQLite migration adds `(account_id, match_score DESC, updated_at_ms DESC, id ASC)`. PostgreSQL
uses the equivalent negated-expression ordering so a deep mixed-direction keyset can be one
row-value range seek. A production-shaped SQLite query-plan fixture requires its index and forbids
a temporary order B-tree.

The same migration adds a partial import-only `(account_id,id)` index and durable per-account,
versioned checkpoint for the 50-row background legacy verified-import repair. One
owner/token/expiry lease is claimed before worker spawn; live SQLite leases exit through a
read-only fast path, and expired owners cannot reread a batch, advance, or release replacements. Monotonic
compare-and-swap prevents regression; parse, decrypt, or storage failure retains the checkpoint,
and relational identity overrides stale embedded JSON.

Workspace and continuation requests emit request-correlated `candidate_count`, `returned_count`,
`has_more`, logical `authority_batch_count`, representation time, ancillary time, database total,
request total, `application_context_count`, and bounded fallback-query batch count. Continuations
report both application-context values as zero. Logs contain no job document, search term, cursor,
account ID, user ID, email, company, title, URL, or application payload.

The source-authority stage reads the account release, all supported page memberships, and all
matching heads as bounded page reads rather than one membership/head pair per posting.
Signed-integrity control/policy/head/attestation/transition/current-revocation reads are likewise
bounded page reads. Focused SQLite evidence proves 50 distinct fully positive ATS targets in 10
`SELECT` statements and 50 fully positive source-to-ATS-to-integrity compositions in 19. Hosted
PostgreSQL runtime and plan evidence remains mandatory; positive batching is not inferred from the
inactive-source fast path.

## Acceptance Gates

1. A 5,500-row account returns its first 50 rows without the former 500-row error.
2. Full traversal over tied scores/timestamps has no duplicate or gap; tenant rows never cross.
3. Malformed, tampered, expired, foreign-account, and page-size-mismatched cursors fail closed.
4. The page limit cannot exceed 100, and metadata continuation invariants fail closed at the
   portal boundary.
5. Expired history is absent interactively but reachable by detail and exhaustive export.
6. Action paths retain independent exact-ID authority revalidation.
7. SQLite/PostgreSQL migration text and runtime registration remain paired.
8. After integration into a successor containing the canonical launcher, every Rust command uses
   `bash scripts/run-bluey-tests.sh -- <cargo command>` (or an equivalently reviewed disposable
   launcher) and proves cleanup after success or failure. No additional Rust command runs in this
   launcher-less Phase 626 base. Full hosted CI remains mandatory before release.
9. A hosted disposable-PostgreSQL gate runs
   `server/tests/fixtures/jobs_workspace_postgres_explain.sql`, captures both first-page and deep
   (5,500 tied score/time rows) plans, and rejects a sequential scan or Sort. The fixture rolls its
   transaction back and must never be pointed at a shared or production database.
10. Full serialized scalar/page ATS parity covers signed general/canary selection, shared-manifest
    general priority and canary fallback, expiry, wrong enrollment, revocation, quarantine,
    circuit, malformed targets, child drift, and channel/scope corruption.
11. A 50-row fully positive source-to-ATS-to-integrity page stays within its fixed statement
    budget; the regression must fail the former scalar-per-posting ATS loop.
12. A workspace with 501 distinct application jobs stays available, keeps frozen/expired labels,
    falls back safely for malformed legacy snapshots, and does not inflate ranked matches.
13. Durable repair reaches row 51 across batches/restart, retains its cursor on parse failure,
    tolerates stale embedded IDs, and rejects equal/backward checkpoint transitions.
14. Fully source/ATS/integrity-positive account export returns all 102 ordered rows in SQLite and
    configured PostgreSQL across the 100/2 authority chunk boundary.
15. First packet commit rejects current Track/profile/preference inclusion drift with balance,
    credits, entitlement usage, allowance, overage ledger, and meter unchanged in both dialects;
    exact prior meter replay remains first.
16. PostgreSQL packet locking statically proves exact replay → global authorities → parent Account
    → exact application/posting → entitlement → bounded reservations, with no reservation SQL
    between application/posting and entitlement. The Account-deletion race must remain deadlock
    free. A second configured race holds the entitlement, proves packet commit waits there, inserts
    a current-period same-company reservation without an Account-lock timeout, then requires the
    awakened commit to re-read it and reject with every packet effect unchanged.
17. The first SQLite repair claim succeeds with all six parameters bound, and a live replacement
    remains single-flight.
18. A load-more response begun under an older workspace authority epoch cannot install after a
    refresh or policy mutation even when its cursor still matches.

## Release Boundary

This round authorizes local implementation and evidence gathering only. It does not authorize a
commit, push, merge, migration application, deployment, cohort change, flag change, application,
message, or other external effect. The independent source/static verdict is GREEN with P0/P1/P2/P3
all zero. Exact-source executable and hosted evidence, artifact promotion authorization, deployment
authorization, and production flag approval remain mandatory before any release decision.
