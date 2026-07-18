# Round 541 - Grounded interview quality and capacity recovery

Date: 2026-07-18

Status: deployed and live verification complete

Runtime source commit:
`5616d85596a9259d46795a090123928a06460bc5`

Live API: `https://bluey.sh/health`

## Outcome

This release closes the answer-quality defects found through repeated real
50-question interview runs, preserves strict truth grounding, and improves the
release-time module boundaries without mixing a high-risk architecture rewrite
into a production correction.

The deployed API now passes the exact targeted Q30, Q39, Q47, and Q48 questions
on real first attempts. A fresh no-retry 50-question run produced 49 raw
accepted outcomes; human review found the sole raw failure to be a semantically
complete Q39 child-operation boundary expressed in a second safe form. The
evidence-backed evaluator replay accepts all 50 outcomes with an average score
of 100 and no remaining issues. All 13 deliberate truth-gap cases returned the
safe user-input intervention instead of fabricating candidate history.

## Product changes

- Lived interview stories and lived follow-ups require one complete,
  question-matching, user-confirmed Situation, Task, Action, and Result source.
  Resume bullets and preparation documents cannot be expanded into invented
  incidents, actions, employers, or outcomes.
- Compact technical interview answers use question-scoped correctness
  contracts for Kafka lag, late events, schema rollout, warehouse effects,
  pipeline validation, RAG grounding, fraud evaluation, graph leakage, first
  90 days, and LLM serving quality.
- Q30 now requires an explicit production release rule: measure p95 latency and
  answer quality against the same baseline and traffic slices, use a bounded
  canary, and roll back on a quality-gate regression. The detector is scoped to
  p95 LLM-serving interview questions and does not rewrite p99, database,
  email, summary, or resume surfaces.
- Q35 standalone messaging recovery now preserves one authoritative
  per-conversation sequence, stable message identity, duplicate suppression
  before delivery, reconnect from the last durable sequence, and fenced
  failover.
- LRU answers require a runnable first-principles hashmap plus doubly linked
  list implementation. Buffered stream and terminal answers remain identical.
- Q39 accepts two exact, equivalent customer-visible child-operation contracts:
  the canonical sentence and the observed safe variant that also states a new
  key. Hidden, negated, incomplete, or new-payment-intent variants still fail.
- Q47 distinguishes present/future guidance such as `In practice, that keeps me
  objective` from unsupported past-work claims. Shared director alignment and
  the policy-governed incident exception remain mandatory.
- Q48 recognizes natural learner ownership such as `they keep ownership`, while
  an isolated coaching safety gate rejects affirmative first-person takeover,
  including rewriting, fixing, implementing, or taking ownership of the work.
- The human-voice score now requires first-person candidate voice for decisions,
  scenarios, lived answers, and direct `how would you` questions, but not for a
  clear conceptual explanation such as LRU mechanics.

## Module boundaries and large-file decision

This release retained the earlier safe splits and placed new behavior in the
smallest relevant module:

| File | Lines | Release decision |
| --- | ---: | --- |
| `server/src/api/router.rs` | 11,329 | No new Q30 logic added here |
| `server/src/api/router/interview_contracts.rs` | 743 | Q30 contract and tests live here |
| `server/src/api/router/visible_output.rs` | 900 | Tests remain in `visible_output/tests.rs` (425 lines) |
| `server/src/api/router/response_artifacts.rs` | 1,409 | Tests remain in `response_artifacts/tests.rs` (146 lines) |
| `server/src/api/router/sse.rs` | 55 | Streaming boundary remains isolated |
| `server/src/api/router/tests.rs` | 4,822 | Story-grounding tests remain in a separate 837-line module |
| `scripts/bluey-interview-eval.py` | 4,493 | Semantic families remain imported modules |
| `scripts/bluey_eval/lru_contracts.py` | 820 | Runnable-code and LRU semantics |
| `scripts/bluey_eval/coaching_contracts.py` | 222 | Q48 affirmative ownership and takeover safety |
| `server/src/db/jobs_tailoring.rs` | 560 | Resume-tailoring behavior is isolated on current mainline |

A fresh alternate-model inventory found that the remaining largest production
files have real cross-cutting boundaries: `crates/cue-daemon/src/app.rs` is
26,484 lines and `server/src/db/jobs.rs` is 12,554 lines. Mainline now isolates
resume-tailoring behavior in `jobs_tailoring.rs`. The remaining inline test
blocks can be extracted safely in a dedicated refactor. Splitting the production
lifecycle, IPC, transaction, lease, and cross-database logic during this release
would raise regression risk. The next architecture refactors
should proceed in this order: daemon app, Jobs database, then router answer-plan
and prompt contracts, each with an isolated equivalence test gate.

## Codex capacity recovery

The interruption message `Selected model is at capacity` was observed in three
delegated read-only tasks while Codex attempted remote compaction. It was a
retryable model-pool/orchestration failure; it did not modify Git state,
production, or the working tree.

The durable operating procedure is:

1. Use `/status` before a long release phase and `/compact` before the context
   window is nearly full.
2. If one model pool is saturated, switch with `/model`; this release retried
   the failed inventory successfully on the alternate Terra pool.
3. Keep noisy exploration and log analysis in bounded subagents so the primary
   task retains requirements and decisions.
4. Use `/worktree` or a dedicated Git worktree for concurrent implementation.
5. Commit and push every verified checkpoint before a remote build or live run.
6. Record exact commit, archive, backup, binary, rollback, and live-result
   evidence in the round document.

