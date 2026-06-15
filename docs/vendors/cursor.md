# Cursor Cloud Agents — vendor dossier

> Owner: the `agent/agent-bridge` integration loop, Bluey.
> Status (top): see "Step 6 — Status report" below — kept at the top of the file so a future reader sees it first. The detailed sections that justify it follow.

---

## Step 6 — Status report (kept at top)

### Done & tested
- **Generic cloud transport primitives** (`crates/cue-agent-bridge/src/cloud/transport.rs`).
  Extended an existing transport scaffold authored by the parallel Anthropic
  integration. Added the `CloudAuth::BasicApiKey` variant (Cursor's `-u key:`
  form, base64 of `key:` with the *required* trailing colon), the
  `BearerAuth`/`BasicAuthApiKey` convenience constants, the `HttpRequest` /
  `HttpResponse` value types, and the `CloudHttpsTransport::send` /
  `::request_for` executors. Both schemes are tested against a wiremock
  server and via header-shape assertions; the credential is dropped from
  scope the moment the header is applied (no in-flight plaintext in heap).
- **OS-keychain token storage** (`crates/cue-agent-bridge/src/cloud/keychain.rs`).
  Backed by the `keyring` crate (workspace dep). Service name
  `bluey_cloud_<vendor>`; Cursor stores under `bluey_cloud_cursor_cloud /
  api_key`. Custom `Debug` impls emit the vendor only — never the value —
  guarded by `test_*_debug_carries_no_secret_shape` assertions.
- **Structured cloud audit log** (`crates/cue-agent-bridge/src/cloud/audit.rs`).
  Every cloud HTTP call emits one structured `tracing::info!` line with
  `vendor`, `endpoint`, `method`, `status`, `latency_ms`, `request_id_present`
  — never the token, prompt, repo URL body, or response body. The
  `AuditEvent` struct's allow-list is explicit so a future field that would
  hold a secret has to be added deliberately.
- **`AgentKind::CursorCloud` variant** added (`crates/cue-agent-bridge/src/lib.rs`
  + `KindTag::CursorCloud` round-trip mapping in `registry.rs`). The daemon's
  `agent_display_name` now resolves through the cloud-aware `display_name_for`,
  which walks the cloud registry too — no daemon-side per-vendor branch.
- **Cursor cloud registry row** (`cloud/cursor.rs::ENTRY`) registered via the
  shared `CLOUD_REGISTRY` table. Declares `billing_model = ApiCredits`,
  `task_shaped = true`, `max_task_duration_secs = 90 * 60`, and a
  consent-warning string that names the repo-storage risk AND the
  cursor.com/dashboard/integrations revocation URL (tested).
- **Cursor adapter** (`crates/cue-agent-bridge/src/cloud/cursor.rs`).
  Pure functions for every doc-published shape:
  `build_create_agent_body`, `create_agent_url`, `get_run_url`,
  `stream_run_url`, `probe_url`, `parse_create_agent_response`,
  `parse_run_response`, `parse_run_status`, `parse_sse_event`,
  `parse_task_response_for_dispatch`, `render_acknowledgement`,
  `parse_error_body`. 38 cursor-specific tests assert the exact JSON body
  shape verbatim against the docs example (with a guard that the optional
  fields stay omitted in v1), the URL builders, the SSE event-to-AnswerChunk
  mapping for every documented event type (`status`, `assistant`,
  `thinking`, `tool_call`, `interaction_update`, `heartbeat`, `result`,
  `error`, `done`), and the run-status normalization (including the casing
  defense and the `EXPIRED → Failed` collapse).
- **End-to-end wiremock round-trip** (`cursor::tests::end_to_end_create_agent_against_wiremock`,
  `…_403_free_plan_surfaces_documented_message`). Exercises the FULL adapter
  path against a mock server: build request → apply Bearer auth → send →
  parse response → render acknowledgement. Verifies the auth header is
  exactly `Bearer crsr_test_key`, the body matches the docs JSON
  field-by-field, the `x-cursor-request-id` correlation header is detected,
  and a 403 returns the documented error envelope intact.
