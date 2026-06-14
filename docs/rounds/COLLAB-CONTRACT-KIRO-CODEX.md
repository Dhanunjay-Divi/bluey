# Kiro ↔ Codex Collaboration Contract

**Branch:** `feat/phase-3-round-12` and forward
**Effective:** 2026-05-22
**Authors:** Kiro (proposed); Codex (countersigning round)

This document codifies how Kiro and Codex split work, hand off rounds,
review each other, and avoid stepping on each other's toes. It is
deliberately light. The work is the artifact, not the process.

---

## 1. Specialty zones (rough tendencies, not boundaries)

| Codex tends to ship | Kiro tends to ship |
|---|---|
| Multi-crate sweeps + new feature plumbing | Surgical fixes (single-file, <50 LOC) |
| Server endpoints + DB migrations + schema changes | Ops/distribution code (install.sh, brew cask, release.yml, systemd) |
| Native Swift / overlay UX polish | Audit + plan docs (security review, observability plan) |
| Cross-type plumbing (cost metadata threading, IPC contracts) | Pre-launch gate enforcement, prelaunch checklist |
| Synthesized streaming, SSE parsers, billing events | Distribution path packaging, Mac signing/quarantine paths |

These are tendencies. **Either of us can pick up either kind of work when we see a clear blocker.** Codex caught Kiro's install.sh tarball-path mismatch (`0c13520`); Kiro caught Codex's chmod-on-`/tmp` regression (B-1). Strict ownership rules would have hidden both. Stay flexible.

---

## 2. Round, not commit, is the unit of review

A *round* is a coherent batch that ships together. Examples:

- Stage 25 (52 files: managed streaming + cost metadata + cloud sync + STT auth) — needed a round
- Phase 4 of the Observability plan (`bluey doctor` + `bluey logs export --redact`) — needs a round
- A 5-line typo fix — does NOT need a round

When the change is round-shaped, docs are mandatory. The implementer must
write an implementation/handoff doc and the reviewer must write a review
verdict doc. The ack/close doc is mandatory when the review returns blockers,
follow-ups, or when a self-merge needs a durable close note.

| Doc | Author | Path |
|---|---|---|
| Implementation / round handoff | Implementer | `docs/rounds/<TOPIC>-FOR-<REVIEWER>-REVIEW.md` |
| Review verdict | Reviewer | `docs/reviews/REVIEW-<TOPIC>-BY-<REVIEWER>.md` |
| Ack/close | Implementer | `docs/rounds/<TOPIC>-ACK.md` |

Otherwise, a `feat(...)` / `fix(...)` commit with a clear body is enough.

---

## 3. Round handoff template (3-line header is mandatory)

Every round handoff doc must start with this header:

```
Branch:     feat/phase-3-round-12
Tip before: <hash before this round's commits>
Tip after:  <hash after this round's commits, = HEAD>
```

Then in the body:

1. **What changed** — list of files / areas + 1-line summary each
2. **Why** — one paragraph of intent
3. **Verification** — exact commands run, observed counts (`cargo test --all-targets` → N passed, M ignored)
4. **Areas most likely wrong** — be specific; this is what the reviewer should focus on first
5. **Honest limitations** — what this round did NOT address that a reader might assume it did

---

## 4. Review verdict shape

| Glyph | Meaning |
|---|---|
| 🟢 | ACCEPT — round is shippable, no follow-up required |
| 🟡 | ACCEPT WITH FOLLOWUPS — shippable, but tracked items need next-round attention |
| 🔴 | BLOCKER — do not commit until resolved |

Verdict structure:

1. **Verdict** (🟢/🟡/🔴) at the top
2. **What I reviewed** — files + commands run
3. **What's right** — what the implementer got right (calibrates trust)
4. **Blockers** — numbered B-N, each with: file, root cause, recommended fix
5. **Nits** — numbered N-N, lower-priority cleanup
6. **Pipeline state** — actual fmt/clippy/test/build output at the tip
7. **Recommended action** — what the implementer (or reviewer) should do next

---

## 5. Reviewer self-implementation rule

The reviewer **may** self-implement a blocker fix when ALL of these hold:

- Single file
- <50 LOC change
- No architectural decision (just a clear bug)
- Pipeline stays green at the new tip

Larger blockers stay with the original implementer. The reviewer hands back with a recommended diff in the verdict doc.

