# REVIEW: Phase 637 Jobs document fidelity and bundle boundaries

Date: 2026-10-28 (America/New_York)
Base: `479377dcc8cb6928fd038a51c72ac672e5fe4abe`
Reviewer role: independent read-only source/design review
Verdict: 🟢 bounded local Phase637 source acceptance; broader release holds remain

## Scope reviewed

- `AGENTS.md` and the `bluey-ops` preflight.
- `docs/rounds/ROUND-637-JOBS-DOCUMENT-FIDELITY-AND-BUNDLE.md`.
- `docs/work/IMPL-PHASE-637-JOBS-DOCUMENT-FIDELITY-AND-BUNDLE.md`.
- Baseline PDF.js use in `jobs/automation/src/documents.ts` and
  `jobs/automation/tests/documents.test.ts`.
- Baseline portal production import path in `jobs/portal/src/lib/documents/import.ts` and Rollup/Vite
  configuration in `jobs/portal/vite.config.ts`.
- Current font correction in `jobs/automation/src/pdfjs-node-options.ts`, `documents.ts` and their
  focused tests.
- Current portal chunk correction in `jobs/portal/bundle-policy.ts`, `vite.config.ts` and the pure
  chunk-policy tests.
- Parent-owned browser import fixture, wrapper and focused controller in
  `jobs/portal/nonprod/document-import.integration.mjs`,
  `jobs/portal/src/lib/documents/browser-import.integration.test.ts` and
  `docs/work/COMMAND-PHASE-637-DOCUMENT-FOCUSED.md`.
- Final Jobs and current-Rust controller recipes in
  `docs/work/COMMAND-PHASE-637-FINAL-JOBS-GATES.md` and
  `docs/work/COMMAND-PHASE-637-CURRENT-RUST-GATES.md`.
- Current `CHANGELOG.md`, `docs/work/JOBS-V1-COMPLETION-LEDGER.md`, both FIX records and
  `docs/work/HANDOFF-PHASE-637-LOCAL-20261028.md`, including its explicitly source-only successor
  inventory. The complete pre-closeout pass covered all five tracked and fifteen new Phase637
  owned paths; inherited `REVIEW-PHASE-628-JOBS-SOURCE-VERIFIER-RELEASE-V3.md` was excluded.
- Installed local PDF.js 6.2.108 Node binary-data factory and Rollup 4.62.2 manual-chunk semantics.

No product or test source was changed by this reviewer. No build, test, browser, service, VM,
network or provider action was run.

## Design findings

No P0-P3 design blocker is identified in the Round 637 plan. The measured baseline correctly
distinguishes written output from a pre-final bundle hook, a deferred Mammoth closure from the
initial closure, and the small PDF-worker URL module from the real worker asset.

The implementation and evidence must retain these boundaries:

1. PDF.js's Node `BaseBinaryDataFactory` forms the standard-font filename by string-concatenating
   `standardFontDataUrl` and the requested filename; `NodeBinaryDataFactory` then passes that value
   to `fs.readFile`. The shared value must therefore be an absolute native filesystem directory,
   derived from the installed `pdfjs-dist` package, but normalized to a literal trailing `/` even
   on Windows. PDF.js's `getFactoryUrlProp` rejects a trailing backslash before the Node factory
   can read the path. A `file:` or remote URL string is not equivalent. Production materialization,
   test extraction and the compiled `dist` build must resolve the same package-owned resource
   without enabling system-font fallback or changing existing PDF limits and snapshot checks.
2. Rollup 4.62.2 defaults `output.onlyExplicitManualChunks` to false. A narrow policy match alone
   is not proof of final ownership: CommonJS virtual/proxy modules and dependencies can affect the
   emitted boundary. The final written graph must show the complete `dingbat-to-unicode` mapping
   deferred with Mammoth, absent from the initial static closure, and must demonstrate that no
   mapping was dropped. Policy tests should cover normalized POSIX/Windows and CommonJS ID forms;
   the real browser DOCX fixture must exercise a mapped symbol through production `importResume`.
3. A second deferred dependency can change Vite's generated preload map even when no module bleeds
   into the initial closure. Round 637's no-initial-growth condition must compare final written
   entry bytes/gzip and actual import closures, not source estimates or a `generateBundle` snapshot.
   The unchanged 500000-byte warning threshold must remain the gate.