- **Wired to the daemon's answer ladder** (`crates/cue-daemon/src/app.rs`,
  `drive_answer_attempt`). The ladder now branches on
  `cue_agent_bridge::cloud::is_cloud_kind(&kind)` before calling `drive`
  — cloud kinds go through `cloud::drive_cloud`, local kinds through the
  existing CLI ladder. No vendor name appears at the daemon site; the
  routing is purely data-driven off the cloud registry.
- **Cloud dispatcher** (`crates/cue-agent-bridge/src/cloud/drive.rs`).
  Extended a `drive_cloud` function authored by the parallel Copilot agent
  with a `CursorCloud` arm: builds the create-agent body via my adapter,
  validates the repo URL is present (honest error if absent), fires through
  the shared transport, parses the response with
  `cursor::parse_task_response_for_dispatch`, emits Started + Delta + Done
  acknowledgement. The repo-shape divergence between Copilot (`owner+repo`)
  and Cursor (`repo_url`) is handled per-arm — each vendor validates its own
  shape.
- `cargo fmt`, `cargo clippy -p cue-agent-bridge -p cue-daemon
  --all-targets -- -D warnings` clean.
  `cargo test -p cue-agent-bridge`: **313 lib tests + 9 integration tests
  pass**. `cargo test -p cue-daemon`: **218 pass + 2 ignored** (pre-existing
  ignores).

### NEEDS-LIVE-VERIFY (no Cursor Pro account here)
- That a real `POST /v1/agents` with a `crsr_…` key returns the documented
  201 body shape verbatim. The adapter parses what the docs publish; a field
  rename on Cursor's side would surface as a parse miss. (The wiremock test
  asserts our REQUEST is correct; the response shape is what the docs say.)
- **SSE streaming is NOT wired end-to-end.** The dispatcher currently kicks
  off the task, emits one acknowledgement Delta with the run id + dashboard
  URL, and Done — task-shaped, not stream-shaped. The SSE event parser
  (`parse_sse_event`) is implemented and unit-tested for every documented
  event type; what's missing is the long-lived SSE consumer + reconnect
  logic with `Last-Event-ID` resume. Adding it requires a live Cursor run
  to validate the event ordering and the retention window.
- The 429 body shape (we treat the documented `{"error","message"}` envelope
  as authoritative; the docs note "no custom rate limit headers" so we fall
  back to a fixed backoff — not yet implemented at the dispatcher level).
- That `crsr_` is the real prefix (not just an SDK-side string-check
  artefact). The format claim came from `cursor.com/docs/api`; not directly
  cross-checked against a live key.
- That `composer-2` is a stable model id. If deprecated mid-life, the user
  gets a 400 with a useful message and `parse_error_body` surfaces it. The
  default would need a config knob to switch.
- The repo-picker piece. The dispatcher currently requires the daemon to
  supply `repo_url` on `CloudTaskInputs`; Bluey doesn't yet have a UI for
  this. Calling `drive_cloud(CursorCloud, …)` from the daemon today returns
  a terminal Error chunk with "this vendor is task-shaped and needs an
  explicit repo URL." The daemon's overlay surfaces this as the standard
  "agent not ready" guidance card — honest, not silent.

### Open product decisions (flagged, not blocking)
- **Cursor cloud is a task-shaped agent, not a turn-shaped one.** A `POST /v1/
  agents` kicks off an async run that produces a *PR*, not a chat reply. That
  collides head-on with Bluey’s "answer the meeting question live" UX — see
  §"Session/task semantics" below. The current wiring exposes Cursor Cloud as
  a route but with an UP-FRONT GUIDANCE CARD: "Cursor Cloud answers by opening
  a PR, which takes minutes. Want to dispatch this and link the PR when it
  lands, or use a local agent for a live answer?" No silent dispatch.
