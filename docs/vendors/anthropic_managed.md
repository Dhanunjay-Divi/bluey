# Anthropic Claude Managed Agents — Vendor Dossier

Author: Bluey agent-bridge integration loop. Last edited: 2026-06-05.

---

## 0. Status report (read this first)

This is the executive summary the product owner needs. Detail is in §1 onward.

### Done

- **Vendor dossier (this file)** — full auth/endpoint/SSE/policy story with
  verbatim quotes from Anthropic's docs, the OpenClaw-ban background from
  independent sources (The Register, VentureBeat, MindStudio, WinBuzzer), error
  / rate-limit shapes, and an explicit OPEN-QUESTIONS section (§11).
- **Kind tagging** — `AgentKind::AnthropicCloud` (in
  `crates/cue-agent-bridge/src/lib.rs`) and `KindTag::AnthropicCloud` (in
  `crates/cue-agent-bridge/src/registry.rs`), with bidirectional maps to/from
  `AgentKind`.
- **Cloud registry row** — `crates/cue-agent-bridge/src/cloud/anthropic.rs`
  replaces the placeholder stub the parallel Copilot agent shipped with the
  real `ENTRY: &'static CloudAgentEntry`, carrying:
  - `kind_tag: KindTag::AnthropicCloud`
  - `display_name: "Claude Agent (Cloud)"` — Anthropic branding-compliant
    (the strings *"Claude Code"* and *"Claude Cowork"* are forbidden for
    partners per the official branding guideline)
  - `billing_model: BillingModel::ApiCredits` — the data field the BYOT
    disclosure UI reads off the row
  - `consent_warning: BYOT_CONSENT_TEXT` — verbatim disclosure copy covering
    the Console-API-key requirement, the *"not your Pro/Max subscription"*
    distinction, ZDR / HIPAA ineligibility, and the OS-keychain promise
  - `task_shaped: false` (Anthropic's surface is session-shaped, not
    task-shaped like Codex Cloud / Copilot Cloud)
- **Per-vendor adapter (`cloud/anthropic.rs`)** — every irreducible bit:
  - `validate_api_key()` — REJECTS `sk-ant-oat01-*` subscription OAuth tokens
    with a typed `KeyValidationError::SubscriptionOAuth` carrying the policy
    citation
  - `CreateAgentBody` / `CreateEnvironmentBody` / `CreateSessionBody` /
    `SendEventBody` (with `UserEvent::Message` / `UserEvent::Interrupt` and
    `ContentBlock::Text` variants) — request body builders whose serialized
    JSON matches the Anthropic quickstart's verbatim shapes
  - `create_agent_request()` / `create_environment_request()` /
    `create_session_request()` / `send_event_request()` /
    `stream_events_request()` / `delete_session_request()` — HttpRequest
    constructors that the shared HTTPS dispatcher fires
  - `AnthropicEvent` (typed) + `parse_sse_event()` + `parse_sse_chunk()` —
    SSE event-stream parser covering `agent.message`, `agent.thinking`,
    `agent.tool_use`, `session.status_idle`, `session.status_terminated`,
    `session.error` (with typed error + `retry_status`), plus `Unknown` for
    forward compatibility
  - `parse_error_response()` — maps the documented Anthropic error envelope
    (`{ "type": "error", "error": { "type", "message" }, "request_id" }`)
    onto a typed `AnthropicError::Api`
  - `TRANSPORT: CloudTransport::Https {...}` — pure data row carrying the
    base URL, the `anthropic-version` + `anthropic-beta` static headers, the
    `x-api-key` auth, and every endpoint path
- **Shared cloud primitives** — `transport.rs`, `keychain.rs`, `audit.rs`,
  `registry.rs` were authored by the parallel Cursor / Copilot / Codex Cloud
  agents in the same batch. The Anthropic adapter conforms to their existing
  contracts (`CloudHttpsTransport::send`, `VendorCredentialStore`,
  `AuditEvent`, `BillingModel`, `CloudAgentEntry`) without changing them —
  the "extend, don't overwrite" coordination rule held.
- **Daemon wiring**:
  - `agent_display_name(&AgentKind)` in `crates/cue-daemon/src/app.rs`
    delegated to `cue_agent_bridge::registry::display_name_for` so cloud rows
    (including this one) get their UI label from the registry data, not from
    a hardcoded match arm.
  - `handle_agent_attach()` now consults `needs_byot_disclosure()`: if the
    kind maps to a cloud row whose `billing_model` is `ApiCredits` and the
    vendor isn't yet in `settings.accepted_byot_vendors`, the daemon emits
    `OverlayCommand::PushBillingDisclosure { vendor_short, vendor_display_name,
    billing_model, disclosure, pending_kind, pending_session_id }` and
    REFUSES to persist the attach until the overlay returns
    `OverlayEvent::BillingDisclosureResponded { ..., accepted: true }`. The
    disclosure modal is unbypassable by design — the registry row's
    `consent_warning` is rendered verbatim.
  - `handle_billing_disclosure_response()` adds the vendor to
    `accepted_byot_vendors`, persists, then re-runs the original attach (the
    gate now passes). Declining clears the pending attach with a guidance
    card.
