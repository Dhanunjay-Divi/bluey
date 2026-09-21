# Bluey Agent Onboarding

Entry-point routing updated: 2026-09-21. Historical snapshot below: 2026-05-25.

> **Codex preflight:** Load the branch-local [$bluey-ops](.agents/skills/bluey-ops/SKILL.md)
> and [agent round checklist](docs/work/BLUEY-AGENT-ROUND-CHECKLIST.md).
> Read the current routing in [AGENT-HANDOFF.md](AGENT-HANDOFF.md), not just
> the historical branch snapshot below. Record ownership and next actions in each round.

For the stopped Phase 625 branch, read the
[recovery handoff](docs/work/HANDOFF-PHASE-625-RECOVERY-20260921.md) first.
It is incomplete and not deployable; this documentation update does not release
the stop instruction. The May 2026 references below are historical context only.

## Historical Repo And Branch Snapshot

- Repo: `/Users/uno/Downloads/cue`
- Current long-running branch: `feat/phase-3-round-12`
- Product name: Bluey
- Public domain target: `bluey.sh`
- Do not stage local `bluey-dev.db`.

## What Bluey Is

Bluey is a managed native AI overlay for live work. The desired customer flow is:

```text
bluey on -> compact pill -> expanded overlay -> listen / attach / analyze / ask
-> streamed answer -> saved synced session
```

The CLI starts and supports the product. The overlay is the user experience.

## High-Level Code Map

- `crates/cue-cli/` - `bluey` CLI commands.
- `crates/cue-daemon/` - local daemon, sessions, audio/STT, overlay IPC, cloud
  sync, balance watch, managed LLM calls.
- `crates/cue-core/` - shared types, storage helpers, settings, logging,
  observability.
- `crates/cue-llm/` - LLM providers, managed provider bridge, streaming types.
- `crates/cue-router/` - classifier/router/lane policy.
- `crates/cue-dashboard/` - Tauri dashboard and React UI.
- `native/macos/` - overlay, audio, whisper helpers.
- `native/windows/` - Windows helper source; not production-shipped yet.
- `server/` - Rust Axum `bluey-server` for auth, billing, routing, RAG/sync,
  STT, usage, metrics, account operations.
- `docs/` - product, deploy, routing, pricing, security, review, and handoff
  docs.

## Standing Rules

- Provider secrets belong on `bluey-server`, not the customer desktop.
- `local`/Ollama/LocalWhisper are hidden dev/offline fallback paths, not paid
  customer model choices.
- `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1` is for local smoke testing only and
  must never ship enabled.
- Paid users are constrained by account-credit balance and provider capacity. Optional
  account throttles are emergency guardrails, not default product behavior.
- Model changes require code, pricing, docs, and tests together.
- New UI must not crop header/balance/model controls or grow the whole overlay
  when transcripts stream in.
- Do not claim "unbacktraceable" or impossible security guarantees. Use honest
  language: capture-excluded, private, encrypted transport, hardened auth, and
  server-side provider keys.

## Development Gate

Run the relevant subset while iterating, and the full gate before handoff:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets --release
bash scripts/run-bluey-tests.sh all
(cd crates/cue-dashboard/ui && npm run build)
swift build -c release --package-path native/macos/cue-overlay
swift build -c release --package-path native/macos/cue-whisper
git -P diff --check
```

## Handoff Pattern

For a new round:

1. Read the current handoff docs.
2. Inspect current `git status` and recent commits.
3. Implement a coherent slice.
4. Add the mandatory round docs before the final response: implementation or
   handoff in `docs/rounds/`, and review verdict in `docs/reviews/` once
   reviewed. This applies to every meaningful coding, architecture, deployment,
   or product/UX round, even if the code change is small. Add an ack/close doc
   when blockers, follow-ups, or self-merge close-out need durable tracking.
5. Run verification.
6. Commit with a conventional commit message.
7. Tell the next agent exactly what to read and what to do next.
