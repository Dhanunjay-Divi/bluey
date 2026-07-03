# Round 327 - Resume Intro First Pass Length

Date: 2026-07-03
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-overlay-spacing-20260626

## Trigger

The owner showed a resume-introduction flow where the first answer to:

```text
give me introduction based on the resume
```

was too short. Only after multiple follow-ups did Bluey produce the fuller, speakable introduction the user expected.

## Root Cause

- Resume/self-introduction questions were detected as behavioral/interview context, but the answer plan still used `output=compact`.
- The generic answer-plan prompt told the model to keep overlay answers compact, which competed with the behavioral prompt that asked for a 45-60 second self-introduction.
- Phrasing such as `give me introduction based on the resume` was weaker than the explicit phrase `tell me about yourself`.
- Follow-ups like `i want a long answer` were not treated as follow-ups strongly enough, so Bluey could ask what topic the user meant instead of expanding the previous resume answer.

## Fix

- Added a dedicated `interview_answer` output shape for behavioral interview answers.
- Behavioral/resume interview prompts now use `output=interview_answer` instead of `output=compact`.
- Added a resume-intro detector for phrasing like:
  - `give me introduction based on the resume`
  - `intro based on attached document`
  - `about yourself` / `about myself` with resume/background context
- Updated answer-plan instructions so `interview_answer` means:
  - give the full first-pass ready-to-say answer
  - do not provide only a teaser
  - do not ask for clarification when resume/JD/context is already supplied
  - use tight speakable paragraphs, usually 45-90 seconds depending on the prompt
- Treated `long answer`, `longer answer`, `more detail`, `expand`, and `elaborate` as follow-up signals and memory-lookup triggers.

Mac/Windows parity: this is shared API routing and prompt logic, so both macOS and Windows clients get the same behavior after the server deploy.

## Verification

Passed locally:

```bash
cargo fmt --check
cargo test --manifest-path server/Cargo.toml answer_plan_ -- --nocapture
cargo check --manifest-path server/Cargo.toml --bin bluey-server
```

New regression coverage:

- `answer_plan_resume_intro_gets_full_first_pass_interview_answer`
- `answer_plan_long_answer_request_is_followup_not_orphan_question`
- existing behavioral/interview evals now expect `AnswerOutput::InterviewAnswer`

## Deployment

Completed:

- Production API server deployed from commit:
  `784e7ee57c9a87b9cc964dc4050e8d4943cc04f2`
- Build tree:
  `/opt/bluey-build-codex-round327-resume-intro`
- Installed binary:
  `/usr/local/bin/bluey-server`
- Binary SHA256:
  `3329b457a10bcbe352df13d0aaf919cc4d887c96bf8cfc6ed4aa6ec61bf35383`
- Previous binary backup:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260703T231206Z`
- `bluey-api.service`: active
- `NRestarts`: `0`
- Public health reports:
  `status=ok`, `commit=784e7ee57c9a87b9cc964dc4050e8d4943cc04f2`
- Recent warning/error logs after restart:
  no entries

## Remaining QA / Gates

- Retest from the overlay with an attached resume:

```text
give me introduction based on the resume
```

Expected first answer:

- full 45-60 second ready-to-say intro
- first-person, role-specific, human
- uses resume facts without inventing unsupported details
- no need to ask again for a long answer