- **IPC plumbing**:
  - New `OverlayCommand::PushBillingDisclosure {...}` and
    `OverlayEvent::BillingDisclosureResponded {...}` in
    `crates/cue-core/src/overlay.rs`. Both serialize to the documented
    `snake_case` wire shape and round-trip cleanly.
  - New `accepted_byot_vendors: Vec<String>` settings field in
    `crates/cue-core/src/config.rs` (default empty; persisted across daemon
    restarts so the user is asked once per vendor).
- **Tests added (all green)**:
  - `cue_agent_bridge::cloud::anthropic::tests` — 30 tests covering key
    validation (Console accept, OAuth reject, empty reject), every
    request-body shape, every HttpRequest constructor (carrying the beta
    header as data), the SSE typed-event parser (every event type + the
    chunk walker + the garbage-skip), the error body parser (documented
    shape + OAuth 401 + malformed-body fallback), branding compliance, BYOT
    consent text presence, and the wiremock-backed live request shape (POST
    `/v1/sessions` with `x-api-key`, `anthropic-version`, `anthropic-beta`,
    plus the 401 path)
  - `cue_daemon::app::tests` — 4 tests pinning the BYOT contract from the
    daemon's side: the Anthropic row is BYOT in the cloud registry,
    `OverlayCommand::PushBillingDisclosure` serializes with the documented
    fields, `OverlayEvent::BillingDisclosureResponded` round-trips, and the
    BYOT gate never fires for local CLI agents.
- **Build is green**:
  - `cargo build -p cue-agent-bridge -p cue-daemon` — clean
  - `cargo test -p cue-agent-bridge --lib` — 309 passed, 0 failed
  - `cargo test -p cue-core --lib` — 95 passed, 0 failed
  - `cargo test -p cue-daemon --lib` — 218 passed, 2 ignored (pre-existing),
    0 failed
  - `cargo fmt --all` — clean
  - `cargo clippy -p cue-agent-bridge -p cue-daemon -p cue-core
    --all-targets -- -D warnings` — clean

### NEEDS-LIVE-VERIFY

(All require an Anthropic Console API key on an account with the
`managed-agents-2026-04-01` beta enabled — flag in the Console / waitlist for
MCP tunnels and dreaming features.)

- 🟡 End-to-end create-agent → create-environment → create-session →
  send-event → stream lifecycle. The request bytes are unit-tested against
  the documented shape (including a wiremock round-trip that confirms the
  HTTP transport sends the expected headers, method, body, and surfaces the
  `request-id` header), but the live server's exact RESPONSE field set
  (response IDs, status enums, model usage envelope) has not been observed.
- 🟡 SSE reconnect / "events buffered until stream attaches" semantics. The
  chunk parser handles `event:` + `data:` line pairs robustly (including
  garbage-line skip), but the precise retry / disconnect / `Last-Event-ID`
  behavior on the wire has not been exercised against the real server.
- 🟡 Real 401/403/429 bodies. The error parser is wiremock-tested against
  the documented Anthropic envelope shape (and against the malformed-body
  fallback), but the exact `request_id` propagation through the
  `session.error` SSE event has not been observed on a live stream.
- 🟡 Sandbox cold-start latency, max concurrent sessions per key, and
  per-session billing line items in the Console.
- 🟡 The `agent_toolset_20260401` tool catalog as it appears in
  `agent.tool_use` events (parser handles it generically, but specific tool
  names are not enumerated).
- 🟡 The per-Bluey-install agent + environment provisioning flow (the
  daemon's "create one named `bluey-cowork` agent + one named `bluey-env`
  environment on first attach" path is wired but not yet implemented inside
  `cloud/anthropic.rs` — adapter exposes the request builders + transport;
  the daemon-side caller that fires them is the next slice).

### Blocked / needs a product decision

- ⚠️ **Agent + Environment lifecycle ownership.** Anthropic models the
  *agent* (model + prompt + tool config) and the *environment* (sandbox shape)
  as user-created resources that exist before a session. Bluey could (a)
  create one Bluey-managed agent + environment per Bluey install, or (b)
  require the user to pre-create them in the Console and paste the IDs. Default
  in this branch is (a) with a deterministic name ("bluey-cowork"), idempotent
  on attach, so the user only needs to paste their API key. Confirm before
  shipping.
