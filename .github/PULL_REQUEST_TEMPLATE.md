## Summary

- 
- 
- 

## Task IDs

<!-- e.g. D0.1, D0.2, B3.4 -->

## Type of Change

- [ ] feat — new feature
- [ ] fix — bug fix
- [ ] docs — documentation only
- [ ] refactor — code restructuring (no behavior change)
- [ ] test — adding/fixing tests
- [ ] chore — build, CI, deps, tooling

## Affected Components

- [ ] Native overlay (macOS/Windows)
- [ ] cue-daemon
- [ ] cue-core
- [ ] cue-cli
- [ ] cue-dashboard (React)
- [ ] Infrastructure / CI
- [ ] Documentation

## Testing Done

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] Manual verification (describe below)

**How verified:**


## Risks & Mitigations

| Risk | Mitigation |
|------|-----------|
|      |           |

## Review Checklist (Principal Engineer Pass)

- [ ] No dead code or unused imports
- [ ] Error handling is explicit (no unwrap in prod)
- [ ] No secrets or hardcoded keys
- [ ] Conventional commit messages
- [ ] IMPL doc exists in `docs/work/`
- [ ] Tests cover acceptance criteria
- [ ] No blocking calls in async context

## CHANGELOG Updated?

- [ ] Yes — link to diff: 
- [ ] N/A (no user-facing changes)
