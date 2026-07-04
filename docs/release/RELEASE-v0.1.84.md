# Bluey 0.1.84

Released: 2026-07-04

## Summary

This release fixes stale screen context being silently reused on typed follow-ups. Old screenshots are now used only as saved text memory unless the user explicitly captures or attaches screen context for the current answer.

## Changes

- Sent question attachment chips are now explicit-only.
- Follow-ups without pending context ids no longer re-upload old screenshots.
- Saved screen context can still help follow-ups through retained text summaries.
- Internal/private-screen guard failures now show a clearer explanation instead of only a generic provider error.

## Verification

```bash
cargo fmt --check
cargo check -p cue-daemon -p cue-cli
cargo test -p cue-daemon follow_up_context --lib
cargo test -p cue-daemon inferred_answer_context --lib
cargo test -p cue-daemon relevant_current --lib
cargo test -p cue-daemon internal_disclosure_blocks_get_specific_user_message --lib
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.84
```

Round doc:

- `docs/rounds/ROUND-345-EXPLICIT-SCREEN-CONTEXT-FOLLOWUPS.md`
