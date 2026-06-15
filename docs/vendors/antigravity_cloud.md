# Google Antigravity 2.0 — CLOUD Vendor Dossier (`AntigravityCloud`)

> Audience: Bluey engineering. Goal: integrate the **cloud / SDK / Managed-Agents**
> surface of Google Antigravity 2.0 into `cue-agent-bridge` as a data-driven
> cloud row, with no code branch on the vendor name. This is the CLOUD surface
> (Managed Agents in the Gemini API), **not** the local Antigravity IDE — that
> already exists as `AgentKind::Antigravity` (drives the bundled `agy`/`gemini`
> CLI, reads brain-JSONL sessions).

---

## STATUS REPORT (top-of-file summary)

**Overall: 🟡 Coded + unit/wiremock-verified, NEEDS-LIVE-VERIFY for the live
interaction round-trip (a Gemini API key with the Antigravity agent preview is
required).**

### ✅ Complete & tested locally (no live key needed)
- **DEFINITIVE: Antigravity cloud == Gemini API Managed Agents.** Same endpoint
  (`POST https://generativelanguage.googleapis.com/v1beta/interactions`), same
  auth (`x-goog-api-key`). The *only* thing that makes a call "Antigravity" vs a
  generic Gemini-cloud call is the `agent` field value
  (`"antigravity-preview-05-2026"`). See the [overlap section](#the-overlap-question-antigravity-cloud-vs-gemini-cloud).
- DATA: `AgentKind::AntigravityCloud` added at the END of the enum in `lib.rs`
  (serde label `antigravity_cloud`), `KindTag::AntigravityCloud` + bidirectional
  mapping in `registry.rs`.
- DATA: Cloud registry row appended at the END of `CLOUD_REGISTRY`
  (`cloud/antigravity_cloud.rs::ENTRY`), declaring `billing_model =
  ApiCredits`, `task_shaped = false` (session/turn-shaped), and the mandatory
  BYOT `consent_warning`.
- CODE (vendor adapter, `cloud/antigravity_cloud.rs`): request builders for
  `POST /v1beta/interactions` (single-turn, multi-turn via
  `previous_interaction_id`, streaming via `stream:true`) and `POST /v1beta/agents`
  (register a named managed agent), the response parser (`output_text` + `id` +
  `environment_id`), the Google error-envelope parser, HTTP→guidance mapping, the
  BYOT consent text, and the two dispatcher helpers (`api_revision_headers`,
  `parse_answer_for_dispatch`) that mirror the sibling `gemini_cloud` contract.
- WIRING: `AntigravityCloud` is hooked into the SHARED turn-shaped dispatch path
  `cloud/drive.rs::spawn_turn_stream` (the same path the parallel Gemini agent
  added for `GeminiCloud`) — three `match agent` arms (request build / error
  render / response parse). So "ask Antigravity Cloud" actually drives and
  streams `Started → Delta → Done`, not a stub.
- CODE reuses the shared, vendor-agnostic primitives unchanged:
  `cloud/transport.rs` (`CloudHttpsTransport`, `CloudAuth::HeaderToken`),
  `cloud/keychain.rs` (`bluey_cloud_antigravity_cloud / api_key`),
  `cloud/audit.rs` (no token field). **No new `CloudAuth` variant was needed** —
  the existing `HeaderToken { header_name, prefix }` covers `x-goog-api-key`.
- TESTS: 20+ unit tests asserting the exact JSON body shape, response parsing,
  error mapping, the `Api-Revision` + `x-goog-api-key` header data, and the
  consent disclosure; plus 3 `wiremock` round-trips (200 sync, 200 multi-turn,
  403 PERMISSION_DENIED) proving the wire shape and that the token never leaks.
- `cargo fmt`, `cargo clippy -p cue-agent-bridge -p cue-daemon -- -D warnings`,
  `cargo test -p cue-agent-bridge -p cue-daemon` all clean.

### 🟡 Coded but NEEDS-LIVE-VERIFY (Gemini API key + Antigravity preview access required)
- The exact `output_text` / `steps` response envelope is from official docs
  (`ai.google.dev/gemini-api/docs/antigravity-agent`, the quickstart, and the
  custom-agents page) plus a developer guide; the precise nesting of `steps[]`,
  the SSE delta event names for `stream:true`, and whether `output_text` is
  top-level vs nested under a `result` object MUST be confirmed against a live
  interaction. The parser is defensive (tries top-level then nested) and the
  unit tests pin the *current* assumed shape so any drift fails loudly.
- The `Api-Revision: 2026-05-20` header value is from the official Antigravity
  Agent page; if Google bumps the revision this is a one-line data change on the
  registry transport.
- The 429 / error envelope is the standard Google `{"error":{"code","message",
  "status"}}` shape (confirmed for the Gemini API generally); the exact
  `status` strings the *Interactions* API returns (e.g. whether a bad agent id is
  `NOT_FOUND` vs `INVALID_ARGUMENT`) are assumed from the generic taxonomy.

### ⚠️ Blocked / needs a product decision
- **De-duplication with the parallel `GeminiCloud` row.** Antigravity cloud and a
  generic Gemini-cloud agent are the SAME API. Today each lands its own row +
  adapter (the `agent` field differs). Once both rows exist, a follow-up should
  collapse the duplicated transport/builders into one shared
  `cloud/gemini_interactions.rs` module that both rows point at, parameterized by
  the `agent` field. This dossier documents the overlap so that refactor is
  mechanical. **Until then, the two adapters intentionally duplicate the
  `/interactions` request builder** — see the overlap section for why this is the
  safe race-order choice.
- Repo/workspace context: the local Antigravity IDE drives a repo on the user's
  machine; the cloud Antigravity agent runs in a *remote ephemeral Linux
  sandbox*. Bluey's repo/source-injection UI (via the `sources[]` field on
  `POST /agents`) is the gating piece for "ask Antigravity cloud about my repo."
  Until that lands, Bluey can only send free-form prompts (meeting-question
  shaped), which is the primary Bluey use case anyway.