4. The browser qualification must load immutable built output and production import code, allow
   only the owned loopback origin, fail on unexpected requests, and prove PDF via the real worker,
   DOCX symbol conversion, TXT import and malformed denial. Direct helper reimplementation or a
   synthetic report without those observed outcomes would be a false green.

## Evidence status

Parent reported baseline session 69748 as successful with an initial entry of 486262 bytes/145043
gzip and a deferred Mammoth chunk of 503986 bytes/131614 gzip across 369 modules. The rendered
dingbat table was 137405 bytes. The real PDF worker was 1262398 bytes, while its URL-export chunk
was 70 bytes. These are historical baseline measurements, not current-source acceptance evidence
and not a latency/SLO result.

The two worker implementations and parent browser fixture are statically clear. Prospective-index
session85771 passed2884 paths/2608 text files with all12 then-owned new files; owned hashes and the
real index remained unchanged and scratch was removed. Focused, full Jobs and current-source Rust
execution subsequently passed as recorded below. A final privacy scan must include the later
handoff, this final review and all other closeout record edits before the local checkpoint.

## Resolved implementation findings

- **Resolved P2 — standard-font warning regression could pass after a failed asset read.** The
  first controlled standard-font case checked only that captured warning text did not contain
  `standardFontDataUrl`. PDF.js also reports a bad or missing local asset as `Unable to load font
  data at: ...`; that warning lacks the checked token, and fallback text extraction may still
  succeed. For the generated Helvetica fixture, require zero `console.warn` calls during both
  validation and extraction, or explicitly reject every PDF.js font-load failure form, while also
  retaining the zero-fetch assertion. The corrected case now requires zero warning calls and zero
  fetch calls across production materialization and extraction.
- **Resolved P2 — Windows path test blessed a mixed trailing separator.** `fileURLToPath` returns a
  directory ending in `\\` on Windows. The first helper appended `/`, producing `\\/`, and the
  unit test derives the same mixed value as its expected result. Normalize trailing `\\` or `/`
  characters to exactly one literal `/`; PDF.js requires that slash, while the preceding path must
  remain a native absolute filesystem path for `fs.readFile`. The corrected helper strips all
  trailing slash/backslash characters before appending `/`; its tests pin POSIX, Windows and mixed
  suffixes, absolute installed-package resolution, the actual Liberation Sans asset and no `file:`
  prefix.

The portal manual-chunk implementation has no static blocker in its current three-file diff. It
normalizes Windows IDs, matches the exact package boundary including CommonJS query/proxy IDs,
excludes lookalikes, and enables `onlyExplicitManualChunks`. Its executable graph, size and symbol
fidelity gates closed through the paired current-input graph and built-browser evidence below.

Candidate graph session 65766 correctly failed the Round's literal zero-initial-growth condition.
The final initial entry was 486304 bytes/145057 gzip versus 486262/145043 at baseline (+42/+14),
while its exact 111-module set and static imports remained unchanged and symbols remained absent.
The deferred symbol chunk was 119992/12366 across four modules; Mammoth was 384036/117259 across
365 modules and imported the CommonJS helper plus symbols. No JavaScript chunk exceeded 500000
bytes. This is not a green result under the current Round wording.

A tightly bounded specification correction is technically defensible because Vite names the new
Mammoth-to-symbol dependency in its parallel-preload bookkeeping; forcing the table/helper back
together would defeat the intended boundary or create a waterfall. Any correction must require the
exact initial module-ID set/count and static imports to match baseline, exclude Mammoth/dingbat and
their CommonJS proxies from the initial closure, identify `document-symbols` as the sole added
preload dependency, cap the bookkeeping delta at 64 raw/32 gzip bytes, retain the unchanged
500000-byte chunk threshold, pin both deferred closures, and pass the built-browser mapped-symbol
DOCX case. It must record the observed +42/+14 and make no latency claim. Round/IMPL now adopt
those conjunctive guards; session65766 remains an expected historical failure, not a waiver.

