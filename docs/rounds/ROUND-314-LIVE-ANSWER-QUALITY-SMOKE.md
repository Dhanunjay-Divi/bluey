# ROUND-314 Live Answer Quality Smoke

Date: 2026-07-03
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Run the same style of questions the owner has been testing manually, compare Bluey's responses against the desired ChatGPT-style behavior, and fix the highest-signal response-quality regressions before the next live build.

## Live Smoke Inputs

The smoke pass used production `/router/complete` with the signed-in local account and covered:

- BI/resume self-introduction: `Tell me about yourself`.
- BI dashboard story and interviewer pushback around upstream refresh lag.
- Coding: `Build me LRU cache in Python`.
- Coding follow-up: explain LRU logic and why a doubly linked list is needed.
- Topic reset: Fibonacci followed by LRU.
- Simple Python code: swap two numbers without a third variable.
- Public lookup with missing docs wording: `Secret Passage Ranch in Virginia`.

## Findings

- Behavioral/dashboard answers could become too specific and invent story details like source systems, metrics, clinical/finance context, or latency windows that were not supplied.
- Behavioral/dashboard answers could still create a `system_design` canvas if the generated text contained words like API, database, cache, latency, and dashboard.
- LRU code used a Python library shortcut in one live run instead of the expected interview-style hashmap plus doubly linked list implementation.
- LRU explanation-only follow-ups could create or replace code artifacts even when the user asked for explanation rather than code.
- Public lookup questions containing `attached docs` wording could be treated as missing context instead of entering the managed research lane.
- The web-search heuristic treated `current session context` as an external/current-web signal because it matched the word `current`.
- A post-deploy live mixed prompt (`write Fibonacci`, reduce complexity, then explain LRU) kept the answer useful but failed to return a code artifact because the `explain` term made the planner treat the whole prompt as explanation-only.

## Changes

- Moved research detection ahead of missing-context detection when the prompt is clearly a public lookup.
- Narrowed `current` web-search detection so `current session/context/screen/transcript` stays local.
- Added explanation-only coding detection for prompts such as `explain`, `why`, `logic`, `walk through`, and `how it works`.
- Kept code artifact output for explicit code/edit requests such as `I want the code`, `give full code`, `implement`, `build`, `fix`, `update`, and `add comments`.
- Added `response_artifact_for_output`, so the canvas/sidebar respects `AnswerPlan.output` instead of reclassifying compact/source answers from text alone.
- Strengthened coding prompt instructions so interview data-structure prompts like LRU use hashmap plus doubly linked list by default.
- Strengthened behavioral prompt instructions to forbid invented metrics, employers, tools, source systems, domains, latency windows, outcomes, and motivations.
- Tightened explanation-only detection so explicit code requests such as `can you write`, `write`, `code for`, and language-specific `Python code` still return a code artifact even if the prompt also asks for explanation.

## Regression Tests

Added/updated coverage for:

- `answer_plan_public_lookup_with_missing_docs_still_researches`
- `answer_plan_explanation_only_code_followup_stays_compact`
- `answer_plan_mixed_write_and_explain_keeps_code_artifact`
- `response_artifact_for_output_suppresses_compact_interview_canvas`
- `answer_plan_code_request_uses_deep_code_artifact`
- `answer_plan_role_interview_prompts_are_behavioral_and_humanized`
- `answer_plan_eval_suite_covers_live_overlay_regressions`

## Verification

```bash
cargo fmt --all
cargo test --manifest-path server/Cargo.toml answer_plan_ -- --nocapture
cargo test --manifest-path server/Cargo.toml response_artifact -- --nocapture
cargo test --manifest-path server/Cargo.toml --lib api::router::tests::answer_plan_eval_suite_covers_live_overlay_regressions -- --nocapture
cargo build --manifest-path server/Cargo.toml
```

All listed checks passed locally.

## Deployment Notes

Post-deploy live replay after the first commit showed:

- Self-intro stayed compact, no artifact, no invented risk words.
- Dashboard pushback stayed compact, no artifact, and did not invent the earlier bad tools/metrics.
- LRU code returned a first-principles code artifact with doubly linked list/hashmap signals and no `OrderedDict`.
- LRU explain-only follow-up returned no artifact, preserving the existing code canvas.
- Secret Passage Ranch with `attached docs` wording entered the research path and stated web search was unavailable for the request instead of asking for unrelated documents.
- Mixed Fibonacci/LRU prompt needed the follow-up fix above so it keeps code artifact output.

Final expected behavior after redeploy:

- Behavioral answer should stay grounded and should not open system-design canvas.
- LRU build should stream a full code block/artifact from first principles.
- LRU explain-only follow-up should remain a compact explanation and preserve the existing code artifact.
- Secret Passage Ranch with `attached docs` wording should enter research/source-answer path; if provider search is unavailable, it must say web search was unavailable instead of asking for session documents.
- Mixed code/explanation prompts should still produce a code artifact when they explicitly ask Bluey to write code.
