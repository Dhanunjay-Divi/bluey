# Round 472: 50-Question Interview Quality Evaluation

Date: 2026-07-10

Backup task id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Exercise Bluey's production managed-answer path with a realistic, cross-role interview corpus and make reliability, latency, routing, human voice, context continuity, artifact behavior, and customer cost measurable before more UI polish or a release.

This round did not deploy, publish a desktop build, run GitHub Actions, or commit/push the work.

## Private Test Sources

The 50 prompts cover software engineering, data engineering, data science/AI, behavioral interviews, production scenarios, coding and code follow-ups, system design, and system-design follow-ups.

The corpus was informed by local resume/JD/behavioral files in Downloads and by recording metadata plus visible summaries/outlines from the owner's authenticated Otter workspace. Otter transcript pages were lazy-loaded and unreliable during inspection, so this round did not copy private raw Otter transcripts into the repository or claim full transcript coverage.

Raw extracted document text, context hashes, model answers, and per-question results remain under the gitignored local directory:

```text
tmp/bluey-interview-eval-20260710/
```

No private resume, job-description, Otter transcript, or answer content was added to this round document.

## Evaluation Harness

Added `scripts/bluey-interview-eval.py`.

The harness:

- builds exactly 50 linked and standalone cases from seven local source files;
- redacts obvious email, phone, URL, and street-address patterns before sending source context;
- logs in through the normal managed account API using an internal smoke account;
- calls the production `/router/complete/stream` SSE path;
- records status events, visible deltas, final text, artifacts, provider/model, tokens, errors, latency, and customer billing fields;
- retries only recognized temporary capacity failures;
- enforces a customer-cost guard;
- emits a local JSONL record, answer review document, summary, report, and source manifest;
- logs out unless `--keep-login` is explicitly requested.

The deterministic score is an evaluation aid, not a substitute for human review. A scorer correction made after the run now recognizes `O(...) time` under a `COMPLEXITY` block as explicit time complexity and reports the actual number of planned cases for limited smoke runs.

## Live Baseline

The production-backed run covered all 50 cases in approximately 10.5 minutes.

| Metric | Result |
| --- | ---: |
| Final reliability | 50/50, 100% |
| First-attempt reliability | 50/50, 100% |
| Dropped/capacity-visible failures | 0 |
| Corrected deterministic average | 90.8/100 |
| Median first token | 2,000 ms |
| p90 first token | 7,145 ms |
| p95 first token | 8,543 ms |
| Maximum first token | 38,926 ms |
| Median total response | 11,011 ms |
| p90 total response | 15,199 ms |
| Maximum total response | 49,383 ms |
| Metered customer usage rows | 266 cents |
| Trial seconds consumed | 577 seconds |
| Paid balance consumed after trial | 19 cents |

The saved baseline `summary.json` still contains the original 90.6 score and two false-positive `missing_complexity` findings. Re-evaluating those two cases with the corrected scorer raises the aggregate to 90.8 and removes both findings. The raw result files were intentionally left immutable.

## Provider Mix

| Final provider/model | Answers |
| --- | ---: |
| OpenAI GPT-5.5 | 24 |
| DeepSeek V4 Flash | 10 |
| DeepSeek V4 Pro | 1 |
| Anthropic Claude Sonnet 4.6 | 8 |
| Z.AI GLM-5.2 | 7 |

Gemini produced no final answer in this sample because cooling/capacity paths fell through to healthy providers. The fallback system preserved 100% user-visible reliability, but two deep paths waited too long before a useful provider began streaming.

Median first-token latency by final provider was approximately 1.35 seconds for Anthropic, 1.12 seconds for DeepSeek, 2.91 seconds for OpenAI, and 2.57 seconds for Z.AI. Final-provider latency does not capture all time spent on failed or cooled routes before fallback.

## Concrete Findings

### Routing

Production misclassified several real prompts:

- project and pipeline walkthroughs were sent to Coding or System Design instead of behavioral interview coaching;
- "tell me about a time" prompts sometimes became Research/web-search work;
- simple graph/cache/API concepts were sometimes sent to Coding;
- system-design prompts such as feature stores, payment processing, and RAG platforms sometimes stayed General and produced no design artifact;
- a URL-shortener design prompt was labeled Behavioral;
- a failure story could become a coding artifact;
- explanatory coding follow-ups were over-promoted to the deep lane.