### 🔴 None.

---

## 1. What Antigravity 2.0 is, and which surface this dossier covers

At Google I/O 2026, Antigravity expanded from a coding IDE into a **platform for
developing and managing teams of autonomous AI agents**, with four surfaces
([blog.google I/O 2026 highlights](https://blog.google/innovation-and-ai/technology/developers-tools/google-io-2026-developer-highlights/),
[MarkTechPost](https://www.marktechpost.com/2026/05/19/google-launches-antigravity-2-0-at-i-o-2026-a-standalone-agent-first-platform-with-cli-sdk-managed-execution-and-enterprise-support/)):

1. A standalone **desktop app** (agent orchestration UI).
2. The **`agy` CLI** (Go-based; successor to Gemini CLI —
   [Gemini CLI → Antigravity CLI transition](https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli/)).
3. The **Antigravity SDK** — "Google's programmatic interface for building custom
   AI agents that run as managed services on the Gemini API"
   ([aimadetools SDK guide](https://www.aimadetools.com/blog/antigravity-sdk-custom-agents-guide/)).
4. **Managed Agents in the Gemini API** — the cloud execution surface.

**This dossier covers surfaces 3 + 4 (the cloud/SDK/Managed-Agents layer).**
Surface 2 (the local `agy`/`gemini` CLI) is already covered by the existing
local `AgentKind::Antigravity` row in `registry.rs`. We are adding a NEW,
distinct kind `AntigravityCloud` for the cloud surface.

---

## 2. The overlap question: "Antigravity cloud" vs "Gemini cloud"

**This is the key question the integration loop asked, and the answer is
unambiguous from the official docs.**

> **Antigravity cloud is NOT its own API. It is the Gemini API Managed Agents
> surface, invoked at the same endpoint with the same auth — differing only in
> the `agent` field of the request body.**

Evidence (all official `ai.google.dev` / `blog.google`):

- The Managed Agents feature is *"powered by the new Antigravity agent"* built on
  Gemini 3.5 Flash
  ([Introducing Managed Agents in the Gemini API](https://blog.google/innovation-and-ai/technology/developers-tools/managed-agents-gemini-api/)).
- The [Antigravity Agent page](https://ai.google.dev/gemini-api/docs/antigravity-agent)
  documents invocation as `POST https://generativelanguage.googleapis.com/v1beta/interactions`
  with body `{"agent": "antigravity-preview-05-2026", "input": "...", "environment": "remote"}`.
- The [Managed Agents quickstart](https://ai.google.dev/gemini-api/docs/managed-agents-quickstart)
  and [custom-agents page](https://ai.google.dev/gemini-api/docs/custom-agents)
  show the SAME endpoint; a *custom* managed agent is registered via
  `POST /agents` with `base_agent: "antigravity-preview-05-2026"` and then
  invoked by referencing the chosen `id` in the `agent` field.

**Consequence for the code:**

- The `CloudTransport` data (base URL, auth header, `Api-Revision` header,
  endpoints) is **identical** to what the parallel `GeminiCloud` row needs. The
  only per-row difference is the constant `DEFAULT_AGENT = "antigravity-preview-05-2026"`
  baked into the request body builder.
- **Race-order handling (resolved mid-flight).** This adapter was written while
  the parallel Gemini-cloud agent's `cloud/gemini_cloud.rs` had not yet landed.
  It landed during implementation, and **independently confirmed every
  cross-vendor fact in this dossier at the code level**: same
  `BASE_URL = https://generativelanguage.googleapis.com`, same
  `INTERACTIONS_PATH = /v1beta/interactions`, same
  `AGENT_ID = "antigravity-preview-05-2026"` (the Gemini-cloud row uses the
  Antigravity agent too!), same `CloudAuth::HeaderToken { "x-goog-api-key", "" }`
  (no new variant), same `Api-Revision: 2026-05-20` header-as-data, same
  `task_shaped: false`, same `BillingModel::ApiCredits`. This is strong
  corroboration that **Antigravity cloud IS the Gemini API Managed Agents**.
- **Shared turn-shaped dispatch (REUSED, not duplicated).** The Gemini agent
  added a `spawn_turn_stream` path to `cloud/drive.rs` for turn-shaped vendors.
  `AntigravityCloud` is wired into that SAME path (three `match agent` arms:
  request build, error render, response parse) calling this adapter's builders —
  the dispatcher is the single allowed place for that wiring, exactly as for the
  task-shaped vendors. The shared *primitives* (`transport.rs`/`keychain.rs`/
  `audit.rs`/`registry.rs` row shape) and the dispatch loop are reused unchanged;
  only the per-vendor request-body builder + parsers are vendor-specific (and are
  the trivial future-merge target).

---

## 3. Auth model

- **Credential:** a **Gemini API key** (a.k.a. Google AI Studio / Google AI for
  Developers API key), e.g. obtained at aistudio.google.com. NOT Google Cloud
  OAuth/ADC for this surface — the `generativelanguage.googleapis.com` host is
  the AI-Studio key path. (The *Vertex / Gemini Enterprise Agent Platform* mirror
  at `aiplatform.googleapis.com/.../interactions` DOES use Google Cloud
  OAuth/ADC + a project id — see OPEN QUESTIONS; Bluey targets the AI-Studio key
  host for BYOT simplicity.)
- **Delivery:** HTTP header `x-goog-api-key: <KEY>`, per the official Antigravity
  Agent page's verbatim curl. This maps **exactly** onto the existing
  `CloudAuth::HeaderToken { header_name: "x-goog-api-key", prefix: "" }` — the
  same generic primitive Anthropic uses for `x-api-key`. **No new auth variant.**
- **Also valid (Gemini API generally):** the `?key=<KEY>` query-param form. We do
  NOT use it — putting the key in the URL risks it landing in logs/audit. The
  header form is what the Antigravity docs show and what keeps the audit log
  (which records the endpoint path, never the URL query) safe.
- **Extra required header:** `Api-Revision: 2026-05-20` (from the official
  Antigravity Agent page). Carried as registry **data** (the `headers` slice on
  the transport row), never in a code branch.
- **Revocation:** revoke/rotate the key in Google AI Studio. A revoked key →
  `403 PERMISSION_DENIED` on the next call; Bluey surfaces a "reconnect"
  guidance card (never escalates to Bluey's own AI).

---

## 4. Endpoints (method / path / exact JSON)

Base URL: `https://generativelanguage.googleapis.com/v1beta`

### 4.1 Invoke an interaction (the answer call) — SESSION/TURN-shaped, synchronous

`POST /interactions`

Verbatim request shape (official Antigravity Agent page + quickstart):

```json
{
  "agent": "antigravity-preview-05-2026",
  "input": "your prompt text",
  "environment": "remote",
  "tools": [{"type": "google_search"}, {"type": "url_context"}]
}
```

- `agent` (REQUIRED): `"antigravity-preview-05-2026"` for the stock Antigravity
  agent, OR the `id` of a custom agent registered via `POST /agents`.
- `input` (REQUIRED): a string, OR an array of content blocks
  `[{"type":"text","text":"..."}, {"type":"image","data":"<base64>","mime_type":"image/png"}]`.
- `environment`: `"remote"` (fresh ephemeral Linux sandbox), a prior
  `environment_id` string (reuse sandbox state), or a config object
  `{"type":"remote"}`.
- `stream` (optional, default `false`): `true` → SSE stream of step deltas.
- `previous_interaction_id` (optional): chains conversation history across turns.
- `tools` (optional): defaults to `code_execution`, `google_search`,
  `url_context`. Override to scope down.
- **Rejected fields:** `max_output_tokens`, `temperature` → `400`
  (per the Antigravity Agent page: "returns 400 error if provided").

Response (synchronous; from quickstart + developer guide):

```json
{
  "id": "interaction_id",
  "environment_id": "env_id",
  "output_text": "...final agent answer...",
  "steps": [ /* reasoning + tool-call trace */ ]
}
```

- Persist `id` (→ `previous_interaction_id` next turn) and `environment_id` (→
  `environment` next turn) for multi-turn.

### 4.2 Register a named managed agent (custom-agent lifecycle)

`POST /agents`

```json
{
  "id": "data-analyst",
  "base_agent": "antigravity-preview-05-2026",
  "system_instruction": "You are a data analyst...",
  "base_environment": {
    "type": "remote",
    "sources": [
      { "type": "inline", "target": ".agents/AGENTS.md", "content": "..." }
    ]
  }
}
```

Then invoke by id: `POST /interactions {"agent":"data-analyst","input":"...","environment":"remote"}`.

Bluey does NOT need to register a custom agent for the meeting-copilot use case —
it can call the stock `antigravity-preview-05-2026` agent directly with the
meeting question as `input`. The `POST /agents` builder is provided for
completeness / future "ask about my repo" (the `sources[]` field injects repo
context).

### 4.3 Streaming

`POST /interactions` with `"stream": true` → SSE. Per the developer guide,
streaming "returns an iterable of step deltas, which are incremental text,
reasoning tokens, and tool call updates." Exact SSE event names are in the
`interactions/streaming` sub-page — **NEEDS-LIVE-VERIFY**. For Bluey's first cut
we use the synchronous form (Bluey's overlay wants a concise answer); the
streaming request builder is provided and unit-tested for shape.

---

## 5. Session / task semantics — is it turn- or task-shaped?

**TURN/SESSION-shaped (synchronous), NOT task-shaped.**

- A single `POST /interactions` blocks and returns `output_text` directly
  (developer guide: `interaction = client.interactions.create(...);
  print(interaction.output_text)`). There is no documented "create task → poll
  status by id → fetch result" async lifecycle for the AI-Studio Interactions
  surface.
- Multi-turn is `previous_interaction_id` + `environment_id` chaining, exactly
  like a chat session — NOT a fire-and-forget PR-producing task (Codex
  Cloud/Copilot Cloud).
- Long interactions auto-compact context at ~135k tokens (developer guide); they
  can still run long and consume millions of tokens for complex tool use, but the
  *call shape* is request→response, not poll.

**Decision:** the registry row sets `task_shaped: false` (matching Anthropic's
session-shaped row, NOT Codex/Copilot's task-shaped rows). The drive dispatcher's
task-ack-card path is therefore NOT used; this vendor streams/answers like
Anthropic. The current `cloud/drive.rs` dispatcher only implements the
task-shaped arms (Copilot/Cursor); session-shaped vendors (Anthropic + this one)
return an honest "session-shaped cloud drive not implemented yet" chunk from the
dispatcher until the session drive path lands — see `drive.rs` and the Anthropic
dossier. The adapter exposes all the request builders + parsers the session drive
will need.

---

## 6. Billing

- **Model:** `BillingModel::ApiCredits` (BYOT — the user's own Gemini API key).
- **What it counts against:** Gemini API token usage on the user's key. The
  Antigravity agent runs on **Gemini 3.5 Flash**:
  **$1.50 / M input tokens, $9.00 / M output tokens**, cached input $0.15/M
  ([Simon Willison](https://simonwillison.net/2026/May/19/gemini-35-flash/),
  [TokenMix](https://tokenmix.ai/blog/gemini-3-5-pro-release-date-google-io-2026),
  [Gemini API pricing](https://ai.google.dev/gemini-api/docs/pricing)).
- **Environment / sandbox compute:** **NOT billed during the preview** — "You pay
  for Gemini model (Gemini 3.5 Flash) tokens only"
  ([philschmid developer guide](https://www.philschmid.de/gemini-managed-agents-developer-guide)).
  This is a preview promise; the consent text says "tokens" and notes compute may
  be billed after preview.
- **Cost shape warning:** agentic runs with many tool calls can hit 3–5M tokens
  (~$5) in a single interaction — the consent text warns this is per-question
  metered, not a flat subscription.
- **Free tier:** the Gemini API has a Free tier, but it is **not available in all
  countries** — a Free-tier call from an unsupported region returns
  `400 FAILED_PRECONDITION` ("enable billing on your project"). The consent text
  and the 400 guidance both surface this.

---

## 7. Rate limits + 429 shape

- Limits are **per-project, not per-key** (multiple keys in one project share
  quota) and measured in RPM / TPM (input) / RPD across tiers Free / Tier 1 / 2 /
  3 ([Rate limits](https://ai.google.dev/gemini-api/docs/rate-limits)). Exact
  numbers live in AI Studio and depend on tier — not pinned here.
- **429 = `RESOURCE_EXHAUSTED`** in the standard Google error envelope (below).
  The adapter maps 429 to a "rate-limited, retry shortly" guidance string and
  never auto-retries in a tight loop.

---

## 8. Error taxonomy

Standard Google API error envelope (the Interactions API inherits it):

```json
{ "error": { "code": 429, "message": "…", "status": "RESOURCE_EXHAUSTED" } }
```

Mapping the adapter implements (from
[troubleshooting](https://ai.google.dev/gemini-api/docs/troubleshooting)):

| HTTP | `status`             | Meaning / Bluey guidance |
|------|----------------------|--------------------------|
| 400  | `INVALID_ARGUMENT`   | Malformed body (internal error) — also the `max_output_tokens`/`temperature` rejection. |
| 400  | `FAILED_PRECONDITION`| Free tier not available in country → enable billing in AI Studio. |
| 401/403 | `PERMISSION_DENIED` / `UNAUTHENTICATED` | Bad/revoked/incorrect API key → reconnect Antigravity Cloud. |
| 404  | `NOT_FOUND`          | Unknown agent id / resource — re-check the registered agent. |
| 429  | `RESOURCE_EXHAUSTED` | Rate / quota exceeded → retry shortly. |
| 500  | `INTERNAL`           | Google-side error (sometimes over-long context). Transient. |
| 503  | `UNAVAILABLE`        | Temporarily overloaded. Transient. |
| 504  | `DEADLINE_EXCEEDED`  | Prompt/context too large or run too long. |

The adapter's `render_error_message` appends the verbatim `error.message` from
the envelope when present (never the request body, never the key).

---

## 9. MCP direction

- The managed agent **consumes** a fixed default tool catalog (`code_execution`,
  `google_search`, `url_context`), overridable via the `tools[]` field. No
  documented general MCP-server consumption on the AI-Studio Interactions surface
  as of the I/O 2026 docs — **NEEDS-LIVE-VERIFY** whether `tools[]` accepts an
  MCP-server descriptor. It does NOT expose an MCP server itself.
- This is unlike the local Antigravity IDE, which consumes the user's
  `~/.gemini` MCP config. The cloud surface's tools are server-side and scoped by
  the `tools[]` request field, which is the safer posture for Bluey (read-only
  tools like `google_search`/`url_context` by default; no filesystem/shell on the
  *user's* machine — the sandbox is remote and ephemeral).

---

## 10. Data residency / region

- Each interaction runs in an **"isolated, ephemeral Linux environment"**
  (sandbox) that is forked per invocation and discarded — "every run starts
  clean" unless an `environment_id` is reused
  ([custom-agents](https://ai.google.dev/gemini-api/docs/custom-agents)).
- Specific region pinning / data-residency guarantees are **not documented** for
  the AI-Studio Interactions preview (the docs link an "Available regions" page
  but it isn't detailed) — **NEEDS-LIVE-VERIFY** / OPEN. The consent text states
  that prompts are sent to Google's remote sandbox and that data residency is
  governed by the Gemini API terms.

---

## 11. Consent text (BYOT — mandatory before storing the key)

The adapter exposes `ANTIGRAVITY_BYOT_CONSENT_TEXT`, which the enrollment UI MUST
show before the key is stored. It encodes the non-obvious promises: (1) it bills
the user's **Gemini API key per token** (Gemini 3.5 Flash rates), NOT a flat
plan; (2) prompts run in a **Google-hosted remote sandbox**; (3) compute is free
**only during preview**; (4) the key lives in the **OS keychain** and only ever
goes to `generativelanguage.googleapis.com`.

---

## 12. PROPOSED SPEC (the row + auth + billing)

```rust
// lib.rs — appended at the END of AgentKind (after any GeminiCloud the
// parallel agent adds), serde label "antigravity_cloud".
AntigravityCloud,

// registry.rs — KindTag::AntigravityCloud + both mapping arms.

// cloud/registry.rs — appended at the END of CLOUD_REGISTRY:
//   antigravity_cloud::ENTRY

// cloud/antigravity_cloud.rs — the row:
pub const ENTRY: &CloudAgentEntry = &CloudAgentEntry {
    kind_tag: KindTag::AntigravityCloud,
    display_name: "Google Antigravity (Cloud)",
    vendor_short: "antigravity_cloud",
    base_url: "https://generativelanguage.googleapis.com",
    billing_model: BillingModel::ApiCredits, // BYOT Gemini API key, per-token
    consent_warning: ANTIGRAVITY_BYOT_CONSENT_TEXT,
    task_shaped: false,                       // session/turn-shaped, like Anthropic
    max_task_duration_secs: 30 * 60,          // soft envelope for the poll sizer
};

// Auth: REUSE the existing primitive — no new CloudAuth variant.
auth: CloudAuth::HeaderToken { header_name: "x-goog-api-key", prefix: "" }
// Static headers (DATA): [("Api-Revision", "2026-05-20")]
// Endpoints: create_agent = "/v1beta/agents",
//            create_session = "/v1beta/interactions" (the interaction = the "session"),
//            send_event/stream_events = "/v1beta/interactions" (same path; turn-shaped),
//            create_task/get_task_status = None.
// NOTE: the `v1beta` prefix is the documented Interactions API version and is
// the SAME path the parallel `gemini_cloud` adapter uses — confirming the
// overlap below at the wire level.
```

Rationale:
- **`task_shaped: false`** because invocation is synchronous request→response with
  conversation chaining (§5), mirroring Anthropic's session-shaped row, NOT
  Codex/Copilot's task rows.
- **`BillingModel::ApiCredits`** because it's BYOT, metered per Gemini token (§6).
- **No new `CloudAuth` variant** — `x-goog-api-key` is a header token with an
  empty prefix, identical in shape to Anthropic's `x-api-key` (§3).
- The `agent` field (`antigravity-preview-05-2026`) is the SOLE differentiator
  from a generic Gemini-cloud row; it lives as a constant in the request-body
  builder, so a future merge with `GeminiCloud` parameterizes exactly one value.

---

## OPEN QUESTIONS (assumptions made explicit + what changes if wrong)

1. **Host = AI-Studio `generativelanguage.googleapis.com`, not Vertex
   `aiplatform.googleapis.com`.** Assumed because Bluey's BYOT model wants a
   simple API key, and the Antigravity Agent page uses the generativelanguage
   host + `x-goog-api-key`. *If wrong / if an enterprise wants Vertex:* the Vertex
   mirror (`POST .../projects/{p}/locations/{l}/interactions`) needs Google Cloud
   OAuth/ADC + a project id — a NEW `CloudAuth` variant (OAuth2/ADC bearer) and a
   project-id field. That's additive; this row is untouched. Documented as a
   future variant, not built now (no live access to verify the OAuth flow).

2. **`output_text` is the answer field and is top-level.** Assumed from the
   quickstart/developer-guide examples. *If wrong* (e.g. nested under `result` or
   only available via `steps[]`): the parser already falls back from top-level to
   a nested `result.output_text`; a third shape is a one-line patch. Unit tests
   pin the current shape so drift fails loudly.

3. **`x-goog-api-key` header (not `?key=` query).** Assumed from the official
   Antigravity curl. One web source claimed Gemini "passes the key as a query
   param, header fails" — that source is about older SDK behavior and contradicts
   the current Antigravity page. *If the header is rejected live:* switch to the
   query-param form — but that needs an audit-log review first (we must NOT log
   URLs with embedded keys). Flagged because of the source conflict.

4. **`Api-Revision: 2026-05-20`.** From the Antigravity Agent page. *If Google
   bumps it:* one-line data change on the transport `headers` slice.

5. **Synchronous, not async/long-running-operation.** Assumed from the SDK
   examples returning `output_text` directly (no `operations.get` poll loop).
   *If a long interaction actually returns a `202` + operation handle:* the row
   flips to `task_shaped: true` and gains a `get_task_status` endpoint — but every
   doc example shows synchronous return, so this is low-risk.

6. **Compute free during preview, tokens billed.** From the developer guide + a
   pricing roundup. *If compute starts being billed:* the consent text already
   warns "during preview"; update the wording, no code change.

7. **MCP / `tools[]` accepts only the built-in catalog.** Assumed; no doc shows a
   custom-MCP descriptor in `tools[]`. *If it does:* additive — Bluey can pass the
   user's scoped read-only MCP servers as data. No row change.

8. **Custom-agent `id` is global to the project and reusable.** Assumed from
   `POST /agents {"id":"data-analyst"}` then invoke-by-id. *If ids are
   per-request resource names returned by the create call:* the builder returns
   the id from the create response (already parsed). Low-risk.

---

## Sources

- [Introducing Managed Agents in the Gemini API (blog.google)](https://blog.google/innovation-and-ai/technology/developers-tools/managed-agents-gemini-api/)
- [I/O 2026 developer highlights (blog.google)](https://blog.google/innovation-and-ai/technology/developers-tools/google-io-2026-developer-highlights/)
- [Antigravity Agent | Gemini API (ai.google.dev)](https://ai.google.dev/gemini-api/docs/antigravity-agent)
- [Managed Agents Quickstart | Gemini API (ai.google.dev)](https://ai.google.dev/gemini-api/docs/managed-agents-quickstart)
- [Building Managed Agents / custom-agents | Gemini API (ai.google.dev)](https://ai.google.dev/gemini-api/docs/custom-agents)
- [Rate limits | Gemini API (ai.google.dev)](https://ai.google.dev/gemini-api/docs/rate-limits)
- [Troubleshooting / error taxonomy | Gemini API (ai.google.dev)](https://ai.google.dev/gemini-api/docs/troubleshooting)
- [Gemini Developer API pricing (ai.google.dev)](https://ai.google.dev/gemini-api/docs/pricing)
- [Gemini Managed Agents: Developer Guide (philschmid.de)](https://www.philschmid.de/gemini-managed-agents-developer-guide)
- [Gemini 3.5 Flash pricing (simonwillison.net)](https://simonwillison.net/2026/May/19/gemini-35-flash/)
- [Transitioning Gemini CLI to Antigravity CLI (developers.googleblog.com)](https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli/)
- [Google launches Antigravity 2.0 at I/O 2026 (MarkTechPost)](https://www.marktechpost.com/2026/05/19/google-launches-antigravity-2-0-at-i-o-2026-a-standalone-agent-first-platform-with-cli-sdk-managed-execution-and-enterprise-support/)
- [Antigravity SDK custom-agents guide (aimadetools)](https://www.aimadetools.com/blog/antigravity-sdk-custom-agents-guide/)
