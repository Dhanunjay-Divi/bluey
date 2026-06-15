//! Auth/login self-resolver — detect a **not-authenticated** agent and, on
//! consent, trigger the agent's OWN login flow in-product instead of telling
//! the user to go type a command.
//!
//! See `docs/work/PLAN-PRODUCTION-VISION.md`. The autonomy model is
//! **propose + approve**: Bluey detects the not-authed state from a real
//! drive/launch error, PROPOSES running the agent's login, and on approval
//! TRIGGERS the login flow itself (surfacing the browser/device-code prompt).
//! It never just punts ("go run `cursor-agent login`").
//!
//! Flow (mirrors `provision.rs`'s plan/run consent split):
//!
//! ```text
//! classify_error(err)  → AuthDiagnosis
//!     (NeedsLogin { … } | NeedsLoginNoCommand | NotAnAuthProblem)
//! plan_login(agent)    → a LoginPlan describing exactly what would run
//! (consent happens at the call site — Bluey shows the plan, the user approves)
//! run_login(plan)      → spawn the agent's OWN login command (interactive)
//! ```
//!
//! Security / trust invariants:
//! - **Data-driven.** The login command comes from the registry row's
//!   [`crate::registry::AgentEntry::login_command`] — never an `if agent == …`
//!   branch, never a string from user input or the network.
//! - **Consent-gated.** This module never logs in on its own; it produces a
//!   [`LoginPlan`] for the caller to confirm, then runs it only when asked.
//! - **Bluey never handles credentials.** It only launches the agent's own
//!   login flow; the agent's flow (browser/device-code) stores its own token.
//!   Bluey captures nothing.
//! - **Headless-friendly.** Where an agent supports it (e.g. cursor-agent's
//!   `NO_OPEN_BROWSER`), the trigger surfaces the device-code/URL instead of
//!   trying to pop a browser, so it works even when Bluey is the overlay.
//! - **Fail-soft.** Every failure is a value, never a panic.

use std::process::{Command, Stdio};

use crate::registry::{self, LoginAuth};
use crate::AgentKind;

/// What a drive/launch error tells us about the failure's *cause*. The login
/// trigger only acts on [`AuthDiagnosis::NeedsLogin`]; an auth failure for an
/// agent with no CLI login is [`AuthDiagnosis::NeedsLoginNoCommand`] (the caller
/// guides the user), and every non-auth shape (missing binary, runtime-version
/// mismatch, model-blocked, rate-limited, …) is deliberately classified as
/// [`AuthDiagnosis::NotAnAuthProblem`] so login is never triggered for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthDiagnosis {
    /// The failure is an authentication problem AND this agent has a CLI login
    /// command Bluey can trigger (consent-gated). Carries the agent and the
    /// exact command to run. This is the only variant [`run_login`] acts on.
    NeedsLogin(NeedsLogin),
    /// The failure IS an authentication problem, but this agent has no CLI
    /// login flow to trigger (e.g. Gemini authenticates on first run / via
    /// `GEMINI_API_KEY`; Copilot via `GH_TOKEN`). Honest, distinct state: Bluey
    /// cannot run a login for the user here, so the caller guides them instead
    /// of pretending it can self-resolve. Carries the agent.
    NeedsLoginNoCommand { agent: AgentKind },
    /// The failure is something other than auth (missing binary, runtime
    /// version, model availability, network, trust prompt, …). The login flow
    /// must NOT be triggered.
    NotAnAuthProblem,
}

/// A typed "this agent needs to log in" result, with the exact command to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeedsLogin {
    /// Which agent needs login.
    pub agent: AgentKind,
    /// The agent's own login invocation (program + args), straight from the
    /// registry row — e.g. `["cursor-agent", "login"]`. Empty only if a row
    /// has no login command (an agent that can't be logged in via CLI).
    pub login_command: &'static [&'static str],
}

/// A concrete, inspectable description of what triggering an agent's login
/// would do — shown to the user for consent before anything runs. The login
/// analogue of `provision::InstallPlan`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginPlan {
    pub agent: AgentKind,
    /// The login invocation (program + args) from the registry row.
    pub command: &'static [&'static str],
    /// A human-readable one-line description of the command, for the consent
    /// prompt (e.g. `cursor-agent login`). Includes any device-code args.
    pub human_command: String,
    /// How this agent's login is made headless-friendly (no-browser env var,
    /// device-code args, or plain interactive) — copied from the registry row
    /// so [`run_login`] applies it without any per-agent branch.
    pub auth: LoginAuth,
}

