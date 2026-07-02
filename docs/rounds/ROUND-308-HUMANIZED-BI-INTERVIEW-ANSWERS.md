# Round 308: Humanized BI Interview Answers

Date: 2026-07-02
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## User-Visible Problem

The user shared a ChatGPT interview-prep thread and asked Bluey to match or beat that answer style. The useful pattern in the shared thread was not just accuracy. It was interview coaching:

- infer what the interviewer is testing,
- give a ready-to-say first-person answer,
- anchor answers in the user's companies, tools, dashboards, pipelines, and metrics,
- explain or recover the story when the interviewer challenges weak logic,
- answer BI/data questions like a production BI engineer, not like a generic chatbot.

Bluey's existing behavioral mode covered self-intros and classic STAR prompts, but missed BI interview prompts such as dashboard-built-from-scratch, favorite SQL function, Tableau/backend lag, and Dive Deep follow-up challenges.

Shared source reviewed:

- `https://chatgpt.com/share/6a4694e4-221c-83ea-a691-ca91fd2c869b`

## What Changed

- Added BI/data interview coaching signals to the managed AnswerPlan detector:
  - dashboard built from scratch,
  - business problem / metrics / visual choices,
  - favorite SQL function,
  - in-depth analysis and focusing on the right problem,
  - Tableau filters / backend lag,
  - interviewer pushback and "how should I answer" prompts.
- Expanded managed behavioral AnswerPlan style so Bluey:
  - infers the interviewer intent internally,
  - gives a ready-to-say candidate answer,
  - anchors on supplied company/project/tool/metric context,
  - adds a brief why-it-works or pushback recovery line when useful,
  - reframes weak story logic in production-realistic terms instead of defending it blindly.
- Mirrored the same BI interview coaching mode in the native/direct daemon provider prompt path, so local overlay and managed server paths stay aligned.
- Kept self-intros constrained to about 45-60 seconds, while fuller interview stories can use 45-90 seconds.
- Added regression tests for the shared-chat patterns.

## Files Changed

- `server/src/api/router.rs`
- `crates/cue-daemon/src/app.rs`

## Verification

```bash
cargo test --manifest-path server/Cargo.toml answer_plan -- --nocapture
cargo test -p cue-daemon provider_messages_enable -- --nocapture
cargo fmt --all
```

All listed checks passed.

## Notes

This round improves planning and prompt behavior. It does not deploy a new production binary or call live providers. The next best live check is to attach the BI resume/JD and ask the exact prompts from the shared thread:

- "Tell me about yourself."
- "Can you talk about a dashboard that you built from scratch?"
- "What is your favorite SQL function?"
- "How did you know you were focusing on the right problem?"
- "What if the interviewer says the dashboard automation did not solve upstream data arrival?"

Expected Bluey behavior: compact, first-person, speakable answers grounded in the attached resume/JD, with practical recovery lines for interviewer pushback.
