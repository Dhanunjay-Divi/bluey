# Bluey Agent Handoff

Last updated: 2026-06-02

> **Codex preflight:** Load `$bluey-ops` from
> `/Users/uno/.codex/skills/bluey-ops/SKILL.md` before continuing this handoff.
> Reconcile its memory against the current branch, code, and newest docs.

Start here when joining the Bluey repo.

## Read First

1. `docs/rounds/END-TO-END-AGENT-CONTEXT-2026-05-25.md`
2. `docs/rounds/END-TO-END-READINESS-PASS-2026-05-29.md`
3. `docs/CONTEXT-INTELLIGENCE-LAB.md`
4. `docs/MODEL-ROUTING.md`
5. `docs/DEPLOYMENT-SCALING.md`
6. `docs/PRODUCTION-READINESS.md`
7. `docs/PRELAUNCH-CHECKLIST.md`
8. `docs/rounds/SESSION-KNOWLEDGE-RAG-2026-06-01.md`

The first file is the complete current context: product flow, architecture,
implemented state, provider routing, capacity policy, cloud/RAG/storage plan,
environment variables, QA commands, known gaps, and what to tell the next
agent.

## Snapshot

- Branch: `feat/phase-3-round-12`
- Code tip before this docs refresh:
  `38d982b feat(audio): tune VAD and managed thinking budgets`
- Product domain target: `bluey.sh`
- Working tree warning: `bluey-dev.db` may appear as local untracked data. Do
  not stage it.

## Product Direction

Bluey is a managed desktop AI overlay for live work and meetings. The broader
product thesis is a live meeting intelligence layer that brings approved
coding-agent/workspace context into conversations, not an interview-only helper.

Customer flow:

```text
install -> bluey on -> link if needed -> compact pill -> expanded overlay
-> listen / attach / analyze screen / ask -> streaming answer + saved session
```

The CLI is the launch/support surface. The overlay is the product surface.

Strategic direction:

- Near term: screen-native assistant for transcript, screen, docs, projects,
  sessions, and managed answers.
- Next product wedge: meeting intelligence for engineering teams. Bluey listens
  to meetings and proactively surfaces relevant repo/docs/ticket/PR/deploy
  context.
- Long term: user-approved agent context bridge for Cursor, Claude Code, Codex,
  Kiro, Copilot/Gemini CLI-style workflows and MCP-style connectors. Do not
  build around hidden platform scraping or raw browser cookie capture.
- Research-to-product loop: Context Intelligence Lab measures which sources
  Bluey has, what is missing, and which safe user-approved source should be
  attached next. This becomes both a product feature and the trust/research
  paper.

## Current Architecture

- Desktop: `bluey`, `bluey-daemon`, native macOS overlay/audio/whisper helpers,
  local SQLite cache, keychain tokens.
- Server: `bluey-server` in `server/`, with auth, device linking, wallet,
  Stripe billing, managed provider routing, sync/RAG, usage, metrics, GDPR
  export/delete, and STT relay/transcribe endpoints.
- Storage target: local SQLite cache plus cloud Postgres/pgvector/object
  storage/Redis.
- Provider keys are server-side only for production.

## Current Session Knowledge Flow

Implemented locally:

- Final transcript segments are indexed into local RAG as they arrive.
- User-approved context artifacts are indexed into local RAG when their
  `text_preview` is ready. This includes CLI `bluey context add`, overlay
  document attach, page capture, screenshot/OCR summaries, and source-file
  previews that flow through `attach_context_artifacts`.
- Attached artifacts are indexed with source labels in the chunk text:
  title, kind, path, and optional note. That keeps retrieved snippets
  self-explanatory even before richer source metadata columns exist.
- Answer assembly uses bounded context:
  - recent transcript turns: max 32 turns / 8,000 characters,
  - recent Q&A,
  - latest attached artifacts first,
  - top current-session RAG hits,
  - top older-session local RAG hits.
- Prompt compaction is only provider-window compaction. It does not delete the
  raw local session, transcript, response, artifact, or RAG records.

Managed cloud path now present:

- Cloud sync uploads `context_artifacts` and `rag_chunks`.
- The server has tenant-scoped sync/RAG query routes.
- Managed `/router/complete` accepts `session_id`, queries account-scoped cloud
  RAG, boosts current-session hits, and silently enriches the upstream prompt
  with up to 6 user-approved snippets. If RAG lookup fails, the paid answer
  still proceeds without retrieved context.
- Next cloud step: server-side embedding/vector search with pgvector or
  equivalent, first-class source cards in the overlay, and deletion/retention
  guarantees that cover context artifacts plus RAG chunks.

