# Delegated STAR live grounding failure

Date: 2026-10-09. Preflight: `bluey-ops`, `pinky-ops`, integration ops.

Exact isolated source `d203d0d4` passed the Short real test (85 words,
884ms first text/1358ms total). Default also completed (174 words,
1721ms/2764ms). The fixed synthetic STAR scenario completed and settled,
but failed labels and manual factual review: 157 words, 989ms/2141ms, no
four section labels, and invented elapsed time, query diagnosis and a lasting
outcome. These were not owner transcripts or real candidate facts. Preserve
the failed result; delivery/accounting success is not answer-quality success.

The shared managed base contract explicitly says to shape STAR internally
while telling an unlabeled natural story (`crates/cue-core/src/prompt_contracts.rs`).
The delegated mode only said to organize as STAR. The new final delegated
rule explicitly replaces that earlier **presentation** preference with four
plain-text labels, while retaining all factual/security rules. It forbids
plausible unsupported dates, tools, diagnoses, actions and outcomes, and
distinguishes a one-time verified result from an ongoing guarantee.

No shared standalone contract or story authority is weakened. No second model
rewrite, trimming, fabricated data or broad memory access is added. Source
tests check the explicit override and non-embellishment instruction. These are
prompt invariants, not semantic proof that arbitrary model answers are true;
the new exact artifact requires real synthetic STAR verification.

Local repair gate passed: real PostgreSQL 1/1, focused delegation 35/35,
all-target server 932/932 and strict all-target Clippy with `-D warnings`.
The owned `/tmp/bluey-pinky-postgres-tests.KuTzVN` DB/build root was removed
and no owned PostgreSQL/compiler process remained. Independent source review
found no P0/P1 defect; arbitrary semantic truth remains a live quality gate.

The exact repair `556525a5` was locally built and cut over only on isolated
preprod. Fresh automated STAR labels/transport/settlement passed, but manual
strict factual review **failed**: output added an improved-plan/access-pattern
claim not supplied. A separate visible answer also claimed timeouts resolved
beyond the verified one-time result. Do not mark this grounding bug closed or
promote the product on those results. See the exact-artifact receipt for source,
hashes, individual timings, cleanup, retained rollback and remaining gates.
