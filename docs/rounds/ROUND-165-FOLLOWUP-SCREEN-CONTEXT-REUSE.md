# Round 165 - Follow-up Screen Context Reuse - 2026-06-24

## Why

Bluey could show that a previous question had screen chips, but a follow-up such as "that's not the answer right?" could still be sent as plain text. The answer model then saw recent Q&A text, but not a usable prior screen/file context item, and replied that the original screen was not visible.

## Change

- Added a bounded follow-up bridge in the daemon answer context path.
- When a new question clearly refers to the previous attached answer, Bluey now includes the prior sent attachment summary in the next request.
- The follow-up bridge now includes the previous question and previous answer next to the retained attachment context, so checks like "that's not right?" can compare against the same conversation instead of asking for a fresh screenshot.
- Added an answer rule that retained previous attachments count as current conversation context for immediate follow-ups.
- For visual follow-ups, prior screen/image attachments are supplied as screen context again, using the saved lightweight local image and summary.
- Normal unrelated questions do not pull old screenshots or documents back into the prompt.
- Explicitly attached context still wins, so the same attachment is not duplicated.
- Increased the saved one-shot image memory preview to an 1800px JPEG so code/SQL screen follow-ups stay readable without keeping the full capture in the bottom pending strip.

## Verification

- `cargo test -p cue-daemon follow_up_context --lib`
- `cargo test -p cue-daemon sent_image_context_becomes_lightweight_memory_only --lib`
- `cargo test -p cue-daemon explicit_context_ids_do_not_duplicate_previous_sent_attachments --lib`
- `cargo check -p cue-daemon`
- `git diff --check -- crates/cue-daemon/src/app.rs`
- `cargo build --release -p cue-cli -p cue-daemon`
- Refreshed local install binaries and restarted `scripts/bluey-visible-local.sh`.