## Current Model Routing

See `docs/MODEL-ROUTING.md` for source of truth.

- Instant: OpenAI `gpt-4o-mini`
- Balanced: Anthropic `claude-3-5-sonnet-latest`
- Deep: Anthropic `claude-3-7-sonnet-latest`
- Vision/screen: OpenAI `gpt-4o`
- STT: Deepgram `nova-3`, OpenAI `gpt-4o-mini-transcribe` fallback, hidden
  LocalWhisper dev/offline fallback.

Do not expose `local` as a customer model. Do not expose `vision` as a normal
route choice when the Screen/Analyze action already implies vision.

## Current Capacity Policy

Bluey should not punish paying users for high usage. Normal usage is controlled
by account-credit balance and provider capacity, not arbitrary per-account throttles.

Implemented:

- Provider/model buckets for OpenAI, Anthropic, Deepgram, OpenAI STT, embeds.
- Optional emergency per-account guardrails, disabled by default.
- Comma-separated provider key pools for approved capacity.
- Redis shared ledger support through `BLUEY_REDIS_URL`.
- Deepgram -> OpenAI fallback on `/router/transcribe`.

Next capacity hardening: run the Redis/shared-ledger path under real load with
approved provider key pools and verify `/admin/metrics` stays quiet for
cooldowns/all-keys-cooling events.

## Debug Flag Warning

`BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1` is local smoke-test only. It must never be
enabled in customer builds or production launch scripts.

## Release Gate Rule

Preprod is local. Use this Mac and the Windows bench for preprod build,
install, visual smoke, and version/hash/signature checks. Do not spend
GitHub Actions credits on preprod loops.

GitHub Actions is for production packaging/publishing only. Before prod,
precheck that every served/bundled surface matches the intended release
id and commit: `bluey --version`, daemon version, overlay/helper sidecar
hashes, release tarball SHA256, server `/health`, static web version,
and release signature/manifest status. For current unsigned macOS alpha,
ad-hoc `codesign` + SHA manifests are the required check; a future signed
manifest becomes mandatory once introduced.

Never rebuild between preprod and prod. If any version, hash, or
signature check mismatches, stop and cut a new release id.

## Full Verification Gate

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets --release
bash scripts/run-bluey-tests.sh all
(cd crates/cue-dashboard/ui && npm run build)
swift build -c release --package-path native/macos/cue-overlay
swift build -c release --package-path native/macos/cue-whisper
git -P diff --check
```

Use `docs/deploy/PHASE2-MAC-SMOKE.md` for visual/product smoke.

## Next Best Work

1. Run Mac smoke steps 1-10 with a real managed account/server path. Stop on the
   first failure and attach screenshot + daemon log excerpt.
2. Finish production-grade overlay UX nits found by smoke: compact pill,
   uncropped/resizable panel, scroll-contained transcript, ChatGPT-like
   composer, session drawer, attached-doc chips, style prompt, balance/cost
   labels, and canvas auto-open only for artifacts.
3. Add server-owned model/routing config so Gemini/newer OpenAI/newer Claude
   candidates can be tested without rebuilding desktop customers.
4. Add the first local workspace/repo context bridge: attach folder, build file
   tree/summaries, index into RAG, and cite source files in meeting answers.
5. Add Context Intelligence v1: source coverage model, missing-context prompt,
   and answer source cards.
6. Deploy `bluey.sh` static pages, host release artifacts, and run live
   account/payment smoke against the managed server.
7. Stand up staging/prod infra with live billing, SMTP, Caddy/TLS, backups, and
   monitoring.
8. Complete Windows QA before claiming Windows support.

Update 2026-06-02: the static `bluey.sh` route shell now exists in
`web/index.html` for `/`, `/link`, `/login`, `/reload`, `/account`,
`/verify-email`, `/password-reset`, `/docs/privacy`, `/docs/terms`, and
`/docs/disguise`. `/account` also lists synced cloud sessions from
`/sync/sessions` and can inspect a cloud session bundle. The remaining web work
is deployment, live account/payment smoke, hosted release artifacts, and the
desktop bridge that restores a cloud-only session into a local active
`MeetingRecord` so users can continue it from the overlay.

## Review Cadence

Use the Pinky-style round flow:

1. Implementation doc or recap.
2. Reviewer verdict doc.
3. Fix doc if red.
4. Merge/proceed only when the round is accepted or accepted with explicit nits.

Keep commits scoped. Document every meaningful product/backend/security decision
where a future agent would otherwise need to infer intent from the chat.
