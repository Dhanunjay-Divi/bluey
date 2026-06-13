# GitHub Copilot Coding Agent (Cloud) — Vendor Dossier

> Vendor: **GitHub Copilot Coding Agent** (formerly "Copilot Workspace" / SWE
> agent). Cloud agent that runs in GitHub Actions, kicked off by assigning a
> bot user to an issue or by calling the Coding-Agent task API directly.
> Last updated: 2026-06-05 (Bluey integration loop).

## Status report — TL;DR

### ✅ Complete + unit-tested (landed in `agent/agent-bridge`)

- **`AgentKind::CopilotCloud`** variant in `crates/cue-agent-bridge/src/lib.rs`,
  serde-renamed to `"copilot_cloud"` automatically — `parse_attached_agent`
  in the daemon resolves the new variant with **zero per-vendor code**,
  verified by `parse_attached_agent_maps_known_snake_case_labels`.
- **`KindTag::CopilotCloud`** in `crates/cue-agent-bridge/src/registry.rs`,
  with the `to_agent_kind` / `from_agent_kind` round-trip wired.
- **Registry row** at `crates/cue-agent-bridge/src/cloud/copilot.rs::ENTRY`
  declares: `vendor_short = "copilot_cloud"`, `base_url = "https://api.github.com"`,
  `billing_model = Subscription`, `task_shaped = true`,
  `max_task_duration_secs = 59 * 60`, and a 500-char consent_warning that
  the disclosure-UI test asserts mentions Actions / PAT / keychain.
- **Endpoint paths + static headers as DATA**
  (`CREATE_TASK_PATH`, `GET_TASK_PATH`, `LIST_TASKS_PATH`,
  `STATIC_HEADERS` pinning `accept: application/vnd.github+json` +
  `x-github-api-version: 2026-03-10`). All under unit-test pinning.
- **Per-vendor adapter helpers** — `CreateTaskRequest::to_json` (drops empty
  optionals, doesn't send `""`), `parse_task_response` (defensive, every
  field `Option`, errors only on missing `id`), `task_state_from_str`
  (covers the documented `queued|in_progress|completed|failed|idle|
  waiting_for_user|timed_out|cancelled` set + defensive `succeeded`/
  `canceled`/`error` aliases; unknown strings stay non-terminal),
  `reject_server_to_server_token` (refuses `ghs_*` installation tokens
  client-side before the request hits the wire), `is_copilot_bot_login`
  (normalizes both `copilot-swe-agent` and `copilot-swe-agent[bot]`),
  `render_acknowledgement` (the task-shaped synchronous-answer text).
- **Shared cloud primitives** in `crates/cue-agent-bridge/src/cloud/`:
  - `transport.rs` — `CloudHttpsTransport` dispatcher + `HttpRequest`/
    `HttpResponse` value types + the shape-data trio
    `CloudTransport`/`CloudAuth`/`CloudEndpoints`. The dispatcher emits one
    structured audit line per call (vendor, endpoint, status, latency,
    correlation-id-presence — **never** the token), reads
    `x-github-request-id` as a recognized correlation header (added by
    this loop, alongside the pre-existing `x-cursor-request-id` and
    generic `x-request-id`).
  - `keychain.rs` — `VendorCredentialStore` trait + `KeychainCredentialStore`
    (production, `keyring` crate) + `MemoryCredentialStore` (tests). Round-
    trip test, key-isolation test, **Debug-impl-leaks-no-secret** test on
    both impls (defense in depth against future drift).
  - `audit.rs` — `AuditEvent` + `emit()`, structured `tracing` line on
    `target = "bluey.cloud_audit"`. Debug-leak test asserts the struct
    cannot grow a field named token / api_key / authorization / Bearer / Basic.
  - `registry.rs` — `CloudAgentEntry`, `BillingModel`,
    `CloudEndpointKind`, `CloudTaskState`, `CLOUD_REGISTRY`. Tests pin every
    row: declares `billing_model`, declares non-empty `consent_warning`,
    declares lowercase `vendor_short`, declares https base_url, and
    `task_shaped` rows declare a non-zero `max_task_duration_secs`.
  - `drive.rs` — **generic dispatcher**: `drive_cloud(kind, question)` reads
    `cloud_entry_for(tag)` and routes the call. No `if vendor == "copilot"`
    in this file — the dispatch is purely a table lookup. The lib-level
    `cue_agent_bridge::drive` checks `cloud::is_cloud_kind(kind)` and picks
    between the local-CLI drive and the cloud drive without naming a vendor.
