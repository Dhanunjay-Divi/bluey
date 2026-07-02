# Round 310: Production Interview Answer Mode

Date: 2026-07-02
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## User-Visible Problem

The user shared another ChatGPT interview-prep thread and Otter interview recordings, then asked Bluey to become more humanized and specialized across roles. The key issue was that Bluey needed more than behavioral STAR handling. Technical interview answers should sound like a real engineer/candidate who built the system in production.

Reference sources inspected:

- `https://chatgpt.com/share/6a46a5d7-36d4-83ea-923f-d50d70422748`
- `https://otter.ai/u/7bMMtxGmuG-ba_5W8vGS8LC-sIw`
- `https://otter.ai/folder/1887796`

Otter notes:

- The exact linked folder `1887796` fully loaded 12 visible recordings in Chrome.
- The adjacent relevant Otter folders exposed 42 total recordings across `amazon`, `DII`, `TD`, `interviews`, and `inteview`.
- Chrome/Otter transcript pages were slow and sometimes stayed in a loading shell, so this round used the loaded folder metadata, visible summaries/outlines where available, and the linked recording summary surface. Private transcript content was not copied into this doc.

## What Changed

- Added `interview_context` to managed `AnswerPlan`.
- Managed prompt now adds a role/domain interview answer contract when interview context is present, even if the intent remains coding or system design.
- Bluey now keeps the correct lane while improving the voice:
  - direct code stays code,
  - system design stays system design,
  - behavioral/story prompts stay behavioral,
  - all can still get interview-quality human/prod answer style.
- Added production interview answer rules:
  - sound like a human candidate or engineer who built the system,
  - start with the speakable answer,
  - explain why decisions were made,
  - include tradeoffs, debugging, reliability, observability, security/auth, evaluation, scaling, and failure handling when relevant,
  - specialize by role/domain from resume/JD/session/API context.
- Added specific AI/ML, RAG, MCP, and agent guidance:
  - ingestion,
  - chunking,
  - embeddings,
  - retrieval,
  - orchestration,
  - grounding/hallucination controls,
  - evals,
  - auth,
  - traces,
  - latency,
  - cost.
- Added SDE/system guidance:
  - ownership,
  - APIs,
  - data flow,
  - concurrency,
  - failure modes,
  - tests,
  - rollout.
- Added data/BI/DE guidance:
  - source systems,
  - ETL,
  - validation,
  - freshness,
  - metrics/KPI definitions,
  - query performance,
  - lineage,
  - stakeholder impact.
- Mirrored the same role/domain interview mode in the native/direct daemon provider path.

## Files Changed

- `server/src/api/router.rs`
- `crates/cue-daemon/src/app.rs`

## Verification

```bash
cargo test --manifest-path server/Cargo.toml answer_plan -- --nocapture
cargo test -p cue-daemon provider_messages_enable -- --nocapture
cargo fmt --all
cargo build -p cue-daemon --bin bluey-daemon
cargo build --manifest-path server/Cargo.toml
```

All listed checks passed.

## Expected Behavior

- "For a Goldman AI/ML interview, how did you evaluate the RAG and MCP agents?" should produce a natural production AI answer, not a generic RAG definition.
- "Write LRU cache code in Python for an SDE interview" should still provide code, but with a concise interview-ready explanation.
- "Design a scalable notification system for an SDE interview" should stay system design, but sound like a candidate explaining tradeoffs.
- "For a data engineer interview, talk about a pipeline you built" should explain source systems, ETL, validation, freshness, failures, and impact.
- "What if the interviewer challenges this story?" should repair the answer realistically instead of defending weak logic.
