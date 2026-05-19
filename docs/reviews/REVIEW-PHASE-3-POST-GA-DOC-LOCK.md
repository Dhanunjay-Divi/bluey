# REVIEW: Phase 3 Post-GA Doc Lock + Auto Router Chain

**Commit range:** `bd10762..ad4f9fc`
**Reviewer:** Codex
**Date:** 2026-05-19

## Per-Task Review

### Runtime Change — Speculative Routing Default-ON

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/src/commands.rs`, `crates/cue-router/src/speculative.rs`, `DECISIONS.md` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟡 `crates/cue-dashboard/src/commands.rs:970-979` implements the requested default-ON semantics correctly: unset means ON, and `0/false/off` disables. That is acceptable for internal v0.1 dogfooding only because `DECISIONS.md:212-218` explicitly says BYOK/internal and reverse if external users or cost concerns appear.
- 🟡 Stale comments remain: `crates/cue-dashboard/src/commands.rs:1105-1109` still says speculative is opt-in and unset falls through; `crates/cue-router/src/speculative.rs:67-68` still says opt-in because of parallel-spend cost. Those comments now contradict the code and should be updated before the next agent relies on them.

---

### Auto Router Daemon/Dashboard Wiring

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/src/commands.rs`, `docs/PRODUCTION-READINESS.md`, `docs/rounds/PHASE-3-ROUND-13-14-HANDOFF-FOR-CODEX-REVIEW.md` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 The docs claim every `request_cue` / `auto_recap` prompt is classified and emits `RouterMeta`, but `auto_recap` still emits `router_meta: None` with a comment saying recap is not routed (`crates/cue-dashboard/src/commands.rs:1271-1285`). This directly contradicts `docs/PRODUCTION-READINESS.md:45-49` and `docs/rounds/PHASE-3-ROUND-13-14-HANDOFF-FOR-CODEX-REVIEW.md:79-82`. Either wire recap through `classify_for_router` and emit metadata on its first chunk, or narrow the docs to `request_cue` only.
- 🟡 `request_cue` classification and speculative dispatch are otherwise in the right shape. `cargo test -p cue-router` and `cargo clippy -p cue-router --all-targets -- -D warnings` passed locally.

---

### Pricing / Monetization Docs

| Field | Value |
|-------|-------|
| Files | `DECISIONS.md`, `docs/PRICING-MODEL.md`, `docs/HOW-IT-WORKS.md`, `docs/PRODUCTION-READINESS.md`, `crates/cue-router/src/policy.rs` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 Customer-facing cost examples conflict across active source-of-truth docs. `docs/PRICING-MODEL.md:36-42` says Hard code is `$0.050`, System design is `$0.104`, and Vision is `$0.046`; `docs/HOW-IT-WORKS.md:187-217` describes a hard speculative question as `$0.30-$0.32`; `DECISIONS.md:74-80` says Hard is about `$0.30` at 200% markup. Pick one canonical per-task price model and propagate it everywhere.
- 🔴 Margin claims conflict. `docs/PRICING-MODEL.md:211-218` correctly states 200% markup gives about 67% gross margin and 150% markup gives about 60%, but `DECISIONS.md:79-81` claims an 87-99% gross-margin band. The latter is not true for the locked markup tiers if upstream inference cost is the main cost basis.
- 🔴 The per-question table cannot be audited against provider pricing as written because it only lists total tokens/images. Official providers price input and output tokens separately, and vision/image inputs are tokenized based on image size/detail rather than "1 image" as a flat unit. Add columns for input tokens, output tokens, image tokens/detail, provider price snapshot date, raw-cost formula, and model availability.
- 🟡 The model set is stale for a forward-looking v0.2 pricing lock. `crates/cue-router/src/policy.rs:57-65` and `docs/PRICING-MODEL.md:37-39` use `claude-3-5-sonnet-latest` / `claude-3-7-sonnet-latest`; Anthropic's current pricing page lists Claude Sonnet 4.6 / 4.5 and marks Sonnet 4 deprecated, with older releases outside the current main pricing table. Keep legacy models if they are intentionally pinned, but document the rationale and availability risk.

