# Product Strategy

Bluey is intended to become a commercial SaaS product, not an open-source/BYOK
clone. The current v0.1.0 track is a macOS arm64 local-first release that should
not be marketed as a finished SaaS until cloud auth/sync/RAG/billing are real.

## Product Positioning

- Paid subscription product.
- Managed cloud account and secure sync.
- Managed model/provider routing by default.
- Enterprise-grade privacy, retention, deletion, and audit controls.
- RAG memory across meetings, documents, screenshots, and user-provided context.
- Live meeting intelligence for teams already using coding agents.
- A bridge between live conversation and the work context that already exists
  in local repos, docs, tickets, and user-approved agent/MCP sessions.

## Privacy And Security Direction

- Store customer data securely in cloud infrastructure with encryption in transit and at rest.
- Use per-user and per-workspace authorization boundaries.
- Keep secrets out of logs and local state files.
- Provide deletion/export controls.
- Build visible, consent-based capture flows.
- Avoid product claims around bypassing monitoring, proctoring, or assessment controls.

## AI Direction

- Managed providers first.
- Local AI may remain a dev/offline fallback, but it is not the main product bet.
- RAG memory should combine:
  - meeting transcripts
  - recaps
  - user-attached documents
  - screenshots and OCR/vision summaries
  - decisions and action items
  - answer-style/persona instructions
- Meeting intelligence should combine:
  - live system/mic transcript
  - current screen/page context
  - attached project folders and repos
  - user-approved coding-agent session context where available
  - team memory across prior meetings and artifacts

## Meeting Agent Context Bridge

The larger product thesis is not "interview assistant". Bluey should become the
live layer between meetings and the coding agents people already use.

Modern engineering teams run Cursor, Claude Code, Codex, Kiro, GitHub Copilot,
Gemini CLI, and other agents that already know their repos, docs, tickets, and
tooling through local workspaces and MCP-style connectors. During a meeting,
that context usually disappears and people manually tab through Jira, Notion,
GitHub, dashboards, and terminal sessions. Bluey's job is to bring that context
into the conversation in real time.

Target flow:

```text
before meeting: attach workspace / repo / agent session / approved connector
during meeting: listen -> detect project/ticket/branch/topic -> surface context
after meeting: decisions/actions -> push back through approved tools/connectors
```

Important boundaries:

- Bluey should use what the user can see, select, attach, copy, or explicitly
  authorize through folders, repos, OAuth/API connectors, or local agent
  session bridges.
- Bluey should not depend on hidden third-party platform scraping, stolen
  cookies, or bypass flows.
- Every proactive card should be explainable: screen, transcript, repo,
  attached doc, session memory, or approved connector.

Near-term implementation should start with local workspace/repo attachment and
session RAG. Later rounds can add local coding-agent session discovery, MCP
context bridges, team meeting context merge, and post-meeting pushbacks.

## Commercial Features To Build

- Authenticated cloud sync.
- Workspace/team accounts.
- Billing and plan enforcement.
- Cloud RAG index.
- Meeting history dashboard.
- Managed model routing, fallbacks, and latency budgets.
- Personas/modes for different meeting types.
- Secure admin controls for retention and deletion.
- Agent/workspace context bridge for Cursor, Claude Code, Codex, Kiro, GitHub
  Copilot/Gemini CLI-style workflows where technically and ethically feasible.
- Proactive meeting cards for tickets, PRs, docs, owners, deploys, risks, and
  prior decisions.
- Team meeting memory that merges each participant's approved context without
  leaking private data across workspace boundaries.

## Bluey Auto Router (USP)

Bluey Auto is the product differentiator: the user does not pick a model.
Bluey classifies each request, chooses the right managed lane, and streams one
visible answer card from that lane. The older draft+refine experiment remains
dev-gated because visible replacement was confusing in live use.

See `docs/AUTO-ROUTING-USP.md` for the architecture and lane mapping. The
routing crate ships in `crates/cue-router/` with a local heuristic classifier
(no network call), a routing policy that maps task type + difficulty + latency
lane onto provider/model pairs, and a router that can optionally run an
Instant draft and a Deep refinement in parallel for internal latency tests.

The classifier covers six task types (general / code / system_design /
meeting / writing / vision), three difficulty levels, three latency lanes
(instant / balanced / deep), context-need flags (transcript / page / files /
screenshot / memory), and a confidence score. Vision is auto-detected from
attachments and overrides the latency lane.

Local-only mode forces every classification onto the Local lane (Ollama by
default), so the same routing layer powers privacy / offline runs.

The tiny-model classifier escalation path is wired as an optional trait
implementation; the managed Bluey cloud router will plug in there once the
endpoint exists. Until then, the heuristic classifier alone covers the
shipping Auto Router product surface.
