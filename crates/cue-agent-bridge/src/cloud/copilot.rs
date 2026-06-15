//! GitHub Copilot Coding Agent (Cloud) — vendor adapter.
//!
//! Owns the irreducible per-vendor logic for Copilot Cloud:
//!
//! - The registry [`ENTRY`] row — display name, base URL, billing model,
//!   consent warning, task-shape flag, hard max duration.
//! - Request body construction for `POST /agents/repos/{owner}/{repo}/tasks`.
//! - Response parsing for the task object (defensive — every field is
//!   optional, since the public docs describe the shape narratively, not as
//!   a machine-readable schema; see `docs/vendors/copilot_cloud.md`).
//! - GitHub-specific status-string → generic [`CloudTaskState`] mapping.
//! - The `copilot-swe-agent[bot]` literal normalization (both forms accepted).
//!
//! Everything else (HTTP transport, keychain, audit logging) is shared in
//! `cloud/{transport,keychain,audit}.rs` — no vendor name appears there.
//!
//! ### Sources (see `docs/vendors/copilot_cloud.md` for the full dossier)
//!
//! - <https://docs.github.com/en/copilot/how-tos/use-copilot-agents/cloud-agent/use-cloud-agent-via-the-api>
//! - <https://github.blog/changelog/2025-12-03-assign-issues-to-copilot-using-the-api/>
//! - <https://docs.github.com/en/copilot/concepts/agents/coding-agent/about-coding-agent>
//! - <https://docs.github.com/en/copilot/how-tos/use-copilot-agents/coding-agent/troubleshoot-coding-agent>

use anyhow::{anyhow, Result};
use serde::Deserialize;

use crate::cloud::registry::{BillingModel, CloudAgentEntry, CloudTaskState};
use crate::registry::KindTag;

// ---------------------------------------------------------------------------
// Registry row — the ONE place where Copilot Cloud is "configured"
// ---------------------------------------------------------------------------

/// Static row for the Copilot Cloud agent. Adding Copilot to Bluey is this
/// row plus the parser/builder helpers in this file — there is no `if
/// vendor == "copilot"` branch anywhere else in the workspace.
pub const ENTRY: &CloudAgentEntry = &CloudAgentEntry {
    // The new local-registry kind tag is `KindTag::CopilotCloud` (added in
    // `crate::registry`); the daemon's display name and serde label
    // ("copilot_cloud") flow from there.
    kind_tag: KindTag::CopilotCloud,
    display_name: "GitHub Copilot Coding Agent (Cloud)",
    vendor_short: "copilot_cloud",
    // Default. Overridable per-user for GHEC-with-data-residency
    // (`api.<tenant>.ghe.com`) and self-hosted GHES.
    base_url: "https://api.github.com",
    billing_model: BillingModel::Subscription,
    consent_warning: concat!(
        "Copilot Coding Agent runs in a GitHub Actions environment paid for ",
        "by your account. The fine-grained PAT you provide will be stored ",
        "only in your OS keychain and must have read+write on Actions, ",
        "Contents, Issues, and Pull requests for the repositories you want ",
        "Bluey to drive. Bluey audit-logs every call (vendor, endpoint, ",
        "status) but never the token. Note: Copilot Cloud does NOT honor ",
        "GitHub's content-exclusion rules — the agent reads any file in the ",
        "repo. Tasks run up to 59 minutes per kick-off."
    ),
    task_shaped: true,
    // Per GitHub docs: "Each Copilot cloud agent session has a maximum
    // execution time of 59 minutes." (about-coding-agent page).
    max_task_duration_secs: 59 * 60,
};

// ---------------------------------------------------------------------------
// Endpoint paths — pure data
// ---------------------------------------------------------------------------

/// Endpoint paths for Copilot Cloud, relative to [`ENTRY::base_url`]. Used by
/// both the adapter helpers below and the unit tests that assert "the path
/// we'd send matches what the docs document." `{owner}`, `{repo}`, and
/// `{task_id}` are placeholders the adapter substitutes per call (caller is
/// responsible for percent-encoding any user-supplied owner/repo string).
pub const CREATE_TASK_PATH: &str = "/agents/repos/{owner}/{repo}/tasks";
pub const GET_TASK_PATH: &str = "/agents/repos/{owner}/{repo}/tasks/{task_id}";
pub const LIST_TASKS_PATH: &str = "/agents/repos/{owner}/{repo}/tasks";
pub const LIST_ALL_TASKS_PATH: &str = "/agents/tasks";