- ⚠️ **Default model.** The registry row hardcodes `claude-opus-4-8` (the
  quickstart's recommended model). Worth a config knob in user settings before
  shipping so cost-sensitive users can flip to a Sonnet/Haiku tier.
- ⚠️ **Branding.** Anthropic's reference page explicitly disallows the strings
  "Claude Code", "Claude Code Agent", "Claude Cowork", "Claude Cowork Agent",
  and Claude Code-styled ASCII art. The UI must show this row as
  **"Claude Agent (Cloud)"** (preferred per the branding guideline:
  *"Claude Agent" (preferred for dropdown menus)*). The registry row's
  `display_name` is set to `"Claude Agent (Cloud)"`. Confirm this is the final
  string before any marketing copy ships.
- ⚠️ **Data-residency / retention copy.** Managed Agents is explicitly NOT
  eligible for Zero Data Retention or HIPAA BAA. Disclosure copy includes a
  line about this. Confirm with legal.

### Policy / billing risks (read carefully)

- 🔴 **BYOT is mandatory.** The user's Anthropic Console API key is the ONLY
  legal credential here. Per Anthropic, *"using OAuth tokens obtained through
  Claude Free, Pro, or Max accounts in any other product, tool, or service —
  including the Agent SDK — is not permitted and constitutes a violation of
  the Consumer Terms of Service."* The code enforces this two ways:
  1. The `cloud::keychain` module stores `sk-ant-api03-*` keys only and the
     anthropic adapter REJECTS any key that begins `sk-ant-oat01-` at
     attach-time with a clear error, so a user who paste-fumbles their CLI
     OAuth token gets a UX-level "wrong key type" message instead of a 401
     storm hours later.
  2. The audit log captures every cloud call (vendor, endpoint, HTTP status,
     request id) and NEVER captures the token; the test
     `audit_line_never_contains_token` enforces this.
- 🔴 **Billing surprise risk.** Managed Agents bills per token to the user's
  Console API key — NOT to their Claude Pro/Max subscription. A Sonnet 4.x
  Tier 1 user has 30k input + 8k output tokens/minute and a $500/month spend
  cap; a long-running agent loop will blow through that. The
  `PushBillingDisclosure` modal explicitly enumerates this. The UI MUST show
  it before the first session is created; the daemon refuses to mark the
  agent `attached` until the modal is acknowledged.
- 🔴 **Branding compliance.** Independent of the BYOT story, Anthropic's
  branding rules ban the "Claude Code" / "Cowork" strings for partners.
  The registry row uses `"Claude Agent (Cloud)"` everywhere; do not change
  the display string without re-reading the branding section.

---

## 1. The product

> *"Claude Managed Agents provides the harness and infrastructure for running
> Claude as an autonomous agent. Instead of building your own agent loop, tool
> execution, and runtime, you get a fully managed environment where Claude can
> read files, run commands, browse the web, and execute code securely."*
> — [Managed Agents overview](https://platform.claude.com/docs/en/managed-agents/overview)

Four primitives:

| Concept       | Description                                                                                     |
|---------------|-------------------------------------------------------------------------------------------------|
| **Agent**     | Model + system prompt + tools + MCP servers + skills (versioned, created once, referenced by ID). |
| **Environment** | Sandbox shape: Anthropic-managed cloud sandbox OR self-hosted sandbox on user infra.          |
| **Session**   | A running agent instance inside an environment, owning persistent FS + conversation history.    |
| **Events**    | Bidirectional messages: user events (`user.message`, `user.interrupt`, `user.tool_confirmation`, …) and server events (`agent.message`, `agent.tool_use`, `session.status_idle`, `session.error`, …). |

Bluey's mapping: a "session" maps to one Bluey "agent answer" interaction. The
agent + environment are Bluey-install-scoped (one per install, idempotently
provisioned on first attach), so the user only pastes their API key once.

---

## 2. Auth model

### Console API key, period

> *"To get started, you need: 1. A [Claude API key](/settings/keys) 2. The
> `managed-agents-2026-04-01` beta header on all requests 3. Access to Claude
> Managed Agents (enabled by default for all API accounts)."*
> — [Overview, "Beta access"](https://platform.claude.com/docs/en/managed-agents/overview)

The key is the same `sk-ant-api03-*` Console API key used everywhere else on
the platform. Header name: `x-api-key`. Format from the quickstart's curl
example:

```http
x-api-key: $ANTHROPIC_API_KEY
anthropic-version: 2023-06-01
anthropic-beta: managed-agents-2026-04-01
content-type: application/json
```

— [Quickstart, "Create an agent"](https://platform.claude.com/docs/en/managed-agents/quickstart)

### Subscription OAuth tokens are REJECTED

This is the policy bit Bluey has to enforce.

- Format of the rejected token: `sk-ant-oat01-…` (the prefix Claude Code's
  device-code OAuth flow returns).
- Where rejected: the Anthropic Messages API and the Managed Agents API both
  return `401 authentication_error` with the message `"OAuth authentication
  is currently not supported."` when an OAuth token is presented as
  `x-api-key`.
- Citations (independent sources):
  - The Register, 20 Feb 2026: ["Anthropic clarifies ban on third-party tool
    access to Claude"](https://www.theregister.com/2026/02/20/anthropic_clarifies_ban_third_party_claude_access/)
  - VentureBeat: ["Anthropic cracks down on unauthorized Claude usage by
    third-party harnesses and rivals"](https://venturebeat.com/technology/anthropic-cracks-down-on-unauthorized-claude-usage-by-third-party-harnesses)
  - MindStudio: ["What Is the OpenClaw Ban?"](https://www.mindstudio.ai/blog/anthropic-openclaw-ban-oauth-authentication)
  - WinBuzzer: ["Anthropic Bans Claude Subscription OAuth in Third-Party
    Apps"](https://winbuzzer.com/2026/02/19/anthropic-bans-claude-subscription-oauth-in-third-party-apps-xcxwbn/)
- The verbatim policy clause (quoted in all the above):
  > *"Using OAuth tokens obtained through Claude Free, Pro, or Max accounts
  > in any other product, tool, or service — including the Agent SDK — is not
  > permitted and constitutes a violation of the Consumer Terms of Service."*

Bluey's enforcement (in `cloud/anthropic.rs::validate_api_key`):

- Reject any key beginning with `sk-ant-oat01-` at attach-time with a typed
  error and a UI message: *"That's a Claude subscription OAuth token. Bluey
  needs a Console API key (starts with `sk-ant-api03-`). Generate one at
  platform.claude.com/settings/keys."*
- Audit-log the rejection event (without the key) so support has a record.

### Scoping

Console API keys are **account-wide** (or workspace-wide if the user creates a
workspace-scoped key). There is no per-resource scoping for Managed Agents
sessions specifically. Revocation is account-wide — Console → Settings →
Keys → Revoke. The audit log Bluey writes is the user's only client-side
record of what Bluey did with the key before revocation.

### Rate-limit per key (organization, really)

> *"Managed Agents endpoints are rate-limited per organization: Create
> endpoints (such as agents, sessions, and environments) 300 requests per
> minute. Read endpoints (such as retrieve, list, and stream) 600 requests per
> minute."*
> — [Reference, "Rate limits"](https://platform.claude.com/docs/en/managed-agents/reference#rate-limits)

In addition the underlying model inference inherits the standard tier limits:

| Tier | Sonnet 4.x RPM | Sonnet 4.x ITPM | Sonnet 4.x OTPM | Monthly spend cap |
|------|----------------|-----------------|------------------|-------------------|
| 1    | 50             | 30k             | 8k               | $500              |
| 2    | 1,000          | 450k            | 90k              | $500              |
| 3    | 2,000          | 800k            | 160k             | $1,000            |
| 4    | 4,000          | 2M              | 400k             | $200,000          |

— [API rate limits](https://platform.claude.com/docs/en/api/rate-limits)

Surge / acceleration limits: the docs warn that *"if your organization has a
sharp increase in usage, you might see 429 errors because of acceleration
limits."* Treatment: ramp gradually, respect `retry-after`. The adapter does
honor `retry-after` on 429.

---

## 3. Endpoints

All requests carry `x-api-key`, `anthropic-version: 2023-06-01`,
`anthropic-beta: managed-agents-2026-04-01`, `content-type: application/json`
(except DELETE and streaming GET). Sourced from
[Quickstart](https://platform.claude.com/docs/en/managed-agents/quickstart),
[Sessions](https://platform.claude.com/docs/en/managed-agents/sessions),
[Session operations](https://platform.claude.com/docs/en/managed-agents/session-operations),
[Events and streaming](https://platform.claude.com/docs/en/managed-agents/events-and-streaming).

### Agent

| Method | Path | Notes |
|---|---|---|
| `POST` | `/v1/agents` | Create. Body example below. Response: `{ id, version, ... }`. |
| (other list/get/update CRUD endpoints exist via SDK but the Bluey adapter only needs create.) |

Create request body (verbatim from the quickstart):

```json
{
  "name": "Coding Assistant",
  "model": "claude-opus-4-8",
  "system": "You are a helpful coding assistant. Write clean, well-documented code.",
  "tools": [
    {"type": "agent_toolset_20260401"}
  ]
}
```

Response: `{ "id": "<agent id>", "version": <int>, ... }`. Save the id.

### Environment

| Method | Path | Notes |
|---|---|---|
| `POST` | `/v1/environments` | Create. Cloud is the default. |

Create request body (verbatim):

```json
{
  "name": "quickstart-env",
  "config": {
    "type": "cloud",
    "networking": {"type": "unrestricted"}
  }
}
```

Response: `{ "id": "<env id>", ... }`. Save the id.

### Session

| Method | Path | Notes |
|---|---|---|
| `POST` | `/v1/sessions` | Create. Body: `{ "agent": "<id>", "environment_id": "<id>", "title": "...", "vault_ids": [...] }`. `title` is optional; `vault_ids` is only for MCP server auth. |
| `GET` | `/v1/sessions/{id}` | Retrieve. Returns `{ id, status, ... }`. |
| `GET` | `/v1/sessions?agent_id=<id>` | List, paginated. |
| `POST` | `/v1/sessions/{id}` | Update (replace tools / MCP servers mid-session; session must be `idle`). |
| `POST` | `/v1/sessions/{id}/archive` | Archive (preserve history, no new events). |
| `DELETE` | `/v1/sessions/{id}` | Delete permanently (events + sandbox). Session must NOT be `running`. |

### Events

| Method | Path | Notes |
|---|---|---|
| `POST` | `/v1/sessions/{id}/events` | Send one or more user events. Body shape: `{ "events": [ <event-object>, ... ] }`. |
| `GET` | `/v1/sessions/{id}/stream` | Open the SSE stream. Headers add `Accept: text/event-stream`. |

Events are buffered server-side until the stream attaches — the doc
explicitly says *"the API buffers events until the stream attaches"*.

### Send-event body (verbatim from quickstart)

```json
{
  "events": [
    {
      "type": "user.message",
      "content": [
        {
          "type": "text",
          "text": "Create a Python script that generates the first 20 Fibonacci numbers and saves them to fibonacci.txt"
        }
      ]
    }
  ]
}
```

### Interrupt a running session

Send a `user.interrupt` event via `POST /v1/sessions/{id}/events`. Body shape
follows the same `{ "events": [{ "type": "user.interrupt" }] }` pattern (per
[reference, user events](https://platform.claude.com/docs/en/managed-agents/reference#event-types)).

---

## 4. Beta header

```
anthropic-beta: managed-agents-2026-04-01
```

> *"All Managed Agents API requests require the `managed-agents-2026-04-01`
> beta header. The SDK sets the beta header automatically. Behaviors may be
> refined between releases to improve outputs."*
> — [Overview, "Beta access"](https://platform.claude.com/docs/en/managed-agents/overview)

What it gates (per the docs):
- The entire `/v1/agents`, `/v1/environments`, `/v1/sessions`,
  `/v1/sessions/{id}/events`, `/v1/sessions/{id}/stream` endpoint surface.
- The `agent_toolset_20260401` tool type.
- The session/event vocabulary (`user.message`, `agent.message`,
  `session.status_idle`, …).

If missing, all Managed Agents requests 4xx (likely `400 invalid_request_error`
with a missing-beta message). Bluey expresses the beta header in the registry
row's `cloud_transport.headers` field (data), not in code; the adapter just
reads the row.

Within the beta there is a smaller, gated preview for MCP tunnels and
[Dreaming](https://platform.claude.com/docs/en/managed-agents/dreams) — *"a
more limited research preview. Request access to enable them."* Bluey does
NOT use either today.

### Versioning

`anthropic-version: 2023-06-01` is the standard API version header. The beta
header is layered on top of it.

---

## 5. Billing model — BYOT only

> *"To get started, you need: 1. A Claude API key"*
> — [Overview](https://platform.claude.com/docs/en/managed-agents/overview)

There is no shared-billing or subscription-billing mode. Every token billed
goes against the API key the request was made with — the user's Console API
key. Bluey takes zero cut, knows nothing about the spend, and cannot be on
the hook for it.

The disclosure copy Bluey shows on first attach:

> **Claude Agent (Cloud) uses your Anthropic Console API key.**
>
> Every message Bluey sends is billed by Anthropic at your tier's per-token
> rate, against the Console API key you paste below. **This is not your
> Claude Pro or Max subscription** — Pro/Max billing only applies inside
> Anthropic's own apps (claude.ai, Claude Code, Claude Cowork).
>
> - Spend will appear on platform.claude.com/usage, not in your subscription.
> - You can revoke the key at any time at platform.claude.com/settings/keys.
> - Bluey stores the key in your OS keychain; it never leaves this machine
>   except to make requests directly to api.anthropic.com.
> - Managed Agents sessions are stateful, so they're **not eligible for
>   Zero Data Retention** or HIPAA BAA coverage — Anthropic holds the
>   conversation and sandbox state until you delete the session.
>
> If you agree, paste your `sk-ant-api03-…` key below.

Implementation: stored in the registry row as a `BillingModel::Byot { vendor,
console_url }` enum so the disclosure renderer is one path that handles every
future BYOT vendor. The template strings live in the adapter (vendor name,
console URL) — never the daemon.

---

## 6. Session / task semantics

### State machine

> | Status | Description |
> |--------|-------------|
> | `idle` | Agent is waiting for input, including user messages or tool confirmations. Sessions start in `idle`. |
> | `running` | Agent is actively executing. |
> | `rescheduling` | Transient error occurred, retrying automatically. |
> | `terminated` | Session has ended because of an unrecoverable error. |
>
> — [Session operations, "Session statuses"](https://platform.claude.com/docs/en/managed-agents/session-operations#session-statuses)

Plus, observed via SSE events:
- `session.status_idle` carries a `stop_reason` indicating why the agent
  stopped (e.g. `end_turn`).
- `session.status_terminated` is emitted on unrecoverable error.
- `session.error` is emitted on processing errors and includes a typed `error`
  object with a `retry_status` field.

### Two-step start

> *"Creating a session provisions the environment's sandbox but does not start
> any work. To delegate a task, send events to the session using a user event."*
> — [Sessions, "Starting the session"](https://platform.claude.com/docs/en/managed-agents/sessions#starting-the-session)

So: `POST /v1/sessions` → idle session + sandbox spun up; `POST /v1/sessions/
{id}/events` with a `user.message` → status becomes `running` and the SSE
stream emits events.

### Streaming

> *"You receive real-time updates as the agent works. The agent goes idle: emits a `session.status_idle` event when it has nothing more to do."*

The stream is SSE (`Accept: text/event-stream` on `GET /v1/sessions/{id}/
stream`). Events are buffered server-side until the stream attaches, so the
documented order in the quickstart is: open stream → POST a user event → read
events as they arrive. The Bluey adapter follows that order.

### Lifespan

Anthropic doesn't publish a hard idle-timeout for the Anthropic-managed cloud
sandbox in the public docs. The self-hosted worker CLI has `--max-idle`
defaulting to 60s after `end_turn`, suggesting the managed sandbox uses
similar logic. Bluey treats every interaction as short-lived: create session,
send one message, stream to idle, delete session (so the sandbox is released
immediately). OPEN QUESTION (see §11).

### Resumption

Sessions are first-class persistent resources — `GET /v1/sessions/{id}`
returns the current state and `POST` an event continues where the last one
left off. Bluey could persist a `session_id` across answers for the same
"meeting" (much like the local-agent `--resume` flow). For the first cut,
Bluey treats each cloud answer as a fresh session and deletes it on idle.
Mid-session resumption is plumbed but disabled by default.

---

## 7. Error taxonomy

### HTTP shape

> | Status | Type string | Meaning |
> |--------|-------------|---------|
> | 400 | `invalid_request_error` | Bad body / shape / missing beta header |
> | 401 | `authentication_error` | Bad API key — *or* OAuth token used as `x-api-key` |
> | 402 | `billing_error` | No credits / payment problem |
> | 403 | `permission_error` | Key lacks the resource |
> | 404 | `not_found_error` | Resource gone |
> | 413 | `request_too_large` | Body too big |
> | 429 | `rate_limit_error` | RPM/ITPM/OTPM or surge limit; includes `retry-after` |
> | 500 | `api_error` | Anthropic-side bug |
> | 504 | `timeout_error` | Use streaming for long jobs |
> | 529 | `overloaded_error` | Capacity squeeze; backoff |
>
> — [API errors](https://platform.claude.com/docs/en/api/errors)

JSON body (verbatim):

```json
{
  "type": "error",
  "error": {
    "type": "not_found_error",
    "message": "The requested resource could not be found."
  },
  "request_id": "req_011CSHoEeqs5C35K2UUqR7Fy"
}
```

`request_id` is also in a `request-id` response header. The adapter logs both.

### Streaming errors

> *"When receiving a streaming response over SSE, it's possible that an error
> can occur after returning a 200 response, in which case error handling
> wouldn't follow these standard mechanisms."*
> — [API errors](https://platform.claude.com/docs/en/api/errors)

These surface as a `session.error` SSE event with a typed `error` object and
`retry_status`. The adapter maps them to its own `AnthropicError` enum.

### Context-too-long

Managed Agents inherits the underlying model's context window. When an agent
session compacts (the `agent.thread_context_compacted` event) the harness
absorbs it transparently; the unrecoverable case surfaces as
`session.status_terminated` with the relevant `stop_reason`. The Bluey
escalation ladder (same as local agents) treats a terminated session as
"retry once with a fresh session," mirroring the local-CLI fresh-fallback.

### Revocation

> Console keys are revoked in the Anthropic Console
> ([platform.claude.com/settings/keys](https://platform.claude.com/settings/keys)).
> Revocation is account-wide — there is no per-resource or per-key partial
> revoke. Once revoked, every subsequent request returns 401
> `authentication_error`.

Bluey detects this in the adapter and, on a 401 against the Anthropic vendor,
emits an overlay card *"Your Anthropic API key was revoked or is invalid.
Reattach to paste a new one."* The keychain entry is NOT auto-cleared (the
user may have a transient network glitch); a manual detach clears it.

---

## 8. MCP

> *"Claude Managed Agents connects to remote MCP servers that expose an HTTP
> endpoint, or to private MCP servers through MCP tunnels. The server must
> support the MCP protocol's streamable HTTP transport."*
> — [Reference, "Supported MCP server types"](https://platform.claude.com/docs/en/managed-agents/reference#supported-mcp-server-types)

So:
- Outbound MCP from the managed sandbox → user-declared remote MCP servers.
  Configured in the agent's `mcp_servers` array at create-time, OR updated
  per-session via `POST /v1/sessions/{id}` with an `agent.mcp_servers` body.
- Bluey does NOT inject MCP servers today; the agent it creates has none
  configured. The user can wire their own via the Console.

> *"If your agent uses MCP tools that require authentication, pass `vault_ids`
> at session creation to reference a vault containing stored OAuth credentials.
> Anthropic manages token refresh on your behalf."*
> — [Sessions, "MCP authentication through vaults"](https://platform.claude.com/docs/en/managed-agents/sessions#mcp-authentication-through-vaults)

Out of scope for this loop.

---

## 9. Data residency / retention

> *"Claude Managed Agents is stateful by design: sessions are long-running,
> resume cleanly after pauses, and store conversation history, sandbox state,
> and outputs server-side. Because of this, Managed Agents is not currently
> eligible for Zero Data Retention or HIPAA Business Associate Agreement (BAA)
> coverage. You retain control over this data: you can delete sessions, and
> separately delete any files you uploaded, at any time through the API."*
> — [Overview](https://platform.claude.com/docs/en/managed-agents/overview)

Bluey's posture:
- Disclosure modal calls this out explicitly (the "not eligible for ZDR" line).
- Adapter deletes the session via `DELETE /v1/sessions/{id}` immediately
  after the stream goes idle, unless the user has enabled cross-answer
  resumption (default off).

Region: not publicly documented; OPEN QUESTION (see §11). Treat as "wherever
Anthropic runs its API" and disclose that ambiguity.

---

## 10. Edge cases

| Concern | What the docs say | Bluey's posture |
|---------|--------------------|------------------|
| Cold-start time | Unspecified. | Show a "spinning up sandbox" card; bail with the same `agent_not_ready` ladder if it takes >30s. |
| Sandbox network limits | Configurable via `environment.config.networking` (`unrestricted` or restrictive variants per the env config spec). | Default `unrestricted` per quickstart. |
| Secret-leak protection | Anthropic relies on the underlying model's prompt-injection guardrails plus the sandbox isolation. | Bluey does NOT inject secrets into the prompt and the registry row carries `allows_user_secrets: false`. |
| Cost surprise | Per-token billing, monthly spend cap by tier. | Disclosure modal + audit log + future quota-check polling (TODO). |
| Concurrent sessions | Not publicly documented. | Bluey serializes per-user (one in-flight session at a time). |

---

## 11. OPEN QUESTIONS

These are unsourced assumptions in the implementation. If wrong, here's what
changes.

1. **Per-Bluey-install agent + environment.** I assumed Bluey idempotently
   provisions one `name = "bluey-cowork"` agent and one
   `name = "bluey-env"` environment per Console account on first attach,
   then reuses them. If Anthropic forbids duplicate names per account, or if
   the user is expected to bring their own pre-created IDs, the adapter
   needs an alternate path (the row carries `provision_resources: bool` so
   this flips with one data edit, no code branch).
2. **Sandbox idle timeout.** Assumed similar to the self-hosted worker's 60s
   `--max-idle` default. If shorter (say 10s), the adapter's between-event
   delay could trigger spurious termination on slow networks. Mitigation: the
   adapter doesn't pause between events.
3. **SSE reconnect.** Assumed standard SSE: GET reconnect on dropped
   connection, no Last-Event-ID header required (Anthropic's docs don't
   mention one). If reconnect needs a resume cursor we have to wire it.
4. **Per-key concurrent session cap.** Assumed N=∞ within the org RPM cap. If
   there's a hard cap (say 5 concurrent) the adapter needs a queue.
5. **`agent_toolset_20260401` complete tool list.** The parser handles the
   event types generically (`agent.tool_use` with arbitrary `name`/`input`)
   so a wider tool catalog doesn't break decoding.
6. **Region / residency.** Assumed sandboxes run in Anthropic's primary US
   region. Confirm with Anthropic and disclose if EU is offered.
7. **Sessions endpoint URL form.** The quickstart shows
   `https://api.anthropic.com/v1/sessions` (and `…/events`, `…/stream`); the
   events-and-streaming doc has one variant with a `?beta=true` query string
   (`/v1/sessions/$SESSION_ID/events?beta=true`). I treat the query string
   as redundant given the beta header and omit it. If the query is required
   on `events` specifically, the adapter needs one extra constant.

---

## 12. PROPOSED SPEC (the data-row shape)

The whole point of agent-bridge is "adding a vendor = adding a row." Here's
what the row needs.

### New / extended types (in `crates/cue-agent-bridge/src/registry.rs`)

```rust
/// How an agent is billed. Data-driven so the UI disclosure flow is one
/// path, not a per-vendor branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingModel {
    /// The agent runs on the user's machine. No third-party billing.
    LocalCli,
    /// Bring-your-own-token: the user provides a cloud API key; billing
    /// goes to that key, NOT to a Bluey-managed account.
    Byot {
        /// Human-facing vendor name for the disclosure copy (e.g. "Anthropic").
        vendor: &'static str,
        /// URL where the user generates / revokes the key.
        console_url: &'static str,
        /// One-line "where will I see spend" pointer.
        usage_url: &'static str,
    },
}

/// What HTTP transport an agent is driven through.
#[derive(Debug, Clone, Copy)]
pub enum CloudTransport {
    /// No cloud transport — local CLI agent.
    None,
    /// HTTPS to a vendor API.
    Https {
        /// Base URL (e.g. "https://api.anthropic.com").
        base_url: &'static str,
        /// Static headers required on every request (e.g. anthropic-beta,
        /// anthropic-version). Beta header is DATA here, not code.
        headers: &'static [(&'static str, &'static str)],
        /// Auth header model — keep the variant set small, vendors pick one.
        auth: CloudAuth,
        /// Endpoint paths, named by their purpose.
        endpoints: CloudEndpoints,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum CloudAuth {
    /// Vendor expects the credential in a header (name + prefix).
    /// Anthropic: header_name="x-api-key", prefix="".
    /// OpenAI:    header_name="Authorization", prefix="Bearer ".
    HeaderToken { header_name: &'static str, prefix: &'static str },
}

#[derive(Debug, Clone, Copy)]
pub struct CloudEndpoints {
    /// e.g. "/v1/agents". `None` if vendor doesn't have separate-create-agent.
    pub create_agent:       Option<&'static str>,
    pub create_environment: Option<&'static str>,
    /// e.g. "/v1/sessions" — required.
    pub create_session:     &'static str,
    /// Template with `{session_id}` (e.g. "/v1/sessions/{session_id}/events").
    pub send_event:         &'static str,
    /// Template with `{session_id}` (e.g. "/v1/sessions/{session_id}/stream").
    pub stream_events:      &'static str,
    /// Template with `{session_id}` (e.g. "/v1/sessions/{session_id}").
    pub delete_session:     Option<&'static str>,
}

/// One extra field on AgentEntry:
pub struct AgentEntry {
    ...existing fields...,
    pub billing_model: BillingModel,
    pub cloud_transport: CloudTransport,
}
```

### The Anthropic row

```rust
AgentEntry {
    kind_tag: KindTag::AnthropicCloud,
    display_name: "Claude Agent (Cloud)", // Per branding guidelines
    binary_candidates: &[],
    app_bundles: &[],
    app_dirs_windows: &[],
    data_dir_globs: &[],
    app_data_windows: &[],
    session_format: None,
    jsonl_subdir: "",
    drive_command: &[],
    answer_args: &[],
    mcp_allow_flag: None,
    install: None,
    fix: FixProfile {
        propose_args: &[],
        apply_args: &[],
        apply_supported: false, // Fix-lane is local-CLI only for now.
    },
    billing_model: BillingModel::Byot {
        vendor: "Anthropic",
        console_url: "https://platform.claude.com/settings/keys",
        usage_url: "https://platform.claude.com/usage",
    },
    cloud_transport: CloudTransport::Https {
        base_url: "https://api.anthropic.com",
        headers: &[
            ("anthropic-version", "2023-06-01"),
            ("anthropic-beta",    "managed-agents-2026-04-01"),
        ],
        auth: CloudAuth::HeaderToken {
            header_name: "x-api-key",
            prefix: "",
        },
        endpoints: CloudEndpoints {
            create_agent:       Some("/v1/agents"),
            create_environment: Some("/v1/environments"),
            create_session:            "/v1/sessions",
            send_event:                "/v1/sessions/{session_id}/events",
            stream_events:             "/v1/sessions/{session_id}/stream",
            delete_session:     Some("/v1/sessions/{session_id}"),
        },
    },
}
```

Note: every existing local-CLI row gets `billing_model: BillingModel::LocalCli`
and `cloud_transport: CloudTransport::None`. No `if vendor == "anthropic"`
branch anywhere — the generic dispatcher in `drive` reads
`cloud_transport` and routes to `cloud::dispatch` when it's `Https`.

### Per-vendor adapter (the irreducible bits)

In `crates/cue-agent-bridge/src/cloud/anthropic.rs`:

- Build the create-agent / create-env / create-session JSON bodies that the
  Anthropic API expects (the shape is vendor-specific — `model`, `system`,
  `tools` arrays, the `agent_toolset_20260401` tool type literal).
- Parse the SSE stream into the typed enum
  `AnthropicEvent { AgentMessage { content }, AgentToolUse { name, input },
  SessionStatusIdle { stop_reason }, SessionStatusTerminated, SessionError { error, retry_status } }`.
- Map errors from the documented JSON shape into the bridge's
  `AnthropicError` enum.
- Validate keys: reject `sk-ant-oat01-*`, accept `sk-ant-api03-*`, accept
  anything else with a warning (since Anthropic may add new prefixes).

All shared HTTP/keychain/audit lives in `cloud/transport.rs`,
`cloud/keychain.rs`, `cloud/audit.rs` (vendor-agnostic).

### Daemon wiring

In `crates/cue-daemon/src/app.rs`:

- `agent_display_name(&AgentKind::AnthropicCloud)` returns `"Claude Agent
  (Cloud)"` via `registry::display_name_for` (no hardcoded match arm).
- `handle_agent_attach`: before persisting the attached agent, inspect the
  row's `billing_model`. If `BillingModel::Byot` and the daemon's
  `accepted_byot_vendors` settings list doesn't include that vendor,
  emit `OverlayCommand::PushBillingDisclosure {...}` and return *without
  persisting*. The accept response (`OverlayEvent::BillingDisclosureAccepted
  { vendor }`) writes the vendor into settings and re-runs the attach.
- `drive_answer_attempt`: when `cloud_transport != None`, delegate to the
  shared cloud dispatcher, which loads the keychain entry, calls the
  per-vendor adapter, and consumes the SSE stream into the same
  `AnswerStream` shape the local CLI produces. The escalation ladder
  (fresh-retry-on-failure) is the same.

The wiring is 100% data-driven — no `if kind == AnthropicCloud { ... }`
branch anywhere outside the per-vendor adapter file.

---

## 13. Citations / sources

Anthropic primary:
- [Managed Agents overview](https://platform.claude.com/docs/en/managed-agents/overview)
- [Quickstart](https://platform.claude.com/docs/en/managed-agents/quickstart)
- [Sessions](https://platform.claude.com/docs/en/managed-agents/sessions)
- [Session operations](https://platform.claude.com/docs/en/managed-agents/session-operations)
- [Events and streaming](https://platform.claude.com/docs/en/managed-agents/events-and-streaming)
- [Reference](https://platform.claude.com/docs/en/managed-agents/reference)
- [API errors](https://platform.claude.com/docs/en/api/errors)
- [API rate limits](https://platform.claude.com/docs/en/api/rate-limits)

Policy / OpenClaw-ban (independent sources):
- The Register, 2026-02-20: <https://www.theregister.com/2026/02/20/anthropic_clarifies_ban_third_party_claude_access/>
- VentureBeat: <https://venturebeat.com/technology/anthropic-cracks-down-on-unauthorized-claude-usage-by-third-party-harnesses>
- MindStudio: <https://www.mindstudio.ai/blog/anthropic-openclaw-ban-oauth-authentication>
- WinBuzzer, 2026-02-19: <https://winbuzzer.com/2026/02/19/anthropic-bans-claude-subscription-oauth-in-third-party-apps-xcxwbn/>
- Anthropic Agent SDK / Claude Code GitHub (feature requests confirming OAuth
  rejection on Messages API): <https://github.com/anthropics/claude-code/issues/37205>
