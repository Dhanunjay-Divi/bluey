# REVIEW: Server Stage 5 — BlueyManagedProvider and ManagedPolicy

**Commit:** `7621b29 feat(daemon): BlueyManagedProvider + ManagedPolicy (Stage 5)`  
**Reviewer:** Codex  
**Date:** 2026-05-19

## Per-Task Review

### Stage 5 — Managed Provider Scaffolding

| Field | Value |
|-------|-------|
| Files | `crates/cue-llm/src/bluey_managed.rs`, `crates/cue-router/src/policy.rs`, `crates/cue-llm/src/router.rs` |
| Verdict | 🔴 blocker |

**Findings:**

- 🔴 `crates/cue-router/src/policy.rs:201` / `crates/cue-router/src/policy.rs:209` — `ManagedPolicy::local_only()` returns `provider_name = "bluey-managed-local"`, which sends privacy/local-only work to `bluey-server`. The server then maps `local` to Ollama but rejects it as unsupported in `server/src/routing/dispatcher.rs:43`. Local-only should route to `LocalFallbackPolicy` / on-device Ollama, not through the managed cloud provider.
- 🔴 `crates/cue-llm/src/bluey_managed.rs:81` / `crates/cue-llm/src/router.rs:49` — managed cloud `Unauthorized`, `TrialEnded`, and `InsufficientBalance` are mapped to `LlmError::Auth` / `LlmError::Quota`, and `LlmRouter` treats those as failover errors. If the daemon later registers `BlueyManagedProvider` beside direct OpenAI/Anthropic/Ollama providers, a no-balance/auth failure can silently fail over to an unmetered provider. Managed billing failures must be terminal in managed mode, or the registry must guarantee managed and direct providers are never mixed in one failover chain.
- 🟡 `crates/cue-llm/src/bluey_managed.rs:52` — `estimated_input_tokens` is always `None`, forcing the server to use the crude `chars/4` fallback. That is acceptable for a scaffold, but it weakens entry checks for large prompts with attachments and should be replaced with router/session token estimates once daemon integration lands.
- 🟡 `crates/cue-llm/src/bluey_managed.rs:84` / `crates/cue-llm/src/bluey_managed.rs:91` — reload URL is hard-coded to `https://bluey.dev/reload` even though `cue-cloud-client` preserves the server `reload_url`. Use the server-provided URL so staging/custom domains and future web app paths work.

## Cross-Task Findings

- Stage 5 is correctly isolated as scaffolding, but the local-only and failover semantics must be fixed before wiring it into production routing.

## Build & Test Verification

```bash
cargo test -p cue-router --lib         # ✅ 30 passed
cargo test -p cue-cloud-client --lib   # ✅ 4 passed
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Managed provider wiring would be unsafe without fixing local-only routing and billing-error failover behavior.

## Follow-ups for Next Batch

- Add an integration test proving a 402 from `BlueyManagedProvider` does not fall through to direct providers in managed mode.
- Add a local-only policy test that asserts local-only never uses `bluey-managed-*`.
