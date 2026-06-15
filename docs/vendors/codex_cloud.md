# OpenAI Codex Cloud — Vendor Dossier

> Audience: Bluey engineering. Goal: integrate OpenAI Codex Cloud as a CLOUD agent
> in `cue-agent-bridge` without branching code on the vendor name. The same
> registry-row philosophy that governs local agents must hold for cloud rows.

---

## STATUS REPORT (top-of-file summary)

### Complete & tested locally (no live account needed)

- DATA: `AgentKind::CodexCloud` variant added in `lib.rs` after the last existing
  variant, with serde snake_case label `codex_cloud`.
- DATA: `KindTag::CodexCloud` and round-trip mapping in `registry.rs`.
- DATA: Registry row for "OpenAI Codex Cloud" at the END of `REGISTRY`, with the
  required `billing_model` declared on a new `CloudProfile` struct.
- DATA: New `Transport::CloudHttps` variant for cloud agents, plus
  `BillingModel`, `AuthScheme`, and `EndpointSpec` types — pure data, vendor-
  agnostic.
- CODE (shared, vendor-agnostic): `cloud/transport.rs` (generic HTTPS transport
  trait + `OAuth2Auth` and `ApiKeyAuth` impls), `cloud/keychain.rs` (OS-keychain
  store, generic over vendor name), `cloud/audit.rs` (structured audit logging
  with redacted tokens).
- CODE (vendor-specific adapter): `cloud/codex_cloud.rs` builds the documented
  HTTP request shapes for `POST /v1/codex/cloud/tasks`, task status polling, and
  the OAuth refresh request shape. Pure: returns an `http::Request`-equivalent
  struct, no network call, so we can unit-test the wire format.
- WIRING: `crates/cue-daemon/src/app.rs` `answer_with_agent` route updated to
  dispatch on the registry row's `agent_shape` (turn vs task) so cloud agents
  hit the task-shaped path. Unknown registry rows fall through to the same
  honest "agent not ready" guidance card — no silent escalation to Bluey's AI.
- TESTS: Unit tests in `cloud/codex_cloud.rs` assert the exact JSON body of
  `POST /v1/codex/cloud/tasks` matches the documented shape; tests in
  `cloud/keychain.rs` (memory backend) round-trip a token; tests in
  `cloud/audit.rs` assert the token is never serialized into the log line;
  tests in `registry.rs` assert every cloud row declares `billing_model`.
- `cargo fmt --all`, `cargo clippy -- -D warnings`, `cargo test -p
  cue-agent-bridge -p cue-daemon` all clean.

### 🟡 Coded but NEEDS-LIVE-VERIFY (ChatGPT Plus/Pro/Team/Enterprise account + Codex Cloud access required)

- The `/v1/codex/cloud/tasks` request body **shape** is from a third-party API
  digest (apidog, see "Sources"), not an official OpenAI reference page. The
  fields (`task_prompt`, `environment`, `repository_context`, `webhook`) and
  the `{"id": "task_id"}` response are the best public approximation. The
  exact field names, status codes, and event taxonomy MUST be confirmed
  against a live OpenAI account before this code routes a production answer.
- Status polling / event streaming endpoint: no official URL is documented.
  The adapter exposes `task_status_request(task_id)` as a stub that emits a
  `GET /v1/codex/cloud/tasks/{id}` request — this is a best-guess REST shape
  and MUST be verified.
- Token refresh: the `~/.codex/auth.json` cache exists and Codex refreshes
  ChatGPT tokens automatically inside the official CLI, but the refresh
  *endpoint* and *response shape* are not in OpenAI's public docs. The
  adapter assumes the standard OAuth2 refresh-token form and surfaces it as
  a TODO with a defensive failure mode.

### ⚠️ Blocked / needs a product decision

