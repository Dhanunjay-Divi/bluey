# Review — Phase 626 conversational formats, models and PostgreSQL

Preflight: `bluey-ops`, `pinky-ops`, `pinky-bluey-integration-ops`;
model contracts reconciled with official provider docs. Updated2026-10-09.

## Source verdict

Accepted source for isolated preprod validation, not production promotion.
Read-only independent reviews found and repaired:

1. Unmeasured GPT6Sol was initially reachable in ordinary fallback chains.
   It now requires exact server-only `BLUEY_GPT6_SOL_BENCHMARK_ENABLED=1`;
   default route lengths/order and existing error expectations are restored.
2. High-context admission prices were also used for exact settlement. The
   final cost now uses exact total prompt-length tiers; admission/provider
   holds keep conservative high rates. Boundary tests cover both thresholds.
3. Short guidance preceded shared planner output instructions. Server-owned
   final presentation rules now come last while retaining all factual/security
   context. Standalone prompts stay unchanged; selected formats remain enums,
   not client-authored system instructions. No transcript import is claimed.
4. Operator transfer now pins source/destination SSH identities, excludes all
   non-provider secrets and installs selected keys atomically/root-only.
   Local/remote interruption handling and no-replace behavior were reviewed.

Provider payloads use Haiku5.5 adaptive/disabled thinking and typed-only visible
text, retaining output usage; GPT6Sol uses explicit reasoning effort and
completion-token limits without incompatible sampling. Historical Haiku4.5
pricing remains. Reviewers found no remaining P0/P1 in these source boundaries.

## Actual final local gate

`scripts/run-pinky-integration-postgres-tests.sh` acquired `mac-heavy`, used
PostgreSQL17.10 on a private Unix socket with no TCP/provider keys, applied
official runtime schemas, and exited0:

- Real PostgreSQL ownership/transaction/racing Stop+admission test: 1/1.
- Focused Pinky: 35/35.
- All-target: 932/932 (848 library +84 integration).
- Strict all-target Clippy `-D warnings`: passed.
- Cargo fmt, documentation entry checks, shell syntax and diff checks: passed.
- PostgreSQL stopped; private DB/build root and owned processes were removed.

Earlier compile/socket failures and the two default-on fallback test failures
are retained in the [PG receipt](FIX-PHASE-626-PINKY-POSTGRES-PARITY.md).
They are not erased or misrepresented as passing attempts.

## Remaining gates

Latest Linux artifact build/cutover, real model request/stream/usage/latency,
fresh Short/STAR quality checks, native physical Mac/Windows and full promotion
matrix remain required. Prompt word limits are probabilistic: test real output,
do not claim a source assertion guarantees every answer. Private Otter memory
requires an explicit import, verified speaker/fact provenance and tenant-aware
retrieval/deletion before it can be described as available.

Bluey/Pinky production, existing Pinky preprod and Jobs remain outside deployment
scope. No GitHub runner or direct-main push is part of this acceptance.
