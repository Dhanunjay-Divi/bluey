# FIX-814: Keep the complete DOCX symbol table deferred without an oversized chunk

Codex preflight: bluey-ops; Round637 defines the conjunctive acceptance gates.

## Issue and root cause

The written baseline Mammoth DOCX chunk was503986 bytes, above the existing
500000-byte JavaScript warning threshold. Its largest rendered module was the
dingbat-to-unicode mapping table. This was a deferred import, not initial loading.

## Fix summary and files

`jobs/portal/bundle-policy.ts` assigns only exact dingbat-to-unicode package IDs,
including normalized Windows/CommonJS forms, to document-symbols. The portal's
actual Vite configuration enables onlyExplicitManualChunks so shared helpers
cannot recursively pull that table into the initial closure. Ten pure policy
cases exclude lookalikes and unrelated libraries. No mappings or parser paths
are removed; no warning threshold, dependency or UI behavior changes.

## Written-artifact evidence and tradeoff

Reference35346 reproduced baseline entry486262/145043gzip and Mammoth503986/
131614gzip. Fully captured candidate18655 exited0 with unchanged96 source files:
exact111 initial module IDs and static imports[] match; all15 preload records
match except Mammoth adds document-symbols. Entry486304/145057 adds42raw/14gzip
metadata bytes. Symbols119992/12366 and Mammoth384036/117259 stay deferred; total
gzip129625 is1989 smaller, with one extra parallel-preload asset. No JS chunk
exceeds500000. This is a size/ownership result, not measured user latency.

Candidate65766 remains a failure against the original literal zero-growth rule.
Round637 was explicitly refined before qualification to require exact ownership,
sole named preload addition and at most64raw/32gzip metadata bytes together.
Candidate93900 built successfully but its tool capture truncated the graph;
it is not the identity proof. Session18655 is the complete replacement evidence.

## Browser and final qualification

The private built-import fixture imports production importResume using actual
Vite configuration. Synthetic PDF/real worker, DOCX/Wingdings F0A7 mapping, TXT,
malformed and oversized cases, exact loopback assets and bounded resource cleanup
are required. Those five runtime cases passed in focused14801 and fullJobs34408
with no unexpected requests/page errors and observed engine/server/listener
shutdown. Fresh final reference93377/candidate9615 cover all98 current portal/
lock inputs: entry486262/145047gzip versus486304/145053 (+42/+6), exact111
module IDs/static imports[], sole named added preload dependency and unchanged
deferred chunks. No JS chunk exceeds500000. Server66480 subsequently passed1872
all-target/38 actualPG/30 focused/strict Clippy/formatting and exact source/
cleanup checks. Final review/privacy remain, with no release claim.
Independent review identified lifecycle weaknesses in the first test harness;
owned engine close/kill escalation and failure-root retention address them before
execution. No owner sessions, candidate documents or external services are used.

## Limits

No live submission, source coverage, runner certification, mail integration,
production enablement, deployment or broader Jobs completion is proven here.