- **There is no documented first-party OAuth flow for THIRD-PARTY apps to
  call Codex Cloud on a user's behalf.** "Sign in with ChatGPT" is for the
  official Codex CLI/IDE/Web — those are first-party clients. A third-party
  app like Bluey has TWO realistic paths:
  1. **BYOT (Bring Your Own Token) — OpenAI Platform API key.** User pastes
     a `sk-...` key. Billing draws against the user's OpenAI Platform account
     (NOT their ChatGPT Plus quota). This is what the wired adapter assumes
     by default. Mandatory disclosure: "this uses your OpenAI API credits,
     not your ChatGPT Plus subscription."
  2. **Shell out to the user's local `codex` CLI App Server.** Spawn `codex
     app-server`, speak JSON-RPC over stdio, let Codex itself hold the
     ChatGPT OAuth token in `~/.codex/auth.json`. This INHERITS the user's
     ChatGPT plan, costs them nothing extra, and means Bluey never touches
     a token. Path (2) is preferred for the **subscription** billing
     experience but requires the user to have run `codex login` once.
  - The registry row declares the billing model so the UI can render the
    right disclosure. Path (1) is the default in the data row; path (2) is a
    future variant (left as a TODO with the registry shape designed to
    accommodate it).
- **The product is task-shaped (assign → minutes → PR), NOT turn-shaped.**
  Same as Copilot Workspace / Cursor Background / Devin. Bluey's overlay is
  built around live turn streaming. A cloud task that returns a PR URL minutes
  later does NOT fit the "answer in the meeting" UX. Product must decide:
  surface as a "delegated this to Codex Cloud — PR will appear in N minutes"
  card with a link, or do not surface Codex Cloud at all for in-meeting Asks.
  The adapter is wired so the daemon emits a deferred-result card; the
  overlay UX work is OUT OF SCOPE for this batch.

### 🔴 Breaks the data-promise / would surprise a corporate user

- **No corporate user should expect Codex Cloud to keep their code inside
  their existing zero-retention CHAT plan boundary.** Codex Cloud spawns a
  cloud sandbox container that clones the repo. That repo content lives in
  OpenAI infrastructure for the task's duration (and per OpenAI's Enterprise
  data-retention policy after). The disclosure card MUST state: "Your repo
  is cloned into an OpenAI-hosted sandbox container for this task." This is
  separate from "Bluey never sends your audio to its own AI" — same value,
  different surface.

---

## 1. What Codex Cloud Is

Codex Cloud is OpenAI's hosted, background-executing variant of the Codex
coding agent. Where the local Codex CLI runs synchronously on the user's
machine, Codex Cloud spawns a sandboxed container in OpenAI infrastructure,
clones the user's GitHub repository, executes the task (writing code, running
tests, opening a pull request), and surfaces the result as a PR. Tasks run in
parallel and asynchronously.

The user-facing entry points are:

- The web UI at `https://chatgpt.com/codex`.
- The local Codex CLI extension's "Delegate to cloud" action (sends a task
  from the IDE to the cloud backend).
- Tagging `@codex` on GitHub issues / PRs (requires a connected ChatGPT
  account).
- The official Slack / Linear integrations (workspace-admin configured).

Codex Cloud is included with **ChatGPT Plus ($20/mo), Pro ($100+/mo),
Business, Edu, and Enterprise plans**. Users who exceed plan limits can buy
top-up credits.