- **Daemon wiring** — `cue-daemon/src/app.rs::drive_answer_attempt` checks
  `cue_agent_bridge::cloud::is_cloud_kind(kind)` and routes to
  `drive_cloud` for cloud kinds, falling through to the existing
  `cue_agent_bridge::drive` for local. The answer ladder, error guidance
  card, and "agent not ready" honest-error path all flow unchanged.
- **Test count:** the bridge has 309 unit tests passing (30 of them
  Copilot-specific, including 3 wiremock round-trips that exercise the
  FULL transport stack — real reqwest, real audit log emission, real
  status-code mapping — against a fake GitHub API in-process). The daemon
  has 218 passing.
- **`cargo fmt --all -- --check`** clean,
  **`cargo clippy -p cue-agent-bridge -p cue-daemon --all-targets -- -D warnings`**
  clean, **`cargo test -p cue-agent-bridge -p cue-daemon`** clean.

### 🟡 Coded, NEEDS-LIVE-VERIFY (no Copilot subscription on hand)

- The actual JSON shape of `POST /agents/...tasks` *response* (task id,
  `state`, `pull_request`, `logs_url`, timestamps). The dedicated REST page
  `rest/copilot/copilot-coding-agent-management` only documents
  **policy** endpoints. The task endpoint shape is described **narratively**
  on `use-cloud-agent-via-the-api`. The parser is fail-soft on every field
  (each is `Option<…>`) — missing fields degrade to "task created, see PR
  list later" rather than panicking. **Needs a real `curl` against
  api.github.com to lock the names down.**
- The actual status-value strings I list (`queued`, `in_progress`, …) come
  from the how-to page. If real responses use different casing or a superset,
  `task_state_from_str` silently routes the extras to
  `CloudTaskState::Other(raw)` (non-terminal, audit log records the raw
  string). No panic, no silent terminal misclassification.
- Polling cadence: not yet implemented. Once a task is kicked off the
  daemon emits the acknowledgement chunk and stops. The follow-up polling
  loop + push notification when the task transitions to a terminal state
  (`Succeeded`/`Failed`/`Cancelled`) is the next milestone. The shape is
  reserved: 30 s for fresh tasks → 60 s steady-state stays well under
  GitHub's 5 000 req/h primary limit and 900 GET-points/min/endpoint
  secondary limit.
- Webhook path: the canonical "task finished" signal is documented as a
  `pull_request` event opened by `copilot-swe-agent[bot]`, with the
  `Agent-Logs-Url` trailer on the commit. The webhook wiring is **not**
  implemented in this batch; polling is the v1. **Live-verify the bot
  login + trailer string** before promoting to webhooks.
- `is_copilot_bot_login` accepts both `copilot-swe-agent` (webhook payload
  form) and `copilot-swe-agent[bot]` (assignee form), which matches every
  example I could find. **Live-verify both forms appear in real responses.**
- `STATIC_HEADERS` pins `x-github-api-version: 2026-03-10` (the version the
  docs page was written against, current as of this dossier). When GitHub
  ships a newer version, the value updates here and nowhere else.
- The token-prefix guard rejects `ghs_*` (installation) and `ghr_*`
  (refresh) tokens client-side. `ghp_*` / `github_pat_*` / `gho_*` / `ghu_*`
  pass. **Live-verify GitHub's actual rejection error for a `ghs_*` token**
  so we can match the server-side message if it appears.

### ⚠️ Blocked / needs a product decision

