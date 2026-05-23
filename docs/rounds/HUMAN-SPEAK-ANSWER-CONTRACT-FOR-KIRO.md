# Human-Speak Answer Contract

Date: 2026-05-23
Owner: Codex
Branch: feat/phase-3-round-12

## Problem

Bluey could stream correct AI answers, but the chat output still felt like an assistant response. In real use, the user needs a speakable answer they can adapt and explain naturally.

## Product Decision

Bluey should produce a short talk track first. The talk track should sound like a human explaining their own approach, while avoiding fake claims about personal experience, shipped work, metrics, or ownership.

This is not a deception layer. It is an answer-shaping rule:

- First person is allowed for approach and reasoning: "I would...", "My approach is...", "The reason I prefer..."
- Bluey must not invent experience or facts not present in session context.
- Bluey should avoid assistant preambles like "Sure", "Here is", "As an AI", or "You can say".
- Bluey should not sound like a polished memo. Avoid source labels, repeated headings, and long markdown checklists in the chat answer.
- For normal spoken answers, use a natural flow: acknowledge the question, give the core answer, then add the reason or example.
- Deep material still belongs in structured sections and canvas artifacts.

## Code Changes

- `crates/cue-daemon/src/llm/answer.rs`
  - Replaced the old generic meeting prompt with a human-speak talk-track contract.
  - Added test assertions that the request prompt includes first-person and no-fabrication rules.

- `crates/cue-daemon/src/app.rs`
  - Added the same contract to provider-backed answer generation.
  - Adjusted the output shape so the direct speakable answer comes before code/design structure.
  - Added regression assertions in `provider_messages_include_overlay_friendly_answer_shape`.

## Expected UX

For simple questions, Bluey should answer in 2-5 concise sentences that the user can say aloud.

For coding/system design/debugging, Bluey should start with a natural talk track, then stream the structured detail needed by the overlay and canvas.

## Remaining Follow-Up

The next best refinement is a user-editable "answer voice" setting with presets such as:

- Direct
- Conversational
- Executive
- Technical
- Interview practice

Those presets should map to safe phrasing rules, not identity or experience fabrication.

## Example-Derived Notes

The Otter transcript and shared ChatGPT answer showed the exact gap:

- Human speech carries context forward and uses simple transitions.
- The AI answer was technically useful, but too list-heavy and polished for live speech.
- Bluey should keep structure for code/design artifacts, but the chat bubble should first read like a natural spoken answer.
- Bluey should not emit "Sources" labels or citation-like document labels inside the speakable answer. Source grounding can remain in metadata and canvas.