- **Default Cloud Agents store user code on Cursor’s servers.** This conflicts
  with Bluey’s "your data never leaves your machine" promise unless the user
  opts in. The registry row carries `corporate_warning: true`; the UI must
  surface it before any first dispatch.

### Breaks of the data promise (must surface to corporate users)
- Using Cursor Cloud sends the prompt **and** repo access to Cursor’s servers.
  Bluey’s "your meeting data never goes to Bluey’s AI" promise is preserved
  (Bluey is only a relay), **but** the data does go to a third party (Cursor)
  the user authenticated against. The Disclosure UI must say so before
  dispatch — not after. See `billing_model = ApiCredits` + a new
  `data_surface = ThirdPartyCloud` marker on the registry row.

---

## Auth model

- **Two interchangeable schemes** on every cloud endpoint
  ([cursor.com/docs/api](https://cursor.com/docs/api)):
  - **Basic auth**: `curl -u $CURSOR_API_KEY:` (the key as username, empty
    password) — i.e. `Authorization: Basic <base64(key:)>`.
  - **Bearer**: `Authorization: Bearer $CURSOR_API_KEY`. The Cloud Agents page
    explicitly lists Bearer as supported
    ([endpoints page](https://cursor.com/docs/cloud-agent/api/endpoints)).
  - Our adapter uses **Bearer**. It’s simpler to construct and the token is
    never reflected in the URL.
- **Where the user gets it**: Dashboard → Integrations
  (https://cursor.com/dashboard/integrations) — *originally* documented as
  Dashboard → Cloud Agents → User API Keys but moved (Cursor forum, 2026).
  We tell the user **both** locations in the onboarding copy.
- **Format**: `crsr_` followed by 64 hex characters (cursor.com/docs/api).
  *Not* SDK-verified at runtime — we treat any non-empty string as a
  potentially-valid key and let the server reject it.
- **Two key types**:
  - **User API key** — what an individual gets from the dashboard. What
    Bluey’s "Connect Cursor Cloud" flow consumes.
  - **Service account API key** — team admins; can mint short-lived
    *user-scoped worker tokens* (`POST /v1/sub-tokens`, 1-hour expiry). NOT
    in scope for Bluey v1. Logged as `future-work` only.
- **Free plan restriction**: "Free Plan API Keys do not allow Background Agent
  API usage (only headless CLI)." We surface this as an explicit error message
  when a 403 with that signal comes back; we don’t pre-flight by plan.
- **Revocation**: only via Cursor’s dashboard; **no documented programmatic
  delete-key endpoint**. So Bluey’s "Disconnect Cursor" button can only delete
  the local keychain entry — it cannot revoke the key. The UI must say so.

## Endpoints

Base URL: `https://api.cursor.com`. All return JSON. Errors use the envelope
`{"error":"<HTTP phrase>","message":"<text>"}`
([cursor.com/docs/api](https://cursor.com/docs/api)).

Source: [cursor.com/docs/cloud-agent/api/endpoints](https://cursor.com/docs/cloud-agent/api/endpoints).

| # | Method | Path | What it does | Used by Bluey v1? |
|---|--------|------|--------------|-------------------|
| 1 | POST | `/v1/agents` | Create an agent (kick off the first run) | **Yes** — dispatch |
| 2 | GET | `/v1/agents` | List agents (paginated, `nextCursor`) | No |
| 3 | GET | `/v1/agents/{id}` | Get one agent | Optional (status poll) |
| 4 | POST | `/v1/agents/{id}/runs` | Follow-up run on an existing agent | Future (resume) |
| 5 | GET | `/v1/agents/{id}/runs` | List runs | No |
| 6 | GET | `/v1/agents/{id}/runs/{runId}` | Get one run (carries `result`, `git`, `durationMs`) | **Yes** — poll/result fetch |
| 7 | GET | `/v1/agents/{id}/runs/{runId}/stream` | SSE stream of events | **Yes** — real-time |
| 8 | POST | `/v1/agents/{id}/runs/{runId}/cancel` | Cancel a run | **Yes** — on stream drop |
| 9 | GET | `/v1/agents/{id}/artifacts` | List artifacts | No |
| 10 | GET | `/v1/agents/{id}/artifacts/download` | Pre-signed S3 URL | No |
| 11 | POST | `/v1/agents/{id}/archive` | Archive agent | No |
| 12 | POST | `/v1/agents/{id}/unarchive` | Unarchive | No |
| 13 | DELETE | `/v1/agents/{id}` | Delete (permanent) | No |
| 14 | POST | `/v1/sub-tokens` | Mint user-scoped worker token (1h) | No (service-account only) |
| 15–18 | – | `/v0/private-workers/*` | Enterprise worker fleet | No |
| 19 | GET | `/v1/me` | API-key info (probe / health check) | **Yes** — connection-probe |
| 20 | GET | `/v1/models` | List available models | Optional |
| 21 | GET | `/v1/repositories` | List GH repos visible to the key | No (separately rate-limited 1/min) |

### Create-agent request body, exact shape we send

```json
{
  "prompt":  { "text": "<the bounded prompt>", "images": [] },
  "model":   { "id": "composer-2" },
  "repos":   [ { "url": "<gh repo URL>", "startingRef": "main" } ],
  "workOnCurrentBranch": false,
  "autoCreatePR": true,
  "skipReviewerRequest": false
}
```

We omit `name`, `envVars`, `mcpServers`, `customSubagents`, `agentId`, and the
`env` block in v1 — every one is doc-optional. The repo URL comes from the
daemon’s current meeting context (the agent registry already knows the cwd /
repo). If we don’t have a repo URL, we do NOT dispatch (Cursor Cloud requires
one — see the docs’ "multi-repo workflows" framing). We surface a guidance
card instead.

### Create-agent response, the fields we read

```json
{
  "agent": {
    "id":            "bc-…",
    "status":        "ACTIVE",
    "url":           "https://cursor.com/agents/bc-…",
    "latestRunId":   "run-…"
  },
  "run": {
    "id":            "run-…",
    "agentId":       "bc-…",
    "status":        "CREATING",
    "createdAt":     "…",
    "updatedAt":     "…"
  }
}
```

We hold both ids in the daemon’s pending-run map and start streaming
`/v1/agents/{id}/runs/{runId}/stream`. The session id we surface back to Bluey
is the **runId** (the unit the user is waiting on); the agentId is for resume.

### SSE event types we parse

From the docs verbatim: `status`, `assistant`, `thinking`, `tool_call`,
`interaction_update`, `heartbeat`, `result`, `error`, `done`.

We map them to Bluey’s `AnswerChunk` enum like this:

| Cursor event | Bluey chunk |
|--------------|-------------|
| first `status` carrying `runId` | `Started { session_id: Some(runId) }` |
| `assistant` with `text` | `Delta(text)` |
| `thinking` | dropped (we don’t surface chain-of-thought) |
| `tool_call` | dropped in v1 (telemetry only; logged at debug) |
| `heartbeat` | dropped (used only to keep the conn alive) |
| `result` | `Delta(text)` if present, then `Done { cost_usd: None }` |
| `error` | `Error(message)` |
| `done` | nothing (stream just ends after) |

`durationMs` is tucked into the `result` event but we have no per-call USD
cost (Cursor bills by tokens/credits, not USD), so `Done` carries `None`.
Surfacing tokens in v1 would be cosmetic; leave for later.

### Resume semantics

The SSE endpoint takes a `Last-Event-ID` header (the docs’ "Resume" section).
If the daemon’s SSE connection drops and reconnects within the retention
window, we resend the last event id and the server replays from there. Outside
the window we get a `410 stream_expired` and fall back to `GET /runs/{runId}`
to pull the terminal `result`.

## Billing model

- Cloud Agents are **billed at API pricing for the selected model**, against a
  user-set **spend limit** (cursor.com/docs/cloud-agent intro).
- Requires a **paid Cursor plan**. Free / Hobby keys *can* call the headless
  CLI but cannot create cloud agents — we surface this as a typed error.
- Pro adds a small monthly **$20 of API agent usage** allowance; Pro+/Ultra
  are tier-priced. Source: cursor.com/docs/account/pricing.
- **Registry row marker**: `billing_model = ApiCredits` (NOT `Subscription` —
  the subscription gates *access* but does not cover usage).

## Rate limits

- **Per-team per-minute** enforcement, NOT per-API-key
  (cursor.com/docs/api). Range "20–100 requests/minute" depending on
  endpoint, single outlier `/teams/user-spend-limit` at 250/min.
- `GET /v1/repositories` is the only doc-explicit limit on Cloud Agents:
  **1 / user / minute, 30 / user / hour**. We don’t call it in v1.
- **429 body**: the standard error envelope above, message
  `"Rate limit exceeded. Please try again later."` — **no `Retry-After`
  header documented** (Cursor docs explicitly say "no custom rate limit
  headers documented"). We use a fixed exponential backoff capped at 30 s.
- A `429` on `POST /v1/agents` surfaces as the same honest "agent not ready,
  try again in a moment" guidance card the local-CLI path uses.

## Session / task semantics

- **Task-shaped, async.** A `POST /v1/agents` kicks off a run that lives in a
  cloud VM, edits files, produces a PR. Runtime is **minutes** for any
  non-trivial change; the docs imply `composer-2` is the typical model.
- **Statuses** (from endpoint docs):
  - Run: `CREATING → RUNNING → FINISHED | ERROR | CANCELLED | EXPIRED`
  - Agent: `ACTIVE | ARCHIVED`
- **Streaming**: SSE via `/runs/{runId}/stream` (see above).
- **Concurrency**: docs say "run as many agents as you want in parallel" but
  the team-level rate limit caps how fast you can *create* them.

## Error taxonomy

- Standard envelope: `{"error":"<HTTP phrase>","message":"<text>"}`.
- Documented status codes seen on cloud-agent endpoints:
  - `400 Bad Request` (invalid params)
  - `401 Unauthorized` (bad/expired key)
  - `403 Forbidden` (free-plan key trying cloud agents, etc.)
  - `404 Not Found` (unknown agent/run id)
  - `409 Conflict` — three documented sub-cases:
    - `agent_id_conflict` (POST /v1/agents with a reused `agentId`)
    - `agent_busy` (POST /runs while an existing run isn’t terminal)
    - `run_not_cancellable` (cancel on a terminal run)
  - `410 Gone` — `stream_expired` (SSE resume past retention)
  - `429 Too Many Requests`
  - `500 Internal Server Error`
- We never panic on a status we don’t recognize — anything unrecognized
  becomes an honest `AnswerChunk::Error("Cursor Cloud returned <status>: <msg
  || body>")`.

## Revocation

- **No documented endpoint to delete an API key.** Only the dashboard.
- Bluey’s "Disconnect Cursor Cloud" button can only:
  1. Clear the keychain entry (`bluey_cloud_cursor / api_key`).
  2. Tell the user, verbatim, that the key itself is still valid on Cursor’s
     servers and must be revoked at https://cursor.com/dashboard/integrations.

## MCP / scoping

- Cursor Cloud agents accept an `mcpServers` array on `POST /v1/agents` and
  `POST /v1/agents/{id}/runs` (HTTP & stdio, OAuth supported).
- Bluey **does not forward Bluey-internal MCP servers** in v1 — the cloud
  agent inherits the user’s team MCP config, not ours. Sending Bluey-side
  MCPs would re-expose Bluey’s tool surface to a third party with a different
  trust model.

## Webhooks / streaming

- **SSE** is the live path. See "SSE event types" above.
- **Webhooks** (cursor.com/docs/background-agent/api/webhooks) deliver a
  single event type today: `statusChange` (fired on `ERROR` or `FINISHED`).
  Signed with `X-Webhook-Signature: sha256=<hex>` over the raw body, HMAC-
  SHA256 with the user’s configured secret.
- **Bluey v1 does not host webhooks** (we have no public endpoint). We rely
  on SSE + polling. Webhook support is `future-work` once the daemon has a
  public-listener story.

## Data residency / compliance

- **Cloud Agents are the *only* Cursor feature that stores customer code on
  Cursor’s servers** (cursor.com/docs/enterprise/privacy-and-data-
  governance). Privacy mode does NOT change this — Cursor itself flags Cloud
  Agents as the exception.
- ZDR agreements exist with the model providers (OpenAI/Anthropic/Vertex/
  xAI) but **not** for the persistent VM/repo state.
- SOC 2 Type II, AES-256 at rest, TLS 1.2+ in transit, CMEK on Enterprise.
- For Bluey’s corporate users this is the headline risk: dispatching to
  Cursor Cloud means a non-Bluey third party now holds repo code. The
  `corporate_warning: true` flag on the registry row drives a pre-dispatch
  confirmation.

## Edge cases the docs warn about

- "Cloud VMs don’t have access to your local home directory" — user-level
  hooks in `~/.cursor/hooks.json` are NOT applied. This affects what the
  cloud agent will and won’t do — Bluey can’t silently fix it.
- **Long-session crashes**: cloud agents can crash past ~500k tokens (forum
  reports). We don’t paginate prompts; we just surface the agent’s error.
- **`.env.local` snapshot risk**: secrets in `.env.local` may be captured into
  a snapshot. Cursor recommends the Secrets tab in Settings instead. Bluey
  never reads or sends `.env*`.

## OPEN QUESTIONS (assumptions + what changes if wrong)

1. **Assumption**: Bearer auth (`Authorization: Bearer <key>`) is identical
   in result to Basic (`-u <key>:`). If wrong (e.g. some endpoints accept only
   Basic), the `BearerAuth` impl needs to fall back to `BasicAuthApiKey`. The
   transport supports both — switching is a one-line change to the registry
   row’s `auth` field.
2. **Assumption**: `composer-2` is a stable default model id. The docs’
   example uses it; `/v1/models` is the source of truth. If `composer-2` is
   deprecated mid-life, the user gets a 400 with a useful message and we
   surface it. NEEDS-LIVE-VERIFY.
3. **Assumption**: a fresh `POST /v1/agents` *requires* `repos` (the docs
   example always includes it). If a no-repo call is actually accepted (e.g.
   for a scratch agent), we are over-blocking. Today we refuse to dispatch
   without a repo URL — that’s the safer default.
4. **Assumption**: the SSE `Last-Event-ID` resume is exactly the
   server-emitted `id:` line (e.g. `1713033000000-0`). If it’s actually a
   monotonic counter, our reconnect path still works for the *first*
   reconnect (we forward whatever id we last saw verbatim).
5. **Assumption**: the SSE retention window is meaningful (multi-minute). If
   it’s seconds, the polling fallback fires almost immediately and the user
   never sees streamed deltas — only the final `result`. Acceptable
   degradation, not a correctness break.
6. **Assumption**: `/v1/me` is the cheapest probe for "is this key valid".
   If `/v1/me` has its own scope check we don’t see, we’ll have a false
   negative on probe even with a working create-agent key. We accept this —
   the dispatch itself is the source of truth.
7. **Assumption**: Cursor’s 429 envelope is the same `{"error","message"}`
   shape. The docs say so; we’ve never seen a 429 in person.
8. **Assumption**: A user with **no GitHub repos linked to Cursor** still
   has a non-empty `/v1/repositories` list as long as they’ve granted the
   GitHub app once. If the list is empty for keys without any GH connection,
   we won’t be able to validate the repo URL up front — we’ll fall back to
   "let the server reject it."

---

## PROPOSED SPEC — how Cursor Cloud slots into the registry

### New `AgentKind` variant

Added *after* `Antigravity` (coordination convention with the parallel
agents):

```rust
pub enum AgentKind {
    // …
    Antigravity,
    /// Cursor Cloud agents (formerly Background Agents).
    /// API-driven; runs in Cursor's VMs; emits a PR, not a chat reply.
    CursorCloud,
    Copilot,
    // …
}
```

`KindTag::CursorCloud` mirrors it; `to_agent_kind` / `from_agent_kind` round-
trip; `display_name_for` returns "Cursor Cloud" off the registry row.

### New `BillingModel` enum (registry-level)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingModel {
    /// LOCAL agent — billed (if at all) wholly outside Bluey's awareness.
    Local,
    /// CLOUD vendor — counts against the user's separate API credit balance
    /// at that vendor (e.g. Cursor Cloud).
    ApiCredits,
    /// CLOUD vendor — bundled into a paid subscription (e.g. Copilot Pro,
    /// Claude Pro). The user pays a fixed monthly; calls don't meter.
    Subscription,
    /// BYOT — the user supplies a third-party LLM key the vendor relays.
    Byot,
}
```

Every registry row gets a `billing_model` field. Existing local rows are
`Local`. The Cursor row is `ApiCredits`. The other three cloud agents
(parallel) fill in their own.

### New `data_surface` marker

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSurface {
    /// All processing local; no third party sees the prompt or repo.
    Local,
    /// Vendor's first-party cloud (the user authenticated against it).
    /// Bluey is a relay; the vendor's own privacy contract is what holds.
    FirstPartyCloud,
    /// Vendor's cloud that further forwards to a different LLM provider
    /// (e.g. Cursor → OpenAI/Anthropic).
    ThirdPartyCloud,
}
```

Cursor Cloud is `ThirdPartyCloud`. The UI prompt before first dispatch reads
verbatim off this field — no hardcoded copy per vendor.

### Why no `Transport` variant

`connectors::Transport` exists for **MCP connectors** (the LLM-side tools
those connectors front), not for how Bluey itself talks to the agent. The
agent’s own transport lives in two places already:
1. `registry::AgentEntry::drive_command` for LOCAL CLI agents.
2. *(new)* a `cloud` field on `AgentEntry` for cloud agents.

So we add a `cloud: Option<CloudSpec>` field to `AgentEntry` rather than
distorting an existing enum. Local rows leave it `None`. Cursor’s row sets
it.

```rust
#[derive(Debug, Clone, Copy)]
pub struct CloudSpec {
    pub vendor:        CloudVendor,        // CursorCloud, CopilotCloud, …
    pub base_url:      &'static str,       // "https://api.cursor.com"
    pub auth:          CloudAuth,          // BearerOrBasicApiKey
    pub keychain_key:  &'static str,       // "api_key"
    pub probe_path:    &'static str,       // "/v1/me"
    pub create_path:   &'static str,       // "/v1/agents"
    pub data_surface:  DataSurface,
    pub corporate_warning: bool,
}
```

Every per-vendor request/response *shape* lives in the vendor adapter
(`cloud/cursor.rs`); the **routes** live as data on the registry row. Adding
the next cloud vendor = adding a row + a 30-50 line adapter.

### Irreducible per-vendor code (~50 lines)

Lives in `crates/cue-agent-bridge/src/cloud/cursor.rs`:
- `build_create_agent_body(prompt, repo) -> serde_json::Value`
- `parse_create_agent_response(body) -> CursorRunHandle { agent_id, run_id }`
- `parse_sse_event(event_name, data) -> Vec<AnswerChunk>`
- `parse_run_response(body) -> RunPollResult { status, text }`

That’s it. Everything else (auth header construction, HTTP firing, keychain
read/write, audit logging) is in the **shared** `cloud/transport.rs`,
`cloud/keychain.rs`, `cloud/audit.rs` — written to be re-used by the three
parallel vendors.