The documented commands and their purposes are in the
[Codex IDE slash-command reference](https://learn.chatgpt.com/docs/developer-commands.md?surface=ide).
The exact capacity wording is not documented as a repository failure and must
not be treated as evidence of lost local work.
No repository setting can reserve remote model capacity; these controls make a
capacity interruption recoverable, not impossible.

## Verification before promotion

- Server library tests: 570 passed, 0 failed on rebased mainline.
- Server end-to-end tests: 75 passed, 0 failed.
- Real-serve, context migration, GDPR cleanup, and PostgreSQL schema integration
  tests passed.
- Strict Clippy across all targets and features with warnings denied passed.
- Rust formatting passed.
- Python compilation for the evaluator and every `bluey_eval` module passed.
- Full 50-case dry preparation from seven local evidence files passed all
  startup and adversarial self-checks.
- Saved Round 562 replay accepts Q39, Q47, and Q48 while continuing to reject
  the pre-fix Q30 answer that omitted measurement/canary wording.
- Independent alternate-model reviews of Q30 and the Python semantic guards are
  clean after every reported false-positive and bypass case was fixed. The Q48
  review replayed all three exact saved production answers as positive controls.
- `git diff --check` passed.

## Immutable build, backup, and rollback evidence

Source archive:

```text
Path     /tmp/bluey-round563-5616d855-source.tar.gz
SHA-256  8e9549a791cc9758ca6528ea16463fba0c850c1d512c4cef629e45deb59cde6e
Bytes    33,117,592
```

Fresh pre-deploy PostgreSQL backup:

```text
Path             /var/backups/bluey-api/hourly/bluey-postgres-20260718T152049Z.pgdump
SHA-256          75fae02b5c06196a0d6ebbdb2458b6675cdcc1c1f583c261156d9cef53983e50
Bytes            24,685,301
Restore entries  356
```

Release and rollback:

```text
Release   /opt/bluey-releases/round563-5616d855
Binary    /opt/bluey-releases/round563-5616d855/artifacts/bluey-server
SHA-256   91326939bf444a462e2cd3982c3f65e726ab77da3f93bf92be0e7053c741d370
Bytes     25,920,208
Rollback  /var/backups/bluey-api/releases/20260718T152614Z-before-round563-68e42fb3
```

The source archive hash matched before extraction. The release binary was built
once from the exact runtime commit under `/run/lock/bluey-api-build.lock`, then
promoted under `/run/lock/bluey-api-deploy.lock`. The previous API binary was
captured before the atomic swap. This release has no database migration.

## Production service evidence

```text
Public health commit    5616d85596a9259d46795a090123928a06460bc5
Loopback health commit  5616d85596a9259d46795a090123928a06460bc5
API PID                 2056986, NRestarts=0
Jobs PID                1980534, NRestarts=0
Caddy PID               1492399, NRestarts=0
API SHA-256             91326939bf444a462e2cd3982c3f65e726ab77da3f93bf92be0e7053c741d370
Jobs SHA-256            35957bec97bd776a25d7548c0fd00039e61d0e62c481f91c48de360e9daab96c
API warnings since swap 0
Root disk               91% used, 5.5 GB available
```

Only `bluey-api.service` was restarted. The Jobs and Caddy PIDs and the Jobs
binary hash stayed unchanged.

## Live production evaluation

Targeted no-retry gate:

- Q30, Q39, and Q47: 100 on their first attempts.
- The first Q48 answer was semantically correct but used the natural phrase
  `they keep ownership`; after the evidence-backed evaluator correction, the
  saved answer passes and a fresh no-retry Q48 run scored 100.
- Targeted first-token p95: 1,792.6 ms.
- Targeted cost: 8 cents total across the four-case run and Q48 confirmation.

Fresh full no-retry 50-question run:

```text
Raw accepted outcomes       49 / 50
Corrected evidence replay   50 / 50
Corrected average score     100.0
Attempts                    50 (one per case; no retries)
Partial stream failures     0
Truth-gap interventions     13 / 13 correct
Overall first-token p95     1,398.8 ms
Answer first-token p95      1,446.7 ms
Intervention first-token p95 244.1 ms
Maximum first-token latency 1,994.5 ms
Provider mix                36 OpenAI, 13 Bluey grounding guard, 1 DeepSeek
Customer cost               54 cents
Balance                     488 -> 434 cents
```

The raw Q39 miss was not a product correctness failure. Its canvas explicitly
contained all four safety properties: new logical partial action, new key,
child provider-operation row under the existing intent, and no new payment
intent. The corrected evaluator accepts that exact visible sentence and keeps
hidden, negated, incomplete, and unsafe variants failing. The raw Q07 style
deduction was also removed because a conceptual explanation does not require a
first-person pronoun; decision and scenario answers still do.

Production remained healthy with zero restart-count growth and zero
warning-or-higher API journal lines after the targeted and full live runs.

## Remaining operational watch

- Root disk headroom is 5.5 GB. This is sufficient for the deployed release,
  but old build targets and release directories should be pruned through the
  storage runbook before the next large native or server build.
- A 50/50 evaluator result is evidence for this fixed suite, not proof that all
  possible interview questions are perfect. Continue sampled production review,
  provider-fallback monitoring, and adversarial contract tests.
- The daemon, Jobs database, and router production splits remain separately
  scoped architecture work; they are not hidden inside this release.
