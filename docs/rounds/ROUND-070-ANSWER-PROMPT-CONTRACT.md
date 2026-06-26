# Round 070 - Answer Prompt Contract - 2026-06-20

## Why

Bluey needs to answer like a capable person in the moment, not like a generic AI assistant. The user-provided Tone prompt should be respected, but it should not replace the actual question or leak into the visible chat as if the user typed it.

## Current Flow

When the user saves Tone / answer style in the overlay, the native overlay emits `instructions_updated`. The daemon stores that text on the active meeting as `answer_instructions`.

When the user presses Answer, Bluey builds an `AnswerRequest` from:

- typed text in the composer
- newly consumed live transcript since the last Answer
- screen context when Analyse Screen was used
- attached document context
- recent session and RAG context
- selected mode, such as Auto, Code, System Design, Vision
- saved Tone / answer style instructions

The saved Tone prompt is merged into `AnswerRequest.instructions` as hidden answer rules. It is not shown as the user question. The model receives it as behavior/style guidance alongside mode-specific rules.

## Product Rules Locked In This Round

- Easy questions get the direct answer first.
- Hard questions get assumptions, reasoning, tradeoffs, and edge cases needed to defend the answer.
- Bluey should not act omniscient. If context is incomplete, it states the assumption and continues with the best practical answer.
- Chat answers stay human and speakable. Deeper detail belongs in structured sections or canvas artifacts.
- Coding follow-ups use in-place edits by default: changed block or unified diff, with the file/function named.
- Full code replacement is allowed only when the user asks for it, the file is new, or replacing is materially safer than patching.
- System design answers are clear and concrete: architecture, data flow, APIs/contracts, storage, scaling, tradeoffs, failure modes, observability, and rollout when useful.
- System design follow-ups update only the affected section unless the user requests a full redesign.

## Reference Prompt

The user-provided `/Users/uno/Downloads/prompt (1).txt` is treated as a reference for style and behavior, not copied wholesale into Bluey defaults. Useful pieces folded into Bluey:

- document-grounded answers
- no stale/repeated context
- natural spoken answer style
- direct follow-ups
- technical clarity with assumptions and tradeoffs
- data/system design structure when useful

Mock-interview-specific behavior, such as pretending to be a candidate or using uploaded LP stories, remains user-provided Tone/session context and is not a global Bluey default.

## Files

- `crates/cue-daemon/src/app.rs`
- `crates/cue-daemon/src/llm/answer.rs`

## Verification

- `cargo fmt --all --check`
- `cargo test -p cue-daemon --lib` - 209 passed, 2 ignored
- `git diff --check`
