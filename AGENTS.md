# AGENTS.md — Development Rules for Cue (Bluey)

## Codex Preflight

Before any Bluey task, load the local `$bluey-ops` skill from
`/Users/uno/.codex/skills/bluey-ops/SKILL.md`. Use it as navigation and operating
memory; this file and the current repository remain authoritative.

## Architecture

Cue is a cross-platform AI meeting copilot built with Tauri 2 (Rust backend + React 19 frontend). The workspace contains three Rust crates (`cue-core` for shared logic, `cue-daemon` for background services, `cue-cli` for developer tooling), a React dashboard (`web/`), and native platform overlays (`native/`). Audio capture, STT, LLM streaming, and RAG are orchestrated by the daemon; the overlay and dashboard consume results via Tauri IPC events.

## Hard Rules

1. **Never push directly to `main`.** All work happens on feature branches (`feat/phase-X-*`).
2. **One branch per task batch.** Each Phase X batch gets its own branch from the previous phase branch.
3. **Conventional commits.** Format: `type(scope): subject` — types: feat, fix, docs, refactor, test, chore, ci, perf.
4. **CHANGELOG updated per PR.** Every PR adds entries under `## [Unreleased]`.
5. **Do not edit `docs/reviews/`** — those are frozen reference documents.
6. **No dead code.** Remove unused imports, deps, and functions before committing.
7. **No secrets in repo.** API keys go in env vars or OS keychain only.

## Workflow: Phase Batches

1. Create branch `feat/phase-X-<name>` from the previous phase branch.
2. Implement tasks in order (D0.1, D0.2, …). One commit per logical unit.
3. Write `docs/work/IMPL-<BATCH>.md` documenting what was done.
4. Run full build + test suite. Fix all warnings.
5. Self-review against `docs/work/TEMPLATE-REVIEW.md`.
6. Open PR with `.github/PULL_REQUEST_TEMPLATE.md` filled in.
7. After review: squash-merge into target branch.

## Code Style

### Rust

- `cargo fmt` — no exceptions, run before every commit.
- `cargo clippy -- -D warnings` — treat all warnings as errors.
- Edition 2021. Use `anyhow::Result` for fallible functions.
- Async: tokio runtime, no blocking calls in async context.
- Tests: `#[cfg(test)] mod tests` in each file + integration tests in `tests/`.

### TypeScript / React

- Prettier (default config) + ESLint (strict mode).
- React 19 with functional components only.
- Tailwind CSS + Radix UI primitives.
- No `any` types. Strict TypeScript (`strict: true`).

### General

- No unused dependencies in Cargo.toml or package.json.
- Prefer explicit error handling over `.unwrap()` in production code.
- Max line length: 100 chars (Rust), 120 chars (TypeScript).

## Test Discipline

- Every task has acceptance criteria defined in the plan docs.
- Unit tests for all pure logic (>80% coverage target for core).
- Integration tests for IPC commands and daemon workflows.
- CI must pass before PR merge: fmt, clippy, build, test.
- Test names describe behavior: `test_chunker_splits_at_sentence_boundary`.
- Run local Bluey Rust tests through `scripts/run-bluey-tests.sh all`, or use
  `scripts/run-bluey-tests.sh -- <command>` for a focused test/check command.
  The launcher isolates Cargo output, primary SQLite, temporary, data, config,
  runtime, and log paths; forces SQLite; removes inherited PostgreSQL URLs; and
  supervises the complete command process group.
- Run `scripts/run-bluey-tests.sh --self-test` before handoff. The launcher
  runs every `all` and direct command from the physical repository root. It
  cleans an owned root after success, failure, HUP, INT, or TERM, but once
  marker establishment begins a missing or malformed marker retains the root
  and makes cleanup fail explicitly. SIGKILL and host crashes are recovered
  only by a later marker-bound stale-run pass; uncertain process inspection is
  treated as active.
- Never point the test run parent at a repository, Git worktree, Downloads
  sibling, broad temporary root, or `/Volumes`. It must be a private
  current-user directory under local `/tmp`; existing directories are never
  chmodded by the launcher.

## Review Discipline

- **FIXES.md per bug**: Use `docs/work/TEMPLATE-FIX.md` for every bug fix.
- **IMPL doc per batch**: Use `docs/work/TEMPLATE-IMPL.md` for every implementation batch.
- **REVIEW doc per batch**: Reviewer fills `docs/work/TEMPLATE-REVIEW.md`.
- **Line-by-line pass**: Every file in the diff gets reviewed for correctness, style, security.
- **Verdicts**: 🟢 accept / 🟡 minor nit / 🔴 blocker.

## File Naming Conventions

| Location | Pattern | Example |
|----------|---------|---------|
| `docs/work/` | `IMPL-<BATCH>.md` | `IMPL-WORKFLOW-DOCS.md` |
| `docs/work/` | `REVIEW-<BATCH>.md` | `REVIEW-WORKFLOW-DOCS.md` |
| `docs/work/` | `FIX-<NUMBER>-<slug>.md` | `FIX-001-audio-dropout.md` |

## Target Audience

This document governs both AI agents (Codex, Codex) and human developers. When in doubt, optimize for clarity and auditability over brevity.
