# Round 309: Role-Agnostic Interview Coaching

Date: 2026-07-02
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## User-Visible Problem

Round 308 improved Bluey's answer style using a BIE interview-prep thread, but the wording and detection were too anchored to BI. The user clarified that Bluey should work across SDE, data engineering, BI, data science, DevOps, security, product, and any role supplied through resume/JD/API context.

The product goal is role-aware interview coaching, not a BIE-only mode.

## What Changed

- Generalized managed AnswerPlan interview coaching detection from BI-only signals to role/domain signals:
  - SDE / software engineer / backend / frontend / full-stack,
  - data engineer / BI engineer / analyst / data scientist / ML,
  - DevOps / platform / cloud / security / product,
  - project, production incident, debugging, pipeline, dashboard, tradeoff, stakeholder, and pushback prompts.
- Updated managed behavioral prompt style to infer the role from supplied resume, JD, documents, transcript, screen, and API context.
- Updated local/direct daemon prompt style to use role/domain interview coaching instead of BI/data-only wording.
- Added guards so direct code or system-design prompts that mention interviews still route as coding/system-design unless the user is asking how to answer or tell a story.
- Added SDE and data-engineering regression cases.

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

Expected behavior now:

- "Tell me about yourself" uses the attached resume/JD role context.
- "For an SDE interview, how should I answer if they ask about a production incident?" produces a candidate-ready SDE answer.
- "For a data engineer interview, talk about a pipeline you built" produces a data-engineering answer.
- "Can you talk about a dashboard you built?" still works for BI.
- "Write LRU cache code in Python for an SDE interview" remains a coding task, not a behavioral story.
- "Design a scalable notification system for an SDE interview" remains system design, not behavioral.
