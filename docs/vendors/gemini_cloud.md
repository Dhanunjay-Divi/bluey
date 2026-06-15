# Google Gemini Cloud (Managed Agents / Interactions API) — Vendor Dossier

> Audience: Bluey engineering. Goal: integrate Google's **Gemini API Managed
> Agents** — the cloud surface announced at Google I/O 2026 ("spin up an agent
> with a single API call that reasons, uses tools and executes code in an
> isolated Linux environment") — as a CLOUD agent in `cue-agent-bridge`, WITHOUT
> branching code on the vendor name. The same registry-row philosophy that
> governs local agents and the four existing cloud vendors (Cursor Cloud,
> Copilot Cloud, Anthropic Managed Agents, Codex Cloud) must hold for this row.
>
> This is the **5th** cloud vendor. It is added as a NEW `AgentKind::GeminiCloud`
> distinct from the existing local `AgentKind::Gemini` (which drives the `gemini`
> CLI). The two never collide.

---

## STATUS REPORT (top-of-file summary)

Legend: ✅ done+tested (no live key) / 🟡 NEEDS-LIVE-VERIFY / ⚠️ product decision / 🔴 data-promise or billing risk.

### ✅ Done & tested locally (no live Gemini key needed)

- **DATA** — `AgentKind::GeminiCloud` added in `lib.rs` (placed AFTER
  `AnthropicCloud`, before `Aider`, to avoid colliding with the parallel
  Antigravity agent's variants), serde snake_case label `gemini_cloud`.
- **DATA** — `KindTag::GeminiCloud` + bidirectional mapping in `registry.rs`
  (`to_agent_kind` / `from_agent_kind`).
- **DATA** — `cloud::gemini_cloud::ENTRY` (a `CloudAgentEntry`) appended at the
  END of `CLOUD_REGISTRY`. Declares `billing_model = ApiCredits`, a non-empty
  BYOT `consent_warning`, `vendor_short = "gemini_cloud"`, `task_shaped = false`
  (turn-shaped — see below).
- **CODE (adapter)** — `crates/cue-agent-bridge/src/cloud/gemini_cloud.rs`
  mirroring the structure of `codex_cloud.rs`: the `ENTRY` row, the `TRANSPORT`
  data (host + the `Api-Revision: 2026-05-20` header carried as DATA, not a code
  branch), pure request-body builders, the synchronous-response parser, the SSE
  event → `AnswerChunk` mapping, HTTP-status → guidance error mapping, the BYOT
  consent text, and unit + `wiremock` round-trip tests.
- **WIRING** — `pub mod gemini_cloud;` in `cloud/mod.rs`; the dispatcher in
  `cloud/drive.rs` gained a **turn-shaped** branch (`spawn_turn_stream`, chosen
  off `entry.task_shaped == false`) so Gemini Cloud streams
  `Started → Delta(s) → Done` like a normal synchronous answer (NOT the
  task-ack card the other four vendors use). The daemon's `parse_attached_agent`
  needs no change — the snake_case serde round-trip resolves `gemini_cloud`
  with no per-vendor branch (verified by the existing daemon test pattern).
- **COORDINATION (parallel run)** — the Antigravity vendor agent landed its
  `AgentKind::AntigravityCloud` row in the SAME loop. It targets the SAME Gemini
  Interactions endpoint (only the `agent` field differs) and BUILT ON the
  `spawn_turn_stream` scaffold added here — both vendors now share the
  turn-shaped dispatch, each with its own `match agent` arm + adapter. The two
  are distinct vendor identities (separate `vendor_short`, keychain service,
  audit label, consent text), so they coexist with no collision. All enum
  variants were appended at the END of their enums per the coordination rule;
  the registry row is the last entry in `CLOUD_REGISTRY`.
- **SECURITY** — token (`AIza…` AI Studio key) lives in the OS keychain only
  (`bluey_cloud_gemini_cloud / api_key`), never on disk, never an env var the
  daemon writes back. Every HTTP call emits one `audit::emit` line
  (vendor/endpoint/method/status/latency/request-id-presence; NO token, NO body).
- **TESTS** — `cargo fmt`, `cargo clippy -p cue-agent-bridge -p cue-daemon -- -D
  warnings`, and `cargo test -p cue-agent-bridge -p cue-daemon` all green.

### 🟡 Coded but NEEDS-LIVE-VERIFY (AI Studio API key required)

- **Non-streaming response field names.** The official docs publish `id`,
  `status` (`completed | requires_action | in_progress`), a `steps`/`outputs`
  array, and `usage`, plus the SDK sugar `output_text` (the last text block(s),
  auto-joined). The REST docs page I could fetch did **not** include a complete
  verbatim non-streaming JSON body, so the exact nesting of the text inside
  `outputs[].content[]` vs `steps[].content[]` is reconstructed from the
  streaming event shapes + SDK helper description. The parser
  (`parse_interaction_response`) is written defensively to walk BOTH shapes and
  falls back across `output_text`, `outputs`, and `steps`. **Confirm the exact
  body against a live call before trusting the non-stream path in production.**
- **SSE event taxonomy.** Confirmed event names from the streaming doc:
  `interaction.created`, `step.start`, `step.delta` (deltas of `type: text` /
  `thought_summary` / `arguments_delta` / `image`), `step.stop`,
  `interaction.completed`, `error`, `done` (`data: [DONE]`). The text-delta
  shape `{"index":0,"delta":{"type":"text","text":"…"},"event_type":"step.delta"}`
  is quoted verbatim from the doc. Edge event ordering (e.g. multiple parallel
  `step` indices, image deltas) is mapped fail-soft (unknown events dropped) but
  not live-exercised.
- **`environment` reuse + `previous_interaction_id`.** Documented as the
  multi-turn / sandbox-reuse mechanism; Bluey's v1 sends a fresh `"remote"`
  sandbox per ask (no resume) so this is built as data but not yet wired into
  the daemon's resume path. NEEDS-LIVE-VERIFY of the returned `environment_id`
  field name on a real response.
- **Rate-limit / 429 retry-delay parsing.** The Google error envelope
  (`{"error":{"code","message","status"}}`) is parsed; the `details[]`
  `RetryInfo`/`QuotaFailure` sub-objects are NOT parsed for a precise backoff
  yet (we surface a generic "rate-limited, retry shortly" guidance string).

### ⚠️ Product decisions

- **Turn-shaped, not task-shaped — this changes the meeting UX.** Unlike
  Cursor/Copilot/Codex Cloud (which kick off a minutes-long job and return a PR
  link), the Gemini Interactions API is **synchronous**: it "provisions a
  sandbox, runs the agent loop, and returns the result" in the HTTP response,
  with optional SSE streaming. So Bluey drives it like a normal answer
  (`Started → Delta → Done`), the same overlay path a local CLI agent uses — NOT
  the "kicked off a task, I'll notify you" acknowledgement card. **However** the
  agent loop (code execution + web browsing in a sandbox) can take *tens of
  seconds to minutes*. The default per-call HTTP timeout in the shared transport
  is 30 s; for the Gemini Cloud answer path we override it to a longer ceiling
  (see `MAX_TASK_DURATION_SECS`) and **strongly recommend driving it via SSE**
  so the overlay shows incremental progress instead of a long opaque spinner.
  Decide: is a 1–3 minute synchronous answer acceptable in the meeting overlay,
  or should long Gemini Cloud asks be parked like a task? (v1 ships synchronous +
  streaming; revisit if live latency is poor.)
- **AI Studio key vs Vertex AI.** Two completely different auth models share the
  "Gemini API" name (see §Auth). Bluey targets the **consumer Gemini Developer
  API** (AI Studio key, `x-goog-api-key` header) because that is what an
  individual Bluey user will have. **Vertex AI is explicitly NOT supported in
  v1** — it rejects API keys and demands OAuth2/ADC/service-account credentials,
  a different host, and a GCP project. The registry row's `base_url` and auth
  are the AI Studio path only. Enterprise/Vertex is a future, separate row.

### 🔴 Data-promise / billing risk

- **Code + repo contents (if attached) execute in a Google-hosted Linux
  sandbox.** Bluey's core promise is "the user's data never goes to Bluey's AI."
  Gemini Cloud honors that (the data goes to *Google's* agent, not Bluey's
  router) — but the user MUST understand their `input` (and anything the agent
  fetches/executes) runs in Google's cloud and is processed under Google's
  terms. The BYOT `consent_warning` states this. Google's stated security model:
  *credentials never enter the sandbox, network egress is allowlisted, the
  environment is ephemeral/isolated, idle after 15 min, deleted after 7 days of
  inactivity.*
- **Billing is BYOT and PREVIEW-PRICED.** During preview, **environment compute
  is not billed** — you pay only for Gemini model (Gemini 3.5 Flash) tokens
  against the user's own Gemini API account. This WILL change when Managed
  Agents leaves preview (sandbox compute is expected to become billable). The
  `consent_warning` calls out "billed to your own Google Gemini API account" and
  the `billing_model` is `ApiCredits`. **Re-verify pricing at GA** — a silent
  switch to billed sandbox-seconds is a real cost-surprise risk for users.
- **Free-tier data-use caveat.** On Google's **free tier**, prompts/responses
  may be used to improve Google's products (standard AI Studio free-tier terms);
  the **paid tier** does not use data to train. Bluey cannot tell from the key
  alone which tier the user is on, so the consent text must not promise "your
  data is never used for training" — it can only relay that this depends on the
  user's Google plan. Documented as an OPEN question.

---

## 1. What this surface is

**Managed Agents in the Gemini API** (announced Google I/O 2026, May 2026,
rolling out in **preview**). One API call spins up an agent — the **Antigravity
agent**, built on **Gemini 3.5 Flash** — that reasons, uses tools, and executes
code (Python, Node.js, Bash), installs packages, manages files, and browses the
web inside an **isolated, ephemeral Linux sandbox** hosted by Google. By default
the agent has `code_execution`, `google_search`, and `url_context` tools.

The surface is the **Interactions API** — Google's new "recommended standard
primitive for building with Gemini," optimized for agentic workflows,
server-side state management, and multi-turn/multi-modal conversations. It sits
alongside (and is superseding) the older `models/*:generateContent` surface.
A May-2026 breaking-changes migration guide moves developers from
`generateContent` to `interactions`.

Two deployment surfaces exist:
- **Gemini Developer API** (AI Studio key) — the consumer/individual path.
  **This is what Bluey targets.**
- **Gemini Enterprise Agent Platform** (Vertex AI) — enterprise, GCP-project +
  OAuth. **Out of scope for v1** (see §Auth).

Sources:
- <https://blog.google/innovation-and-ai/technology/developers-tools/managed-agents-gemini-api/>
- <https://blog.google/innovation-and-ai/technology/developers-tools/google-io-2026-developer-highlights/>
- <https://ai.google.dev/gemini-api/docs> (Gemini API docs index — "Agents", "Live API", "Interactions API")
- <https://ai.google.dev/gemini-api/docs/interactions/quickstart>
- <https://ai.google.dev/gemini-api/docs/interactions-breaking-changes-may-2026>
- <https://ai.google.dev/gemini-api/docs/interactions/streaming>
- <https://docs.cloud.google.com/gemini-enterprise-agent-platform/build/managed-agents> (enterprise/Vertex variant)
- <https://www.philschmid.de/gemini-managed-agents-developer-guide>
- <https://byteiota.com/gemini-api-managed-agents-full-sandbox-one-api-call/>

---

## 2. Auth model

### 2.1 Gemini Developer API (AI Studio) — what Bluey uses

- **Credential**: a Gemini API key created in **Google AI Studio**
  (<https://aistudio.google.com/app/apikey>). Standard keys start with the
  prefix **`AIza`** followed by a long alphanumeric string (the universal Google
  API-key format). Newer/restricted accounts may instead issue keys with an
  **`AQ.`** prefix, which "have different authentication requirements and may not
  work with standard Gemini API endpoints" — Bluey should accept any non-empty
  key but warn if it doesn't look like `AIza…`.
- **Header**: `x-goog-api-key: <KEY>`. (The key can also be a `?key=` query
  param, but the header is the documented standard and keeps the key out of URL
  logs — Bluey uses the header.)
- **Required schema header**: `Api-Revision: 2026-05-20` — opts in to the new
  Interactions API schema. (`Api-Revision: 2026-05-07` is the temporary
  opt-out/older schema.) This is Gemini's analog of Anthropic's
  `anthropic-version` / `anthropic-beta` headers and MUST be carried as **DATA**
  on the registry row, never hardcoded in a vendor branch.
- **Env vars** (for one-shot import only): the unified `google-genai` SDK reads
  `GEMINI_API_KEY` **and** `GOOGLE_API_KEY`; if both are set it prefers
  `GOOGLE_API_KEY`. Bluey imports from `GEMINI_API_KEY` (the Gemini-specific
  var) and mirrors into the keychain — it never writes a key back out to the
  environment.

curl shape (verbatim from the docs, streaming variant):
```bash
curl -X POST "https://generativelanguage.googleapis.com/v1beta/interactions" \
      -H "x-goog-api-key: $GEMINI_API_KEY" \
      -H "Content-Type: application/json" \
      -H "Api-Revision: 2026-05-20" \
      --no-buffer \
      -d '{
        "model": "gemini-3-flash-preview",
        "input": "Count to from 1 to 25.",
        "stream": true
      }'
```

### 2.2 Vertex AI (Gemini Enterprise Agent Platform) — NOT supported in v1

- **Different host**: `…aiplatform.googleapis.com` / regional endpoints, scoped
  to a GCP project + location.
- **API keys are rejected.** Calling the Vertex endpoint with a Google API key
  returns: *"API keys are not supported by this API. Expected OAuth2 access
  token or other authentication credentials that assert a principal."*
- **Auth is OAuth2 / Application Default Credentials / service-account JSON**
  (`GOOGLE_APPLICATION_CREDENTIALS`), short-lived bearer tokens (default 1 h),
  managed by Google Cloud IAM. The SDK flips to this model when
  `GOOGLE_GENAI_USE_VERTEXAI=true`.
- **Why excluded**: completely different credential lifecycle (token refresh,
  no static key), different consent/billing story (GCP project billing, not a
  personal API key), and the typical Bluey user does not have a GCP project.
  A future `GeminiVertexCloud` row with an OAuth2/ADC `CloudAuth` variant would
  handle this — additive, not a change to this row.

Sources:
- <https://ai.google.dev/gemini-api/docs/api-key> (`x-goog-api-key`, AI Studio key management, security best practices)
- <https://docs.cloud.google.com/vertex-ai/docs/authentication> (ADC / service account)
- <https://docs.cloud.google.com/vertex-ai/generative-ai/docs/migrate/openai/auth-and-credentials>
- <https://github.com/google-gemini/gemini-cli/issues/5739> ("API keys not supported by Vertex" error verbatim)
- <https://geminicli.com/docs/get-started/authentication/>

---

## 3. Endpoints, methods, exact JSON

### 3.1 Create / run an interaction (the single call)

- **Method + path**: `POST https://generativelanguage.googleapis.com/v1beta/interactions`
- **Synchronous** by default: provisions the sandbox, runs the agent loop,
  returns the result in the HTTP response. With `"stream": true` it streams SSE
  (see §4).

**Request body** (field names, from the docs + SDK):

| Field | Type | Notes |
|---|---|---|
| `model` | string | e.g. `"gemini-3.5-flash"`, `"gemini-3-flash-preview"`. Used for a plain model interaction. |
| `agent` | string | e.g. `"antigravity-preview-05-2026"`. Used for the **Managed Agent** (sandbox) experience. `model` and `agent` are alternative entry points; the Managed Agent path sets `agent`. |
| `input` | string OR array | The prompt. Array form: `[{type, text}]` and `[{type, data, mime_type}]` for multimodal. |
| `environment` | string OR object | `"remote"` = fresh ephemeral sandbox; an env id like `"env_abc123"` = reuse an existing sandbox (preserves files/state); or a config object. Setting `environment` enables the filesystem tools automatically. |
| `tools` | array | `[{ "type": "code_execution" | "google_search" | "url_context" }]`. Optional; the Antigravity agent has these by default. |
| `previous_interaction_id` | string | Multi-turn continuation; the server manages history. |
| `stream` | bool | `true` → SSE. |
| `response_format` | object | Structured-output JSON schema (optional). |
| `generation_config` | object | Model behavior params (optional). |

**Minimal Managed-Agent request** (verbatim from the Antigravity-agent doc):
```json
{
  "agent": "antigravity-preview-05-2026",
  "input": "Read Hacker News, summarize top 10 stories, save as PDF.",
  "environment": "remote"
}
```

**Non-streaming response** (field names; ⚠️ exact nesting NEEDS-LIVE-VERIFY):
- `id` — interaction id, e.g. `"int_123"`.
- `status` — `"completed" | "requires_action" | "in_progress"`.
- `outputs` / `steps` — array of output/step blocks. The May-2026 migration
  renamed `outputs` → `steps`; both names appear across docs of different
  vintages. Each block carries typed content (`{type: "text", text: "…"}` etc.).
- `usage` — token consumption.
- `environment_id` — the sandbox id, returned so a follow-up can reuse it.
- **`output_text`** — SDK convenience accessor: the last text block(s) in the
  response, auto-joined if split across consecutive text blocks. (This is SDK
  sugar over `outputs`/`steps`, not necessarily a raw REST field — the parser
  reads it if present and otherwise walks the arrays.)

### 3.2 Environment lifecycle

- **Create**: implicit — `POST /v1beta/interactions` with `environment: "remote"`
  creates a fresh sandbox; the response returns its `environment_id`.
- **Reuse / resume**: pass that id back as `environment` (or chain turns with
  `previous_interaction_id`).
- **TTL**: sandbox is **idle after 15 minutes**; **deleted after 7 days of
  inactivity** (TTL resets on use). Forked environments guarantee an identical
  baseline per invocation (no dependency drift / state contamination).
- **Explicit delete endpoint**: not documented in the material fetched —
  presumably `DELETE /v1beta/environments/{id}` by REST convention, but
  UNCONFIRMED. Bluey relies on the 7-day TTL for cleanup in v1.

Sources:
- <https://ai.google.dev/gemini-api/docs/interactions/quickstart>
- <https://ai.google.dev/gemini-api/docs/interactions-breaking-changes-may-2026>
- <https://www.philschmid.de/gemini-managed-agents-developer-guide> (TTL: idle 15 min, deleted after 7 days; `environment_id`, `previous_interaction_id`, "returns the result" = synchronous)

---

## 4. Streaming (SSE)

- **Same endpoint**, `POST /v1beta/interactions` with `"stream": true` in the
  body (no `?alt=sse` query param, no `:stream` suffix). Server-Sent Events.
- **Event types** (confirmed from the streaming doc):

| `event:` | meaning | Bluey mapping |
|---|---|---|
| `interaction.created` | initial event with interaction id + metadata | `Started { session_id: Some(id) }` |
| `step.start` | a new step begins (`model_output`, `thought`, `function_call`, …) | dropped (telemetry) |
| `step.delta` | incremental content delta | text deltas → `Delta(text)`; thought/args/image → dropped |
| `step.stop` | a step completes | dropped |
| `interaction.completed` | final event with usage stats | `Done { cost_usd: None }` |
| `error` | error notification | `Error(message)` |
| `done` | stream termination (`data: [DONE]`) | dropped (stream ends) |

- **Delta object shapes** (verbatim):
  - text: `{"type": "text", "text": "Hello..."}`
  - thought: `{"type": "thought_summary", "content": {"type": "text", "text": "..."}}`
  - function-call args: `{"type": "arguments_delta", "arguments": "{...}"}`
  - image: `{"type": "image", "mime_type": "image/jpeg", "data": "..."}`
- **Example SSE frame** (verbatim):
  ```
  event: step.delta
  data: {"index": 0, "delta": {"type": "text", "text": "1, 2, 3"}, "event_type": "step.delta"}

  event: done
  data: [DONE]
  ```

Bluey surfaces only **`text`** deltas as `Delta` (the user-visible answer);
`thought_summary`, `arguments_delta`, and `image` deltas are dropped fail-soft in
v1 (telemetry / not rendered in the meeting overlay). Unknown event types are
dropped without breaking the stream — same defensive posture as the Cursor SSE
parser.

Source: <https://ai.google.dev/gemini-api/docs/interactions/streaming>

---

## 5. Billing

- **Preview pricing**: *"Environment compute … is not billed during preview. You
  pay for Gemini model (Gemini 3.5 Flash) tokens only."* → `BillingModel::ApiCredits`
  (BYOT — billed to the user's own Gemini API account, not Bluey).
- **Free tier vs paid tier**: the Gemini Developer API has a free tier (subject
  to lower rate limits and, per standard AI Studio terms, free-tier data may be
  used to improve Google's products) and a paid tier (higher limits, data not
  used for training). Managed Agents in preview count Gemini-model tokens against
  whichever tier the key is on.
- **GA risk** 🔴: sandbox compute is expected to become billable at GA. Re-verify
  the pricing page when Managed Agents leaves preview; a silent switch to billed
  sandbox-seconds would surprise users. The `consent_warning` already says
  "billed to your own Google Gemini API account" (not a fixed promise of "free").

Sources:
- <https://www.philschmid.de/gemini-managed-agents-developer-guide> ("not billed during preview … Gemini 3.5 Flash tokens only")
- <https://ai.google.dev/gemini-api/docs/pricing> (general Gemini API pricing / free vs paid tier)

---

## 6. Rate limits & 429 shape

- Gemini API enforces per-project/per-key quotas (RPM/TPM/RPD), tiered by free
  vs paid. Exceeding them returns **HTTP 429** with the standard Google error
  envelope:
  ```json
  {"error":{"code":429,"message":"Resource has been exhausted (e.g. check quota).","status":"RESOURCE_EXHAUSTED"}}
  ```
- Extended 429s include a `details[]` array with
  `type.googleapis.com/google.rpc.QuotaFailure` (quota metric, e.g.
  `generativelanguage.googleapis.com/generate_content_free_tier_input_token_count`)
  and `google.rpc.RetryInfo` (retry delay). Bluey parses the top-level
  `code/message/status` and surfaces a generic "rate-limited, retry shortly"
  guidance; precise `RetryInfo` backoff parsing is a NEEDS-LIVE-VERIFY follow-up.

Sources:
- <https://discuss.ai.google.dev/t/429-resource-exhausted/111737>
- <https://github.com/google-gemini/gemini-cli/issues/5119> (429 handling)

---

## 7. Error taxonomy (HTTP status → user guidance)

All `generativelanguage.googleapis.com` errors use the `google.rpc.Status`
envelope `{"error":{"code","message","status"}}`. Bluey maps status codes to
honest, user-facing guidance (never escalates silently to Bluey's own AI):

| HTTP | `status` enum | Bluey guidance |
|---|---|---|
| 400 | `INVALID_ARGUMENT` / `FAILED_PRECONDITION` | malformed request (internal) / API not enabled for this key |
| 401/403 | `UNAUTHENTICATED` / `PERMISSION_DENIED` | key rejected or lacks access — reconnect Gemini Cloud; check the key is an AI Studio (`AIza…`) key, not a Vertex credential |
| 404 | `NOT_FOUND` | interaction/environment not found (may have expired) |
| 429 | `RESOURCE_EXHAUSTED` | rate-limited / quota exhausted — retry shortly or check the plan |
| 500/503 | `INTERNAL` / `UNAVAILABLE` | Gemini temporarily unavailable |
| 504 | `DEADLINE_EXCEEDED` | the agent run timed out |

Source: <https://ai.google.dev/api> (error model), forum 429 threads above.

---

## 8. Revocation

- The user revokes/rotates the key in **Google AI Studio → API Keys**
  (<https://aistudio.google.com/app/apikey>) — delete or regenerate. As with the
  other BYOT vendors, **disconnecting Bluey does NOT revoke the key on Google's
  side**; the consent text says so and points the user at AI Studio. Bluey's
  "disconnect" only clears the keychain entry locally.

---

## 9. MCP support direction

- The Antigravity agent / Managed Agents surface centers on built-in tools
  (`code_execution`, `google_search`, `url_context`) and skill files
  (`AGENTS.md` / `SKILL.md`). Google's broader 2026 agent stack (Antigravity 2.0,
  Gemini CLI) supports MCP, and the Gemini API has an MCP/function-calling story,
  but a documented "attach an MCP server to a Managed Agent via the Interactions
  API" field was not found in the fetched material. v1 sends the default tools
  only; MCP attachment is a future, additive `tools`/config extension. NEEDS-LIVE-VERIFY.

---

## 10. Data residency / region

- The Gemini **Developer API** (AI Studio) does not expose per-call region
  pinning the way Vertex AI does (Vertex is explicitly regional, GCP-project
  scoped). For data-residency-sensitive enterprise users, the Vertex path
  (future `GeminiVertexCloud` row) is the correct surface. v1 (AI Studio key)
  runs in Google's default global infrastructure. Documented as OPEN.

---

## 11. Mapping onto Bluey's cloud contracts (PROPOSED SPEC)

Read against `crates/cue-agent-bridge/src/registry.rs`, `src/lib.rs`, and the
just-built `src/cloud/registry.rs` + `src/cloud/codex_cloud.rs`. **Pure data —
no vendor-name branches in any logic path.**

### 11.1 `AgentKind` / `KindTag`

- `lib.rs`: add `AgentKind::GeminiCloud` **after `AnthropicCloud`** (before
  `Aider`), serde label `gemini_cloud`. Distinct from the existing local
  `AgentKind::Gemini`.
- `registry.rs`: add `KindTag::GeminiCloud` at the END of the cloud cluster, and
  the two arms in `to_agent_kind` / `from_agent_kind`.

### 11.2 `CloudAgentEntry` row (the proposed data)

```text
CloudAgentEntry {
    kind_tag:               KindTag::GeminiCloud,
    display_name:           "Google Gemini Agent (Cloud)",
    vendor_short:           "gemini_cloud",          // keychain svc + audit label
    base_url:               "https://generativelanguage.googleapis.com",
    billing_model:          BillingModel::ApiCredits, // BYOT, preview = tokens only
    consent_warning:        <BYOT text: runs in Google's sandbox; billed to your
                             own Gemini API account; data-use depends on your
                             Google plan (free vs paid); revoke in AI Studio>,
    task_shaped:            false,                    // TURN-shaped (synchronous + SSE)
    max_task_duration_secs: 0 is INVALID for the registry test only when
                             task_shaped == true; turn-shaped rows may set 0.
                             We set a non-zero soft ceiling (300s) used purely to
                             size the long-poll/stream timeout, NOT to gate a task.
}
```

### 11.3 `CloudAuth` variant

- **Reuse `CloudAuth::HeaderToken { header_name: "x-goog-api-key", prefix: "" }`**
  — identical shape to Anthropic's `x-api-key` (token IS the whole header value,
  no `Bearer ` prefix). **No new auth variant needed.** Vertex AI's OAuth2/ADC
  model WOULD need a new variant, but Vertex is out of scope for v1.

### 11.4 `CloudTransport` data

```text
CloudTransport::Https {
    base_url: "https://generativelanguage.googleapis.com",
    headers:  &[("Api-Revision", "2026-05-20")],   // schema opt-in, carried as DATA
    auth:     CloudAuth::HeaderToken { header_name: "x-goog-api-key", prefix: "" },
    endpoints: CloudEndpoints {
        create_session: Some("/v1beta/interactions"),   // the single call
        send_event:     None,   // multi-turn is previous_interaction_id in the body
        stream_events:  Some("/v1beta/interactions"),   // same path, stream:true
        // task-shape fields all None (this vendor is turn-shaped)
        ..Default::default()
    },
}
```

### 11.5 Drive path

Because `task_shaped == false`, the dispatcher (`cloud/drive.rs`) takes a
**turn-shaped** branch: build `POST /v1beta/interactions`, fire it, and emit
`Started → Delta(output_text) → Done`. (A future SSE variant streams
`step.delta` text as incremental `Delta`s; v1 ships the synchronous form +
the SSE parser unit-tested, with the streaming wire-up gated on live latency.)
This is the **same overlay contract a local CLI agent uses** — no task-ack card.

---

## 12. OPEN QUESTIONS (assumptions stated explicitly + what changes if wrong)

1. **Non-streaming response nesting.** *Assumption*: the answer text is reachable
   via `output_text` OR the last `{type:"text",text}` block inside
   `outputs[]`/`steps[]`. *If wrong*: the parser yields an empty/partial answer;
   fix is a one-line path change in `parse_interaction_response`. The unit tests
   pin the current assumed shape so drift fails loudly. **Resolve with one live
   call.**
2. **`agent` vs `model` for the Managed-Agent sandbox.** *Assumption*: the
   sandbox experience requires `agent: "antigravity-preview-05-2026"` (+
   `environment`), while `model: "gemini-3.5-flash"` is a plain model turn.
   Bluey's row defaults to the **agent** form so the user gets the sandbox. *If
   wrong* (e.g. `model` + `tools:[{code_execution}]` is the canonical Managed
   form): swap the body builder's top field — data-only change. The agent id
   string is preview-dated and WILL roll (`…-05-2026` → next); kept as a single
   `const` for one-line bumps.
3. **`Api-Revision` value.** *Assumption*: `2026-05-20` is current. *If wrong*:
   the header is one data tuple on the row; bump it. (Older `2026-05-07` is the
   documented fallback.)
4. **Synchronous latency in a meeting.** *Assumption*: a sandbox agent answer is
   tolerable synchronously (seconds–low minutes) if streamed. *If wrong* (multi-
   minute, poor overlay UX): flip `task_shaped` to `true` and route through the
   task-ack card instead — the registry row is built so this is a one-field flip
   plus a poll loop, no adapter rewrite.
5. **Free-tier training use.** *Assumption*: free-tier prompts may be used to
   improve Google's products; paid-tier not. *If wrong / changes*: update the
   consent text. Bluey cannot detect the tier from the key, so the consent text
   deliberately does NOT promise "never used for training."
6. **Explicit environment delete.** *Assumption*: rely on the 7-day idle TTL;
   no explicit delete wired. *If wrong / a delete is needed for hygiene*: add a
   `DELETE /v1beta/environments/{id}` endpoint to the row (additive).
7. **Vertex AI demand.** *Assumption*: individual Bluey users have an AI Studio
   key, not a GCP project; Vertex is out of scope for v1. *If wrong* (enterprise
   demand): add a separate `GeminiVertexCloud` row with an OAuth2/ADC `CloudAuth`
   variant — additive, this row untouched.
8. **MCP attachment.** *Assumption*: no documented per-interaction MCP attach
   field today; default tools only. *If wrong*: extend the `tools`/config in the
   body builder (additive data).