Source: [Web – Codex | OpenAI Developers](https://developers.openai.com/codex/cloud)
Source: [Pricing – Codex | OpenAI Developers](https://developers.openai.com/codex/pricing)

---

## 2. Auth Model

### 2.1 The two first-party paths (used by `codex` CLI, VS Code extension, web)

1. **"Sign in with ChatGPT"** — browser OAuth. Returns an access token that
   the local Codex CLI caches at `~/.codex/auth.json` (plaintext) or, when
   `cli_auth_credentials_store = keyring`, in the OS keychain. Codex
   automatically refreshes tokens before they expire. Token format is not
   officially documented (the docs only call it an "access token"); empirical
   inspection of `~/.codex/auth.json` shows a JWT-shaped string plus refresh
   token plus account/plan metadata.
2. **OpenAI Platform API key** — `sk-...`. Pasted via `codex login --api-key`
   or `OPENAI_API_KEY`. Bills against the user's OpenAI Platform account
   (separate ledger from ChatGPT subscription).

The auth doc also describes an experimental `CODEX_ACCESS_TOKEN` env var that
lets the CLI ingest an externally-obtained ChatGPT access token via
`codex login --with-access-token`. This is the closest thing to a third-party
hand-off that exists today, and it still relies on the user obtaining the
token themselves — there is **no public third-party OAuth client-registration
flow**.

Source: [Authentication – Codex | OpenAI Developers](https://developers.openai.com/codex/auth)

### 2.2 What this means for Bluey (third-party app)

There is no documented third-party-app OAuth flow comparable to "Connect
GitHub" or "Connect Slack". The realistic integration paths, in order of
preference for a user with an existing ChatGPT Plus subscription:

1. **Local App Server pass-through (preferred for subscription billing).**
   Detect the local `codex` CLI (already in the registry as `AgentKind::Codex`,
   discovered via `codex` on PATH and `~/.codex` dotfile). Spawn `codex
   app-server` once, speak JSON-RPC 2.0 (header omitted) over stdio. The user's
   already-cached ChatGPT auth in `~/.codex/auth.json` covers billing; Bluey
   never sees the token. Trade-off: requires `codex login` to have been run
   once, and the task RPCs are still local-shaped (turn-based) — Codex's *cloud
   task delegation* isn't a documented App Server RPC.
2. **BYOT (Bring Your Own Token) HTTPS, OpenAI Platform API key.** User
   provides a `sk-...` key. Bluey stores it in the OS keychain (via the
   `keyring` crate, namespaced per vendor), reads it on each call, attaches
   `Authorization: Bearer <key>` to `POST /v1/codex/cloud/tasks`. Billing
   draws against the OpenAI Platform account — NOT the user's ChatGPT Plus
   quota. This **must be disclosed** in the consent card.

The wired adapter implements path (2) — the BYOT path — because it is the
only path the user can complete entirely inside Bluey without depending on
local CLI state. Path (1) is a future row.

### 2.3 Token storage hard requirement

- Token MUST live in the OS keychain (macOS Keychain, Windows Credential
  Manager, Linux Secret Service). NEVER plaintext disk.
- The `keyring` crate (already in `[workspace.dependencies]`) is used,
  namespaced as `service = "bluey.cue-agent-bridge"`, `username =
  "<vendor>.api_key"`.
- The token never appears in audit logs (token field is replaced by its SHA-256
  prefix, e.g. `sk-...[truncated:abc123]`).

### 2.4 Revocation

- ChatGPT OAuth path: the user revokes the Codex CLI authorization at
  `https://chatgpt.com/settings/applications` (Codex appears in the connected
  applications list when they ran `codex login`).
- API key path: the user revokes the key at
  `https://platform.openai.com/api-keys`.
- Bluey-side: a "Forget Codex Cloud token" button calls the keyring delete
  for the namespaced entry. This is the only revocation Bluey can perform
  unilaterally.

---

## 3. Endpoints

### 3.1 First-party documented programmatic surface: the App Server (LOCAL)

The Codex App Server is a long-lived JSON-RPC 2.0 (header omitted, JSONL
framing) process. The user runs `codex app-server` and Bluey speaks the
protocol over stdio. Methods relevant to a "send a question, get an answer"
flow:

| Method | Direction | Purpose |
|--------|-----------|---------|
| `initialize` | C → S | Hand-shake. Required first. Returns `userAgent`, `codexHome`, `platformOs`. |
| `initialized` | C → S notification | "Ready for traffic." |
| `account/read` | C → S | Returns current auth mode and plan. Use to verify the user is logged in. |
| `account/login/start` | C → S | `{type: chatgpt}` opens browser, returns `authUrl`. Generally Bluey would NOT call this — the user runs `codex login` once. |
| `thread/start` | C → S | Open a new thread. Returns a `threadId`. |
| `turn/start` | C → S | `{threadId, input: [{type:"text", text:"..."}], cwd, model, approvalPolicy, sandboxPolicy}` — starts an agent turn. |
| `turn/started`, `turn/completed`, `item/started`, `item/completed`, `item/agentMessage/delta` | S → C notifications | Streaming events Bluey would parse to render the answer. |
| `turn/interrupt` | C → S | Cancel an in-flight turn. |
| `account/rateLimits/read` | C → S | Surface remaining quota to the UI. |

**Critical observation:** The App Server protocol does NOT expose a
"delegate to cloud" RPC in the public spec. Cloud delegation appears to be
a first-party-only flow inside `chatgpt.com/codex` and the IDE extension's
hidden command surface. So shelling out to the App Server gives Bluey
*local* Codex (already covered by the existing `AgentKind::Codex` row),
not cloud Codex.

Source: [App Server – Codex | OpenAI Developers](https://developers.openai.com/codex/app-server)
Source: [codex/codex-rs/app-server/README.md](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md)

### 3.2 The (beta, third-party-reported) Cloud Task REST API

Per a third-party API digest (apidog), OpenAI exposes a beta endpoint:

```
POST https://api.openai.com/v1/codex/cloud/tasks
Authorization: Bearer <OPENAI_API_KEY>
Content-Type: application/json

{
  "task_prompt": "Refactor this module for TypeScript",
  "environment": {
    "runtime": "node:18",
    "packages": ["typescript"]
  },
  "repository_context": "https://github.com/user/repo/main",
  "webhook": "https://your-webhook.com",
  "multimodal_inputs": null
}
```

Response (synchronous, returns task id; work continues async):

```json
{ "id": "task_<opaque>" }
```

This endpoint is **NOT in OpenAI's official docs**. The shape is reproduced
above from the apidog digest. It is the only public reference to a
programmatic cloud-task surface. **NEEDS-LIVE-VERIFY** before routing real
traffic.

Status polling / event retrieval is **not** documented anywhere. By REST
convention the adapter assumes:

```
GET https://api.openai.com/v1/codex/cloud/tasks/{task_id}
Authorization: Bearer <OPENAI_API_KEY>

→ 200 OK
{
  "id": "task_<opaque>",
  "status": "queued" | "running" | "completed" | "failed",
  "result": null | { "pr_url": "https://github.com/...", "summary": "..." },
  "error": null | { "code": "...", "message": "..." }
}
```

This is **best-guess shape**. The adapter exposes it as a separate function so
swapping the real wire format is one PR.

Source: [What API Endpoints Are Available for Codex in 2026 (apidog)](https://apidog.com/blog/what-api-endpoints-available-codex-2025/)

### 3.3 The (also beta, third-party-reported) Code Review endpoint

```
POST https://api.openai.com/v1/codex/reviews
{
  "pull_request_url": "https://github.com/user/repo/pull/456",
  "focus_areas": ["security", "bugs"],
  "sandbox_config": {"network": "restricted"}
}
```

Out of scope for the answer route; documented here for completeness.

---

## 4. Billing Model

| Auth path | Quota counter | Disclosure required |
|-----------|---------------|---------------------|
| ChatGPT OAuth (path 1, future) | User's ChatGPT plan: Plus/Pro/Business/Edu/Enterprise. Per-5-hour window message budget (15–80 for Plus, scaled for Pro/Business). When exhausted, top-up credits apply. | "Uses your ChatGPT Plus subscription. No extra cost." |
| OpenAI API key (path 2, default) | User's OpenAI Platform account. Per-token rate (GPT-5.5: 125 credits / 1M input tokens). | "Uses your OpenAI API credits — billed separately from your ChatGPT subscription." |

This is THE key billing question and the cautious default is path 2 with the
disclosure. The registry row declares `billing_model: BillingModel::ApiCredits`
for the BYOT row; a future ChatGPT-OAuth row would declare
`BillingModel::Subscription`.

Source: [Using Codex with your ChatGPT plan | OpenAI Help Center](https://help.openai.com/en/articles/11369540-using-codex-with-your-chatgpt-plan)
Source: [Pricing – Codex | OpenAI Developers](https://developers.openai.com/codex/pricing)

---

## 5. Rate Limits

- ChatGPT-plan path: 5-hour rolling windows. Per-window message counts vary by
  model. The App Server exposes `account/rateLimits/read` to query the
  remaining quota; cloud REST surface returns standard HTTP 429 with
  `Retry-After` (assumed, NEEDS-LIVE-VERIFY).
- API-key path: standard OpenAI Platform per-org TPM / RPM limits apply on top
  of any Codex-specific quota. Cloud REST surface returns 429 with
  `Retry-After` and `X-RateLimit-Remaining-*` headers (standard OpenAI
  pattern).

The adapter retries once on 429 after `Retry-After` seconds, then surfaces an
"Agent rate-limited" guidance card (never escalates to Bluey's AI).

---

## 6. Session / Task Semantics

Codex Cloud is **task-shaped**, not turn-shaped:

1. Client submits `POST /v1/codex/cloud/tasks` → returns `task_id`, returns
   immediately (HTTP < 1s).
2. OpenAI spins up a sandbox container, clones the repo, runs the task.
3. Result becomes available **minutes later** (small tasks: 2–5 min; large
   tasks: 15–30 min — empirical, NEEDS-LIVE-VERIFY).
4. Client either polls `GET /v1/codex/cloud/tasks/{id}` or registers a
   webhook (the `webhook` field in the request body).
5. Final artifact is typically a **pull request URL** opened against the
   user's GitHub repo, plus a summary string.

Compare to a local turn (Claude Code, local Codex): user asks → answer
streams in seconds → done in one HTTP roundtrip / one process invocation.

This is the same shape as GitHub Copilot Workspace, Cursor Background Agents,
and Devin. The daemon's existing `answer_with_agent` ladder is turn-shaped;
the cloud row introduces an `agent_shape: AgentShape::Task` discriminator so
the route knows to emit a deferred-result card instead of attempting to
stream a live answer in the meeting.

---

## 7. Error Taxonomy

- `401 Unauthorized` → token revoked, expired, or invalid. UI: surface
  "Reconnect Codex Cloud" guidance, clear the keychain entry only after the
  user confirms.
- `403 Forbidden` → user lacks Codex Cloud entitlement on this account
  (e.g. free ChatGPT). UI: "Codex Cloud requires a ChatGPT Plus or higher
  plan, or a Platform API key with Codex access."
- `404 Not Found` (on `GET /v1/codex/cloud/tasks/{id}`) → task id is unknown
  or expired. UI: "Codex Cloud task no longer exists."
- `409 Conflict` → repo not connected, GitHub app not installed. UI:
  guidance to set up the integration at chatgpt.com/codex/settings.
- `422 Unprocessable Entity` → bad task body. Bug in Bluey; surface as an
  internal error.
- `429 Too Many Requests` → quota exhausted. Single retry after
  `Retry-After`; then "Codex Cloud is rate-limited."
- `500/502/503/504` → transient. Single retry with jitter; then "Codex
  Cloud is temporarily unavailable."

All shapes above are **convention-based** for OpenAI REST surfaces; the
Codex-specific error envelope is NEEDS-LIVE-VERIFY.

The App Server protocol's documented errors (`ContextWindowExceeded`,
`UsageLimitExceeded`, `HttpConnectionFailed`, `BadRequest`, `Unauthorized`,
`SandboxError`) apply when speaking JSON-RPC to the local server.

---

## 8. Webhooks / Streaming

- The cloud REST surface accepts a `webhook` URL in the task body. OpenAI is
  expected to POST a task-completion event to that URL when the task finishes.
  Payload shape is undocumented (NEEDS-LIVE-VERIFY).
- The App Server protocol exposes JSONL-framed streaming notifications
  (`turn/*`, `item/*`) over stdio for the LOCAL path.
- There is no documented SSE endpoint for cloud tasks. Bluey defaults to
  polling `GET /v1/codex/cloud/tasks/{id}` every 5s with exponential backoff
  up to 30s.

---

## 9. Data Residency / Corporate Compliance

- ChatGPT Enterprise: residency and retention follow the existing Enterprise
  controls — Zero Data Retention for App/CLI/IDE, but cloud sandboxes
  necessarily hold the cloned repo for the task duration. Audit logs are
  retrievable via the ChatGPT Compliance API (`/compliance/workspaces/{id}/logs`,
  event types `CODEX_LOG`, `CODEX_SECURITY_LOG`).
- Encryption at rest: AES-256. In transit: TLS 1.2+.
- The Codex Cloud sandbox runs on OpenAI infrastructure. Physical region
  selection / residency requires Enterprise contact.

Source: [Admin Setup – Codex | OpenAI Developers](https://developers.openai.com/codex/enterprise/)

---

## 10. MCP

Codex *exposes* an MCP server (`codex-mcp-server`) so other agents (e.g.
Claude Code) can call Codex as a tool. The local Codex CLI also *consumes*
MCP servers configured under `~/.codex/config.toml`.

Direction for Bluey:

- Bluey is the OUTER agent. Codex Cloud is the inner agent doing the work.
- Bluey does NOT expose itself as an MCP server to Codex in this batch.
- Codex's own configured MCP servers would run inside the cloud sandbox —
  not inside Bluey's process — so they don't affect Bluey's transport
  surface.

---

## 11. Edge Cases

- **Long tasks (>10 min):** poll up to 30 min total; then mark task as
  "still running" and detach. The PR URL is the eventual artifact; the user
  follows it manually.
- **Parallel tasks:** Plus/Business have small parallel quotas; the adapter
  serializes Bluey-initiated tasks per (user, repo) by default. Real
  parallel-task semantics NEEDS-LIVE-VERIFY.
- **Repo clone failures:** surface the OpenAI error message verbatim; do
  not retry (the failure is almost always a permissions / GitHub-app issue
  the user must fix).
- **Stale task_id between Bluey restarts:** persist task_id + initiating
  request_id in the daemon's existing card store so a restart can rehydrate
  the "Codex Cloud working on it" card and resume polling.

---

## 12. OPEN QUESTIONS (assumptions made; here is what changes if wrong)

These are the gaps the official docs do not close. Each is an explicit
assumption Bluey ships with; if the assumption is wrong, the noted line is
what must change.

1. **The `/v1/codex/cloud/tasks` request body shape is correct.**
   Assumption from third-party apidog digest; not on OpenAI's docs. If wrong,
   `cloud/codex_cloud.rs::create_task_request` body construction changes;
   tests assert the shape so the failure is loud.
2. **The task-status URL is `GET /v1/codex/cloud/tasks/{id}` with a
   `status`/`result` envelope.** If wrong (e.g. status comes via a separate
   `/v1/codex/cloud/tasks/{id}/events` SSE), the polling loop swap is a
   single function in the adapter.
3. **There is no third-party-app OAuth flow that grants ChatGPT-Plus billing
   to Bluey.** Assumption: confirmed by absence in current docs (June 2026).
   If OpenAI ships one, add a new registry row with
   `auth_scheme: AuthScheme::OAuth2 { token_url, ... }` and
   `billing_model: BillingModel::Subscription`. Existing row stays.
4. **The local `codex app-server` JSON-RPC does NOT expose cloud-task
   delegation as a public RPC.** Assumption: not in the listed methods. If a
   `cloudTask/start`-style RPC exists, prefer it (uses ChatGPT auth, no token
   handling in Bluey). Add as a new registry row variant.
5. **Bluey is responsible for displaying the disclosure that a Codex Cloud
   task clones the user's repo into OpenAI-hosted infra.** No automated
   contract enforces this. If the disclosure changes, update the daemon's
   guidance-card text — the registry row carries no UI strings.
6. **Webhook payload shape is unspecified.** Assumption: the polling fallback
   is sufficient for v1; webhooks are not wired in this batch. If a webhook
   contract appears, add a verifier + endpoint to the daemon.
7. **5-second polling cadence is conservative.** Assumption: reasonable
   default until OpenAI publishes a recommended cadence. Configurable via the
   registry row's `poll_interval_ms`.

---

## 13. PROPOSED SPEC

Pure-data shape. No vendor name in any code branch.

### 13.1 New `AgentKind` variant

```rust
pub enum AgentKind {
    // ... existing local variants ...
    Codex,           // local CLI (existing)
    // ...
    CodexCloud,      // NEW: hosted task-shaped agent
}
```

Serde label: `"codex_cloud"`. Round-trips through the same JSON path the
overlay uses to attach an agent (`parse_attached_agent` in `cue-daemon`).

### 13.2 New `KindTag` variant

`KindTag::CodexCloud`, with the bidirectional mapping added to
`to_agent_kind` / `from_agent_kind`.

### 13.3 New cloud-side data types (in `cloud/spec.rs`)

```rust
/// How an agent is reached.
pub enum AgentSurface {
    /// Local CLI subprocess — existing path (Claude, Cursor, Codex CLI, …).
    LocalCli,
    /// Remote HTTPS REST surface (Codex Cloud, Cursor Cloud, Copilot Spaces).
    CloudHttps {
        auth: AuthScheme,
        endpoints: EndpointSpec,
    },
}

/// Whether an agent is turn-shaped (live streaming) or task-shaped
/// (assign → minutes → artifact). Drives the daemon's route shape and the
/// overlay's card type.
pub enum AgentShape {
    Turn,
    Task,
}

/// Where this agent's usage shows up on the user's bill. Surfaces in the
/// consent card so the user is never surprised. EVERY cloud row MUST declare.
pub enum BillingModel {
    /// Counts against the user's vendor subscription (e.g. ChatGPT Plus).
    Subscription { plan_family: &'static str },
    /// Counts against the user's API credit balance (BYOT API key).
    ApiCredits { provider_label: &'static str },
    /// Bluey-managed billing relationship (not used for any cloud agent yet).
    BlueyManaged,
}

/// How Bluey gets a credential to call this cloud agent.
pub enum AuthScheme {
    /// User pastes an API key; stored in OS keychain; sent as `Authorization:
    /// Bearer <key>`. NEVER plaintext disk.
    ApiKeyHeader {
        keychain_account: &'static str,    // e.g. "openai.api_key"
        env_fallback: &'static str,        // e.g. "OPENAI_API_KEY"
        consent_text: &'static str,
    },
    /// Future: full third-party OAuth2 with authorization code + refresh.
    /// Not used by Codex Cloud's row in this batch (no documented flow).
    OAuth2 {
        authorize_url: &'static str,
        token_url: &'static str,
        client_id_env: &'static str,
        scopes: &'static [&'static str],
        keychain_account: &'static str,
    },
}

/// REST endpoints expressed as data — never branched on vendor name.
pub struct EndpointSpec {
    pub base_url: &'static str,                       // "https://api.openai.com"
    pub create_task_path: &'static str,               // "/v1/codex/cloud/tasks"
    pub get_task_status_path_template: &'static str,  // "/v1/codex/cloud/tasks/{id}"
    pub poll_interval_ms: u64,                        // 5000
    pub max_total_wait_ms: u64,                       // 30 * 60 * 1000
}

/// Per-cloud-vendor profile sitting alongside `FixProfile` on the registry row.
pub struct CloudProfile {
    pub surface: AgentSurface,
    pub shape: AgentShape,
    pub billing: BillingModel,
    /// Default model id the vendor expects (e.g. "gpt-5-codex"). Pure data.
    pub default_model: &'static str,
}
```

### 13.4 The Codex Cloud registry row (data, appended at END)

```rust
AgentEntry {
    kind_tag: KindTag::CodexCloud,
    display_name: "OpenAI Codex Cloud",
    binary_candidates: &[],
    app_bundles: &[],
    app_dirs_windows: &[],
    data_dir_globs: &[],   // No local footprint — cloud-only.
    app_data_windows: &[],
    session_format: None,  // No local store.
    jsonl_subdir: "",
    drive_command: &[],    // Never spawned — cloud HTTPS.
    answer_args: &[],
    mcp_allow_flag: None,
    install: None,         // Account, not a binary.
    fix: FixProfile {
        propose_args: &[],
        apply_args: &[],
        apply_supported: false,  // Cloud tasks are PRs, not local diffs.
    },
    cloud: Some(CloudProfile {
        surface: AgentSurface::CloudHttps {
            auth: AuthScheme::ApiKeyHeader {
                keychain_account: "openai.api_key",
                env_fallback: "OPENAI_API_KEY",
                consent_text: "Bluey will store your OpenAI API key in your \
                    OS keychain and send it on each Codex Cloud call. \
                    Your repo is cloned into an OpenAI-hosted sandbox for \
                    each task. This bills your OpenAI API credits, NOT your \
                    ChatGPT subscription.",
            },
            endpoints: EndpointSpec {
                base_url: "https://api.openai.com",
                create_task_path: "/v1/codex/cloud/tasks",
                get_task_status_path_template: "/v1/codex/cloud/tasks/{id}",
                poll_interval_ms: 5_000,
                max_total_wait_ms: 30 * 60 * 1_000,
            },
        },
        shape: AgentShape::Task,
        billing: BillingModel::ApiCredits {
            provider_label: "OpenAI Platform",
        },
        default_model: "gpt-5-codex",
    }),
},
```

### 13.5 Irreducible per-vendor adapter code

What CAN'T be pushed to data:

1. **Building the request body for `POST /v1/codex/cloud/tasks`.** The body
   shape (`task_prompt`, `environment`, `repository_context`, …) is
   Codex-specific. Lives in `cloud/codex_cloud.rs` as `build_create_task_body
   (prompt: &str, repo: &str) -> serde_json::Value`. Pure; testable; no I/O.
2. **Parsing task-status responses into a normalized `TaskState` enum.**
   Maps Codex's `status` strings into Bluey's `Queued | Running | Completed |
   Failed`. Lives in the same adapter as `parse_task_status(body:
   &serde_json::Value) -> TaskState`. Pure; testable.
3. **Mapping HTTP errors to user-facing guidance strings.** Codex's exact
   error envelope. Lives in `cloud/codex_cloud.rs::guidance_for_status`.
   Pure; testable.

Everything else (HTTPS dispatch, retry/backoff, audit logging, keychain
get/put) sits in `cloud/transport.rs`, `cloud/keychain.rs`, `cloud/audit.rs`
— vendor-agnostic.

---

## 14. Sources

- [Web – Codex | OpenAI Developers](https://developers.openai.com/codex/cloud)
- [SDK – Codex | OpenAI Developers](https://developers.openai.com/codex/sdk)
- [Authentication – Codex | OpenAI Developers](https://developers.openai.com/codex/auth)
- [Pricing – Codex | OpenAI Developers](https://developers.openai.com/codex/pricing)
- [App Server – Codex | OpenAI Developers](https://developers.openai.com/codex/app-server)
- [codex/codex-rs/app-server/README.md (GitHub)](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md)
- [Admin Setup – Codex | OpenAI Developers](https://developers.openai.com/codex/enterprise/)
- [Changelog – Codex | OpenAI Developers](https://developers.openai.com/codex/changelog)
- [Quickstart – Codex (setup=cloud) | OpenAI Developers](https://developers.openai.com/codex/quickstart?setup=cloud)
- [Using Codex with your ChatGPT plan | OpenAI Help Center](https://help.openai.com/en/articles/11369540-using-codex-with-your-chatgpt-plan)
- [Codex rate card | OpenAI Help Center](https://help.openai.com/en/articles/20001106-codex-rate-card)
- [Codex Pricing 2026 (Verdent Guides)](https://www.verdent.ai/guides/codex-pricing-2026)
- [What API Endpoints Are Available for Codex in 2026 (apidog)](https://apidog.com/blog/what-api-endpoints-available-codex-2025/) — sole public reference to `/v1/codex/cloud/tasks` shape; NEEDS-LIVE-VERIFY.
- [The Codex App-Server: Building Custom Integrations with the JSON-RPC Protocol (Daniel Vaughan)](https://codex.danielvaughan.com/2026/03/28/codex-app-server-json-rpc-protocol/)
- [OpenAI Codex App Server (Promptfoo)](https://www.promptfoo.dev/docs/providers/openai-codex-app-server/)
