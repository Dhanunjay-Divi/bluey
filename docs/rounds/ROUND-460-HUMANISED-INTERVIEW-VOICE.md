# ROUND-460 Humanised Interview Voice

Date: 2026-07-09
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Why

Bluey interview answers were still sometimes framed like advice from an assistant:
`I would say...`, `Based on the resume...`, or a compressed factual summary.
That misses the core product feel. For interview, resume, and self-introduction
answers, Bluey should produce wording the user can say directly on a call.

## What Changed

- Tightened the managed server AnswerPlan prompt for behavioral/interview answers.
- Tightened the native daemon provider prompt for the same behavior.
- Tightened the local LLM fallback prompt.
- Added prompt-contract assertions so self-introduction and resume-introduction
  prompts explicitly require:
  - answer as the candidate speaking
  - start with `I'm...` or `My name is...` when a name is available
  - avoid `I would say`, `You can say`, and `Based on the resume` openings
- Kept coding and system-design behavior separate so those answers can still use
  structured approach, code, complexity, and workbench artifacts.

## Expected Product Behavior

For prompts like `tell me about yourself`, `give me an introduction from this
resume`, or messy live-caption interview prompts, Bluey should now start with a
natural first-person answer:

```text
I'm <name>, a ...
```

or:

```text
My name is <name>, and I ...
```

When the exact name is not available, Bluey should still answer in first person
without pretending a name exists.

## Verification

- Passed `cargo test --manifest-path server/Cargo.toml --lib answer_plan_self_intro_is_behavioral_not_system_design --quiet`
- Passed `cargo test --manifest-path server/Cargo.toml --lib answer_plan_resume_intro_gets_full_first_pass_interview_answer --quiet`
- Passed `cargo test --manifest-path server/Cargo.toml --lib answer_plan_role_interview_prompts_are_behavioral_and_humanized --quiet`
- Passed `cargo test -p cue-daemon provider_messages_enable_self_intro_interview_mode_with_resume_context --quiet`
- Passed `cargo test -p cue-daemon test_answer_llm --quiet`
- Passed `cargo fmt --manifest-path server/Cargo.toml --check`
- Passed `cargo fmt -p cue-daemon -- --check`
- Passed `git diff --check -- server/src/api/router.rs crates/cue-daemon/src/app.rs crates/cue-daemon/src/llm/answer.rs docs/rounds/ROUND-460-HUMANISED-INTERVIEW-VOICE.md`

## Notes

- `cargo fmt --all --check` is still blocked by an existing formatting diff in
  `crates/cue-cli/src/app.rs` from prior local work. This round did not format
  or claim that unrelated file.
- `crates/cue-daemon/src/app.rs` already had unrelated local trial-balance
  changes before this round. This round only changed the interview voice prompt
  text and prompt-contract assertions in that file.

## Not Deployed

Per instruction, this round was not deployed and did not use GitHub Actions.