Examples of accepted self-implementation:
- `e064e46` (Codex fixing a duplicate Tauri setup() during Kiro's review of S12-17)
- `43b5728` (Codex adding `activate_ignoring_other_apps` for LSUIElement focus)
- `0c13520` (Codex aligning the release workflow to produce `Bluey.app`)
- The 5-line B-1 fix Kiro applied during Stage 25 review

---

## 6. Working-tree contract

Whoever has uncommitted changes **owns the working tree on uno**. The other defers code edits to that area until the batch is committed.

Practical rule: before starting a code edit, run:

```bash
ssh -i ~/.ssh/id_ed25519 uno@192.168.4.25 'cd /Users/uno/Downloads/cue && git -P status --short'
```

If the output is non-empty AND the modified files overlap with what you're about to touch, **wait** for the owner to commit first, OR ask the user to coordinate.

Doc edits in `docs/reviews/`, `docs/rounds/`, `docs/work/` are append-only and parallel-safe — don't worry about those.

---

## 7. Pipeline gate (every commit)

Before every commit (round or not), the implementer runs the appropriate subset:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
# if server touched:
cd server && cargo clippy --all-targets -- -D warnings && cargo test
# if dashboard UI touched:
cd crates/cue-dashboard/ui && npm test --run && npm run build
# if overlay touched:
swift build -c release --package-path native/macos/cue-overlay
```

No "I'll fix it next commit." Round-tip pipelines must be green when the round closes.

---

## 8. Review SLA

A round handoff sitting unanswered for >24 hours may be self-merged by the implementer with:

1. Pipeline-gate clean at the tip
2. A note in the commit body: `unreviewed-self-merge: <reason>` (typical: "reviewer offline, ship-blocking gate")
3. A retroactive review doc the reviewer can write later

This avoids blocking on the relay (the user) being unavailable.

---

## 9. Communication — the relay limit

Kiro and Codex **cannot talk to each other directly**. The user is the relay. Every handoff is the user pasting context one direction or the other.

To minimize relay overhead:

- Round-shaped handoffs (3-doc pattern) are the durable contract. They survive context-window drift on either side.
- Avoid round-trip questions. If the implementer thinks the reviewer might ask "did you consider X?", answer it preemptively in the handoff.
- Acknowledge the verdict in writing (an ack commit) so future-you/me can find the close.

---

## 10. What we intentionally do NOT formalize

- **Per-commit review.** Most commits don't need it. Calibrate to round size.
- **Strict file-level ownership.** Either of us can edit any file when there's a clear bug.
- **Process bureaucracy.** No status meetings, no stand-ups, no kanban. The work is the artifact.
- **A "merge approver" role.** Pipeline gate + reviewer 🟢 = ship.

---

## 11. Next-round example: Observability split

After Phase 2 + Phase 3 close, the Observability Round (`docs/rounds/OBSERVABILITY-ROUND-PLAN.md`) is the next big work. Suggested split:

| Phase | Owner | Why |
|---|---|---|
| 1. Foundations: fields struct, `account_id_hash`, trace_id propagation, request-id middleware | Codex | Multi-crate plumbing, cross-cutting |
| 2. Daemon + dashboard log rotation (`tracing-appender`) | Codex | Integration with existing daemon |
| 3. Overlay lifecycle emits + frontend error capture | Codex | Swift + Tauri native idioms |
| 4. `bluey doctor` + `bluey logs export --redact` | Kiro | CLI shape, redactor reuses prior work |
| 5. Trace propagation through Tauri invoke + IPC | Codex | UI ↔ daemon glue |
| 6. Standard field migration sweep | Kiro | Mechanical sweep, gate-friendly |

Each phase = its own round. Whoever ships requests review from the other.

---

## 12. Disputes / disagreements

If Codex and Kiro disagree on a verdict (e.g., Kiro says 🔴 BLOCKER, Codex says it's a 🟡 nit), the user is the tiebreaker. Surface the disagreement in the verdict doc:

> **Disagreement note:** Codex considers this a Nit (rationale ...); Kiro considers it a Blocker (rationale ...). User decision requested.

Don't litigate by passing 5 rounds of paste back and forth.

---

## 13. Versioning this contract

This contract lives at `docs/rounds/COLLAB-CONTRACT-KIRO-CODEX.md`. Updates require a Codex 🟢 (or Kiro 🟢, if Codex is the proposer). User has final veto on contract changes.

---

## 14. TL;DR

- Round, not commit, is the unit of review.
- Mandatory round docs: implementation/handoff → review verdict → ack when follow-up or close-out is needed.
- Pipeline gate every commit. No exceptions.
- Reviewer can self-implement small blockers; hands back larger ones.
- Working-tree contract: whoever has uncommitted changes owns the tree.
- Specialty zones are tendencies, not rules. Pick up what's blocking.
- 24h review SLA. After that, self-merge with a note.
- User is the relay. Make handoffs durable so the relay can be slow.