impl LoginPlan {
    /// Whether this login can surface a device-code / printed URL instead of
    /// opening a browser (so it works when Bluey is the overlay).
    pub fn supports_device_code(&self) -> bool {
        matches!(
            self.auth,
            LoginAuth::NoBrowserEnv { .. } | LoginAuth::DeviceCodeArgs(_)
        )
    }
}

/// Outcome of running a [`LoginPlan`]. The login itself is INTERACTIVE — Bluey
/// starts it and the user finishes in the browser/device-code prompt — so a
/// clean spawn-and-complete reports [`LoginOutcome::LoginFlowCompleted`]
/// (the child exited 0), while a child that never started or exited non-zero
/// is reported distinctly. Bluey never inspects or stores any credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginOutcome {
    /// The login child process ran to a clean exit (0). For most agents this
    /// means the user completed the browser/device-code flow and the agent
    /// stored its own token. The caller should re-probe drive to confirm.
    LoginFlowCompleted { binary: String },
    /// The login child ran but exited non-zero (user aborted, timed out, the
    /// agent reported a login error). Carries the tail of its output.
    LoginFailed { detail: String },
    /// The login command could not even be spawned (binary not on PATH, spawn
    /// error). Carries the reason.
    CouldNotStart { detail: String },
    /// This agent has no CLI login command in the registry, so there is nothing
    /// to trigger (e.g. an agent that authenticates only via an interactive
    /// first-run, or only via an env-var API key). The caller should fall back
    /// to guiding the user. Carries a human reason.
    NoLoginCommand { detail: String },
}

/// Detector: classify a drive/launch error string as an auth problem (or not),
/// for a known agent. Generic across agents — it matches on the *shape* of the
/// message (the phrases CLIs use for "not signed in"), never on a per-agent
/// hardcoded string. Returns [`AuthDiagnosis::NeedsLogin`] with the agent's
/// registry login command when the message is auth-shaped AND the agent has a
/// login command; otherwise [`AuthDiagnosis::NotAnAuthProblem`].
///
/// `error` is whatever the drive layer surfaced — typically the text of a
/// terminal [`crate::AnswerChunk::Error`] (which carries the agent's stderr),
/// or a launch error. Case-insensitive.
pub fn classify_error(agent: &AgentKind, error: &str) -> AuthDiagnosis {
    if !looks_like_auth_error(error) {
        return AuthDiagnosis::NotAnAuthProblem;
    }
    // It's auth-shaped. If we have a login command to offer, this is a
    // self-resolvable NeedsLogin; otherwise it's still an auth problem but one
    // Bluey can't trigger a login for (the caller guides the user instead).
    match login_command_for(agent) {
        Some(cmd) if !cmd.is_empty() => AuthDiagnosis::NeedsLogin(NeedsLogin {
            agent: agent.clone(),
            login_command: cmd,
        }),
        _ => AuthDiagnosis::NeedsLoginNoCommand {
            agent: agent.clone(),
        },
    }
}