Captured successor graph session18655 satisfied the refined static graph shape for its snapshot:
the exact111 initial module IDs and static imports matched reference35346, both reports contained15
preload rows, and the sole difference was Mammoth's added `document-symbols` dependency. The written
entry remained +42 raw/+14 gzip. Session93900 is retained as incomplete because its report was
truncated. A later full portal build produced a different observed entry/CSS hash, so18655 was not
reused as exact-current written-artifact identity; no cause is inferred from that observation.

Final paired current-input graph sessions93377 (reference) and9615 (candidate) both exited zero
with the same98 portal/lock inputs unchanged. The reference entry was
`Ccg1lsbi`,486262 bytes/145047 gzip; the candidate was `By-iAj3X`,486304/145053, an exact +42 raw/
+6 gzip bookkeeping delta. Both had the exact same111 initial module IDs, no static imports and15
preload rows. The sole preload difference was Mammoth to `document-symbols`. Symbols remained
deferred at119992/12366 across four modules and Mammoth at384036/117259 across365 modules; neither
entered the initial closure. The maximum JavaScript chunk was486304, below the unchanged500000
threshold. Candidate identity matched the actual full-Jobs portal build, all parent assertions
passed, and both canonical graph parents were empty. This closes the refined bundle measurement
gate without a latency claim.

## Resolved browser/controller findings

- **Resolved P1 — timeout and cleanup could hang, skip, or unlink owned resources.** The first
  wrapper supplied only SIGTERM, and sequential child cleanup allowed a browser-close failure to
  skip HTTP shutdown. The corrected child uses single-flight cleanup, independently settles the
  client connection, owned BrowserServer process and HTTP listener, bounds graceful close and
  owned-handle kill, awaits the engine process `close` event, and requires exact-port
  `ECONNREFUSED`. The wrapper uses bounded TERM/KILL, rejects oversized output and deletes its root
  only after exit zero plus the complete cleanup report; failures retain the exact root for the
  canonical controller.
- **Resolved P1 — forced browser cleanup did not revalidate current UID.** The focused controller
  initially revalidated PID and private executable path but not the UID immediately before each
  signal. It now rechecks current UID and command beside the PID, tolerates only `ESRCH`, inventories
  processes and open files even on failure, and makes any forced cleanup fail the gate. Canonical
  process-group TERM/KILL remains the final containment boundary.

The fixture otherwise uses the correct production seam. Its test-only virtual entry imports the
real `importResume` under the actual Vite config; its route and server allow only exact built GET
assets on one loopback origin. Successful PDF extraction requires the observed greater-than-1MB
worker, and Wingdings F0A7-to-`▪` conversion exercises the deferred dingbat mapping. TXT,
malformed and oversized cases are exact and synthetic.

## Focused executable evidence

Session14801 is parent-reported runtime evidence; this reviewer did not independently rerun it. It
exited zero on a frozen766-file scoped manifest with SHA-256
`bb12c1d0dcbb772053e1b54a5b0982c1785fe72b5233a8622a5e1cae115097b4` and base479377dc.

- Portal typecheck passed. The10 pure policy cases and one configured-Chromium wrapper passed with
  no skip. All five PDF/DOCX-symbol/TXT/malformed/oversized browser cases passed through production
  `importResume`; the real PDF worker was observed, and unexpected requests/page errors were zero.
- Browser connection, owned engine process, HTTP server and exact TCP listener all closed. The
  controller found zero private-engine processes/open files and performed no forced cleanup.
- The private compiled-automation font smoke passed with zero warnings and fetches while preserving
  exact source PDF bytes/SHA and the frozen materialized snapshot.
- All766 source hashes, HEAD and scoped status matched. Parent independently rehashed the same
  manifest, observed the exact root and parent empty, and found no owned process residue.

This is focused synthetic local evidence only. It is complemented, rather than replaced, by the
full Jobs and Rust/PostgreSQL/source-identity evidence below.

## Full Jobs evidence

Parent-reported session34408 exited zero on the same766-file/source manifest. The three Node suites
reported8/7/17 tests, and the five workspaces reported807/219/308/321/398 tests:2085 total with zero
skips. All five typechecks and builds, schema parity114 tables/95 indexes and CI guards passed. The
actual portal configuration wrote only private output. Final and independent checks found zero
private-engine processes/open files, no forced cleanup, unchanged hashes/HEAD/status, and an absent
exact root with empty parent.

