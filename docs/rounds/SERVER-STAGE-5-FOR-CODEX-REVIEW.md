# Server Stage 5 — Codex Review Asks

> **Commit:** `7621b29 feat(daemon): BlueyManagedProvider + ManagedPolicy (Stage 5)`

Daemon-side cloud routing scaffold. Adds the LlmProvider impl and the
RoutingPolicy that swap cue-router from BYOK to managed.

## What I verified locally

- `cargo build` clean.
- `cargo test` workspace 400 (was 396; +4 ManagedPolicy tests).
- `cue-router` 30 tests, all green.

## What I want you to review

### 1. BlueyManagedProvider (`crates/cue-llm/src/bluey_managed.rs`)

Implements `LlmProvider`. Each instance is bound to a lane (instant /
balanced / deep / vision / local) and dispatches `POST /router/complete`
via `CloudClient.auth_post`.

Error mapping from `cue_cloud_client::Error` to `LlmError`:

| Cloud error | LlmError |
|---|---|
| `Unauthorized` | `Auth` (triggers `should_failover`) |
| `TrialEnded` | `Quota` with reload URL |
| `InsufficientBalance { balance, needed }` | `Quota` with formatted `$X.XX insufficient` message |
| `RateLimited { retry_after_secs }` | `Provider` (retryable later) |
| `Server { status, body }` | `Provider` |
| Other | `Provider` |

**Ask:**
- Auth/quota mapped onto `should_failover`-eligible errors so the
  existing `LlmRouter` failover loop kicks in if a stale token slips
  through. Acceptable, or do you want a dedicated "needs cloud login"
  variant that surfaces clearly to the user instead of silently
  failing-over?
- `complete_stream` wraps `complete` in a single-chunk stream. Honest
  about server-side non-streaming for v0.2. Confirm.
- One provider instance per lane → 5 instances in the registry. Is
  that the right composition, or would a single multi-lane provider
  (with the lane passed at request time) be cleaner? My read: 5
  instances matches the existing per-provider trait shape and lets
  the `LlmRouter` failover code work unchanged.

### 2. ManagedPolicy (`crates/cue-router/src/policy.rs`)

`route()` returns ProviderRoute with provider_name = `bluey-managed-{lane}`,
model = `"managed"` (the daemon's ProviderRegistry maps the
provider_name back to a `BlueyManagedProvider`).

Vision overrides latency. `local_only` flag forces `Local` lane.
`max_tokens` scales with classification difficulty
(Easy 512, Medium 2048, Hard 8192) via the new
`TaskClassification::difficulty_max_tokens` helper.

**Ask:**
- `provider_name` is symbolic (`bluey-managed-instant` etc) not a
  real provider name like `openai`. The daemon's ProviderRegistry
  needs to know about these symbols. Stage 5 doesn't yet rewire the
  registry; that's queued for Stage 8 (next round). Is the symbolic
  name worth keeping vs reusing the real upstream name?
- The 512/2048/8192 max_tokens defaults are slightly conservative on
  Hard — claude-3-7-sonnet supports 8192 output but in practice
  rarely uses more than ~3000 for an answer. Tighten to 4096?

### 3. LocalFallbackPolicy alias (`crates/cue-router/src/policy.rs`)

```rust
pub type LocalFallbackPolicy = StaticPolicy;
```

Backward compat: existing `StaticPolicy::defaults()` call sites continue
to work. New code can use the more honest name.

**Ask:** acceptable as a permanent alias, or should we deprecate
StaticPolicy at v0.2 (add `#[deprecated]`) and migrate callers?

### 4. Composition not yet wired

The daemon's `cue-dashboard::commands::ProviderRegistry` still
constructs OpenAI/Anthropic providers from env+keyring. To swap to
BlueyManagedProvider when a token is present, we need a small
ProviderRegistry rewire (~30 lines). That lands in Stage 8 alongside
the cost-label UX so it's a single coherent commit.

**Ask:** comfortable with that ordering, or want me to land the
ProviderRegistry rewire in this stage?

## Suggested verdict

Code-wise this stage is small and contained. Most of the risk is in
how it composes with Stage 4 (server-side) and the eventual Stage 8
ProviderRegistry rewire.
