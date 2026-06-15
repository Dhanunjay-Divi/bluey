# Bluey — Frontend Integration Plan (agent feature + mode toggle + aurora-glass redesign)

> **Status:** Plan for approval. Built on a verified cross-branch reconciliation + backend↔frontend audit (2026-06-15).
> **Goal:** One codebase with BOTH modes (managed Bluey AI ↔ your own agent), a premium aurora-glass UI, and the agent/session/continuation backend we built fully surfaced — easy, scalable, production-grade.
> **Ownership:** We own the **your-own-agent** side + the **mode toggle**. The dev owns the **managed-AI** side. The merge brings them together.

---

## 0. The verified reality (foundation — not assumed)

- **Two product branches, forked from `2403015`, never merged:**
  - `origin/codex/bluey-ai-site` (dev's trunk) = the **React dashboard** + managed AI (`CueManaged`) + marketing site + evolved `server/`. Tip `56984c2`.
  - `agent/agent-bridge` (ours) = the **agent-bridge engine** (`crates/cue-agent-bridge`) + continuation. Tip `d5f92a1`. **+38,874 / −54 lines — almost purely additive.**
- **The two modes already exist in the type system:** `AiProviderKind` (`crates/cue-core/src/ai.rs:91`) has `CueManaged` AND `Agent`. The daemon already dispatches on it (`app.rs:4912`): `attached_agent` set → `Agent` route; else → managed. **The toggle is ~90% wired in the backend already.**
- **A 3-way merge of the two branches conflicts in only 4 files** — 3 trivial Rust unions, 1 hard (the Swift overlay, both rewrote it, 21 regions). Everything else auto-merges.
- **Backend is 100% done + CLI-proven; the dashboard has ZERO agent UI.** The gap is thin Tauri wrappers + UI pages, not backend work.

### 3 gotchas (must respect)
1. **Two settings stores:** the dashboard writes settings to local SQLite; the daemon reads its own `CueSettings` JSON. The toggle MUST go through `AgentAttach`/`AgentDetach` IPC — NOT the SQLite settings — or it silently won't reach the daemon.
2. **Consent flag:** `allow_agent_session_history` gates session listing (returns empty if off). The session picker needs the consent toggle built alongside it.
3. **Swift overlay merge:** the one genuinely hard conflict (21 regions). Hand-merge carefully; both products diverged it heavily.

---

## Phase 0 — Get to ONE codebase (the merge) [GATES EVERYTHING]

Strategy: **codex as base, replay ours on top.** Done on a NEW integration branch — never touching `main` or the dev's branch directly.

**Safety prep (before any merge):**
- Push our 16 continuation commits to `origin/agent/agent-bridge` first (so they're safe on the remote — a bad merge can't lose them). [needs user OK to push]
- Commit the design mockups + this plan so they're tracked.

**The merge:**
1. New branch `integration/agent-ui` FROM `origin/codex/bluey-ai-site` (inherit dashboard + managed AI + site + dev's evolved server/overlay).
2. Merge `agent/agent-bridge` into it. Expect the 4-file conflict set:
   - `cue-core/src/ai.rs` enum → auto-unions to both variants (verified).
   - `cue-daemon/src/app.rs` (2 regions) → union the `OverlayEvent` arms; take our superset `resolve_answer_route` signature.
   - `cue-core/src/overlay.rs` (4 regions) → additive `Agent*` variants.
   - `cue-cli/src/app.rs` (1 region) → trivial.
   - `native/.../main.swift` (21 regions) → **hand-merge** (the real labor): keep the dev's overlay polish + our agent UI.
3. **Verify the merged tree compiles + tests pass** (the reconciliation only checked structural mergeability — the merged tree's build is unverified until we do it). Reconcile any API drift: our daemon's managed path is the *stale fork* copy; codex *evolved* `bluey_managed.rs`/`dispatcher.rs` — wiring may need follow-up.
4. Full gate: `cargo fmt`, `clippy -D warnings`, `cargo test` across the workspace.

Outcome: one branch with the dashboard, managed AI, the agent engine, continuation, and a buildable tree. **Nothing pushed to a shared branch; handed to you/the dev to review + merge.**

---

## Phase 1 — Wire the agent backend to the dashboard (thin glue)

Add ~5 Tauri command wrappers in `crates/cue-dashboard/src/commands.rs` (mirror the existing `daemon_ipc` pattern at `commands.rs:65`), register in `lib.rs:52`:
- `agent_list() -> Vec<AgentSummary>` → `daemon_ipc(AgentList)`
- `agent_attach(kind, session_id: Option<String>) -> Vec<AgentSummary>` → `AgentAttach`
- `agent_detach()` → `AgentDetach`
- `agent_sessions(kind) -> Vec<AgentSessionSummary>` → `AgentSessions{kind}`
- `agent_connectors(kind) -> Vec<AgentConnectorInfo>` → `AgentConnectors{kind}`
- A consent setter for `allow_agent_session_history` (routed to the daemon's CueSettings, NOT SQLite).

TS types mirroring `agent_ui.rs` DTOs (`AgentSummary`, `AgentSessionSummary`, `AgentConnectorInfo`).

---

## Phase 2 — The mode toggle (managed ↔ your agent)

- **Global toggle** (segmented control) at the top of Settings + a compact indicator in `DashboardLayout`.
- **Managed** → `agent_detach()` → clears `attached_agent` → managed route.
- **Your agent** → pick agent (`agent_list`) → optional session (`agent_sessions`) → `agent_attach(kind, session_id)` → agent route + resume.
- **Mode-aware UI:** derive `mode` from `agent_list()`'s `attached` flag, lift to layout context:
  - Managed mode → show `BalanceIndicator` + managed model picker; hide agent panel.
  - Agent mode → hide `BalanceIndicator`; show "Using <Agent>" chip + connectors/session info.
- Route through IPC, never SQLite (gotcha #1).

---

## Phase 3 — The agent/chat UI (from the mockup)

- New sidebar section "Coding agents" → `Agents & chats` route.
- `pages/Agents.tsx`: the agent picker (cards: name, version, chat count, connectors, resume/replay capability — from `AgentSummary`).
- `pages/AgentSessions.tsx`: sessions grouped by `project`, title + `updated_at`, a **Continue** button → `agent_attach(kind, session_id)` then ask.
- Consent toggle for session history (gotcha #2).
- Command palette: "Attach coding agent…", "Continue last agent session".

---

## Phase 4 — The aurora-glass design system (premium look)

Single source of truth (the researched Apple-premium spec, refined to the aurora-glass direction the user approved):
- **Tokens:** Tailwind v4 `@theme` block in `cue-dashboard/ui/src/index.css` (currently a 1-line `@import` — no token layer exists, so this is additive + high-leverage). Mirror tokens in a Swift `Color` extension for the overlay.
- **Aurora-glass:** dashboard gets its own subtle aurora background (soft, deep, desaturated blue/magenta/teal/violet blobs, heavily blurred) with frosted-glass panels (`backdrop-filter: blur saturate`) floating on top. Overlay = transparent glass over the real desktop (NSVisualEffectView vibrancy).
- **Premium rules:** deep neutral base (`#0B0D10`), ONE refined blue accent (`#3B82F6`) used sparingly, hairline borders + top-highlight (no glow), SF type with negative tracking, 8pt grid, soft shadows, tabular-nums. NO emoji.
- **Re-skin order:** token layer first (small, reviewable) → then migrate pages from hardcoded `zinc-*`/`blue-*` to semantic tokens, page by page (the dev edits these files, so coordinate). The new agent pages are built premium from the start.
- **Reference:** `docs/design/bluey-premium-mockup.html` (dashboard, aurora) + `docs/design/bluey-overlay-glass-mockup.html` (overlay, glass).

---

## Phase 5 — Verify the whole flow (production-grade)

- End-to-end: toggle to "your agent" → pick Claude → see chats grouped by project → Continue one → answer streams (with continuation/resume) → toggle back to managed → managed answers. Live.
- Mode-aware UI verified (balance hidden in agent mode, agent panel hidden in managed).
- `cargo` + UI build/lint gates green. No emoji. Aurora-glass applied.
- Honest report of what's proven live vs needs the dev's managed-AI side.

---

## Risk + coordination

- **Merge risk: LOW-MODERATE** — 4 conflict files, the Swift overlay is the only hard one.
- **Coordination:** the merge inherits the dev's work as base (respectful); we add ours on top. The integration branch is handed back for review — we don't force it onto their trunk. The dev owns managed-AI; we own agent + toggle.
- **Scalable by design:** the IPC contract is typed + decoupled (DTOs carry shape-only, no secrets); adding an agent is a registry row; the toggle is one routing decision already wired. The design system is token-driven (change once, applies everywhere).