This closes the aggregate Jobs regression gate. The exact-current paired graph is proven above.

## Current-source Rust and PostgreSQL evidence

Parent-reported session66480 terminated exit zero on the same frozen766-source manifest and
HEAD479377dc. It passed38/38 actual private-PostgreSQL selections,30/30 focused cases, formatting
and strict all-target Clippy in1m50s. PostgreSQL stopped cleanly and its PID-file absence was
checked before the SQLite/full-suite stages.

The observed all-target summary vector was1732,0,1,14,1,1,2,113,2,1,5:1872 passes,0 failed and0
ignored, comprising1732 library,1 binary,134 integration and5 guarded-example tests. The library
stage took2489.29s and integration_e2e's113 cases took542.20s. All766 source hashes, the manifest,
HEAD and scoped status matched at terminal. Parent independently rehashed the manifest and checked
the exact root absent, parent empty, owner PID84412 and PGID84486 absent, and no executable under
the old root; free disk returned from30GiB to38GiB.

Launcher self-test22258 also exited zero. Its deliberate live-group cleanup refusal/status39 is an
expected negative case, not a failed cleanup. All14 known Phase637 parents were reported empty or,
for the unused self-test path, absent. This reviewer did not independently rerun these commands.

## Pre-closeout source and record review

The line-by-line current20-path pass found no P0/P1 issue in the Phase637 implementation,
qualification harnesses or evidence records. The font helper resolves only the installed package
asset, production retains the existing PDF limits, the chunk policy is package-boundary exact, and
the browser/controller cleanup remains fail-closed. `CHANGELOG.md`, the Round, IMPL, FIX records,
completion ledger and handoff record Rust66480's terminal evidence without promoting it to release
or production proof. The handoff's previously stale reviewer sentence was corrected to identify
only the then-pending review/privacy/checkpoint rather than the already-complete server/PG gate.

- **Open P2 successor hold, pre-existing and outside the Phase637 diff — ordinary cloud run and
  browser-session identities do not currently compose at lease admission.** `queue_application_run`
  derives a40-hex run ID but stages only `cloud-{application_id}` as the browser-session ID.
  Workflow command materialization and the activity/runner preserve both values. In contrast,
  `validate_execution_target_payloads` requires `session.id == run_id`, and the SQLite/PostgreSQL
  target lookups query the browser-session table by run ID. No additional ordinary-route alias
  session was found in the inspected path. This is a source-level fail-closed availability finding,
  not a demonstrated effect bypass, exposed vulnerability or runtime reproduction. Before cloud
  execution qualification, add an ordinary queue-to-lease regression without a fabricated alias
  session and bind the distinct IDs through durable command authority. Do not broaden lookup with
  an OR condition or move the new active-admission dependency into heartbeat, finish or submitted-
  receipt replay; those terminal paths retain separate frozen authority as the handoff records.

Parent-reported final scratch privacy40010 passed2887 indexed paths/2611 text files. All15 owned
new-file hashes and the real index remained unchanged, and the exact scratch root was removed. The
local staging/checkpoint remains pending: it must include only the20 owned Phase637 paths, repeat
cached whitespace/privacy checks, and must not stage inherited
`REVIEW-PHASE-628-JOBS-SOURCE-VERIFIER-RELEASE-V3.md`.

## Current verdict and limits

🟢 **ACCEPT — BOUNDED LOCAL PHASE637 SOURCE QUALIFICATION.** The reviewed font and chunk
changes, actual built-browser import proof, paired complete-input graph, full Jobs regressions and
current-source Rust/PostgreSQL gates passed on frozen source with the recorded cleanup and identity
checks. No P0-P3 finding remains in the Phase637 diff. Final staged cached whitespace/privacy and
the local checkpoint remain mechanical pre-commit gates and must include this finalized review.

This is not manual QA, production readiness, merge/release approval, provider or real-candidate
proof, runner/receipt authority, or completion of the broader Jobs objective. The pre-existing P2
ordinary cloud run/browser-session identity hold above remains successor work and blocks claiming
actual cloud-runner qualification.
