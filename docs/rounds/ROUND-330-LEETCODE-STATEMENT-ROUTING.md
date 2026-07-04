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

Completed.

- Commit: `a11feb9a9f97b6e631a5487b41a8e7d0a7c1eaa1`
- API host: `root@165.227.77.152`
- API build directory: `/opt/bluey-build-codex-round330-leetcode-routing`
- API binary backup before replacement:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260704T062707Z`
- API binary SHA256:
  `2d417c8d6ff3c0c8df1c6addf8c71a2ebb6d1367111f59a5f97726364cfed789`
- API service after restart:
  `ActiveState=active`, `SubState=running`, `MainPID=1110744`, `NRestarts=0`
- Public health:
  `https://bluey.sh/health` returned commit `a11feb9a9f97b6e631a5487b41a8e7d0a7c1eaa1`.
- Recent API warning/error scan after restart was clean.

Desktop release:

- Version: `0.1.68`
- Public manifest: `https://bluey.sh/latest.json`
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.68/bluey-0.1.68-darwin-arm64.tar.gz`
- Darwin arm64 SHA256:
  `68d8a490856291e8f51d8f4a3d8b058e0d3a4b8cbf80a7f23dc75d8b8f2de516`
- Publish verification passed:
  - release artifact dev-flag/secret scan
  - `latest.json` signature verification
  - `install.sh` MIME check: `application/x-shellscript`
  - `install.ps1` MIME check: `application/x-powershell`
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` report `0.1.68`
- Local public install completed from `https://bluey.sh/install.sh`.
- Local binaries:
  - `/Users/uno/.bluey/bin/bluey --version` -> `bluey 0.1.68`
  - `/Users/uno/.bluey/bin/bluey-daemon --version` -> `bluey-daemon 0.1.68`
- Local daemon restarted into fresh session `71215cf6-00da-4c79-bb10-c38de846820c`.

Live QA after deploy:

- First Alice/Bob prompt request: `75403b9f-33b6-4564-99fe-4f3d541e9edf`
  - Answer contained Approach, complete Python code, Explanation, Line notes, Complexity, and Edge cases.
  - Server plan: `answer_intent=coding`, `answer_output=code_artifact`, `effective_lane=deep`.
  - Gemini returned a 429; Bluey cooled that key and fell back to OpenAI `gpt-5.5`.
- Short follow-up request: `bc90f545-7419-4477-ab66-ae454f3550cb`
  - Prompt: `Can you give me Python code?`
  - Answer reused the prior Alice/Bob problem and returned full Python code instead of asking for the prompt again.
  - Server plan: `answer_intent=coding`, `answer_output=code_artifact`, `context_coding_signal=true`.

## Remaining QA / Gates

- Continue monitoring live overlay streaming separately; this round only fixed misrouting and follow-up context for algorithmic coding prompts.