/// The generic auth-error matcher. True when the message contains a phrase that
/// CLIs use to mean "you are not authenticated / please sign in". This is the
/// heart of the detector and is deliberately conservative: it must fire on the
/// real cursor-agent / codex / claude / gemini not-authed messages, yet NOT on
/// a runtime-version mismatch, a missing-binary error, a model-availability
/// error (Gemini's `429 No capacity`), or a rate-limit.
///
/// Matched signals (case-insensitive substrings), drawn from REAL CLI output:
/// - "authentication required" / "authentication failed" — cursor-agent.
/// - "authenticate" — cursor-agent's invalid-key message
///   ("…or authenticate without it.").
/// - "not signed in" / "sign in" / "please sign in" — codex / claude.
/// - "not logged in" / "log in" / "please log in" / "please run … login" /
///   "run `<bin> login`" — generic.
/// - "unauthorized" / "401" / "403 forbidden" (auth-scoped) — HTTP auth.
/// - "CURSOR_API_KEY" / "OPENAI_API_KEY" / "ANTHROPIC_API_KEY" /
///   "GEMINI_API_KEY" — "set the API key" hints imply a missing credential.
/// - "invalid api key" / "api key is invalid" / "expired token" /
///   "token … expired" / "credentials" (with login/auth nearby).
///
/// Carefully EXCLUDED (must stay NotAnAuthProblem) — these are handled by the
/// runtime / model-fallback / provisioning resolvers, not here:
/// - "requires node" / version mismatches (runtime resolver).
/// - "command not found" / "no such file" / "not on PATH" (provisioning).
/// - "no capacity" / "model not found" / "overloaded" (model-fallback).
/// - bare "429" / "rate limit" (transient; retried, not a login).
fn looks_like_auth_error(error: &str) -> bool {
    let e = error.to_ascii_lowercase();

    // ---- Hard exclusions first: shapes that are NEVER an auth problem, even
    // if a stray word like "key" appears. These belong to sibling resolvers. --
    const NOT_AUTH: &[&str] = &[
        "requires node",
        "requires node.js",
        "node version",
        "unsupported node",
        "command not found",
        "no such file or directory",
        "not on path",
        "did not appear on path",
        "no capacity available",
        "model not found",
        "unknown model",
        "is overloaded",
        "context length",
        "maximum context",
    ];
    // A pure rate-limit (429 / "rate limit") with NO auth phrasing is transient,
    // not a login problem. We only bail here when there's no auth signal too.
    if NOT_AUTH.iter().any(|p| e.contains(p)) {
        return false;
    }

    // ---- Positive auth signals. ------------------------------------------
    const AUTH_PHRASES: &[&str] = &[
        "authentication required",
        "authentication failed",
        "authentication error",
        "not authenticated",
        "must authenticate",
        "please authenticate",
        "authenticate without it", // cursor-agent invalid-key message tail
        "reauthenticate",
        "re-authenticate",
        "not signed in",
        "please sign in",
        "you are not signed in",
        "sign in to continue",
        "not logged in",
        "please log in",
        "you must log in",
        "log in to continue",
        "login required",
        "please login",
        "run 'agent login'", // cursor-agent's exact instruction
        "run \"agent login\"",
        "agent login' first",
        "login' first",
        "login\" first",
        "unauthorized",
        "401 unauthorized",
        "403 forbidden",
        "invalid api key",
        "api key is invalid",
        "invalid_api_key",
        "expired token",
        "token has expired",
        "token expired",
        "session expired",
        "credentials are invalid",
        "invalid credentials",
        "missing credentials",
        // Env-var "set your API key" hints — agent-name-agnostic; the var name
        // itself is the signal that a credential is absent.
        "cursor_api_key",
        "openai_api_key",
        "anthropic_api_key",
        "gemini_api_key",
        "google_api_key",
        "api key",
    ];
    if AUTH_PHRASES.iter().any(|p| e.contains(p)) {
        return true;
    }

    // Generic "please run <something> login" / "run `<bin> login`" — match a
    // "login" token that is preceded by a run/please-run instruction, which is
    // how every agent phrases "go authenticate". This catches future agents
    // without enumerating their names.
    let has_login_word = e.contains("login") || e.contains("log in");
    let has_run_instruction = e.contains("please run")
        || e.contains("run `")
        || e.contains("run '")
        || e.contains("run \"");
    if has_login_word && has_run_instruction {
        return true;
    }

    false
}

/// The registry login command for an agent, if it has one. Pure data — reads
/// the row's `login_command`. `None` for agents with no CLI login (or unknown).
pub fn login_command_for(agent: &AgentKind) -> Option<&'static [&'static str]> {
    let tag = registry::KindTag::from_agent_kind(agent)?;
    registry::entry_for(tag).and_then(|e| e.login_command)
}

/// Build a [`LoginPlan`] for an agent, if it has a registry login command.
/// Returns `None` for agents with no CLI login (VS Code, Windsurf, Aider, …).
/// Does no I/O — never spawns anything.
pub fn plan_login(agent: &AgentKind) -> Option<LoginPlan> {
    let tag = registry::KindTag::from_agent_kind(agent)?;
    let entry = registry::entry_for(tag)?;
    let command = entry.login_command?;
    if command.is_empty() {
        return None;
    }
    // The human-readable form shows exactly what will run, including any
    // device-code args we'll append for headless friendliness.
    let mut human = command.join(" ");
    if let LoginAuth::DeviceCodeArgs(extra) = entry.login_auth {
        if !extra.is_empty() {
            human.push(' ');
            human.push_str(&extra.join(" "));
        }
    }
    Some(LoginPlan {
        agent: agent.clone(),
        command,
        human_command: human,
        auth: entry.login_auth,
    })
}

