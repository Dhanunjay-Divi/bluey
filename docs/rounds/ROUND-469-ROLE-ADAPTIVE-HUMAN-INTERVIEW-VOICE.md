# ROUND-469 Role-Adaptive Human Interview Voice

Date: 2026-07-10
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Use the strongest qualities from the supplied human-answer reference without
hardcoding Bluey to one resume, company, or job family. Bluey should answer as
the person in the role: an engineer should sound like an experienced engineer,
a manager should sound accountable for people and decisions, and other roles
should emphasize the work and judgment natural to their domain.

## Design

- Added one shared role-adaptive practitioner contract in `cue-core` so local
  and managed answer paths cannot drift into different interview voices.
- Bluey now infers role, seniority, domain, and decision level from the current
  question plus available resume, job description, transcript, screen, files,
  and retained conversation.
- Individual-contributor engineering answers emphasize ownership, technical
  choices, production constraints, failure modes, tests, rollout, and lessons.
- Engineering and people-manager answers emphasize direction, prioritization,
  delegation, coaching, disagreement and risk handling, stakeholder alignment,
  team outcomes, and accountability while retaining role-appropriate technical
  judgment.
- Data, BI, product, program, project, cloud, platform, SRE, DevOps, and
  security answers receive their own decision vocabulary rather than being
  forced into a generic software-engineer template.
- Behavioral answers use STAR as an internal structure but remain a natural,
  speakable story. Scenario questions reason directly instead of inventing an
  unrelated past story.
- Follow-ups continue from the same role, project, facts, and decisions instead
  of restarting with a generic answer.

## Grounding Boundary

- When supplied context confirms a real project or story, Bluey may answer in
  first person using only the supported employer, tools, constraints, actions,
  metrics, and outcomes.
- When context does not confirm personal experience, Bluey uses a practitioner
  framing such as `In that situation, my approach would be...`.
- Bluey must not invent employers, titles, team size, metrics, tools, incidents,
  scope, or outcomes merely to make an answer sound experienced.
- Experience should come through concrete decisions and tradeoffs, not
  unsupported claims or buzzwords.

## Implementation

- Added `crates/cue-core/src/prompt_contracts.rs` and exported it from
  `crates/cue-core/src/lib.rs`.
- Applied the shared contract to the local `AnswerLlm` path.
- Applied the shared contract to native provider prompt construction when
  role/domain interview mode is active.
- Applied the same contract to the server AnswerPlan prompt when interview
  context is detected.
- Expanded desktop and server role detection for engineering manager, people
  manager, technical manager, team lead, tech lead, project manager, director,
  and senior-manager phrasing.
- Added manager-specific regression tests to prove the request is classified as
  a behavioral interview answer and receives manager decision voice rather than
  individual-contributor-only wording.

## Verification

- `cargo test -p cue-core prompt_contracts --lib --quiet`: 1 passed.
- `cargo test -p cue-core --lib --quiet`: 94 passed.
- `cargo test -p cue-daemon llm::answer::tests --lib --quiet`: 7 passed.
- `cargo test -p cue-daemon provider_messages_use_manager_decision_voice_for_manager_interviews --lib --quiet`: 1 passed.
- `cargo test -p cue-daemon app::tests --lib --quiet`: 145 passed.
- `cargo test --manifest-path server/Cargo.toml answer_plan_manager_interview_uses_manager_decision_voice --lib --quiet`: 1 passed.
- `cargo test --manifest-path server/Cargo.toml answer_plan --lib --quiet`: 38 passed.
- `cargo clippy -p cue-core -p cue-daemon --lib --bins --quiet`: passed.
- `cargo clippy --manifest-path server/Cargo.toml --lib --quiet`: passed.

## Deployment

- No deployment.
- No release build.
- No GitHub Actions.
