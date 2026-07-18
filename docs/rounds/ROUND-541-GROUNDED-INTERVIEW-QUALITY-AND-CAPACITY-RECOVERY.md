# Round 541 - Grounded interview quality and capacity recovery

Status: local implementation and release verification complete; production
promotion and live acceptance pending

## Trigger

The Round 548 production-backed 50-question run produced 36 accepted outcomes
out of 50 on its raw evaluator. Two subsequent evaluator corrections raised the
evidence-backed replay result to 38 out of 50, but manual human review found a
more important product defect: several answers that passed lexical checks had
invented first-person incidents, ownership, implementation details, or results
from sparse resume bullets. Two other answers ended on provider `length`
terminals and were correctly failed, unbilled, and refunded.

During the correction pass, three delegated Codex analyses also stopped with
`Selected model is at capacity` during remote compaction. Local Git state and
completed evidence were intact, but the interruption exposed the need for a
durable recovery procedure.

## Product changes

- Lived interview stories and lived follow-ups now require one complete,
  question-matching, user-confirmed Situation, Task, Action, and Result source.
  A resume achievement alone is no longer expanded into an invented incident.
- Direct user-confirmed facts remain usable. Explicitly hypothetical or
  coaching-framed questions remain proposed approaches and do not trigger the
  story-facts intervention.
- The evaluator now expects a safe `needs_story_facts` intervention for Q02,
  Q03, Q04, Q12, Q14, Q15, Q25, and Q26, in addition to the five existing
  truth-gap cases.
- Balanced live interview answers prefer the measured fast-quality OpenAI route
  only when OpenAI is already inside the configured preferred tier. Provider
  health, capacity fallback, cost-optimized routing, static quality routing,
  and operator rollback remain authoritative. Resume and job-description edit
  or summary tasks preserve normal provider-mix rotation.
- Compact technical interview answers receive narrowly scoped correctness
  contracts for Kafka lag, late events, independent schema deployment,
  Kafka-to-warehouse effect boundaries, pipeline validation, RAG grounding,
  cost-sensitive fraud evaluation, point-in-time graph fraud, and first-90-day
  role plans.
- The RAG launch-evaluation answer now requires an explicit human-judge
  calibration sentence in addition to the baseline and per-slice launch gate.
- Compact interview output uses the same streamed and terminal appendix guard
  as full interview answers unless the user explicitly requests reasoning.

## Module boundaries

- Added `server/src/api/router/interview_contracts.rs` as the isolated home for
  nine question-scoped interview correctness contracts and their unit tests.
- Moved the contiguous story-grounding test suite into
  `server/src/api/router/tests/story_grounding.rs`.
- `server/src/api/router/tests.rs` fell from 4,787 to 4,034 lines; the new child
  module contains 837 lines. Test behavior and discovery remain intact.
- Earlier in the same release chain, payment evaluator checks were split into
  `scripts/bluey_eval/payment_key_sharing.py`.

## Codex recovery hardening

The compaction handoff now records an evidence-preserving capacity workflow:
use `/status`, compact proactively with `/compact`, switch to an available model
with `/model`, isolate concurrent tasks with worktrees, and commit/push safe
checkpoints before continuing. A remote compaction failure is treated as a
retryable orchestration failure, never as proof that local work was lost.

## Local verification

- Server library tests: 528 passed, 0 failed.
- Server end-to-end tests: 75 passed, 0 failed.
- Real-serve, migration, GDPR cleanup, PostgreSQL compatibility, and doc-test
  binaries: all passed.
- Router tests: 199 passed, 0 failed.
- Interview-contract tests: 2 passed, 0 failed.
- Strict Clippy across all targets and features with warnings denied: passed.
- Rust formatting check: passed.
- Python compilation for the evaluator and extracted payment module: passed.
- Full 50-case evaluator dry preparation from seven local source files,
  including all contract self-checks: passed.
- `git diff --check`: passed.

## Production and rollback gate

This round has no database migration. Before promotion, preserve a fresh
custom-format PostgreSQL backup with a verified restore list, capture the
currently running API binary and checksum in a timestamped rollback directory,
build from the exact immutable source commit, atomically switch only the API
binary, and leave Jobs and Caddy byte-identical and uninterrupted.

After promotion, require the exact commit from public and origin health,
`NRestarts=0`, no new warning-or-higher API journal entries, and a targeted live
acceptance run containing all 13 truth-gap interventions plus Q16, Q18, Q19,
Q21, Q27, Q28, Q31, and Q50. Run the full 50-question evaluation only after the
targeted gate is perfect.

## Production evidence

Pending immutable checkpoint, deployment, and live acceptance. No production
claim is made in this document yet.
