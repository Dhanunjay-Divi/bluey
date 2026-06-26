# Round 102 - Answer Em Dash Guard

Date: 2026-06-22

## What Changed

- Added an explicit Bluey answer-style rule: generated answers should not use em dashes.
- Added answer text sanitizing in the daemon stream path, replay path, final answer path, managed provider path, local provider path, and OpenAI-compatible stream path.
- Added the same guard to the older daemon LLM helper.
- Sanitized managed canvas artifact bodies too, so code and design workbench text follows the same rule.

## Why

The answer style was reading too much like a generic AI explainer. This keeps generated answers more natural and consistent with the product voice even if a model tries to emit long dash punctuation.

## Verification

- `cargo fmt --check --package cue-daemon`
- `cargo test -p cue-daemon test_sanitize_answer_text_removes_em_dashes`
- `cargo test -p cue-daemon llm_overlay_artifact_preserves_managed_code_canvas`
- `cargo build -p cue-daemon --release`

## Local Install

Copied the rebuilt daemon to:

- `~/.bluey/bin/bluey-daemon`
- `~/.bluey/bin/cue-daemon`

Restarted Bluey with the rebuilt daemon.