These were AnswerPlan errors, not merely provider-writing differences.

### Human Voice

The baseline contained:

- 37 visible em dashes in server-direct answers;
- five assistant/meta openers such as "Sure" or "Here is";
- two self-introductions that did not begin directly with "I'm" or "My name is";
- six strict speakability findings where the answer read as advice instead of the candidate speaking naturally.

The strongest answers used first person, stated the decision early, connected technical work to impact, and left enough structure to skim without sounding like a template.

### Artifacts And Follow-Ups

Five design cases that should have produced a workbench artifact did not. One design path produced a correct artifact, proving the renderer was capable but the plan was inconsistent. Several follow-ups retained enough semantic context in prose but selected the wrong intent/lane, which risks stale or irrelevant canvas behavior.

### Latency

Most fast answers began in roughly one to three seconds, but long-tail routing was unacceptable:

- a flaky third-party API scenario took about 16.7 seconds to first token and 39.4 seconds total;
- a failure-story prompt took about 38.9 seconds to first token and 49.4 seconds total.

These cases justify a tighter first-token/connect budget for deep fallback routes. The idle timeout remains longer so valid long code streams are not cut off after they begin.

## Local Fixes

The following changes are present locally in `server/src/api/router.rs` and are not deployed:

- deep first-token fallback budget reduced from 15 seconds to 8 seconds;
- deep route-connect budget reduced from 25 seconds to 15 seconds;
- deterministic visible-answer sanitization removes em dashes before display, persistence, and artifact extraction;
- quick concepts can remain quick even when resume/session context exists;
- direct behavioral and system-design intent is decided before coding intent;
- direct code-generation detection now requires a real generation/debug signal instead of loose words such as `code`, `API`, `class`, or a language name;
- explanation-only coding and code follow-ups use the balanced lane;
- behavioral detection now covers project walkthroughs, ownership, failures, ambiguous requirements, challenged decisions, coaching, and priority negotiation;
- system-design detection now requires a design frame plus a design target, with explicit continuation rules for design follow-ups;
- explanatory design follow-ups remain compact while change/redesign requests can update the workbench;
- the universal answer contract begins with the answer itself, uses natural paragraphs and blank lines, and forbids filler/meta openers and em dashes.

Regression tests encode production-derived prompts for concepts, behavioral stories, coding, system design, and follow-up semantics.

## Verification

Passed locally:

```bash
python3 -m py_compile scripts/bluey-interview-eval.py
/Users/uno/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/bin/python3 scripts/bluey-interview-eval.py --dry-run --output tmp/bluey-interview-eval-dryrun-round472
cargo test --manifest-path server/Cargo.toml api::router::tests --lib --quiet
cargo test --manifest-path server/Cargo.toml --lib --quiet
cargo clippy --manifest-path server/Cargo.toml --lib -- -D warnings
git diff --check -- server/src/api/router.rs scripts/bluey-interview-eval.py
```

Results:

- router tests: 96 passed;
- server library tests: 294 passed;
- strict clippy: passed;
- evaluator compile and 50-case dry run: passed;
- scoped whitespace check: passed.

## Operational Note

The saved password for the internal smoke account had become stale. Only that internal test account's password hash was repaired through the production database using the existing saved test credential and the normal bcrypt cost. Authentication was not weakened, no customer account was changed, and no balance was granted. The live run used 577 remaining trial seconds and then 19 cents from its existing balance.

## Release Gate

This live baseline validates current production behavior. It does not validate the local fixes until they are deployed.

Before a signed release is considered ready:

1. Deploy the exact reviewed artifact to a canary/preproduction target.
2. Rerun the same 50 cases against that target.
3. Require 50/50 final and first-attempt reliability.
4. Compare intent, lane, provider fallback, artifact type, first-token p50/p90/p95, total latency, and customer cost against this baseline.
5. Require zero meta openers, zero visible em dashes, correct self-introduction openers, and correct code/design artifact behavior for the encoded cases.
6. Manually review the raw local answers for truthfulness, natural speech, role fit, and context continuity before promotion.

No deployment was performed in this round, per owner instruction.
