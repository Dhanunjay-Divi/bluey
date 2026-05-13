# IMPL: Phase 0 — Workflow Documentation

## Scope

**Does:** Establishes development discipline docs, CI, codex agent configs, and skill cards that govern all subsequent phases.

**Does NOT:** Modify any Rust code, add dependencies, or change build output.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `CLAUDE.md` | Created | Development rules for AI agents + humans |
| `CHANGELOG.md` | Created | Keep-a-Changelog format, tracks all changes |
| `.github/PULL_REQUEST_TEMPLATE.md` | Created | Required PR sections |
| `.github/workflows/ci.yml` | Created | Cargo fmt/clippy/build/test on push/PR |
| `docs/work/README.md` | Created | Index of work documentation |
| `docs/work/TEMPLATE-FIX.md` | Created | Bug fix documentation template |
| `docs/work/TEMPLATE-IMPL.md` | Created | Implementation doc template |
| `docs/work/TEMPLATE-REVIEW.md` | Created | Batch review doc template |
| `.codex/config.toml` | Created | Codex CLI project config |
| `.codex/agents/code-reviewer.toml` | Created | Code review agent |
| `.codex/agents/backend-architect.toml` | Created | Rust backend agent |
| `.codex/agents/frontend-developer.toml` | Created | React/Tauri frontend agent |
| `.codex/agents/test-engineer.toml` | Created | Testing strategy agent |
| `.codex/agents/debugger.toml` | Created | Debug/investigation agent |
| `.codex/agents/ui-ux-designer.toml` | Created | UI/UX design agent |
| `.codex/agents/fullstack-developer.toml` | Created | End-to-end feature agent |
| `.codex/skills/tauri-patterns.md` | Created | Tauri 2 IPC/events/state patterns |
| `.codex/skills/rust-async.md` | Created | Tokio, channels, cancellation |
| `.codex/skills/macos-native.md` | Created | NSPanel, CoreAudio, ScreenCaptureKit |
| `.codex/skills/windows-native.md` | Created | WASAPI, Win32, content protection |
| `.codex/skills/audio-dsp.md` | Created | CPAL, ringbuf, VAD, resampling |
| `.codex/skills/llm-streaming.md` | Created | Anthropic/OpenAI streaming, fallback |
| `.codex/skills/prompt-engineering.md` | Created | XML composition, patch mode, budgets |
| `.codex/skills/rag-sqlite-vec.md` | Created | sqlite-vec, chunking, search |
| `.codex/skills/testing-patterns.md` | Created | Async tests, mocks, snapshots, proptest |
| `.codex/skills/review-checklist.md` | Created | Principal engineer review checklist |
| `docs/work/IMPL-WORKFLOW-DOCS.md` | Created | This file |

## Build & Test

```bash
# No build impact — pure documentation + CI config
cargo build   # ✅ unchanged (no code modified)
cargo test    # ✅ unchanged
cargo clippy  # ✅ unchanged
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| .codex/agents use .toml not .md | TOML provides structured fields (constraints, references) vs freeform markdown |
| 10 skill cards (not "5-10") | All 10 topics are relevant to the project scope |

## Known Follow-ups

- CI may need Tauri system deps (webkit2gtk on Linux) once frontend builds are added
- Agent configs will be refined as actual coding begins in Phase 1+

## Review Checklist (for reviewer)

- [ ] All files listed above exist in the commit
- [ ] CLAUDE.md is under 300 lines
- [ ] Each agent config is under 100 lines
- [ ] Each skill card is under 200 lines
- [ ] CI workflow runs fmt/clippy/build/test only (no release automation)
- [ ] No code files modified
