# Round 330 - LeetCode Statement Routing

Date: 2026-07-04
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-overlay-spacing-20260626

## Trigger

After publishing Round 329, a fresh live CLI smoke against `0.1.67` showed the Alice/Bob coding statement was still logged as `question_intent="general"`:

```text
You are given an array of positive integers nums...
Return true if Alice can win this game, otherwise return false.
```

That explains the weak first answer in the overlay: the prompt is a LeetCode-style coding problem, but it does not literally say `write code`, `implement`, `algorithm`, or `leetcode`.

## Fix

- Added an algorithmic challenge detector for prompts shaped like:
  - `you are given...`
  - `given an array/string/list/matrix...`
  - `return true/false/the...`
  - data-structure/problem words such as `array`, `integer`, `nums`, `matrix`, `tree`, or `graph`
- Added the same `return true if ... otherwise return false` fallback for common boolean challenge statements.
- Wired the detector into:
  - daemon diagnostics (`question_intent_label`)
  - shared local router heuristic (`cue-router`)
  - managed server AnswerPlan (`looks_like_coding_question`)
- Kept the rule conservative so normal prose with the word `return` is not treated as code.
- Bumped the desktop workspace version from `0.1.67` to `0.1.68`.

## Verification

Passed locally:

```bash
cargo fmt --check
cargo test -p cue-daemon answer_diagnostics_classify_question_and_text_shape_without_content -- --nocapture
cargo test -p cue-router leetcode_statement_is_code_and_deep -- --nocapture
(cd server && cargo test answer_plan_leetcode_statement_uses_deep_code_artifact -- --nocapture)
cargo check -p cue-daemon
(cd server && cargo check)
```

Expected classifications now:

- daemon diagnostics: `code_or_debug`
- shared local router: `TaskType::Code`, `Difficulty::Hard`, `LatencyLane::Deep`
- server AnswerPlan: `Coding`, `CodeArtifact`, `deep`

## Deployment

Pending at initial doc write.

## Remaining QA / Gates

- Package and publish desktop `0.1.68`.
- Deploy the managed API classifier fix.
- Retest the Alice/Bob prompt and `Can you give me Python code?` follow-up in a fresh session.
