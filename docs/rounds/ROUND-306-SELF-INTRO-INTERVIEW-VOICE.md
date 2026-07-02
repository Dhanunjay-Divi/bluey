# ROUND-306-SELF-INTRO-INTERVIEW-VOICE

Date: 2026-07-02
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-overlay-spacing-20260626

## User Issue

The user attached `Sukruthi_Korukonda_BIE.docx` and asked Bluey:

> So can you tell me about yourself?

The answer used the resume facts correctly, but it sounded like a compressed resume paragraph instead of a natural interview answer. It lacked the interview arc a human would use: current role, past experience, proof points, and why the background fits the role.

## Diagnosis

- Managed/server AnswerPlan already classified self-intro prompts as behavioral.
- The behavioral style instruction was too generic and did not teach the self-introduction shape.
- The local/direct daemon behavioral interview mode only triggered for story prompts such as `tell me about a time`, not `tell me about yourself`.
- Result: Bluey could extract resume facts, but it did not reliably activate the stronger interview-answer mode for self-intros.

## Changes

### Managed Server

Updated behavioral AnswerPlan instructions to explicitly handle self-introductions:

- avoid compressing the resume into one facts paragraph,
- use a speakable `present-past-fit` arc,
- include current specialty, relevant past experience, proof points, and role fit,
- aim for a 45-60 second response in 2-3 tight paragraphs,
- do not invent metrics, employers, tools, or motivation beyond supplied resume/JD/context,
- never route resume/self-intro prompts into system design.

### Local / Direct Daemon

Updated behavioral interview answer mode:

- triggers for:
  - `tell me about yourself`
  - `tell me about myself`
  - `introduce yourself`
  - `walk me through your resume`
  - `walk me through your background`
  - `my background`
  - `my experience`
- adds self-intro-specific guidance before STAR story guidance,
- keeps the answer first-person and speakable,
- avoids bullets unless the user asks for notes.

## Tests

Added / updated regression coverage:

- `answer_plan_self_intro_is_behavioral_not_system_design`
- `provider_messages_enable_self_intro_interview_mode_with_resume_context`

## Verification

```bash
cargo test -p cue-daemon provider_messages_enable_self_intro_interview_mode_with_resume_context --lib
cargo test -p cue-daemon provider_messages_enable_behavioral_interview_mode_with_resume_context --lib
cargo test --manifest-path server/Cargo.toml answer_plan_self_intro_is_behavioral_not_system_design --lib
cargo fmt -p cue-daemon
cargo fmt --manifest-path server/Cargo.toml
git diff --check
```

All checks passed.
