# Bluey Agent Handoff

> **Read this first if you are a new engineer or AI agent picking up
> Bluey work.** Mirrors Pinky's `AGENT-HANDOFF.md` shape but Bluey-only.
>
> Last updated: 2026-05-19, post v0.1.0 GA.

---

## 1. Source-of-truth docs (read in this order)

1. **`ARCHITECTURE.md`** — system shape, server topology, layered
   model, monetization plug-points, staged rollout. **Read this first.**
2. **`SERVER-REFERENCE.md`** — exact paths on each server when stood
   up. (Currently mostly forward-looking; no servers yet.)
3. **`DECISIONS.md`** — historical decisions and dead ends. Read
   before considering revisits.
4. **`FUTURE-IMPLEMENTATIONS.md`** — canonical tracker for deferred work.
5. **`docs/PRODUCTION-READINESS.md`** — implementation matrix. What
   ships vs what's pending.
6. **`docs/AUTO-ROUTING-USP.md`** — Auto Router product framing.
7. **`docs/BLUEY-DISTRIBUTION-ARCHITECTURE.md`** — Layer 2 path
   comparison.
8. **`docs/release/RELEASE-v0.1.0.md`** — current release notes.
9. **`docs/work/PHASE-3-ROUND-14-PLAN.md`** — current-round work.
10. **`docs/work/PHASE-3-WINDOWS-BRIEF-FOR-CODEX.md`** — Windows scope
    for the codex agent.

`README.md`, `INSTALL.md`, `CONTRIBUTING.md` are user-facing and OK as a
quick orient, but the docs above are the engineering source of truth.

---

## 2. Current state (snapshot)

| Field | Value |
|---|---|
| Latest tag | `v0.1.0` (GA, 2026-05-19) on commit `c34592a` |
| Working branch | `feat/phase-3-round-12` (post-GA work continues here) |
| Latest tip | (run `git -P log --oneline -1` on uno) |
| Test count | 392 cargo + 15 vitest |
| Dev workstation | uno at `192.168.4.25` (Apple Silicon Mac) |
| Source location on uno | `/Users/uno/Downloads/cue/` |

---

## 3. Active surfaces

| Surface | State |
|---|---|
| `bluey on` / `bluey off` | shipping in v0.1.0 |
| Pill UX (macOS) | shipping; 112×28 capture-excluded NSWindow |
| Auto Router classifier | shipping; default-ON speculative dispatch |
| LaneBadge UI | shipping; reads `router_meta` from cue_response_chunk |
| Local RAG | shipping; 37ms at 10k chunks / 1536-dim |
| Distribution server | not stood up; design = `docs/BLUEY-DISTRIBUTION-ARCHITECTURE.md` |
| Product server (cloud) | not built; spec = `ARCHITECTURE.md` Section 5, R14.9 |
| Windows native overlay | source exists, not shipping; brief for codex at `docs/work/PHASE-3-WINDOWS-BRIEF-FOR-CODEX.md` |

---

## 4. Operational basics

### Connect to uno

```bash
ssh -i ~/.ssh/id_ed25519 uno@192.168.4.25
cd /Users/uno/Downloads/cue
export PATH=/opt/homebrew/bin:/Users/uno/.cargo/bin:/usr/local/bin:$PATH
```

### Pipeline before any commit

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets
cargo test --all-targets
( cd crates/cue-dashboard/ui && npm test && npm run build )
swift build -c release --package-path native/macos/cue-overlay
swift build -c release --package-path native/macos/cue-whisper
git -P diff --check feat/phase-3-round-10..HEAD
```

If any step fails, fix before committing. The chain `cargo fmt` →
`clippy -D warnings` → `test` → swift builds is THE quality gate codex
expects.

### Build artifacts

```bash
make package-darwin-arm64       # arm64-only tarball
make package-darwin-universal   # arm64+x86_64 lipo'd tarball
```

Outputs under `dist/`. Per-asset `.sha256` files are produced
automatically. The universal target depends on having both arches built
first; the Makefile chain handles it.

### Smoke

```bash
bash scripts/smoke-test.sh
# end-to-end: daemon + overlay + transcript + instructions + context +
#             memory + audio + AI routing + cloud + ask + action-items +
#             recap + archive
```

### Installer end-to-end

```bash
tmp=$(mktemp -d)
BLUEY_ARCHIVE=dist/bluey-0.1.0-darwin-universal.tar.gz \
  BLUEY_INSTALL_DIR="$tmp/bluey" \
  BLUEY_BIN_DIR="$tmp/bin" \
  bash scripts/install.sh
"$tmp/bin/bluey" on
"$tmp/bin/bluey" off
rm -rf "$tmp"
```

---

## 5. Standing rules (do not break)

These are baked into how this codebase is reviewed:

- **No `git push`** — bits stay on uno until the user explicitly says
  push to a specific destination.
- **No history rewrite** on commits that have been pushed (none today
  but the rule applies once any push lands).
- **Conventional Commits** — every commit subject is
  `<type>(<scope>): <description>`. Body explains what + why, not how.
- **Cargo.toml stays at workspace version `0.1.0`.** Tags carry the
  release identity (`v0.1.0`, `v0.1.0-alpha`).
- **No new features without a round plan.** Update
  `docs/work/PHASE-3-ROUND-N-PLAN.md` first or as part of the same
  commit.
- **Tests required.** New behaviour gets at least one Rust test or
  Vitest test. Pipeline must stay green.
- **Cross-layer changes update `ARCHITECTURE.md`** in the same commit.

---

## 6. Codex / Kiro split

Bluey work is split across two AI agents:

| Agent | Lives where | Owns |
|---|---|---|
| **Kiro** (this agent) | user's laptop, ssh's into uno | macOS work, Auto Router, daemon, overlay, distribution scaffolding, docs |
| **Codex** | uno directly via Tailscale | Windows work, code review of Kiro's output, occasionally Linux |

Handoffs:

- **Kiro → Codex:** write a `docs/work/PHASE-3-...-HANDOFF-FOR-CODEX-REVIEW.md`
  with branch tip, per-area changes, reviewer asks, pipeline status.
  Codex writes a `REVIEW-PHASE-3-ROUND-N.md` in the same dir.
- **Codex → Kiro:** codex writes a
  `docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md` describing changes made
  in-place on uno's worktree. Kiro reviews + commits per codex's
  suggested split, then verifies the pipeline.

For Windows specifically: `docs/work/PHASE-3-WINDOWS-BRIEF-FOR-CODEX.md`
is the canonical W1–W7 spec. Codex picks that up when ready.

---

## 7. When something is broken

1. Read `DECISIONS.md` first. The bug may be a deliberate decision.
2. Read the latest `docs/work/PHASE-3-ROUND-N-PLAN.md` to see if the
   item is already tracked.
3. If genuinely new, open the next round plan (R15+) before fixing.
4. Fix in the smallest commit that captures the problem + the test
   that proves it.

---

## 8. When in doubt — questions for the user

These are perma-pending and require user input:

- **Distribution path** (A / B / C) and **domain** for Layer 2.
- **Monetization timeline** (X / Y / Z) and **product server language**
  (Go / Rust) for Layer 3.
- **Where do bits go** when a release is cut and we want external
  testers to install? (As of 2026-05-19: still uno-only.)

`ARCHITECTURE.md` Section 8 captures these with safe defaults to fall
back on if the user goes silent.
