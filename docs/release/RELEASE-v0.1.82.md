# Bluey Release v0.1.82

Date: 2026-07-04

Branch: `codex/bluey-overlay-spacing-20260626`

## Summary

This release fixes sent screen-context chips, hides sent attachment chips from the composer strip, and strengthens code-answer comment guidance.

## Changes

- Sent question cards now show the attached screen/document chips even when Bluey inferred the context from the current session instead of receiving explicit overlay ids.
- After sending a question, the bottom attachment strip clears. Existing files remain available from `Show N file(s)` in the header.
- Code-answer instructions now require comments above important blocks and decision lines for non-trivial code.
- Code-answer instructions still require `Line notes:` outside the code fence for clean explanatory notes.
- Production API prompt rules were deployed from commit `2a0da7dbb4bc0e02cb29c1e7933195adb00a04f8`.

## Verification

```bash
cargo fmt --check
cargo check -p cue-cli -p cue-daemon
cargo check --manifest-path server/Cargo.toml --bin bluey-server
cargo test -p cue-daemon inferred_answer_context_produces_question_attachment_chips -- --nocapture
cargo test -p cue-daemon mode_instructions_specialize_default_answer_shapes -- --nocapture
cargo test -p cue-daemon general_mode_keeps_code_shape_for_coding_questions -- --nocapture
cargo test --manifest-path server/Cargo.toml answer_plan_prompt_explains_unavailable_web_search --lib -- --nocapture
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.82
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
curl -fsS https://bluey.sh/health
```

## Round Doc

- `docs/rounds/ROUND-343-SCREEN-CONTEXT-SENT-CHIPS-CODE-COMMENTS.md`
