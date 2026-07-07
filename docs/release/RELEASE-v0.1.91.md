# Bluey 0.1.91

## Summary

- Keeps full coding complexity details in the code canvas, including both time and space complexity with the explanatory lines that say what `n`, `rows`, or `cols` mean.
- Merges complexity details from the streamed chat answer back into the final code artifact if the provider only put them in prose.
- Keeps the left chat answer focused on approach, explanation, line notes, and complexity while the right canvas owns the complete code. This avoids the confusing moment where code appears duplicated on the left after the final artifact is available.

## Verification

- `cargo test --manifest-path crates/cue-daemon/Cargo.toml answer_overlay_artifact --quiet`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml visible_answer_body --quiet`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml code_artifact --quiet`
- `cargo test --manifest-path server/Cargo.toml response_artifact --quiet`
- `git diff --check`
