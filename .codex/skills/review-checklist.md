# Skill: Review Checklist (Principal Engineer Pass)

## Per-File Checks

### Rust Files
- [ ] No `unwrap()` or `expect()` in non-test code (use `?` or handle)
- [ ] No blocking calls in async functions (check for `std::fs`, `std::thread::sleep`)
- [ ] Error types are meaningful (not just `String`)
- [ ] Public API has doc comments (`///`)
- [ ] No unused imports or dead code
- [ ] Proper `Drop` for resources (file handles, connections)
- [ ] `Clone` only where necessary (prefer references)
- [ ] No `unsafe` without `// SAFETY:` comment explaining invariants

### TypeScript Files
- [ ] No `any` types (use `unknown` + type guards if needed)
- [ ] Event listeners cleaned up on unmount
- [ ] Error boundaries around fallible components
- [ ] Accessible: ARIA labels on interactive elements
- [ ] No inline styles (use Tailwind classes)

### All Files
- [ ] No secrets, API keys, or credentials
- [ ] No TODO without a task ID reference
- [ ] File is under 400 lines (split if larger)
- [ ] Naming is clear and consistent with codebase

## Per-PR Checks

- [ ] Conventional commit messages
- [ ] CHANGELOG.md updated
- [ ] IMPL doc exists in docs/work/
- [ ] CI passes (fmt, clippy, build, test)
- [ ] No unrelated changes in the diff
- [ ] New dependencies justified and version-pinned

## Architecture Checks

- [ ] Crate boundaries respected (no circular deps)
- [ ] Shared types in cue-core, not duplicated
- [ ] IPC commands have proper error handling
- [ ] Streaming uses events, not polling
- [ ] State changes are observable (events emitted)

## Security Checks

- [ ] API keys from keychain/env, never hardcoded
- [ ] User input validated before use
- [ ] File paths sanitized (no path traversal)
- [ ] Network requests use TLS (rustls)
- [ ] Overlay content protected from screen capture

## Performance Checks

- [ ] No allocations in audio callback path
- [ ] Channels bounded (no unbounded queues)
- [ ] Database queries use indexes
- [ ] Large data processed in chunks, not loaded entirely into memory