/// Run an approved [`LoginPlan`]: spawn the agent's OWN login command so the
/// user can complete the interactive browser/device-code flow. Bluey starts it
/// and lets the user finish; it never captures or stores credentials.
///
/// Caller MUST have obtained user consent before calling this — it launches a
/// process. The child inherits stdio (so the device-code/URL and prompts reach
/// the user's terminal); for agents that support a no-browser mode we set the
/// agent's env var (e.g. `NO_OPEN_BROWSER=1`) so the code/URL is PRINTED rather
/// than a browser being popped — essential when Bluey is the overlay. Never
/// panics; every failure is a [`LoginOutcome`].
pub fn run_login(plan: &LoginPlan) -> LoginOutcome {
    let Some((program, args)) = plan.command.split_first() else {
        return LoginOutcome::NoLoginCommand {
            detail: format!("{:?} has no login command", plan.agent),
        };
    };

    let mut cmd = Command::new(program);
    cmd.args(args);

    // Make the flow headless-friendly using the row's DATA — no per-agent
    // branch. Either set a no-browser env var (cursor-agent) or append
    // device-code args (codex); a plain interactive login runs as-is.
    match plan.auth {
        LoginAuth::NoBrowserEnv { name, value } => {
            cmd.env(name, value);
        }
        LoginAuth::DeviceCodeArgs(extra) => {
            cmd.args(extra);
        }
        LoginAuth::InteractiveOnly | LoginAuth::None => {}
    }

    // Interactive: the user completes the flow. Inherit stdio so prompts, the
    // device code, and the verification URL reach the user. Bluey starts the
    // agent's own flow and never reads or stores any credential.
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    match cmd.status() {
        Ok(status) if status.success() => LoginOutcome::LoginFlowCompleted {
            binary: program.to_string(),
        },
        Ok(status) => LoginOutcome::LoginFailed {
            detail: match status.code() {
                Some(code) => format!("`{}` exited with status {code}", plan.human_command),
                None => format!("`{}` was terminated by a signal", plan.human_command),
            },
        },
        Err(e) => LoginOutcome::CouldNotStart {
            detail: format!("could not start `{}`: {e}", plan.human_command),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- The detector on REAL not-authed strings (captured live from the
    // installed CLIs on 2026-06) → must classify as NeedsLogin. ------------

    #[test]
    fn classifies_real_cursor_agent_auth_required_as_needs_login() {
        // cursor-agent's documented not-authed error (verbatim).
        let err = "Authentication required. Please run 'agent login' first, \
                   or set CURSOR_API_KEY environment variable.";
        let diag = classify_error(&AgentKind::Cursor, err);
        match diag {
            AuthDiagnosis::NeedsLogin(n) => {
                assert_eq!(n.agent, AgentKind::Cursor);
                assert_eq!(n.login_command, &["cursor-agent", "login"]);
            }
            other => panic!("expected NeedsLogin, got {other:?}"),
        }
    }

    #[test]
    fn classifies_real_cursor_agent_invalid_key_as_needs_login() {
        // cursor-agent's REAL invalid-key output (captured live with a bogus
        // --api-key): "Warning: The provided API key is invalid. … or
        // authenticate without it."
        let err = "⚠ Warning: The provided API key is invalid.\n\
                   Please check you have the right key, create a new one, \
                   or authenticate without it.";
        assert!(matches!(
            classify_error(&AgentKind::Cursor, err),
            AuthDiagnosis::NeedsLogin(_)
        ));
    }

    #[test]
    fn classifies_codex_not_signed_in_as_needs_login() {
        // codex's not-signed-in phrasing.
        for err in [
            "Error: not signed in. Please run `codex login`.",
            "You are not signed in. Run codex login to authenticate.",
            "401 Unauthorized: refresh token already used",
        ] {
            assert!(
                matches!(
                    classify_error(&AgentKind::Codex, err),
                    AuthDiagnosis::NeedsLogin(_)
                ),
                "codex auth error not classified: {err:?}"
            );
        }
    }

    #[test]
    fn classifies_claude_login_prompt_as_needs_login() {
        // Claude HAS a CLI login (`claude setup-token`) → self-resolvable.
        assert!(matches!(
            classify_error(
                &AgentKind::ClaudeCode,
                "Invalid API key · Please run /login"
            ),
            AuthDiagnosis::NeedsLogin(_)
        ));
    }

    #[test]
    fn gemini_auth_error_is_needs_login_no_command() {
        // Gemini's failure IS auth-shaped, but it has no CLI login subcommand —
        // so the honest result is NeedsLoginNoCommand (Bluey can't run a login;
        // the caller guides the user to set GEMINI_API_KEY / sign in on first
        // run), NOT a misleading NeedsLogin nor a swallowed NotAnAuthProblem.
        assert_eq!(
            classify_error(
                &AgentKind::Gemini,
                "Please set GEMINI_API_KEY or sign in to continue."
            ),
            AuthDiagnosis::NeedsLoginNoCommand {
                agent: AgentKind::Gemini
            }
        );
    }

    // ---- The detector must NOT misclassify non-auth failures. ------------

    #[test]
    fn does_not_classify_runtime_version_error_as_login() {
        // The GitHub Copilot CLI's real "wrong Node" launch error → runtime
        // resolver's job, NOT a login.
        let err = "The GitHub Copilot CLI requires Node.js version 24 or newer. \
                   You are running v20.11.0.";
        assert_eq!(
            classify_error(&AgentKind::Copilot, err),
            AuthDiagnosis::NotAnAuthProblem
        );
    }

    #[test]
    fn does_not_classify_missing_binary_as_login() {
        let err = "could not run `cursor-agent`: No such file or directory (os error 2)";
        assert_eq!(
            classify_error(&AgentKind::Cursor, err),
            AuthDiagnosis::NotAnAuthProblem
        );
        let err2 = "agent cursor-agent: command not found";
        assert_eq!(
            classify_error(&AgentKind::Cursor, err2),
            AuthDiagnosis::NotAnAuthProblem
        );
    }

    #[test]
    fn does_not_classify_model_capacity_error_as_login() {
        // Gemini's REAL model-blocked error (captured live) — model-fallback
        // resolver's job. It even contains "Authorization:" in a header dump,
        // so this guards against a naive substring match.
        let err = "Attempt 1 failed with status 429. Retrying with backoff... \
                   _GaxiosError: No capacity available for model \
                   gemini-3.1-flash-lite on the server. \
                   headers: { Authorization: '<<REDACTED>>' }";
        assert_eq!(
            classify_error(&AgentKind::Gemini, err),
            AuthDiagnosis::NotAnAuthProblem
        );
    }

    #[test]
    fn does_not_classify_plain_rate_limit_or_overload_as_login() {
        assert_eq!(
            classify_error(
                &AgentKind::Codex,
                "Error: 429 rate limit exceeded, retry later"
            ),
            AuthDiagnosis::NotAnAuthProblem
        );
        assert_eq!(
            classify_error(&AgentKind::ClaudeCode, "Error: model is overloaded (529)"),
            AuthDiagnosis::NotAnAuthProblem
        );
    }

    #[test]
    fn auth_shaped_error_for_agent_without_login_command_is_needs_login_no_command() {
        // Windsurf / VS Code have no CLI login command. An auth-shaped error
        // must NOT yield NeedsLogin (nothing to trigger), but it IS still an
        // auth problem → NeedsLoginNoCommand so the caller can guide the user.
        let err = "Unauthorized: please sign in.";
        assert_eq!(
            classify_error(&AgentKind::Windsurf, err),
            AuthDiagnosis::NeedsLoginNoCommand {
                agent: AgentKind::Windsurf
            }
        );
        assert_eq!(
            classify_error(&AgentKind::VsCodeFork, err),
            AuthDiagnosis::NeedsLoginNoCommand {
                agent: AgentKind::VsCodeFork
            }
        );
    }

    // ---- The login-command map (data-driven, from the registry rows). ----

    #[test]
    fn login_command_map_matches_real_cli_subcommands() {
        // Verified via `<bin> --help` on a real machine (2026-06):
        // cursor-agent login, codex login, claude setup-token. gemini has no
        // non-interactive login subcommand (authenticates on first run / via
        // GEMINI_API_KEY) → no login command.
        assert_eq!(
            login_command_for(&AgentKind::Cursor),
            Some(&["cursor-agent", "login"][..])
        );
        assert_eq!(
            login_command_for(&AgentKind::Codex),
            Some(&["codex", "login"][..])
        );
        assert_eq!(
            login_command_for(&AgentKind::ClaudeCode),
            Some(&["claude", "setup-token"][..])
        );
        // The Claude app rows drive through the same `claude` CLI, so they
        // share its login command.
        assert_eq!(
            login_command_for(&AgentKind::ClaudeCodeApp),
            Some(&["claude", "setup-token"][..])
        );
        // Gemini: no CLI login subcommand exists → None (guide the user).
        assert_eq!(login_command_for(&AgentKind::Gemini), None);
        // Agents with no CLI at all → None.
        assert_eq!(login_command_for(&AgentKind::Windsurf), None);
        assert_eq!(login_command_for(&AgentKind::VsCodeFork), None);
        assert_eq!(login_command_for(&AgentKind::Unknown), None);
    }

    // ---- plan_login: the consent surface. --------------------------------

    #[test]
    fn plan_login_describes_cursor_command_and_device_code_support() {
        let plan = plan_login(&AgentKind::Cursor).expect("cursor has a login command");
        assert_eq!(plan.command, &["cursor-agent", "login"]);
        assert_eq!(plan.human_command, "cursor-agent login");
        assert!(
            plan.supports_device_code(),
            "cursor-agent supports NO_OPEN_BROWSER"
        );
        assert!(
            matches!(
                plan.auth,
                LoginAuth::NoBrowserEnv {
                    name: "NO_OPEN_BROWSER",
                    ..
                }
            ),
            "cursor-agent uses the NO_OPEN_BROWSER env var"
        );
    }

    #[test]
    fn plan_login_codex_folds_device_code_args_into_command() {
        let plan = plan_login(&AgentKind::Codex).expect("codex has a login command");
        assert_eq!(plan.command, &["codex", "login"]);
        // The device-code args are shown to the user AND appended at run time.
        assert_eq!(plan.human_command, "codex login --device-auth");
        assert!(plan.supports_device_code());
        assert!(matches!(plan.auth, LoginAuth::DeviceCodeArgs(_)));
    }

    #[test]
    fn plan_login_none_for_agents_without_login_command() {
        assert!(plan_login(&AgentKind::Gemini).is_none());
        assert!(plan_login(&AgentKind::Windsurf).is_none());
        assert!(plan_login(&AgentKind::VsCodeFork).is_none());
        assert!(plan_login(&AgentKind::Unknown).is_none());
        assert!(plan_login(&AgentKind::Other("x".into())).is_none());
    }

    // ---- run_login: the trigger spawns the RIGHT command, fail-soft. ------

    #[test]
    fn run_login_reports_could_not_start_for_missing_binary() {
        // A plan whose program does not exist must fail soft as CouldNotStart,
        // never panic. (We can't complete a real browser login in a test, but
        // we CAN prove the trigger targets the right program and degrades
        // gracefully.)
        let plan = LoginPlan {
            agent: AgentKind::Other("nope".into()),
            command: &["definitely-not-a-real-binary-xyz123", "login"],
            human_command: "definitely-not-a-real-binary-xyz123 login".into(),
            auth: LoginAuth::None,
        };
        match run_login(&plan) {
            LoginOutcome::CouldNotStart { detail } => {
                assert!(detail.contains("definitely-not-a-real-binary-xyz123"));
            }
            other => panic!("expected CouldNotStart, got {other:?}"),
        }
    }

    #[test]
    fn login_outcomes_are_distinct() {
        // Callers must be able to tell the outcomes apart.
        let completed = LoginOutcome::LoginFlowCompleted {
            binary: "cursor-agent".into(),
        };
        let failed = LoginOutcome::LoginFailed {
            detail: "exited 1".into(),
        };
        assert_ne!(completed, failed);
    }
}
