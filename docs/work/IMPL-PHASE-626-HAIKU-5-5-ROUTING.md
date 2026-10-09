# IMPL: Phase 626 — Haiku 5.5 and GPT-6 Sol bounded routing

> **Codex preflight:** Loaded the `bluey-ops`, `pinky-bluey-integration-ops`,
> and `openai-docs` skills and reconciled them against the current worktree.

## Scope

**Does:**

- Promote Anthropic `claude-haiku-5-5` to the fast trusted candidate while
  retaining Haiku 4.5 pricing for historical usage reconciliation.
- Serialize Haiku 5.5's provider-documented adaptive/disabled thinking shape,
  omit incompatible sampling and legacy-budget fields, and expose only typed
  text blocks from non-streaming and streaming responses.
- Add `gpt-6-sol` as a default-off final instant/balanced fallback with
  explicit `reasoning_effort`, `max_completion_tokens`, and no temperature.
  Only exact server value `BLUEY_GPT6_SOL_BENCHMARK_ENABLED=1` admits it;
  `gpt-5.4-mini` remains the initial live baseline.
- Reserve both new candidates at conservative high-context prices with 200%
  markup, then settle exact provider usage at the documented low/high input
  threshold so short-context requests are not overcharged.
- Add focused serializer, typed-content, route, and pricing regressions.

**Does NOT:**

- Change the primary OpenAI fast model, default route lengths, deep or vision
  candidate order, provider keys, database state, customer entitlements, or
  production deploy.
- Read provider credentials, issue paid inference, run a live completion smoke,
  or claim model quality/latency results.
- Modify Pinky integration storage/HTTP files or shared module registration.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `server/src/routing/dispatcher.rs` | Modified | Haiku 5.5/GPT-6 Sol routes, safe serializers, typed extraction, regressions |
| `server/src/pricing/mod.rs` | Modified | High-tier admission, tier-aware exact settlement, historical Haiku 4.5 |
| `docs/MODEL-ROUTING.md` | Modified | Exact candidate order and provider request contracts |
| `docs/PRICING-MODEL.md` | Modified | Price snapshot, context-tier rationale, and historical note |
| `docs/work/IMPL-PHASE-626-HAIKU-5-5-ROUTING.md` | Created | Scoped implementation and validation receipt |

## Provider Evidence

- Anthropic's [Haiku 5.5 overview](https://platform.claude.com/docs/en/models/haiku-5-5/overview),
  [migration guide](https://platform.claude.com/docs/en/models/haiku-5-5/migration-guide),
  and [thinking guide](https://platform.claude.com/docs/en/build-with-claude/thinking)
  define the exact model id, adaptive default, disabled non-thinking shape,
  incompatible sampling fields, typed content blocks, usage, and tiered prices.
- OpenAI's [GPT-6 Sol model card](https://developers.openai.com/api/docs/models/gpt-6-sol)
  and [reasoning guide](https://developers.openai.com/api/docs/guides/reasoning)
  establish Chat Completions support, explicit `none` reasoning effort,
  completion-token limits, sampling constraints, and long-context pricing.
- The owner-provided metadata probes returned HTTP 200 for exact ids
  `claude-haiku-5-5` and `gpt-6-sol`. This agent did not read the keys.

## Build & Test

```text
NOT RUN by this agent: Cargo build, test, clippy, database, and live provider
smoke are explicitly owned by the parent validation queue.

Source-only checks performed after freeze:
- all helper call sites inspected
- route/docs/pricing id reconciliation inspected
- git diff --check
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Admission always uses conservative high-context rates | Exact settlement now applies the provider's full-input threshold, separating no-under-reserve admission from fair customer charging |

## Known Follow-ups

- Parent must run the queued dispatcher/pricing tests and clippy on the frozen
  source.
- The default-off routing and tier-aware settlement repairs address the review
  blockers found after the first source-only pass; they are not live evidence.
- Operator must complete bounded funded preproduction smokes before enabling or
  promoting either candidate. A successful metadata lookup is not a completion
  smoke and authorizes no production rollout.
- Measure GPT-6 Sol latency/quality/cost before any primary-route decision.

## Review Checklist (for reviewer)

- [ ] Files match the scope described above
- [ ] No unrelated changes included
- [ ] Tests cover route order, serializer shape, typed extraction, and pricing
- [ ] Existing refusal/truncation fences remain intact
- [ ] Historical Haiku 4.5 pricing remains available
- [ ] No TODOs without linked task IDs
