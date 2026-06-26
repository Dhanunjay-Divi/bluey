# Round 185 - Answer Quality And Web Search Plan

## Trigger

Owner asked how to make Bluey answer more like ChatGPT or Claude: organized, neat, able to use web search for questions that cannot be answered from current transcript, screen, attachments, or saved context.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`  
Workspace: `/Users/uno/Downloads/cue`  
Round completed: 2026-06-26 06:17 EDT

## Current Code Reality

- Bluey already has an answer pipeline that creates the user question card, creates an answer card, gathers session/screen/attachment/RAG context, then streams provider output into the overlay.
- Managed provider prompts already include human-speak rules, follow-up behavior, coding/system-design shapes, canvas/workbench split, and security boundaries.
- The current overlay formatter is intentionally light. It strips provider status lines, replaces em dashes, guards private prompt disclosure, and splits a few inline bullets/headings. It is not a full answer planner.
- Local answer context can include current session memory, attachments, recent sent attachments, and local RAG hits. Local RAG is capped by `BLUEY_ANSWER_RAG_TIMEOUT_MS`, defaulting to `120 ms`.
- Managed server completions can add cloud RAG snippets before streaming. Cloud RAG has a default `300 ms` retrieval budget and proceeds without retrieved context if the query times out.
- Managed streaming currently exposes model lifecycle events such as start, delta, safety, usage, complete, and error. It does not expose retrieval status, source chips, citation metadata, or a web-search phase.
- Repository search did not show a real external web-search lane in the daemon or managed router. The server routes model completions, embeddings, transcription, RAG, billing, sync, and account flows, but not public web retrieval.

## Gap

The missing piece is not just a nicer prompt. Bluey needs an answer orchestrator before the model call:

- decide what kind of answer the user is asking for
- decide whether current context is enough
- retrieve the right evidence when it is not enough
- choose compact overlay output versus deeper canvas/detail output
- stream status so the user knows whether Bluey is reading docs, checking saved memory, using the screen, or searching the web
- attach sources when outside information was used

Today, Bluey often has to choose between "answer from supplied context" and "say what is missing." ChatGPT/Claude-style behavior requires a middle lane: "context is missing, but this is a public/current fact, so retrieve it safely and answer with sources."

## Proposed Pipeline

1. Add an `AnswerPlan` step before provider streaming.
   - Classify intent: quick answer, follow-up, coding/debugging, system design, writing, meeting recap, screen analysis, public/current research, or missing-context recovery.
   - Decide desired shape: compact overlay answer, structured answer, canvas artifact, or sourced research answer.
   - Decide evidence needs: transcript, current screen, selected attachments, local/cloud RAG, web search, or user clarification.

2. Build an evidence bundle.
   - Keep the existing hot context first: visible screen, attached files, current transcript, current session summary.
   - Add local/cloud RAG as bounded background memory.
   - Add web search only when the plan says it is useful and allowed.
   - Mark evidence by source type so the final answer can say "from screen," "from attached docs," or cite external sources only when appropriate.

3. Add managed web search on the server, not inside the overlay.
   - Use a search API or provider-native web retrieval behind the managed server.
   - Build sanitized search queries from the user's public-facing question. Do not send private transcript/file contents as raw search terms unless the user explicitly asks and the query is safe.
   - Fetch only bounded snippets or selected pages through a safe fetcher.
   - Strip page instructions and treat fetched content as untrusted evidence.
   - Return short source metadata: title, URL, snippet, fetch time, and confidence/relevance.

4. Compose answers with templates.
   - Quick factual: direct answer, one short reason, source if web was used.
   - Research/current info: answer first, then cited bullets, then caveat/date if relevant.
   - Coding/debugging: approach, patch or changed block, explanation, edge cases.
   - System design: recommendation first, then architecture/detail in canvas.
   - Missing context: say exactly what is missing and ask for the smallest next input.
   - Overlay stays compact; canvas/details can hold the fuller ChatGPT-style structure.

5. Stream retrieval status before the final model deltas.
   - `Using screen context`
   - `Reading attached docs`
   - `Checking saved Bluey memory`
   - `Searching web`
   - `Found 3 sources`
   - Then stream the answer.

6. Add source chips and citations.
   - Overlay answer can show small source chips when web or docs are used.
   - Canvas/detail view can show full source list.
   - Do not show internal RAG ids unless the user explicitly asks for source/debug detail.

## Web Search Guardrails

- Server-side only for production accounts.
- Account-level rate limits, spend guards, idempotency, and per-day search quotas.
- No arbitrary browser automation from the daemon.
- Query sanitation to avoid leaking private files, transcript fragments, emails, keys, or hidden prompts.
- SSRF protections for any page fetcher: block private IP ranges, localhost, cloud metadata hosts, unsupported protocols, and oversized responses.
- Domain allow/deny support for risky or low-quality domains.
- Cache search results briefly to reduce cost and duplicate calls.
- Prompt-injection boundary: web pages are evidence, not instructions.
- Cite source URLs only after filtering and size caps.
- Keep answer generation separate from source retrieval so failures degrade to "I could not verify this live" instead of duplicate sends or blank answers.

## Mac Windows Parity

Most of this is daemon/server behavior and therefore applies to both macOS and Windows automatically.

UI parity is still required for:

- retrieval status rows
- source chips
- citations or source drawer
- compact overlay versus canvas/detail behavior
- any new answer mode controls

Mac and Windows should receive the same overlay event schema before either platform ships web-search answers.

## Implementation Order

1. `AnswerPlan` without web search.
   - Add intent/evidence/shape planning.
   - Improve answer formatting using existing context only.
   - Add tests for quick, coding, screen, missing-context, and follow-up questions.

2. Retrieval status events.
   - Extend stream events with retrieval/status/source metadata.
   - Show the same status states on macOS and Windows.

3. Managed web search beta.
   - Add server-side search provider abstraction.
   - Add account quotas, cost accounting, cache, safe fetch, and source metadata.
   - Use it only for explicit web requests or clearly current/public facts.

4. Sourced answer composer.
   - Add citation-aware prompt sections.
   - Add source chips in overlay and source list in canvas/detail.
   - Add tests that sourced answers cite web results and private context is not leaked into search queries.

5. Evaluation harness.
   - Golden prompts for coding, interview, screen, attachments, current-events, missing context, multi-image, and follow-up cases.
   - Track answer organization, source correctness, no duplicate sends, no empty transcript auto-send, and first-token latency.

## Verification

Code inspection only, no product code changed in this round.

Checked:

- `crates/cue-daemon/src/app.rs` answer card, prompt, formatting, context, and managed provider paths
- `crates/cue-daemon/src/llm/answer.rs` legacy/simple answer prompt path
- `server/src/api/router.rs` managed streaming and cloud RAG retrieval path
- `server/src/routing/dispatcher.rs` model routing surface
- `crates/cue-core/src/ai.rs` answer stream event types
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift` overlay/canvas/source-adjacent UI surface
- `native/windows/cue-overlay/main.c` Windows overlay status/context surface

No compile/test run was needed because this round is an architecture and product plan.

## Current Recommendation

Start with the answer planner and formatting pass before adding web search. That gives immediate ChatGPT/Claude-style clarity from existing context and reduces the risk that web retrieval hides underlying context-routing bugs.

Then add web search as a managed, metered, cited retrieval lane. It should not be a local overlay capability, and it should not search by default for every question.

The target user experience is:

- simple questions answer instantly
- screen/doc questions say exactly what context was used
- current/public questions can search and cite sources
- missing-context questions ask for the smallest next input
- longer structured answers open cleanly in canvas/detail instead of flooding the overlay

## Remaining Gates

- Decide which search provider to use for the first beta.
- Define product pricing for search calls under paid credits.
- Add privacy copy that explains when Bluey searches the web and what is sent.
- Build source chips/status UI on both macOS and Windows.
- Add e2e tests that prove search does not duplicate sends, does not send empty transcript, and does not leak private context into external queries.
