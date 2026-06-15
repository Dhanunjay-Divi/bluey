# SDK Unification Investigation — Can One Vendor SDK Drive CLI + GUI + Cloud?

> **Status:** Architecture investigation (READ-ONLY). No code in `crates/` was changed by this document.
> **Date:** 2026-06-05
> **Scope:** Six vendors — Claude (Anthropic), Cursor, OpenAI Codex, GitHub Copilot, Google Gemini, Google Antigravity.
> **Question driving this:** The product owner's hypothesis that "a single vendor SDK is one unified control surface for CLI mode AND GUI mode AND cloud mode," which — if true — would let Bluey lean on one SDK per vendor instead of hand-wiring three transports (local CLI, local GUI bridge, cloud REST).
> **Why this matters:** Bluey got burned earlier by an unverified assumption (`claude -p --resume` could cheaply continue a big GUI session — it couldn't; it overflowed because resume re-loads the on-disk transcript). This document does not repeat that mistake: every claim is sourced, and unverifiable claims are marked `UNVERIFIED`.

---

## TL;DR

**The SDK does NOT unify the three transports in the way that matters, and — critically — no vendor SDK can attach to an already-running GUI session.** Across all six vendors, the "local" SDK mode does exactly what Bluey's CLI path already does: it **spawns a fresh headless engine and resumes context by replaying the on-disk transcript**, not by attaching to the warm, in-memory, already-compacted session the user has open in the desktop app / IDE. The Claude Agent SDK is explicit that sessions live at `~/.claude/projects/<encoded-cwd>/*.jsonl` and resume reads from that file — the exact same store Bluey's `claude_app.rs` reader already reads, and the exact overflow trap the owner already hit. The "attach to a running session" capability is a **filed-but-unshipped feature request** for Codex (issue #11166), and is **undocumented / absent** for Claude, Cursor, Gemini, and Antigravity. **Therefore the SDK is, for the GUI-attach problem, just a nicer CLI — it solves nothing Bluey's CLI path doesn't already solve.** Separately, the SDKs *are* genuinely useful for the **cloud** surface and for a clean local agent loop, and one finding changes the language calculus: **GitHub's Copilot SDK shipped a first-party Rust SDK at GA (2026-06-02)** — the only native-Rust agent SDK among the six. **Recommendation: keep the cloud layer as raw-REST-in-Rust (Option A) — it was the right call and the SDK finding does not invalidate it — and do NOT adopt a Node/Python sidecar to "solve" GUI-attach, because no SDK actually offers GUI-attach. The warm-GUI-session problem is unsolved by every vendor today; Bluey should treat it as an open industry gap, not something a sidecar buys.**

---

## The one-sentence answer to each question

- **Q1 (does the SDK unify CLI+GUI+cloud?):** No. "Local" and "cloud" are usually separate entry points or separate products under one brand; "GUI" is never a real SDK target — the SDK spawns its own engine, it does not drive the user's open app.
- **Q2 (can the SDK attach to a running GUI session?):** **No, for every single vendor.** All offer at most resume-from-disk (replay), which is what burned the owner before. Codex has the closest architecture (a JSON-RPC app-server) but multi-client attach to a live session is an open feature request, not shipped.
- **Q3 (language reality):** All are TypeScript and/or Python — **except GitHub Copilot, which now ships a first-party Rust SDK.** Everything else has a documented REST/JSON-RPC protocol Rust can wrap directly.
- **Q4 (options):** Recommend **Option A (raw-REST-in-Rust)** as the spine, with a narrow, *optional* exception to adopt the **Copilot Rust SDK** (Option C, but Rust-native, so no runtime-dep cost). Reject the Node/Python sidecar (Option B) — it costs the corporate "one .app" story and buys no GUI-attach.
- **Q5 (does this change what we just built?):** No. The raw-REST-in-Rust cloud layer is correct and should stand. The SDK question only ever mattered for GUI-attach, and GUI-attach is unsolved by SDKs anyway.

---

## Per-vendor matrix (Q1 / Q2 / Q3)

> Legend for **Q2 (the critical column)**:
> 🔴 **Spawn-fresh / resume-from-disk only** — no warm-session attach. Equivalent to Bluey's existing CLI path; no GUI benefit.
> 🟡 **Architecturally close** — a daemon/RPC exists, but live multi-client attach is unshipped/unverified.
> 🟢 **True attach to a running GUI session** — none qualify.

| Vendor | **Q1 — Unifies CLI/GUI/cloud?** | **Q2 — Attach to a RUNNING GUI session? (CRITICAL)** | **Q3 — Languages / Rust? / raw protocol** |
|---|---|---|---|
| **Claude (Anthropic) — Claude Agent SDK** | **Partial, and not via GUI.** `query()` (TS) / `query()` + `ClaudeSDKClient` (Py) spawn/control a **local** Claude Code engine. The SDK does **not** target the desktop app as a surface. Cloud is a **separate product** ("Claude Managed Agents," `api.anthropic.com/v1/sessions`, beta header), not a mode-switch on the same client. ([sessions doc](https://code.claude.com/docs/en/agent-sdk/sessions), [TS ref](https://platform.claude.com/docs/en/agent-sdk/typescript)) | 🔴 **No.** Resume reads the **on-disk transcript**: *"Sessions are stored under `~/.claude/projects/<encoded-cwd>/*.jsonl`"* and *"The session file also needs to exist on the current machine."* There is **no attach-to-running-app API**; `ClaudeSDKClient` only holds a session *within its own process*. Resuming a large session re-loads that transcript → **this is exactly the overflow the owner already hit.** ([sessions doc](https://code.claude.com/docs/en/agent-sdk/sessions)) The experimental V2 `createSession()` streaming API was **removed** in 0.3.142. **Correction (verified 2026-06-14):** the SDK *does* now ship session-enumeration helpers — `listSessions()` / `getSessionInfo()` / `getSessionMessages()` / `renameSession()` / `tagSession()`, returning `SDKSessionInfo { sessionId, summary, customTitle, firstPrompt, cwd, gitBranch, createdAt, lastModified }` — which read the **same on-disk `~/.claude/projects/<encoded-cwd>/*.jsonl` store Bluey already reads natively**. This does **not** change the recommendation (it *reinforces* it): the SDK is still a TS/Python wrapper over the very files our Rust readers parse, so adopting it would only add a runtime dependency for data we already extract — and it still offers **no GUI-session attach**. Bluey's native readers already surface session id, title (incl. `aiTitle`/`customTitle`), and `cwd`; the SDK adds nothing here. | **TS** (`@anthropic-ai/claude-agent-sdk`) + **Python** (`claude_agent_sdk`). **No Rust SDK.** Underneath: the SDK drives the `claude` binary locally; cloud is the documented Anthropic **Messages/Sessions REST API** (what `cloud/anthropic.rs` already wraps). ([Py ref](https://code.claude.com/docs/en/agent-sdk/python)) |
| **Cursor — `@cursor/sdk`** | **Yes-ish, but local≠GUI.** *"the same agents that power the desktop app, CLI, and web app … with a few lines of TypeScript."* One `Agent.create()` with a **mode switch**: `local: { cwd }` vs `cloud: { repos }`. ([SDK blog](https://cursor.com/blog/typescript-sdk)) But "local" = a fresh headless run in a cwd, **not** the IDE's open conversation. | 🔴 **No.** No API to attach to the **running Cursor IDE** session. The integration goes the *other* direction: *"cloud agent runs show up in Cursor's Agents Window … start a task programmatically and then jump into Cursor to inspect."* You can hand a job *to* the GUI; you cannot read the GUI's warm in-memory session *out*. ([SDK blog](https://cursor.com/blog/typescript-sdk)) Bluey's existing `vscdb.rs` already reads Cursor's on-disk store — the SDK adds nothing here. | **TypeScript only** for the SDK; *"Python users should call the Cloud Agents REST API directly."* **No Rust SDK.** Raw protocol: **Cloud Agents REST API** (`api.cursor.com`) — what `cloud/cursor.rs` already wraps. ([SDK blog](https://cursor.com/blog/typescript-sdk), [forum](https://forum.cursor.com/t/cursor-sdk-cloud-agents-api-updates/159284)) |
| **OpenAI Codex — `@openai/codex-sdk` + App Server** | **Local and cloud are SEPARATE.** The SDK *"controls the **local** Codex app-server over JSON-RPC"* — **local-only**. Codex **Cloud** is a distinct **REST** surface (`/v1/codex/cloud/tasks`). The App Server is the unifying *protocol* (JSON-RPC 2.0) but the cloud uses it *inside its own container*, not as a client endpoint you reach over the internet. ([SDK](https://developers.openai.com/codex/sdk), [App Server](https://developers.openai.com/codex/app-server), [cloud](https://developers.openai.com/codex/cloud)) | 🟡 **Closest of all six — but still NO today.** The app-server is a long-lived JSON-RPC process with `thread/loaded/list` ("thread ids **currently loaded in memory**") and `--listen unix://` / `ws://`. **But** attaching an *external* client to a session another process (the IDE) is actively running is an **open feature request**: issue #11166 — *"let the app-server listen on a Unix socket or TCP port so that remote clients can attach to running Codex sessions"* — i.e. **not shipped**; *"each client currently spawns its own isolated app-server."* `thread/resume` reopens a thread **by reading the persisted rollout file**, not by joining live in-memory state. ([README](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md), [issue #11166](https://github.com/openai/codex/issues/11166)) | **TS** (`@openai/codex-sdk`) + **Python** (`openai-codex`). **No first-party Rust SDK** — *but Codex's core is Rust* (`codex-rs`) and the protocol types are dumpable (`codex app-server generate-ts`, `generate-json-schema`). Raw protocol: **JSON-RPC over stdio/unix/ws** (local) + **REST** (cloud, wrapped by `cloud/codex_cloud.rs`). ([App Server](https://developers.openai.com/codex/app-server)) |
| **GitHub Copilot — Copilot SDK (GA 2026-06-02)** | **Yes — genuinely the broadest.** One SDK, *"the same engine behind Copilot CLI."* Bundles the Copilot CLI and talks **JSON-RPC**. Supports **both** local runtime **and** *"Cloud and remote sessions: create cloud-backed sessions with repository metadata or enable remote session URLs on demand."* ([GA changelog](https://github.blog/changelog/2026-06-02-copilot-sdk-is-now-generally-available/), [blog](https://github.blog/news-insights/company-news/build-an-agent-into-any-app-with-the-github-copilot-sdk/)) | 🔴 **No.** Copilot has **no persistent local GUI session** to attach to in the first place (the "GUI" is the IDE extension/VS Code Chat, and the cloud agent runs in GitHub Actions). The SDK **creates** sessions (`createSession()`); **no attach-to-running-session** is documented. So there is no warm-GUI-session to capture — the question is moot for Copilot, not solved. ([GA changelog](https://github.blog/changelog/2026-06-02-copilot-sdk-is-now-generally-available/)) | **Node/TS, Python, Go, .NET, Java, and — new at GA — RUST** (`cargo add github-copilot-sdk`, *"bundles the Copilot CLI binary by default"*). **This is the only first-party Rust agent SDK among the six.** Raw protocol: **JSON-RPC to the bundled CLI** (local) + GitHub **Agent Tasks REST API** (cloud, wrapped by `cloud/copilot.rs`). ([GA changelog](https://github.blog/changelog/2026-06-02-copilot-sdk-is-now-generally-available/), [agent-tasks REST](https://github.blog/changelog/2026-05-13-start-copilot-cloud-agent-tasks-via-the-rest-api/)) |
| **Google Gemini — Headless Coder SDK / `@google/gemini-cli-core` + ACP** | **Local headless + ACP; cloud is a separate API.** Gemini CLI runs **headless** (`@google/gemini-cli-core`), and the new **Headless Coder SDK** wraps it with *"full ACP compatibility … a unified API for Codex, Claude Code, and Gemini."* Cloud generation is the **separate** Gemini API (`ai.google.dev`). Not one mode-switching client. ([discussion #12794](https://github.com/google-gemini/gemini-cli/discussions/12794), [headless docs](https://google-gemini.github.io/gemini-cli/docs/cli/headless.html)) | 🔴 **No.** ACP's model is: *"the editor **spawns** the agent as a **subprocess** … JSON-RPC over **stdio**."* `loadSession: true` **reconstructs a session from disk**, it does not join a live process. Remote/HTTP transport (which *could* enable attach) is *"currently a proposal … on the roadmap."* So Gemini = spawn-fresh + replay-from-disk. ([ACP repo](https://github.com/agentclientprotocol/agent-client-protocol), [Kiro ACP](https://kiro.dev/docs/cli/acp/)) | **TypeScript/Node** (`@google/gemini-cli-core`, Headless Coder SDK packages). General Gemini model access also has a **Python** SDK (`google-genai`). **No Rust SDK.** Raw protocol: **ACP = JSON-RPC over stdio** (local) + **Gemini REST API** (cloud). ([npm](https://www.npmjs.com/package/@google/gemini-cli-core), [ACP](https://github.com/agentclientprotocol/agent-client-protocol)) |
| **Google Antigravity — Antigravity SDK (2.0, I/O 2026)** | **Three SEPARATE surfaces.** Explicitly *"the third surface … alongside the Desktop app and the `agy` CLI."* The SDK is **managed-cloud-first**: *"Google provisions a secure Linux sandbox … you do not need to manage infrastructure."* Not one client spanning GUI+CLI+cloud. ([SDK blog](https://antigravity.google/blog/introducing-google-antigravity-sdk), [MarkTechPost](https://www.marktechpost.com/2026/05/19/google-launches-antigravity-2-0-at-i-o-2026-a-standalone-agent-first-platform-with-cli-sdk-managed-execution-and-enterprise-support/), [aimadetools](https://www.aimadetools.com/blog/antigravity-sdk-custom-agents-guide/)) | 🔴 **No.** The SDK *"only describes creating new programmatic agents via API calls"*; **no documented way to attach to a running Desktop app session.** Execution is Google-hosted managed sandboxes — the opposite of reading the user's warm local IDE session. ([aimadetools](https://www.aimadetools.com/blog/antigravity-sdk-custom-agents-guide/)) `UNVERIFIED — assumption:` Antigravity Desktop still writes a local transcript store (Bluey's `protobuf.rs` targets `*.pb`); the SDK does not expose it. | **Python** (`from antigravity import AntigravityClient`, Pydantic v2 models). **No Rust SDK; no documented standalone REST** for third parties beyond the Python SDK + *"Managed Agents in the Gemini API."* `UNVERIFIED — assumption:` a REST surface exists under the Python SDK but is not publicly documented for direct Rust wrapping. ([SDK blog](https://antigravity.google/blog/introducing-google-antigravity-sdk), [aimadetools](https://www.aimadetools.com/blog/antigravity-sdk-custom-agents-guide/)) |

### The single most important row, restated plainly

For **all six vendors**, the strongest "local" capability the SDK offers is **resume-by-ID, which replays the conversation from the on-disk transcript**. Not one of them can hand Bluey a handle to the *live, in-memory, already-compacted* session the user has open in the desktop app or IDE. The Claude SDK says so in the clearest possible terms (resume reads `~/.claude/projects/.../*.jsonl`, which is **the same file Bluey already reads**, and which **already overflows** on big sessions). Codex is architecturally closest (a real JSON-RPC daemon), but the multi-client attach is a filed feature request (#11166), not a shipped capability.

---

## Q4 — Integration options + recommendation

Bluey's stated goals: **dynamic, scalable, production-grade, secure, lean, no-runtime-dependency, corporate-distributable ("download one .app").**

### Option A — Wrap raw REST/JSON-RPC in Rust (what `crates/cue-agent-bridge/src/cloud/` already does)

- **Cost:** Medium per-vendor. You reimplement request/response/auth/error parsing the SDK would give for free; you track each vendor's REST/protocol drift yourself. The shared primitives already built (`transport.rs`, `keychain.rs`, `audit.rs`, the data-driven `CLOUD_REGISTRY`) amortize this well — a new vendor is a new adapter file + a registry row, not a new code path.
- **Unlocks:** Everything stays in **one Rust binary**. No Node/Python runtime in the `.app`. Single supply chain to audit. Tokens stay in the OS keychain (`keychain.rs`); every call is audited (`audit.rs`). Fully aligned with "lean / secure / corporate-distributable." Cloud vendors are *task/session REST shapes* — exactly what raw-REST models cleanly.
- **Sacrifices:** No SDK-maintained client (you own protocol changes). **Does NOT get GUI-session attach** — but **neither does any SDK**, so this is not a real loss versus Option B.

### Option B — Embed a Node/Python sidecar running the vendor SDKs; Rust talks to it over a local socket

- **Cost:** **High and ongoing.** Ship and sandbox a Node and/or Python runtime inside the `.app`; manage its lifecycle, version skew, and a much larger dependency/CVE surface; cross the IPC boundary for every call. Directly damages the "download one .app, no runtime deps" corporate story (notarization, size, AV false-positives on a bundled interpreter).
- **Unlocks:** The full SDK surface — *if* a vendor's SDK uniquely offered something Rust can't get from REST. The decisive question is whether that "something" includes **GUI-session attach.** Per Q2, **it does not, for any vendor.** The SDKs' "local" mode is spawn-fresh/resume-from-disk — which Bluey's **CLI path already does** without a sidecar.
- **Sacrifices:** Leanness, security posture, and the single-binary distribution — for a benefit (GUI-attach) that **doesn't exist**. The only genuine win would be a slightly faster path to a vendor's complex *local agent loop* (multi-turn tool orchestration), but Bluey's product is "drive the user's agent for one answer," not "host a long agent loop," so this is low-value.

### Option C — Hybrid: Rust-REST as the spine, an SDK only where it *uniquely* unlocks something

- **Cost:** Low-to-medium, *if* the exception is itself Rust-native (no runtime dep). High if the exception is a Node/Python SDK (then it collapses into Option B's costs for that vendor).
- **Unlocks:** The pragmatic best case. Concretely, the only SDK that is both (a) first-party and (b) **native Rust** is the **GitHub Copilot Rust SDK** (`cargo add github-copilot-sdk`). Adopting it for Copilot would give a vendor-maintained client *with zero runtime-dependency cost* (it's a crate that bundles the CLI binary) — strictly better than hand-wiring Copilot's JSON-RPC ourselves, and it keeps the single-binary story.
- **Sacrifices:** A second integration style (SDK crate for Copilot, raw-REST for the rest) — a small consistency tax, contained because it's still Rust.

### RECOMMENDATION

**Adopt Option A as the architectural spine, with a single, optional, Rust-native exception (Option C) for GitHub Copilot. Explicitly reject Option B (the Node/Python sidecar).**

Reasoning:

1. **The premise that motivated the sidecar is false.** The whole appeal of embedding vendor SDKs was "one SDK unifies CLI+GUI+cloud, including attaching to the warm GUI session." **Q2 shows no vendor SDK can attach to a running GUI session.** So the sidecar's marquee benefit evaporates, while all its costs (runtime dep, larger attack surface, broken "one .app" story) remain. For a product whose differentiator is "lean, secure, corporate-distributable," that trade is clearly wrong.

2. **Raw-REST-in-Rust is the correct shape for the cloud surface specifically.** Cloud agents are task/session REST resources (create task → poll/stream → artifact). That maps perfectly onto the existing `transport.rs` + data-driven `CLOUD_REGISTRY`. An SDK would add a runtime to do what `reqwest` already does in one binary.

3. **The one place to bend is Copilot — and only because the bend is free.** GitHub shipped a **Rust** SDK at GA that bundles its CLI and reaches both local and cloud sessions. Because it's a crate, not a sidecar, it imposes no runtime dependency and preserves the single-binary distribution. It's worth evaluating as a *replacement for hand-wiring Copilot's JSON-RPC*, not as a precedent for embedding TS/Python SDKs. (Keep `cloud/copilot.rs`'s REST path as the fallback; treat the crate as an optimization, not a dependency the build can't ship without.)

4. **The warm-GUI-session problem stays explicitly open.** No vendor solves it today. The honest posture: Bluey's GUI bridge should keep doing what it does — **read the app's on-disk transcript store** (`claude_app.rs`, `vscdb.rs`, `protobuf.rs`) for *context*, and drive a *fresh* headless run for the *answer* — and accept that "continue the exact warm in-memory IDE session" is not achievable through any current SDK. The closest future unlock is **Codex's app-server multi-client attach (issue #11166)**; if/when that ships, it would be the first real path to true session-attach, and it's a **JSON-RPC-over-unix-socket** protocol Rust can speak directly (via `codex app-server generate-json-schema`) — again **no sidecar required.** Worth a watch item, not a bet.

---

## Q5 — Does the SDK finding change what we just built (the `cloud/` layer)?

**No. The raw-REST-in-Rust cloud layer was the right call and should stand unchanged.** Detailed verdict:

- **The cloud question and the SDK question are orthogonal.** The cloud adapters (`anthropic.rs`, `cursor.rs`, `codex_cloud.rs`, `copilot.rs`, and the Gemini/Antigravity adapters being added) wrap **task/session REST endpoints**. Every vendor exposes those endpoints as REST regardless of whether an SDK also exists — Cursor *tells Python users to call the REST API directly*; Codex Cloud is REST (`/v1/codex/cloud/tasks`); Anthropic Managed Agents is REST (`/v1/sessions`); Copilot cloud is the Agent Tasks REST API. The SDK would, at best, be a thin convenience wrapper over the same HTTP — and would cost a runtime to embed. **Wrapping REST in Rust is strictly leaner for the cloud surface.**

- **The SDK only ever mattered for GUI-attach, and GUI-attach is unsolved by SDKs.** The product owner's hypothesis was worth checking precisely because *if* an SDK could attach to the warm GUI session, it could justify reconsidering the architecture. It can't (Q2). So there is **no finding that retroactively invalidates** the cloud work.

- **One forward-looking adjustment, not a rewrite:** when the Copilot Rust SDK is evaluated (per Q4 Option C), the `cloud/copilot.rs` adapter is the natural place to *optionally* delegate to the crate while keeping the REST path as fallback. That's an enhancement contained to one adapter behind the existing data-driven dispatch — not a reconsideration of the layer's design. The vendor-agnostic `transport.rs`/`registry.rs` split already accommodates "this vendor has a richer client" without special-casing by name.

- **Net:** Ship the cloud layer as built. Do **not** introduce a Node/Python sidecar. Keep the GUI bridge on the read-transcript-from-disk + drive-fresh-headless model. File a watch item on Codex app-server attach (#11166) and the Copilot Rust SDK as the two developments that could *additively* improve things later — both speakable from Rust without a runtime dependency.

---

## Sources

**Bluey code reviewed (read-only):**
`crates/cue-agent-bridge/src/lib.rs`, `drive/cli.rs`, `cloud/mod.rs`, `cloud/transport.rs`, `sessions/claude_app.rs` (the on-disk two-hop reader confirming Bluey already reads `~/.claude/projects/<encoded-cwd>/<cliSessionId>.jsonl`).

**Claude (Anthropic):**
- Sessions (resume reads on-disk transcript; no attach): https://code.claude.com/docs/en/agent-sdk/sessions
- TypeScript ref: https://platform.claude.com/docs/en/agent-sdk/typescript
- Python ref: https://code.claude.com/docs/en/agent-sdk/python

**Cursor:**
- SDK blog (local/cloud switch; "jump into Cursor to inspect" = hand-to-GUI, not read-from-GUI; TS-only, Python uses REST): https://cursor.com/blog/typescript-sdk
- SDK / Cloud Agents API updates: https://forum.cursor.com/t/cursor-sdk-cloud-agents-api-updates/159284
- Headless CLI: https://cursor.com/docs/cli/headless

**OpenAI Codex:**
- SDK (controls the local app-server over JSON-RPC; TS + Python; local-only): https://developers.openai.com/codex/sdk
- App Server (JSON-RPC, transports, `thread/loaded/list`, `generate-ts`): https://developers.openai.com/codex/app-server
- App Server README: https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md
- **Issue #11166 — attach-to-running-session is a feature request, NOT shipped:** https://github.com/openai/codex/issues/11166
- Cloud (Web): https://developers.openai.com/codex/cloud

**GitHub Copilot:**
- **SDK GA — six languages incl. first-party Rust (`cargo add github-copilot-sdk`); cloud+remote sessions; JSON-RPC:** https://github.blog/changelog/2026-06-02-copilot-sdk-is-now-generally-available/
- Build-an-agent blog: https://github.blog/news-insights/company-news/build-an-agent-into-any-app-with-the-github-copilot-sdk/
- Agent Tasks REST API: https://github.blog/changelog/2026-05-13-start-copilot-cloud-agent-tasks-via-the-rest-api/

**Google Gemini:**
- Headless Coder SDK announcement (TS; ACP-compatible): https://github.com/google-gemini/gemini-cli/discussions/12794
- Headless mode docs: https://google-gemini.github.io/gemini-cli/docs/cli/headless.html
- `@google/gemini-cli-core`: https://www.npmjs.com/package/@google/gemini-cli-core

**Agent Client Protocol (ACP — Gemini/Codex/Claude interop):**
- Spec repo (editor spawns agent as subprocess over stdio; HTTP transport is a proposal): https://github.com/agentclientprotocol/agent-client-protocol
- Kiro ACP (`loadSession: true` reconstructs from disk): https://kiro.dev/docs/cli/acp/

**Google Antigravity:**
- SDK blog (third surface; Python; managed cloud): https://antigravity.google/blog/introducing-google-antigravity-sdk
- Launch coverage (separate surfaces; managed execution): https://www.marktechpost.com/2026/05/19/google-launches-antigravity-2-0-at-i-o-2026-a-standalone-agent-first-platform-with-cli-sdk-managed-execution-and-enterprise-support/
- SDK technical guide (Python `AntigravityClient`, managed sandbox, no desktop-attach): https://www.aimadetools.com/blog/antigravity-sdk-custom-agents-guide/

---

## Confidence & caveats

- **High confidence:** Claude (resume = on-disk replay, no attach) — stated verbatim in the official sessions doc. Codex (attach is unshipped) — confirmed by issue #11166. Copilot (Rust SDK exists at GA) — confirmed in the GA changelog. Cursor (TS-only SDK; integration is hand-to-GUI not read-from-GUI) — confirmed in the SDK blog.
- **Medium confidence:** Gemini Headless Coder SDK exact package surface and whether a stable first-party (vs community) wrapper is the canonical one — the ACP subprocess/stdio model is well-established, but the "Headless Coder SDK" naming may be community/early.
- **`UNVERIFIED — assumptions` (flagged inline above):** (1) Antigravity Desktop's local on-disk transcript format and whether any documented REST exists beneath the Python SDK; (2) whether Gemini's cloud and headless-local are ever exposed as a single mode-switching client (current evidence says no — they're separate). None of these `UNVERIFIED` points change the recommendation, because the recommendation hinges on Q2 (GUI-attach), which is answered with high confidence as **"no vendor offers it."**