Sources checked for current pricing basis:
- OpenAI API pricing: https://platform.openai.com/docs/pricing
- OpenAI image input cost model: https://platform.openai.com/docs/guides/images-vision
- Anthropic Claude API pricing: https://docs.anthropic.com/en/docs/about-claude/pricing

---

### Doc Reorganization / Usability

| Field | Value |
|-------|-------|
| Files | `AGENT-HANDOFF.md`, `AGENT-ONBOARDING.md`, `ARCHITECTURE.md`, `FUTURE-IMPLEMENTATIONS.md`, `SERVER-REFERENCE.md`, `docs/PRODUCTION-READINESS.md`, `docs/RELEASE-RUNBOOK.md`, `docs/AUTO-ROUTING-USP.md` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 Canonical docs still point at `docs/work/PHASE-3-ROUND-14-PLAN.md` and `docs/work/PHASE-3-WINDOWS-BRIEF-FOR-CODEX.md`, but those files were moved to `docs/rounds/`. Examples: `AGENT-HANDOFF.md:25-27`, `AGENT-HANDOFF.md:156-164`, `ARCHITECTURE.md:152`, `ARCHITECTURE.md:196`, `FUTURE-IMPLEMENTATIONS.md:26-40`, `SERVER-REFERENCE.md:77`, `docs/PRODUCTION-READINESS.md:127`. New agents following the "read first" docs will hit dead links.
- 🟡 `docs/AUTO-ROUTING-USP.md:121-130` still says the `SpeculativeRouter` is structural and daemon wiring is next round. That is stale after `request_cue` wiring and the default-ON flip.
- 🟢 `git log --follow` spot checks on moved round/review docs preserved history. The move mechanics are fine; the broken references need cleanup.

---

### Architecture / R14 Split

| Field | Value |
|-------|-------|
| Files | `ARCHITECTURE.md`, `FUTURE-IMPLEMENTATIONS.md`, `docs/HOW-IT-WORKS.md` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟢 The plug-point design in `ARCHITECTURE.md:227-260` is directionally sound: the router abstractions can swap from local/static providers to managed provider/policy without rewriting the classifier.
- 🟡 `ARCHITECTURE.md:292-299` says BYOK still works when no account is logged in during Stage 3, while `DECISIONS.md:110-145` says production is managed-only and BYOK is dev-mode-only / gated. Make that sentence say "dev-mode BYOK remains gated for internal testing" so it does not reopen the rejected product path.
- 🟢 R14.10-R14.14 are split at a good granularity: client, managed provider/policy, fallback, wallet/metering, and cost-label UX are separable review units.

## Cross-Task Findings

- The router crate is moving in the right direction, and the latest fix wave addresses the earlier classifier/policy pitfalls.
- The blocker set is now mostly integration/docs truthfulness rather than algorithm design.
- Do not green-light the post-GA strategy lock until the commercial docs are internally consistent and the moved-path references are repaired.

## Build & Test Verification

```bash
cargo test -p cue-router                                  # ✅ 26 passed
cargo clippy -p cue-router --all-targets -- -D warnings   # ✅
```

Full workspace pipeline was not re-run by Codex in this pass; Kiro reports it green at 392 cargo + 15 vitest.

## Overall Verdict

🔴 **REQUEST CHANGES** — Blockers must be resolved before treating this as the v0.2 strategy source of truth.

## Follow-ups for Next Batch

- Wire `auto_recap` router metadata or narrow the docs.
- Fix all stale `docs/work/...` references to `docs/rounds/...` or restore compatibility shims.
- Reconcile `DECISIONS.md`, `docs/HOW-IT-WORKS.md`, and `docs/PRICING-MODEL.md` to one pricing source of truth.
- Add pricing formulas with input/output/image-token assumptions and current provider price snapshot dates.
- Refresh stale Auto Router docs/comments after the default-ON speculative flip.
