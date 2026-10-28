# Round637 — Jobs offline document fidelity and measured bundle boundaries

Date:2026-10-28. Base:`479377dcc8cb6928fd038a51c72ac672e5fe4abe`.
Branch:`feat/phase-637-jobs-document-fidelity-and-bundle`.
Preflight:bluey-ops, current AGENTS/local handoff, security/performance/SPARC and
native scoped team. No framework installation/hooks, owner credentials or SSD.
Status: bounded implementation, current-source Jobs/PostgreSQL/Rust, independent
review and scratch privacy passed. Actual-index recheck/local checkpoint pending;
not release readiness.

## Problems and measured baseline

Phase636 is a conditional local checkpoint, not a warning-clean/current-byte
release. Its two new migration EOF-only cleanups need final-source qualification
with this batch; prior1872 Rust/2071 Jobs gates remain dated pre-format evidence.

1. Node document validation uses PDF.js6.2.108 with system fonts disabled but no
   standard-font data directory. Real materialization tests emit warnings; the
   test extraction helper independently omits the same setting. Installed PDF.js
   NodeBinaryDataFactory calls fs.readFile with the supplied string, so a literal
   file-URL string is not the correct Node path. The factory requires trailing `/`.
2. Written-artifact baseline69748 (not pre-final generateBundle estimates):
   initial entry486262 bytes/145043 gzip; deferred Mammoth DOCX chunk503986/
   131614 gzip,369 modules. It is not in the static initial-import closure.
   Largest rendered modules include dingbat-to-unicode, JSZip and xmldom.
   Do not infer user latency from chunk sizes. Existing PDF worker is a real
   1262398-byte asset; its70-byte URL-export chunk is not a missing worker.

## Acceptance

1. Derive offline standard-font location from the installed pdfjs-dist package,
   convert to an absolute local filesystem path and preserve required trailing
   slash/cross-platform behavior. It is server/module-owned, never candidate-
   supplied or remote. Use the shared setting in production and test extraction.
   Do not enable system-font fallback, suppress warnings, fetch a CDN or relax
   input/page/text limits, stopAtErrors, immutable bytes or snapshot validation.
2. Prove a representative standard-font PDF extracts expected text without the
   missing-font warning or network calls. Preserve original materialized bytes/
   SHA and existing corruption, replacement, truncation, path-swap and size/page
   negatives. Validate the compiled Node build as well as Vitest transformation.
3. Test an explicit, narrow symbol-table chunk boundary for Mammoth. Do not drop
   mappings or change document parsing. Keep Rollup dependencies explicit so a
   CommonJS helper cannot drag the symbol table into initial loading. No global
   warning-limit increase, dependency swap or blanket vendor chunk.
4. Measure final WRITTEN artifact bytes/gzip, import closures and module ownership
   using the same actual portal config, inputs and environment. Require no JS
   chunk above the unchanged500000-byte warning threshold and no symbols/Mammoth
   in initial closure. Initial module-ID set/count111 and static imports must be
   identical to the measured default-chunk reference. The only allowed added
   preload dependency is document-symbols; allow at most64 raw/32 gzip entry bytes
   for that metadata only (observed42/14). This is not a general budget increase.
   Record deferred closure
   total bytes/gzip and request changes; no unmeasured latency/SLO claims.
5. Exercise real production portal importResume using built output and configured
   private Chromium: synthetic PDF (real worker), DOCX (including a mapped symbol)
   and TXT, plus malformed input denial. Serve only owned immutable build/fixture
   bytes over loopback, abort unexpected/remote browser requests and use no API,
   account, mailbox, provider, employer or real candidate resume. Prove browser/
   server shutdown and exact temporary cleanup. This is local import proof, not
   source-upload/queue/runner/receipt authority or a real application.
6. Full gates run only after static review and a COMPLETE prospective-index
   whitespace/privacy preflight (including owned new files; inherited REVIEW628
   excluded). Freeze all admitted source until terminal. Run focused document/
   chunk/browser checks first, then38 actual privatePG selections, full server
   all-target tests/strict Clippy/formatting and all Jobs tests/typechecks/builds
   with actual portal config and private output. No silent skips in qualification.
7. Preserve all failed diagnostics and exact source/HEAD/status/cleanup evidence.
   Run final launcher self-test/privacy and independent AI review. No production
   or overall-goal claim follows; runner/receipt, Linux/x86_64, hosted providers,
   source coverage, monthly billing/OAuth, MCP/C2C and release holds remain.

## Ownership and execution

- Parent: docs, measured artifact graph, isolated portal-import fixture/gates,
  command execution, source freeze, cleanup and final integration.
- Existing Terra: automation documents.ts, documents.test.ts and only narrowly
  necessary shared Node-font options module/tests. No portal/docs changes.
- Existing Sol portal specialist: portal vite.config.ts, narrow chunk-policy
  helper and pure policy tests only. No document semantics or automation edits.
- Existing separate Sol: read-only source/security review; REVIEW637 only.

Workers are not alone; preserve other edits and inherited untracked REVIEW628.
No independent heavy execution. Parent holds shared mac-build queue and canonical
launcher; private HOME/data/cache/browser/output and cleanup on every outcome.
No push/PR/merge/rebase/retarget/deploy, signing/publication, paid resource,
production flag, real application/email/calendar, competitor probing, VM start
or desktop/overlay/Pinky mutation. Do not modify meeting-owned `/Users/uno/Downloads/cue`.

## Reviewed acceptance refinement, before qualification

Candidate65766 is retained as a FAIL against the original literal zero-growth
wording, not silently accepted: entry486304/145057gzip vs486262/145043 (+42/+14),
111 modules/imports[]; symbols remain deferred119992/12366 and Mammoth384036/
117259. Combined deferred gzip129625 vs131614 improves by1989 bytes, with one
additional parallel-preload asset. Forcing a shared helper into the symbols chunk
would broaden unrelated document dependencies; removing preload would introduce
a DOCX waterfall. Independent review supports the bounded metadata-only rule
above, conjunctive with exact module/preload identity and browser correctness.
Those remaining proofs are still pending. No startup-latency improvement claimed.

## Complete-current-input qualification

Focused14801 passed the actual five synthetic browser cases, compiled offline
font/snapshot proof and11 portal tests with zero skips. FullJobs34408 passed2085
tests/zero skips, five typechecks/builds and schema/CI guards on frozen766 inputs
with manifest bb12c1d0dcbb772053e1b54a5b0982c1785fe72b5233a8622a5e1cae115097b4.
Both exact private roots were removed after zero engine process/open-file counts.

With the browser fixture now included, generated CSS/entry hashes changed;
18655 is not reused as final artifact identity. Fresh paired
reference93377/candidate9615 use all98 current portal/lock inputs, unchanged:
reference entry486262/145047gzip; candidate486304/145053 (+42/+6), exact111
module IDs/static imports[],15 preload records with only the named symbol
dependency added. Deferred chunks remain119992/12366 and384036/117259; no JS
chunk exceeds500000. The candidate entry hash By-iAj3X matches the full Jobs
build. These are current bundle/fidelity proofs, not user-latency or production
proof. Current66480 subsequently passed1872 all-target Rust/zero failed or ignored,
38 actual privatePG and30 focused cases plus strict Clippy/formatting. Independent
source/root/parent/PID/group checks passed. Self-test22258 passed. Final privacy/
review/checkpoint remain; broader runner/product/release holds are unchanged.
