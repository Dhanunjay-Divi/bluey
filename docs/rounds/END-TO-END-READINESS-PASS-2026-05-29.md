# End-To-End Readiness Pass - 2026-05-29

This pass treats Bluey as a real customer-facing AI app, not a pile of feature
branches. It captures the expected user journey, the runtime contracts that
must hold, and the known gates for tomorrow's managed test.

## Customer Journey Contract

```text
install -> bluey on -> compact pill -> click pill -> expanded overlay
-> optional login -> listen / ask / attach / screen -> streamed answer
-> cost + balance label -> session saved locally -> cloud sync when logged in
```

Rules:

- `bluey on` starts a fresh recording and shows the pill. It must not force-open
  a browser. If the user is not linked, the overlay and terminal show
  `bluey login` / `https://bluey.sh/link` as an optional next step.
- The pill is the default shell. Clicking opens the expanded overlay; Hide goes
  back to pill; Close asks to turn Bluey off and tells the user to run
  `bluey on` again.
- Live captions are context. They do not auto-send an answer by default. The
  user sends by Enter/Send or Screen/Answer. Auto-submit can be a future explicit
  mode, but default live-call behavior must not surprise users.
- Typed/sent user text belongs on the right side of the chat stream. Bluey
  answers stream on the left side. Transcript preview stays in the fixed live
  caption strip and should not inflate the panel.
- Canvas opens only when the answer carries or infers a real artifact:
  code, system design, diagram/screen analysis, document rewrite, or structured
  plan. Follow-ups should refine/replace the existing answer/canvas artifact
  instead of appending random duplicate cards.

## Expanded Product Thesis

Bluey is not limited to interview-style use. The product should become a live
meeting intelligence layer for engineering and work teams:

```text
before meeting: attach workspace / repo / approved agent session
during meeting: transcript -> intent detection -> relevant context card
after meeting: decisions/actions -> approved connector or agent pushback
```

The key unlock is that users already run coding agents and workspace tools that
know their repos, docs, tickets, branches, and deployments. Bluey should bridge
that context into live conversations instead of forcing people to tab through
tools mid-meeting.

The research-paper direction is now part of the product as Context
Intelligence. Bluey should show what context it has, what is missing, and which
approved source will improve the answer. The research harness proves those
signals in controlled mock IDE/work-app environments and feeds the customer
feature.

Initial scope should stay consent-based:

- local folder/repo attachment
- approved docs and project files
- visible screen/page context
- cloud/team RAG
- context coverage/source cards
- later: local coding-agent session/MCP context bridges

Do not make the product depend on hidden third-party scraping, raw browser
cookies, or proctor/assessment bypasses.

## What This Pass Fixed

- Unsupported picker selections are now rejected before attachment ingestion.
  Even if the OS picker returns a `.mp4`, `.mov`, `.wav`, `.p12`, or another
  non-readable file, the daemon skips it and shows a warning card instead of
  letting it become a context chip.
- Deepgram endpointing and utterance-end timing can now be disabled explicitly
  with `BLUEY_DEEPGRAM_ENDPOINTING_MS=off` or
  `BLUEY_DEEPGRAM_UTTERANCE_END_MS=off`. This lets us A/B test local VAD-only
  versus provider endpointing without changing code.
- The Mac smoke doc now spells out the send behavior, unsupported-file behavior,
  and the fixed-size/cropped-header regression guard.
- `docs/MODEL-ROUTING.md` now states the product stance: Auto in the UI,
  server-owned provider choices behind the scenes, and a route-config table as
  the next clean way to test Gemini/newer Claude/newer OpenAI routes.

## Tomorrow's Must-Pass Tests

Run `docs/deploy/PHASE2-MAC-SMOKE.md` from top to bottom. Stop at first failure.

Highest-risk gates:

1. **Pill/expanded overlay:** header, balance, model route, Hide, Close, and
   composer controls must be visible and uncropped on the user's screen.
2. **Listen:** transcript preview must scroll within its strip and never resize
   the window.
3. **Ask:** question right, streamed answer left, cost label visible, canvas
   opens only when an artifact is present.
4. **Attach:** supported docs attach as chips; unsupported files are disabled or
   skipped with a warning.
5. **Screen:** overlay must be excluded from capture in production mode.
   `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1` is dev-only and must never ship.
6. **Style:** inline style text saves and affects the next answer.
7. **Sessions:** drawer lists old sessions, loads them, and supports inline
   rename.
8. **Balance/cloud:** `bluey usage`, overlay balance, per-answer cost, cloud
   sync, and RAG retrieval agree after real managed calls.

## Provider/Infra Gates From User

Needed for true managed smoke:

- `BLUEY_PUBLIC_URL=https://bluey.sh`
- `BLUEY_JWT_SECRET`
- `OPENAI_API_KEY` or approved `OPENAI_API_KEYS`
- `ANTHROPIC_API_KEY` or approved `ANTHROPIC_API_KEYS`
- `DEEPGRAM_API_KEY` or approved `DEEPGRAM_API_KEYS`
- Redis URL for shared provider capacity before more than one server instance
- Postgres/pgvector for cloud sessions/RAG when we move beyond one-node SQLite
- Square/Stripe billing credentials for the chosen billing path
- SMTP provider for email verification/password reset

Do not put provider keys in the desktop. Production keys live on the server.

## Scale Contract

- Paying usage is constrained by wallet balance and available provider capacity,
  not arbitrary per-account throttles.
- Edge limits protect unauthenticated abuse. Provider/model/key buckets protect
  upstream allocations. Redis makes those buckets global across server
  instances.
- On 429/capacity, Bluey should cool down the exact provider/model/key, try the
  next approved key, then try the next lane candidate before showing an outage.
- Local fallback is for dev/offline/degraded UX, not a replacement for managed
  cloud quality.

## Known Deferred Work

- Real upstream-token streaming through `bluey-server`; current managed SSE can
  chunk after completion, but true provider streaming is a separate server stage.
- Server-owned route config table so model/provider changes do not require a
  desktop rebuild.
- Gemini route adapter and pricing table entries after managed smoke proves the
  current path.
- Meeting agent-context bridge: local workspace/repo indexing first, then
  user-approved coding-agent/MCP session context and proactive meeting cards.
- Context Intelligence Lab: coverage meter, missing-context prompts, source
  cards, and controlled research harness for screen/workspace/connector context
  quality.
- Clean Windows QA: overlay parity, WASAPI audio, whisper.cpp, installer, and
  capture-exclusion verification.
- Full cloud RAG on Postgres/pgvector/object storage; local SQLite remains the
  desktop cache.

## Next Agent Prompt

Read `AGENT-HANDOFF.md`, then this file, then
`docs/deploy/PHASE2-MAC-SMOKE.md`. Do not stage `bluey-dev.db`. First priority:
run the Mac smoke with a managed account and capture screenshots/log fragments
for any failure. Second priority: implement the server-owned route config table
and true managed SSE streaming.