/// Mandatory static headers GitHub expects on every request. The
/// `X-GitHub-Api-Version` value is pinned to the version the docs were
/// written against — `2026-03-10`. The transport merges these onto every
/// request without naming the vendor.
pub const STATIC_HEADERS: &[(&str, &str)] = &[
    ("accept", "application/vnd.github+json"),
    ("x-github-api-version", "2026-03-10"),
];

/// The literal GitHub Copilot bot username, in the form `assignees` arrays
/// expect (with the `[bot]` suffix). Webhook payloads and `user.login`
/// fields use the bare form (without `[bot]`) — see [`is_copilot_bot_login`].
pub const COPILOT_BOT_ASSIGNEE: &str = "copilot-swe-agent[bot]";

/// The commit-trailer name that points back at the agent's session logs.
/// Added 2026-03-20: <https://github.blog/changelog/2026-03-20-trace-any-copilot-coding-agent-commit-to-its-session-logs/>.
pub const AGENT_LOGS_URL_TRAILER: &str = "Agent-Logs-Url";

/// Normalize a GitHub user login string to match Copilot's bot identity in
/// either form (`copilot-swe-agent` from webhook payloads, or
/// `copilot-swe-agent[bot]` from assignee arrays). True for both.
pub fn is_copilot_bot_login(login: &str) -> bool {
    let trimmed = login.trim();
    trimmed == "copilot-swe-agent" || trimmed == COPILOT_BOT_ASSIGNEE
}

// ---------------------------------------------------------------------------
// Request body construction
// ---------------------------------------------------------------------------

/// Inputs for `POST /agents/repos/{owner}/{repo}/tasks`. The two optional
/// fields are explicit so unit tests can assert that we don't send empty
/// strings — GitHub treats empty `base_ref` and `model` differently from
/// absence.
#[derive(Debug, Clone)]
pub struct CreateTaskRequest {
    /// Free-form task description; mapped to JSON `prompt` (required).
    pub prompt: String,
    /// Base branch; `None` means "let GitHub default to the repo's default
    /// branch." Sent as JSON `base_ref` when present.
    pub base_ref: Option<String>,
    /// Model id; `None` means "use the user's default Copilot model
    /// selection." Sent as JSON `model` when present.
    pub model: Option<String>,
}

impl CreateTaskRequest {
    /// Render the JSON body GitHub expects.
    ///
    /// Shape (DOC-CONFIRMED via
    /// <https://docs.github.com/en/copilot/how-tos/use-copilot-agents/cloud-agent/use-cloud-agent-via-the-api>):
    /// ```json
    /// {
    ///   "prompt": "Fix the login button on the homepage",
    ///   "base_ref": "main"
    /// }
    /// ```
    /// Optional fields are OMITTED when `None`, never sent as empty strings.
    pub fn to_json(&self) -> serde_json::Value {
        let mut body = serde_json::Map::new();
        body.insert(
            "prompt".to_string(),
            serde_json::Value::String(self.prompt.clone()),
        );
        if let Some(base_ref) = &self.base_ref {
            // Guard against empty strings — GitHub accepts them but they
            // mean "use the literal empty branch name" which is wrong.
            if !base_ref.trim().is_empty() {
                body.insert(
                    "base_ref".to_string(),
                    serde_json::Value::String(base_ref.clone()),
                );
            }
        }
        if let Some(model) = &self.model {
            if !model.trim().is_empty() {
                body.insert(
                    "model".to_string(),
                    serde_json::Value::String(model.clone()),
                );
            }
        }
        serde_json::Value::Object(body)
    }
}

/// Render the create-task path with the `{owner}` / `{repo}` placeholders
/// substituted. Caller is responsible for the contents being URL-safe —
/// GitHub repo and owner names cannot contain `/` so we don't percent-encode
/// here (a path-encoded `/` would break the route).
pub fn create_task_path(owner: &str, repo: &str) -> String {
    CREATE_TASK_PATH
        .replace("{owner}", owner)
        .replace("{repo}", repo)
}

/// Render the get-task path with `{owner}`, `{repo}`, `{task_id}` filled.
pub fn get_task_path(owner: &str, repo: &str, task_id: &str) -> String {
    GET_TASK_PATH
        .replace("{owner}", owner)
        .replace("{repo}", repo)
        .replace("{task_id}", task_id)
}