- **Async UX**: Copilot is task-shaped. A single ask can run 30 s to 59 min.
  The existing overlay answer flow assumes "answer in a few seconds." This
  batch returns an acknowledgement immediately ("Kicked off task `<id>` in
  `<owner>/<repo>` — Copilot will draft a PR in ~5–15 minutes…") and stops.
  The *result* needs a follow-up notification path that doesn't exist yet
  (the existing local-agent code path is purely synchronous). The shape is
  reserved on `CloudAgentEntry::task_shaped`/`max_task_duration_secs`, and
  the daemon's `drive_answer_attempt` distinguishes cloud kinds via
  `cloud::is_cloud_kind`, but the user-visible "ding! your PR is ready"
  experience is product work, not engineering plumbing. **Needs a design
  call** before shipping to real users.
- **Repo/owner scoping**: Bluey holds one PAT per user, but each task needs
  a `(owner, repo)` pair. Today there is no UI for "which repo do you want
  Copilot to work in?" — `CloudTaskInputs::from_question` returns
  `owner = None, repo = None` and the adapter surfaces an honest error
  ("this vendor is task-shaped and needs an explicit (owner, repo). Bluey's
  repo-picker UI is the gating piece here — until it lands, kick-off is
  refused"). **Needs a repo picker** in the overlay before this is
  production-honest. The error is HONEST today (not silent), so no user is
  ever lied to.

### 🔴 Data-promise / surprise-corporate-user risks

- **PAT scope sprawl**: the documented scopes for `assignees:
  copilot-swe-agent[bot]` are *read + write* on Actions, Contents, Issues,
  and Pull requests — that is a HUGE token. Bluey holding it crosses the
  "scoped credential only" promise from CLAUDE.md unless we make the user
  actively choose Copilot Cloud (vs Copilot CLI which uses the user's
  existing `gh` auth out of band). The registry row's `consent_warning`
  static string is verified by unit test to mention Actions / PAT / keychain
  before the UI can render it; the disclosure-UI flow MUST surface this
  string verbatim before storing a credential. **Do not auto-enroll.**
- **Data residency**: GHEC-with-data-residency users in EU/AU/JP expect all
  inference to stay in-region. Bluey holding the PAT and calling
  `api.github.com` (vs `<tenant>.ghe.com`) would break that promise. The
  transport reads its base URL from `ENTRY.base_url` (`https://api.github.com`
  default). Per-user override is reserved future work; until then, GHEC-
  residency users SHOULD NOT enroll Copilot Cloud through Bluey — the
  enrollment UI must surface this disclaimer alongside the consent warning.
- **Server-to-server tokens are NOT supported** by the agent-tasks
  endpoints (GitHub Apps installation tokens are rejected by the server).
  Bluey holds a user-to-server token only. `reject_server_to_server_token`
  catches `ghs_*` installation tokens client-side before they leak to the
  network; the matching unit test covers `ghp_`, `github_pat_`, `gho_`,
  `ghu_` (all accepted) and `ghs_`, `ghr_` (rejected). **Token revocation
  is a user responsibility** — Bluey detects revocation lazily (401/403 →
  `Capability::NeedsReauth`) but cannot pre-rotate. Every cloud HTTP call
  emits one audit line tagged `vendor = "copilot_cloud", endpoint = …,
  status = …` so an auditor can correlate Bluey's calls against GitHub's
  own audit log, never the token.

---

## 1. What it is, in one paragraph

GitHub Copilot Coding Agent is a cloud autonomous agent. You feed it a prompt
(usually an issue title + body, or a free-form prompt via the task API). It
spins up a GitHub-Actions-hosted environment (Linux runner with checked-out
repo, MCP servers, custom instructions, optional custom agents), uses the
chosen model (Claude Sonnet, GPT-5, etc.) to research/plan/code/test, then
opens a **draft pull request** authored by `Copilot` (bot login
`copilot-swe-agent[bot]`) and tags the requester for review. Sessions are
hard-capped at 59 minutes. The agent runs in the background — invocation is
fire-and-forget, completion is signaled via a PR appearing.

## 2. Auth model

**Required token type: user-to-server.**
- Personal access token (PAT) — classic OR fine-grained.
- OAuth app token.
- GitHub App **user**-to-server token (NOT installation/server-to-server —
  those are rejected by the agent task endpoints).

**Why user-to-server only:** Copilot is billed per user. The agent invocation
must be attributable to a paying user. Source: GitHub changelog
[Assign issues to Copilot using the API (2025-12-03)](https://github.blog/changelog/2025-12-03-assign-issues-to-copilot-using-the-api/)
and discussion [community #164267](https://github.com/orgs/community/discussions/164267).

**Fine-grained PAT permissions needed (read+write on each):**
- `Actions` (the agent runs in Actions; the task creation API touches it)
- `Contents`
- `Issues`
- `Pull requests`

Source: community discussion above. **OPEN: does the task API itself need a
distinct `Copilot` permission?** The docs don't say. I assumed "no — the
above four cover it" because the discussion shows that combo working in
practice. If wrong, the impact is HTTP 403 from `POST /agents/.../tasks` and
the user has to add one permission; the bridge surfaces that 403 verbatim as
`needs_reauth`.

**Headers (mandatory):**
```
Authorization: Bearer <token>
Accept: application/vnd.github+json
X-GitHub-Api-Version: 2026-03-10
User-Agent: Bluey-Cue/<version>           # GitHub recommends a UA
```

**How the user provides the credential:** the user generates a fine-grained
PAT in [GitHub Settings → Developer settings → Fine-grained tokens](https://github.com/settings/personal-access-tokens/new),
scoped to ONLY the repositories they want Copilot to touch through Bluey, and
pastes it into Bluey once. Bluey stores it in the OS keychain (macOS
`Keychain.app`, Windows Credential Manager, Linux libsecret) under the
service name `dev.bluey.cue.cloud-agent.copilot_cloud`. Token expiry is
selected by the user (we default-suggest 30 days). Bluey emits a structured
audit log line on every cloud API call (vendor, endpoint, status, duration,
request id) so a user/admin can correlate against GitHub's own audit log.

## 3. Endpoints

The whole task surface lives under `/agents/`. Source:
[Using Copilot cloud agent via the API](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/cloud-agent/use-cloud-agent-via-the-api).

### 3.1 Start a task

```
POST https://api.github.com/agents/repos/{owner}/{repo}/tasks
Content-Type: application/json

{
  "prompt": "<required: free-form task description>",
  "base_ref": "main",                  // optional, default = repo default branch
  "model": "claude-sonnet-4.5",        // optional, default = user's selection
  "create_pull_request": true          // optional, default = true
}
```

Response (200 / 201) — **inferred** shape, parser is fail-soft on every
field:
```json
{
  "id": "task_xxxxxxxxxxxxxxxx",       // OPEN: format unspecified in docs
  "state": "queued",                   // see §3.4 for state strings
  "owner": "OWNER",
  "repo": "REPO",
  "prompt": "Fix the login button...",
  "created_at": "2026-06-05T12:00:00Z",
  "updated_at": "2026-06-05T12:00:00Z",
  "pull_request": null,                // populated once a draft PR opens
  "logs_url": "https://github.com/OWNER/REPO/agent-sessions/...",
  "model": "claude-sonnet-4.5"
}
```

### 3.2 List tasks for a repo
```
GET https://api.github.com/agents/repos/{owner}/{repo}/tasks
```
Response: array (or paginated object) of task objects (§3.1).

### 3.3 List all tasks (across repos the token can see)
```
GET https://api.github.com/agents/tasks
```

### 3.4 Get task status
```
GET https://api.github.com/agents/repos/{owner}/{repo}/tasks/{task-id}
```
Documented possible `state` values:
`queued | in_progress | completed | failed | idle | waiting_for_user |
timed_out | cancelled`. Source: how-to page above.

### 3.5 Assign Copilot via the standard issue API (alternative entry)
The community/changelog confirms you can also kick off a task by assigning
the bot to an issue:
```
POST https://api.github.com/repos/{owner}/{repo}/issues/{issue}/assignees
{
  "assignees": ["copilot-swe-agent[bot]"],
  "agent_assignment": {
    "target_repo": "OWNER/REPO",
    "base_branch": "main",
    "custom_instructions": "",
    "custom_agent": "",
    "model": ""
  }
}
```
Or create a new issue:
```
POST https://api.github.com/repos/{owner}/{repo}/issues
{
  "title": "...",
  "body": "...",
  "assignees": ["copilot-swe-agent[bot]"],
  "agent_assignment": {...}
}
```
**The Bluey adapter uses the dedicated `/agents/.../tasks` endpoint, not the
issue-assignment path.** Rationale: cleaner, no leftover GitHub issue we then
have to attribute to the user, lower coupling to issue conventions. The
issue-assignment path is still documented for completeness (and as a fallback
if `/agents/.../tasks` returns 404 on a given GHES variant).

### 3.6 Cancel
**OPEN: no documented `DELETE /agents/.../tasks/{id}` endpoint exists in the
references I read.** The user-facing UI has a "Stop session" button (managed
via the agent management page). For now, the bridge does NOT expose cancel;
if the user wants to abort they go to `https://github.com/copilot/agents`.
This is honest UX — a fake cancel that silently does nothing would be worse.

### 3.7 Cloud-agent policy + permission endpoints (admin)
There's a *separate* surface for admin policy (org-level enable, repository
allow-list, enterprise policy). Bluey does NOT call these — they're admin
operations a user-shaped token usually can't perform. Full list documented
at [rest/copilot/copilot-coding-agent-management](https://docs.github.com/en/rest/copilot/copilot-coding-agent-management):
`PUT /enterprises/{enterprise}/copilot/policies/coding_agent`, the
`/orgs/{org}/copilot/coding-agent/permissions[/repositories[/{id}]]` family.

## 4. Billing model

Source:
[GitHub Copilot Plans & pricing](https://github.com/features/copilot/plans),
[2026-06-01 Copilot billing changes](https://github.blog/changelog/2026-06-01-updates-to-github-copilot-billing-and-plans/).

As of 2026-06-01, all GitHub Copilot plans moved to **usage-based billing
with AI Credits** (1 credit = $0.01). Each cloud agent task consumes:
1. **GitHub Actions minutes** (deducted from the user's account's monthly
   free Actions minutes — runs on `ubuntu-latest` by default).
2. **AI credits** from the monthly Copilot allowance (overage billable).

**Legacy plan note:** Pro/Pro+ on legacy annual plans still see "1 premium
request per cloud agent session" until they're migrated.

**Free tier:** None for Copilot Cloud specifically — you need a paid Copilot
plan (Pro, Pro+, Business, or Enterprise). Source: about-page.

**Bluey's registry row therefore declares `billing_model: Subscription`** —
the UI MUST disclose "this counts against your paid Copilot plan, including
Actions minutes" before first use.

## 5. Rate limits

GitHub REST API primary limits (source:
[REST API rate limits](https://docs.github.com/en/rest/overview/rate-limits-for-the-rest-api)):
- **Authenticated user**: 5 000 req/h.
- **Authenticated user under GHEC enterprise**: 15 000 req/h.
- **Unauthenticated** (irrelevant here): 60 req/h.

Secondary limits (per token):
- 100 concurrent requests.
- 900 points/min/endpoint (GET = 1 pt; POST/PATCH/PUT/DELETE = 5 pts).
- 80 content-generating requests/min.

Bluey's call pattern per active task: 1 POST (create) then GETs every 30 s
during the first 5 minutes, then every 60 s. That is well under the limits
even for 100 concurrent tasks per user (which is itself implausible — Copilot
runs limit concurrency on their side too).

Headers consumed:
`x-ratelimit-limit`, `x-ratelimit-remaining`, `x-ratelimit-reset`,
`x-ratelimit-resource`, `retry-after`. The transport reads these and slows
its poll cadence at < 20 % remaining; on 429 it honors `retry-after` exactly.

## 6. Session / task semantics

Copilot Cloud is **task-shaped, async, long**:
- A task is the unit. There is no "turn" — a task starts, runs ≤59 min, and
  finishes by opening a draft PR (or fails).
- The "session" in the docs is the agent's *internal* run trace
  (`https://github.com/copilot/agents/...`), surfaced via session logs. It is
  not a multi-turn dialog the way a `claude --resume <id>` is.
- **You cannot "follow up" on a finished task via the task API.** To iterate,
  you (a) `@copilot` mention in the draft PR's review, or (b) start a new
  task. The Bluey adapter therefore does NOT implement a resume field for
  this vendor — `Question.resume` is silently ignored with a debug log.
- The 59-minute hard cap means no task is *truly* synchronous. Even a trivial
  prompt takes minutes (Actions runner cold start, repo clone, model warm-up,
  tests).

## 7. Error taxonomy

From [troubleshoot-coding-agent](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/coding-agent/troubleshoot-coding-agent)
and inferred standard GitHub error shapes:

| Class | HTTP | Body shape | Bluey treatment |
|---|---|---|---|
| Token missing/invalid | 401 | `{"message":"Bad credentials","documentation_url":"..."}` | `Capability::NeedsReauth` + audit-log |
| Insufficient permissions on PAT | 403 | `{"message":"Resource not accessible by personal access token","documentation_url":"..."}` | `NeedsReauth` with hint |
| Repo not enabled for cloud agent | 403 | `{"message":"Copilot Coding Agent is not enabled for this repository"}` (paraphrased — exact string NEEDS-LIVE-VERIFY) | Surface verbatim — admin must enable |
| Server-to-server token used | 403 | docs say "user-to-server only"; exact body NEEDS-LIVE-VERIFY | Surface verbatim |
| Invalid prompt / repo not found | 404 / 422 | `{"message":"Not Found"}` or validation errors | Surface verbatim |
| Primary rate limit | 403 | `x-ratelimit-remaining: 0` + retry-after | Sleep until `x-ratelimit-reset`, then resume |
| Secondary rate limit | 429 | `retry-after` header in seconds | Sleep `retry-after`, exponential backoff |
| Task failed at runtime | task `state: failed` | task object has `error` / log url | Surface as failure with link to session log |
| CI failure during agent run | not an HTTP error — task `state` likely stays `in_progress` until agent gives up | Reflect as task state | Same as above |
| Branch protection / ruleset blocks Copilot | task `state: failed`; error msg mentions ruleset | Surface verbatim — user must add Copilot as bypass actor |

Every HTTP error is also audit-logged with `vendor=copilot_cloud,
endpoint=<path>, status=<code>, request_id=<gh-request-id>`; the token is
NEVER logged.

## 8. Revocation

- **PAT revocation**: user revokes at
  `https://github.com/settings/personal-access-tokens`. No GitHub-side push
  notification to Bluey — discovered lazily on next 401.
- **GitHub App uninstall**: same — discovered on next 401. We do not use a
  GitHub App for this surface.
- **Per-repo revoke**: not directly an auth concept — the user removes a repo
  from the fine-grained PAT's allow-list, which becomes a 404 on that repo's
  task endpoint. The bridge treats 404 as "this repo isn't reachable for
  you" not "the token is dead."

Bluey supports user-driven revocation via `bluey agent forget copilot_cloud`
(removes the keychain entry; the next call fails with a clean "no
credential" error).

## 9. MCP

- **Direction**: **Inbound** — the Copilot agent calls MCP servers as a
  *consumer*. The agent does NOT expose itself as an MCP server we can call.
- **Default MCP servers in every task**: GitHub MCP server (read-only on the
  current repo via a scoped token) + Playwright MCP server. Source:
  [configure-mcp-servers](https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/configure-mcp-servers).
- **Per-repo config**: repo admins add a JSON config in repo Settings →
  Copilot → MCP servers. Bluey **does not** configure MCP on behalf of the
  user — that's a settings-page operation. The bridge surfaces "this repo
  has N MCP servers configured" as a read-only info if it's later useful.
- **Per-task MCP override**: NOT supported via the task API as of this
  research. OPEN — if a future API parameter lets us inject MCP at task
  creation, we'd surface it as an opt-in.

## 10. Webhooks

Native webhook events that signal task progress:
- `pull_request` `opened` where `pull_request.user.login = "copilot-swe-agent[bot]"`
  — task created a draft PR.
- `pull_request` `ready_for_review` from same user — task finished and is
  asking the human to review.
- `pull_request_review` `submitted` where review body contains an
  `@copilot` mention — user is iterating mid-PR.
- `workflow_run` from the Actions job that hosts the agent — progress / fail.
- `issue_comment` from `copilot-swe-agent[bot]` — status updates ("eyes"
  reaction, log links).

The canonical "I'm done" signal is the draft PR moving from `draft` to
`ready_for_review`, OR a non-draft PR being opened directly. The commit
trailer `Agent-Logs-Url: https://github.com/...` (added 2026-03-20) gives a
permanent traceable link.

**Bluey v1 uses polling, NOT webhooks** — webhook delivery requires either a
publicly reachable HTTP endpoint or a GitHub webhook forwarder. Polling
fits the local-only architecture cleanly. Webhooks are a future improvement.

## 11. Data residency / corporate compliance

- **Variants**: standard GitHub.com, GHE Cloud (no residency), GHE Cloud
  **with data residency** (EU/AU/US/JP), GHES self-hosted. Source:
  [GHEC with data residency](https://docs.github.com/en/enterprise-cloud@latest/admin/data-residency/github-copilot-with-data-residency).
- GHEC-with-residency tenants use a tenant-scoped host like
  `<tenant>.ghe.com` and `api.<tenant>.ghe.com`. The registry row's
  `base_url` is overridable at runtime by the user.
- **GHES** (on-prem) Copilot Coding Agent support is partial as of 2026
  (search results unclear on GA status). Bluey treats GHES as
  user-configurable host + same endpoints; if endpoints 404 the user sees a
  clean error.
- **SSO/SAML**: a user-to-server token derived from a SAML-required org must
  be SSO-authorized — the user authorizes the token in GitHub UI after
  creating it. If unauthorized, requests return 403 with a clear message;
  the bridge surfaces it verbatim.

## 12. Edge cases / gotchas

- **`copilot-swe-agent[bot]` literal string**: must be that exact value in
  `assignees`. `copilot` and `copilot-swe-agent` both fail with HTTP 422.
- **Assigning Copilot does NOT trigger immediately if the repo lacks the
  feature**: returns 422 instead. Always check repo enablement first.
- **The bot login is `copilot-swe-agent`** (no `[bot]` suffix) in
  *webhook payloads* and `user.login` fields. The `[bot]` suffix appears in
  `assignees` API calls and in user-facing UI. The bridge normalizes both.
- **Content exclusions are NOT honored** by Copilot Cloud Agent — the agent
  reads files even if Copilot content-exclusion rules say "don't suggest
  from this file." Surface this as a consent-time warning.
- **Image attachments in prompts**: max 3 MiB; larger images silently
  stripped. Bluey doesn't attach images to prompts today, so no-op.
- **Branch protection compatibility**: certain ruleset configurations block
  Copilot. Workaround is to add Copilot as a bypass actor. Surfaced in the
  task failure message verbatim.
- **No `--resume`-equivalent**: see §6.
- **One repo per task**: Copilot can't span repos. The bridge enforces this
  by requiring `(owner, repo)` at task creation.

## OPEN QUESTIONS (assumptions made explicit)

These are things the public docs don't pin down. I made the safer assumption
and wrote it down so a live-verify session can flip the bit.

1. **Exact task-object JSON shape.** Docs describe state strings narratively
   but no machine-readable schema page exists. I assumed the conventional
   GitHub shape (`id`, `state`, `created_at`, `updated_at`, `pull_request`
   sub-object with `html_url`, `logs_url`). Parser is fail-soft on every
   field. **If `id` is numeric vs string, or `state` is camelCase, our
   `from_str` mapping still works but `TaskId` is currently typed as
   `String`** (broader; accommodates either).
2. **Does the task API ever return 202 Accepted vs 200 OK on create?** I
   accept any 2xx. Either works.
3. **Pagination on `GET /agents/tasks`.** Standard GitHub Link-header
   pagination assumed. Untested.
4. **Exact PAT permission set required.** Community discussion shows
   Actions/Contents/Issues/PRs (read+write) working. **Is `copilot` a
   distinct permission name now under fine-grained PATs?** Assumed no.
5. **Status `idle` vs `waiting_for_user`.** Both listed in the how-to page;
   docs don't disambiguate. I map both to `CloudTaskState::Waiting` for the
   UI; the raw string is kept for the audit log.
6. **GHEC-residency host name pattern.** Assumed `<tenant>.ghe.com` /
   `api.<tenant>.ghe.com`. Live-verify required for the API host.
7. **Cancel endpoint.** Assumed non-existent — only UI Stop. If a `DELETE
   /agents/.../tasks/{id}` lands, plumbing is one row in the adapter.
8. **Streaming logs.** No streaming API for session logs is documented; we
   show the static `logs_url` and the user clicks through.
9. **Server-to-server token rejection mode.** Doc says "not supported"
   without quoting the exact 403 body. I check before sending: if the token
   looks like a GitHub App installation token (`ghs_` prefix per GitHub's
   prefix convention), we refuse client-side with a clear "use a PAT
   instead" error rather than letting GitHub reject it.
10. **`copilot-swe-agent` vs `copilot-swe-agent[bot]` in different
    surfaces.** I normalize on read (both accepted), use the `[bot]` form
    when writing as a literal string.

---

## PROPOSED SPEC — data-driven, no agent-name branches

> Cross-vendor: the four parallel agents (this one, Cursor Cloud, Anthropic
> API, Codex Cloud) all share the same machinery. Per-vendor code is ONLY
> the irreducible JSON adapter in `cloud/<vendor>.rs`.

### `AgentKind` — new variant
```rust
pub enum AgentKind {
    // …existing local variants…
    CursorCloud,      // (parallel agent owns this row)
    CopilotCloud,     // ← THIS dossier owns this row
    AnthropicApi,     // (parallel agent owns this row)
    CodexCloud,       // (parallel agent owns this row)
    // …
}
```
Serde rename `"copilot_cloud"` works automatically (snake_case derive).
`parse_attached_agent("copilot_cloud")` resolves via the existing JSON
round-trip.

### `KindTag` — matching variant
```rust
pub enum KindTag {
    // …existing…
    CursorCloud,
    CopilotCloud,
    AnthropicApi,
    CodexCloud,
}
```
The `to_agent_kind` / `from_agent_kind` round-trip is one match arm per new
variant.

### Registry row — pure DATA
The existing `AgentEntry` struct grew fields for local-CLI assumptions
(`binary_candidates`, `data_dir_globs`, `drive_command`, `mcp_allow_flag`, …)
that don't apply to a cloud vendor. Rather than bend the local struct,
**cloud vendors live in a sibling `CloudAgentEntry`** registered through the
same module:

```rust
// crates/cue-agent-bridge/src/cloud/registry.rs (NEW — added by this batch)
pub struct CloudAgentEntry {
    pub kind_tag: KindTag,
    pub display_name: &'static str,

    pub transport: Transport,             // see cloud/transport.rs
    pub billing_model: BillingModel,      // Subscription | ApiCredits | Byot
    pub consent_warning: &'static str,    // UI must show before enroll
    pub task_shaped: bool,                // true = async, returns "kicked off"
    pub max_task_duration_secs: u32,      // Copilot = 59 * 60

    /// The adapter function pointers (irreducible per-vendor logic).
    pub adapter: &'static dyn CloudAdapter,
}

pub const CLOUD_REGISTRY: &[&CloudAgentEntry] = &[
    &cloud::copilot::ENTRY,
    // &cloud::cursor::ENTRY,
    // &cloud::anthropic::ENTRY,
    // &cloud::codex_cloud::ENTRY,
];
```

`Transport::CloudHttps { base_url, auth, endpoints }` carries:
- `base_url: &str` (default `https://api.github.com`; overridable per-user
  for GHEC-residency).
- `auth: &dyn AuthHeaders` (Bluey-provided `PatAuth` for Copilot; the other
  vendors plug in their own — `BearerKey` for Anthropic API, etc.).
- `endpoints: EndpointSpec { create_task, get_task, list_tasks }` — pure
  string templates.

### Where the irreducible per-vendor code lives
`crates/cue-agent-bridge/src/cloud/copilot.rs`:
- Builds the create-task JSON body from a `Question`.
- Parses the task response into `CloudTaskHandle { id, state, pull_request_url, logs_url }`.
- Maps GitHub-specific state strings → generic `CloudTaskState`.
- Knows about the `Agent-Logs-Url` trailer + `copilot-swe-agent[bot]`
  literal — Bluey's other vendors will never need this string.

Every shared concern (HTTP transport, retry, auth headers, audit log, token
storage) lives in `cloud/transport.rs`, `cloud/keychain.rs`, `cloud/audit.rs`
and is GENERIC over vendor. Adding a new vendor adds ONE row + ONE adapter
file, NEVER a branch in the shared transport.

### Daemon answer route — feature-flag, not if-name
`answer_with_agent` reads the registry row's `task_shaped` flag. When true,
it kicks off the task via the adapter and returns a single chunk:
```
Task #task_xxxxx kicked off in OWNER/REPO. Copilot will open a draft PR at
<pr_url_or_logs_url> in ~5–15 minutes. I'll notify you when it's ready.
```
…then parks the task id in a `pending_cloud_tasks` map. The poll loop
(driven by the existing tokio runtime) advances the task and pushes a
follow-up overlay event when the state flips to `completed` (or `failed`).
No agent name is hardcoded in `answer_with_agent`.

### Why this stays data-driven
- Adding GitHub Copilot Cloud = 1 enum variant + 1 registry row + 1 adapter
  file. No branches in `answer_with_agent`, no branches in the transport, no
  branches in audit/keychain.
- Adding Cursor Cloud or Anthropic API or Codex Cloud = same recipe.
- A future vendor with a completely different auth (e.g. SSO-only) plugs in
  by adding ONE struct that impls `AuthHeaders`; nothing else changes.