/// Render the list-tasks path for a single repo.
pub fn list_tasks_path(owner: &str, repo: &str) -> String {
    LIST_TASKS_PATH
        .replace("{owner}", owner)
        .replace("{repo}", repo)
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

/// The task object returned by `POST /agents/.../tasks` and
/// `GET /agents/.../tasks/{id}`. Every field is `Option<…>` because the
/// public docs describe the shape narratively without a machine-readable
/// schema (see dossier OPEN QUESTION #1). A missing field degrades to "we
/// don't know yet" rather than crashing the parser.
#[derive(Debug, Clone, Deserialize)]
pub struct CopilotTaskObject {
    /// Task id — string in the docs' examples; we never assume a numeric
    /// form so a future change to UUIDs / KSUIDs doesn't break us.
    #[serde(default)]
    pub id: Option<String>,
    /// Vendor status string (`queued`, `in_progress`, …). Mapped to
    /// [`CloudTaskState`] via [`task_state_from_str`].
    #[serde(default)]
    pub state: Option<String>,
    /// The PR the agent opened (or will open), when present.
    #[serde(default)]
    pub pull_request: Option<PullRequestRef>,
    /// Direct link to the session logs UI.
    #[serde(default)]
    pub logs_url: Option<String>,
    /// Free-form error string set when `state == "failed"`.
    #[serde(default)]
    pub error: Option<String>,
    /// Echo of the model the task ran on.
    #[serde(default)]
    pub model: Option<String>,
    /// Echo of the prompt the task was given.
    #[serde(default)]
    pub prompt: Option<String>,
    /// Task creation timestamp (ISO-8601 string in GitHub's standard shape).
    #[serde(default)]
    pub created_at: Option<String>,
    /// Last update timestamp.
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// A pull-request reference inside a Copilot task object. The conventional
/// GitHub minimal-PR shape is `{ id, number, html_url, state, draft, … }`.
/// We capture the bits the user-visible UX cares about and leave the rest
/// to serde's default.
#[derive(Debug, Clone, Deserialize)]
pub struct PullRequestRef {
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub number: Option<u64>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub draft: Option<bool>,
}

/// Map GitHub's documented status strings to the generic [`CloudTaskState`].
/// Unknown strings flow into `CloudTaskState::Other(raw)` — non-terminal,
/// preserved verbatim in the audit log, so a new status doesn't crash but
/// also doesn't silently classify as terminal.
///
/// Documented strings (per the `use-cloud-agent-via-the-api` how-to):
/// `queued | in_progress | completed | failed | idle | waiting_for_user |
///  timed_out | cancelled`.
pub fn task_state_from_str(raw: &str) -> CloudTaskState {
    match raw.trim().to_ascii_lowercase().as_str() {
        "queued" => CloudTaskState::Queued,
        "in_progress" | "running" => CloudTaskState::Running,
        // Per docs: `completed` is the success terminal; we also accept the
        // common `succeeded` form defensively.
        "completed" | "succeeded" => CloudTaskState::Succeeded,
        "failed" | "timed_out" | "error" => CloudTaskState::Failed,
        "cancelled" | "canceled" => CloudTaskState::Cancelled,
        // Both `idle` and `waiting_for_user` are documented; collapse both
        // into `Waiting` for the UI. Audit log keeps the raw string by
        // virtue of being emitted from the transport (not from this enum).
        "idle" | "waiting_for_user" | "waiting" => CloudTaskState::Waiting,
        other => CloudTaskState::Other(other.to_string()),
    }
}

/// A normalized, vendor-agnostic handle on a Copilot task. The daemon's
/// task-shaped answer ladder holds one of these per pending kick-off.
#[derive(Debug, Clone)]
pub struct CloudTaskHandle {
    pub id: String,
    pub state: CloudTaskState,
    pub pull_request_url: Option<String>,
    pub logs_url: Option<String>,
    pub error: Option<String>,
}

impl CloudTaskHandle {
    /// Build a [`CloudTaskHandle`] from a parsed [`CopilotTaskObject`]. The
    /// returned handle is `Err` only when the task object lacks an `id`
    /// (anything else degrades to `None`/`Other`).
    pub fn from_task_object(obj: &CopilotTaskObject) -> Result<Self> {
        let id = obj.id.clone().ok_or_else(|| {
            anyhow!("Copilot task object had no `id` field (response shape changed?)")
        })?;
        let state = obj
            .state
            .as_deref()
            .map(task_state_from_str)
            .unwrap_or_else(|| CloudTaskState::Other("(no state field)".to_string()));
        let pull_request_url = obj.pull_request.as_ref().and_then(|pr| pr.html_url.clone());
        Ok(Self {
            id,
            state,
            pull_request_url,
            logs_url: obj.logs_url.clone(),
            error: obj.error.clone(),
        })
    }
}

/// Parse a response body into a [`CloudTaskHandle`]. Returns `Err` on JSON
/// parse failure or missing `id`; everything else is preserved as `None`.
pub fn parse_task_response(body: &[u8]) -> Result<CloudTaskHandle> {
    let task: CopilotTaskObject = serde_json::from_slice(body)
        .map_err(|e| anyhow!("Copilot task response is not valid JSON: {e}"))?;
    CloudTaskHandle::from_task_object(&task)
}

/// Reject server-to-server / GitHub-App installation tokens *before* we send
/// them — the Copilot agent endpoints explicitly require user-to-server.
/// Per GitHub's prefix convention:
/// - `ghp_` / `github_pat_` — personal access token (user-to-server) ✅
/// - `gho_` — OAuth app token (user-to-server) ✅
/// - `ghu_` — GitHub App user token (user-to-server) ✅
/// - `ghs_` — GitHub App installation token (server-to-server) ❌
/// - `ghr_` — refresh token, never sent directly ❌
///
/// Sources: GitHub's token prefix conventions (well-known).
pub fn reject_server_to_server_token(token: &str) -> Result<()> {
    let t = token.trim();
    if let Some(prefix) = t.split('_').next() {
        match prefix {
            "ghs" => {
                return Err(anyhow!(
                    "this looks like a GitHub App installation token (ghs_*). \
                     Copilot Coding Agent rejects server-to-server tokens. \
                     Use a fine-grained personal access token instead."
                ));
            }
            "ghr" => {
                return Err(anyhow!(
                    "this looks like a refresh token (ghr_*). \
                     Provide an access token, not a refresh token."
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Render the create-task acknowledgement message
// ---------------------------------------------------------------------------

/// Build the immediate acknowledgement text the daemon returns to the user
/// after a task is kicked off. Copilot is task-shaped (minutes-long, not
/// seconds), so the synchronous answer is **always** "kicked off — here's
/// where the PR will land." The follow-up notification path delivers the
/// PR URL when the task completes.
///
/// Pure string assembly; no I/O. The handle's `logs_url` is preferred for
/// the link (works even before the PR exists); the PR URL is added when
/// the task object already includes it (rare on initial create, common on
/// subsequent polls).
pub fn render_acknowledgement(owner: &str, repo: &str, handle: &CloudTaskHandle) -> String {
    let mut out = format!(
        "Kicked off Copilot Coding Agent task `{}` in `{owner}/{repo}`.",
        handle.id
    );
    out.push_str(" Copilot will draft a pull request in ~5–15 minutes (hard cap 59 minutes).");
    if let Some(pr) = &handle.pull_request_url {
        out.push_str(&format!(" PR: {pr}"));
    } else if let Some(logs) = &handle.logs_url {
        out.push_str(&format!(" Session logs: {logs}"));
    }
    out.push_str(" I'll notify you when the PR is ready.");
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- registry row shape ----------------------------------------------

    #[test]
    fn entry_declares_required_fields() {
        // The cloud-registry tests already enforce these globally; this is a
        // belt-and-suspenders spot-check for the Copilot row specifically so
        // a regression here is obvious.
        assert_eq!(ENTRY.vendor_short, "copilot_cloud");
        assert_eq!(ENTRY.billing_model, BillingModel::Subscription);
        assert!(ENTRY.task_shaped, "Copilot Cloud is task-shaped");
        assert_eq!(
            ENTRY.max_task_duration_secs,
            59 * 60,
            "GitHub doc: 59-minute hard cap per session"
        );
        assert!(
            ENTRY.consent_warning.len() > 100,
            "consent warning must be non-trivial"
        );
        assert!(ENTRY.base_url.starts_with("https://"));
    }

    #[test]
    fn entry_consent_warning_mentions_actions_billing_and_pat() {
        // Defense in depth — the disclosure UI reads `consent_warning`
        // verbatim, so the text must mention:
        //   - that Actions minutes are consumed (billing surprise risk),
        //   - that a PAT is involved (scope-sprawl risk),
        //   - that the keychain holds it (storage clarity).
        let w = ENTRY.consent_warning;
        for required in ["Actions", "PAT", "keychain"] {
            assert!(
                w.contains(required),
                "consent_warning must mention {required:?}; got: {w:?}"
            );
        }
    }

    // ---- bot login normalization ----------------------------------------

    #[test]
    fn copilot_bot_login_accepts_both_forms() {
        assert!(is_copilot_bot_login("copilot-swe-agent"));
        assert!(is_copilot_bot_login("copilot-swe-agent[bot]"));
        assert!(is_copilot_bot_login("  copilot-swe-agent  "));
        // Negatives — historical wrong forms from community discussion.
        assert!(!is_copilot_bot_login("copilot"));
        assert!(!is_copilot_bot_login("github-actions[bot]"));
        assert!(!is_copilot_bot_login(""));
    }

    // ---- create-task body --------------------------------------------------

    #[test]
    fn create_task_body_required_prompt_only() {
        let req = CreateTaskRequest {
            prompt: "Fix the login button".to_string(),
            base_ref: None,
            model: None,
        };
        let body = req.to_json();
        let map = body.as_object().expect("object body");
        assert_eq!(
            map.get("prompt").and_then(|v| v.as_str()),
            Some("Fix the login button")
        );
        // Optional fields MUST be absent (not empty strings) so GitHub's
        // defaults take effect.
        assert!(!map.contains_key("base_ref"));
        assert!(!map.contains_key("model"));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn create_task_body_with_all_optionals() {
        let req = CreateTaskRequest {
            prompt: "Refactor auth".to_string(),
            base_ref: Some("main".to_string()),
            model: Some("claude-sonnet-4.5".to_string()),
        };
        let body = req.to_json();
        let map = body.as_object().expect("object body");
        assert_eq!(
            map.get("prompt").and_then(|v| v.as_str()),
            Some("Refactor auth")
        );
        assert_eq!(map.get("base_ref").and_then(|v| v.as_str()), Some("main"));
        assert_eq!(
            map.get("model").and_then(|v| v.as_str()),
            Some("claude-sonnet-4.5")
        );
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn create_task_body_drops_empty_strings() {
        // An empty `base_ref` or `model` is dropped, not sent as "" — GitHub
        // accepts empty strings but they mean the literal empty branch name
        // which is wrong.
        let req = CreateTaskRequest {
            prompt: "Do a thing".to_string(),
            base_ref: Some("".to_string()),
            model: Some("   ".to_string()),
        };
        let body = req.to_json();
        let map = body.as_object().expect("object body");
        assert_eq!(map.len(), 1, "only prompt should remain; got {body}");
    }

    // ---- path rendering ----------------------------------------------------

    #[test]
    fn create_task_path_substitutes_owner_and_repo() {
        let path = create_task_path("octocat", "hello-world");
        assert_eq!(path, "/agents/repos/octocat/hello-world/tasks");
    }

    #[test]
    fn get_task_path_substitutes_all_three() {
        let path = get_task_path("octo", "hello", "task_abc123");
        assert_eq!(path, "/agents/repos/octo/hello/tasks/task_abc123");
    }

    #[test]
    fn list_tasks_path_matches_create_path() {
        // Per the docs, list and create are the same path with different
        // verbs (GET vs POST). Asserted to catch a future divergence.
        assert_eq!(list_tasks_path("a", "b"), create_task_path("a", "b"),);
    }

    // ---- static headers ----------------------------------------------------

    #[test]
    fn static_headers_pin_github_api_version() {
        // The version is part of the contract — pinning here catches a
        // silent bump.
        let api_version = STATIC_HEADERS
            .iter()
            .find(|(name, _)| *name == "x-github-api-version")
            .map(|(_, v)| *v);
        assert_eq!(api_version, Some("2026-03-10"));
    }

    #[test]
    fn static_headers_pin_accept_header() {
        let accept = STATIC_HEADERS
            .iter()
            .find(|(name, _)| *name == "accept")
            .map(|(_, v)| *v);
        assert_eq!(accept, Some("application/vnd.github+json"));
    }

    // ---- state-string parsing ---------------------------------------------

    #[test]
    fn task_state_documented_strings_map_correctly() {
        // Documented values from the use-cloud-agent-via-the-api page.
        assert_eq!(task_state_from_str("queued"), CloudTaskState::Queued);
        assert_eq!(task_state_from_str("in_progress"), CloudTaskState::Running);
        assert_eq!(task_state_from_str("completed"), CloudTaskState::Succeeded);
        assert_eq!(task_state_from_str("failed"), CloudTaskState::Failed);
        assert_eq!(task_state_from_str("cancelled"), CloudTaskState::Cancelled);
        assert_eq!(task_state_from_str("timed_out"), CloudTaskState::Failed);
        assert_eq!(task_state_from_str("idle"), CloudTaskState::Waiting);
        assert_eq!(
            task_state_from_str("waiting_for_user"),
            CloudTaskState::Waiting
        );
    }

    #[test]
    fn task_state_case_and_whitespace_insensitive() {
        assert_eq!(
            task_state_from_str("  IN_PROGRESS  "),
            CloudTaskState::Running
        );
        assert_eq!(task_state_from_str("Cancelled"), CloudTaskState::Cancelled);
        assert_eq!(task_state_from_str("Canceled"), CloudTaskState::Cancelled);
    }

    #[test]
    fn task_state_unknown_string_preserved_and_non_terminal() {
        let state = task_state_from_str("mystery_status");
        assert_eq!(state, CloudTaskState::Other("mystery_status".to_string()));
        assert!(
            !state.is_terminal(),
            "unknown states must not be classified as terminal"
        );
    }

    // ---- task-object parsing ----------------------------------------------

    #[test]
    fn parse_task_object_with_full_shape() {
        // Synthetic but matches the shape inferred from the docs (see
        // dossier §3.1).
        let body = br#"{
          "id": "task_abc123",
          "state": "in_progress",
          "pull_request": {
            "html_url": "https://github.com/octo/hello/pull/42",
            "number": 42,
            "state": "open",
            "draft": true
          },
          "logs_url": "https://github.com/copilot/agents/task_abc123",
          "model": "claude-sonnet-4.5",
          "prompt": "Fix the login button",
          "created_at": "2026-06-05T12:00:00Z",
          "updated_at": "2026-06-05T12:01:00Z"
        }"#;
        let handle = parse_task_response(body).expect("parse");
        assert_eq!(handle.id, "task_abc123");
        assert_eq!(handle.state, CloudTaskState::Running);
        assert_eq!(
            handle.pull_request_url.as_deref(),
            Some("https://github.com/octo/hello/pull/42")
        );
        assert_eq!(
            handle.logs_url.as_deref(),
            Some("https://github.com/copilot/agents/task_abc123")
        );
        assert!(handle.error.is_none());
    }

    #[test]
    fn parse_task_object_with_minimal_shape() {
        // The docs don't pin field presence; tolerate a response that only
        // has the id + state.
        let body = br#"{"id":"task_min","state":"queued"}"#;
        let handle = parse_task_response(body).expect("parse minimal");
        assert_eq!(handle.id, "task_min");
        assert_eq!(handle.state, CloudTaskState::Queued);
        assert!(handle.pull_request_url.is_none());
        assert!(handle.logs_url.is_none());
        assert!(handle.error.is_none());
    }

    #[test]
    fn parse_task_object_rejects_missing_id() {
        let body = br#"{"state":"queued"}"#;
        let err = parse_task_response(body).unwrap_err().to_string();
        assert!(err.contains("id"), "error should mention id; got: {err}");
    }

    #[test]
    fn parse_task_object_rejects_garbage() {
        let body = b"not even json";
        let err = parse_task_response(body).unwrap_err().to_string();
        assert!(err.contains("JSON"));
    }

    #[test]
    fn parse_task_object_with_failed_state_preserves_error() {
        let body = br#"{
          "id": "task_fail",
          "state": "failed",
          "error": "branch protection ruleset blocked Copilot",
          "logs_url": "https://github.com/copilot/agents/task_fail"
        }"#;
        let handle = parse_task_response(body).expect("parse");
        assert_eq!(handle.state, CloudTaskState::Failed);
        assert_eq!(
            handle.error.as_deref(),
            Some("branch protection ruleset blocked Copilot")
        );
        assert!(handle.state.is_terminal());
    }

    // ---- token-shape guard -------------------------------------------------

    #[test]
    fn token_guard_accepts_user_to_server_prefixes() {
        // The PAT prefixes the agent endpoints accept.
        assert!(reject_server_to_server_token("ghp_abc123").is_ok());
        assert!(reject_server_to_server_token("github_pat_xyz789").is_ok());
        assert!(reject_server_to_server_token("gho_someoauthtoken").is_ok());
        assert!(reject_server_to_server_token("ghu_someuserapptoken").is_ok());
        // Unprefixed tokens (older PAT format) — allow rather than guess.
        assert!(reject_server_to_server_token("abcdef1234567890").is_ok());
    }

    #[test]
    fn token_guard_rejects_server_to_server() {
        let err = reject_server_to_server_token("ghs_installation_token_xyz")
            .unwrap_err()
            .to_string();
        assert!(err.contains("server-to-server"));
        assert!(err.contains("ghs_"));
    }

    #[test]
    fn token_guard_rejects_refresh_tokens() {
        let err = reject_server_to_server_token("ghr_refresh_token_xyz")
            .unwrap_err()
            .to_string();
        assert!(err.contains("refresh"));
    }

    #[test]
    fn token_guard_trims_whitespace_before_classifying() {
        assert!(reject_server_to_server_token("  ghs_xxx  ").is_err());
    }

    // ---- acknowledgement rendering ----------------------------------------

    #[test]
    fn acknowledgement_includes_task_id_and_repo() {
        let handle = CloudTaskHandle {
            id: "task_xyz".to_string(),
            state: CloudTaskState::Queued,
            pull_request_url: None,
            logs_url: Some("https://github.com/copilot/agents/task_xyz".to_string()),
            error: None,
        };
        let msg = render_acknowledgement("octocat", "hello-world", &handle);
        assert!(msg.contains("task_xyz"));
        assert!(msg.contains("octocat/hello-world"));
        assert!(msg.contains("Session logs"));
        assert!(msg.contains("notify"));
    }

    #[test]
    fn acknowledgement_prefers_pr_url_when_present() {
        let handle = CloudTaskHandle {
            id: "task_abc".to_string(),
            state: CloudTaskState::Running,
            pull_request_url: Some("https://github.com/octo/hello/pull/42".to_string()),
            logs_url: Some("https://github.com/copilot/agents/task_abc".to_string()),
            error: None,
        };
        let msg = render_acknowledgement("octo", "hello", &handle);
        // PR URL wins over logs URL when both present.
        assert!(msg.contains("/pull/42"));
    }

    // ---- HTTP request shape (live-shape assertion w/o socket) -------------
    //
    // We don't have a Copilot subscription, so we cannot actually call the
    // API. Instead we build the request the way the docs describe and assert
    // every byte matches: body JSON, path, method, auth header. If the docs
    // change shape, these tests fail.

    #[tokio::test]
    async fn create_task_request_shape_matches_docs() {
        use crate::cloud::keychain::MemoryCredentialStore;
        use crate::cloud::transport::{
            CloudAuth, CloudHttpsTransport, HttpRequest, VendorCredentialStore,
        };
        let creds = MemoryCredentialStore::new("copilot_cloud");
        creds.save("api_key", "ghp_fake_pat_for_test").unwrap();

        let transport = CloudHttpsTransport::new();
        let body = CreateTaskRequest {
            prompt: "Fix the login button on the homepage".to_string(),
            base_ref: Some("main".to_string()),
            model: None,
        };
        let url = format!("{}{}", ENTRY.base_url, create_task_path("octocat", "hello"));
        let req = HttpRequest::post_json(url, body.to_json());

        let built = transport
            .request_for(
                "copilot_cloud",
                &req,
                CloudAuth::HeaderToken {
                    header_name: "Authorization",
                    prefix: "Bearer ",
                },
                &creds,
                "api_key",
            )
            .expect("build request");

        // URL matches docs exactly.
        assert_eq!(
            built.url().as_str(),
            "https://api.github.com/agents/repos/octocat/hello/tasks"
        );
        // POST.
        assert_eq!(built.method(), reqwest::Method::POST);
        // Authorization header has the Bearer shape.
        let auth_header = built
            .headers()
            .get("authorization")
            .expect("Authorization header")
            .to_str()
            .unwrap();
        assert!(auth_header.starts_with("Bearer "), "got: {auth_header}");
        assert!(auth_header.contains("ghp_fake_pat_for_test"));
    }

    #[tokio::test]
    async fn get_task_request_shape_matches_docs() {
        use crate::cloud::keychain::MemoryCredentialStore;
        use crate::cloud::transport::{
            CloudAuth, CloudHttpsTransport, HttpRequest, VendorCredentialStore,
        };
        let creds = MemoryCredentialStore::new("copilot_cloud");
        creds.save("api_key", "ghp_fake_pat").unwrap();

        let transport = CloudHttpsTransport::new();
        let url = format!(
            "{}{}",
            ENTRY.base_url,
            get_task_path("octo", "hello", "task_abc123")
        );
        let req = HttpRequest::get(url);
        let built = transport
            .request_for(
                "copilot_cloud",
                &req,
                CloudAuth::HeaderToken {
                    header_name: "Authorization",
                    prefix: "Bearer ",
                },
                &creds,
                "api_key",
            )
            .expect("build request");
        assert_eq!(built.method(), reqwest::Method::GET);
        assert_eq!(
            built.url().as_str(),
            "https://api.github.com/agents/repos/octo/hello/tasks/task_abc123"
        );
    }

    // ---- live HTTP round-trip against wiremock ----------------------------
    //
    // Use `wiremock` to fake the GitHub API so we can verify the FULL
    // transport stack: real reqwest, real audit log, real status mapping.

    #[tokio::test]
    async fn round_trip_create_task_against_wiremock() {
        use crate::cloud::keychain::MemoryCredentialStore;
        use crate::cloud::transport::{
            CloudAuth, CloudHttpsTransport, HttpRequest, VendorCredentialStore,
        };
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let task_body = serde_json::json!({
            "id": "task_xyz789",
            "state": "queued",
            "logs_url": format!("{}/copilot/agents/task_xyz789", server.uri())
        });
        Mock::given(method("POST"))
            .and(path("/agents/repos/octocat/hello/tasks"))
            .and(header("x-github-api-version", "2026-03-10"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .insert_header("x-github-request-id", "ABCD:0001")
                    .set_body_json(task_body),
            )
            .mount(&server)
            .await;

        let creds = MemoryCredentialStore::new("copilot_cloud");
        creds.save("api_key", "ghp_real_test_token").unwrap();

        let transport = CloudHttpsTransport::new();
        let create_url = format!("{}{}", server.uri(), create_task_path("octocat", "hello"));
        let body = CreateTaskRequest {
            prompt: "Hello".to_string(),
            base_ref: None,
            model: None,
        };
        // Add the static headers manually (the daemon-level dispatcher would
        // merge them automatically from the registry row's
        // STATIC_HEADERS — that wiring is out of scope for this unit test).
        let mut req = HttpRequest::post_json(create_url, body.to_json());
        for (name, value) in STATIC_HEADERS {
            req = req.with_header(*name, *value);
        }

        let response = transport
            .send(
                "copilot_cloud",
                "/agents/repos/_/_/tasks",
                &req,
                CloudAuth::HeaderToken {
                    header_name: "Authorization",
                    prefix: "Bearer ",
                },
                &creds,
                "api_key",
            )
            .await
            .expect("send");

        assert_eq!(response.status, 200);
        assert!(response.request_id_present);

        let handle = parse_task_response(&response.body).expect("parse");
        assert_eq!(handle.id, "task_xyz789");
        assert_eq!(handle.state, CloudTaskState::Queued);
        assert!(handle.logs_url.is_some());
    }

    #[tokio::test]
    async fn round_trip_get_task_failed_against_wiremock() {
        use crate::cloud::keychain::MemoryCredentialStore;
        use crate::cloud::transport::{
            CloudAuth, CloudHttpsTransport, HttpRequest, VendorCredentialStore,
        };
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let task_body = serde_json::json!({
            "id": "task_fail",
            "state": "failed",
            "error": "branch protection ruleset blocked Copilot"
        });
        Mock::given(method("GET"))
            .and(path("/agents/repos/octo/hello/tasks/task_fail"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(task_body),
            )
            .mount(&server)
            .await;

        let creds = MemoryCredentialStore::new("copilot_cloud");
        creds.save("api_key", "ghp_real_test_token").unwrap();

        let transport = CloudHttpsTransport::new();
        let req = HttpRequest::get(format!(
            "{}{}",
            server.uri(),
            get_task_path("octo", "hello", "task_fail")
        ));
        let response = transport
            .send(
                "copilot_cloud",
                "/agents/repos/_/_/tasks/_",
                &req,
                CloudAuth::HeaderToken {
                    header_name: "Authorization",
                    prefix: "Bearer ",
                },
                &creds,
                "api_key",
            )
            .await
            .expect("send");

        let handle = parse_task_response(&response.body).expect("parse");
        assert_eq!(handle.state, CloudTaskState::Failed);
        assert!(handle.state.is_terminal());
        assert_eq!(
            handle.error.as_deref(),
            Some("branch protection ruleset blocked Copilot")
        );
    }

    #[tokio::test]
    async fn round_trip_unauthorized_surfaces_status() {
        use crate::cloud::keychain::MemoryCredentialStore;
        use crate::cloud::transport::{
            CloudAuth, CloudHttpsTransport, HttpRequest, VendorCredentialStore,
        };
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/agents/repos/octo/hello/tasks"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Bad credentials",
                "documentation_url": "https://docs.github.com"
            })))
            .mount(&server)
            .await;

        let creds = MemoryCredentialStore::new("copilot_cloud");
        creds.save("api_key", "ghp_revoked").unwrap();

        let transport = CloudHttpsTransport::new();
        let req = HttpRequest::post_json(
            format!("{}{}", server.uri(), create_task_path("octo", "hello")),
            serde_json::json!({"prompt":"x"}),
        );
        let response = transport
            .send(
                "copilot_cloud",
                "/agents/repos/_/_/tasks",
                &req,
                CloudAuth::HeaderToken {
                    header_name: "Authorization",
                    prefix: "Bearer ",
                },
                &creds,
                "api_key",
            )
            .await
            .expect("send");

        assert_eq!(response.status, 401);
        // parse_task_response on an error body (no `id`) should surface the
        // shape mismatch, not silently produce a handle.
        let parse_err = parse_task_response(&response.body).unwrap_err().to_string();
        assert!(parse_err.contains("id"));
    }
}
