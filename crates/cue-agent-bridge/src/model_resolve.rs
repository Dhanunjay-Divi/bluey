//! Model-fallback self-resolver — detect a **model-blocked** drive failure and,
//! on consent, either retry under a model the account DOES support or surface
//! the BYOT (API-key) path. Bluey does the work after approval; it never just
//! punts ("go fix OpenAI / get an API key").
//!
//! ## The problem this solves (found by real testing, not theory)
//!
//! The user's Codex CLI is installed and authenticated (via a ChatGPT account),
//! yet every drive fails with the REAL error (captured live, 2026-06):
//!
//! ```text
//! The 'gpt-5.1-codex-max' model is not supported when using Codex with a ChatGPT account.
//! ```
//!
//! As of 2026-06-02 OpenAI restricts newer Codex models for ChatGPT-subscription
//! accounts — they now require an OpenAI API key. The user's `~/.codex/config.toml`
//! pins the blocked model. Telling the user "get an API key" is a punt. A
//! production system DETECTS the model-policy block and either (a) falls back to
//! a model the account supports, or (b) surfaces the BYOT (API-key) path —
//! consent-gated.
//!
//! ## Autonomy model: propose + approve
//!
//! Detecting and *applying* the fix are separate, exactly like the sibling
//! [`crate::runtime_resolve`] and [`crate::auth_resolve`] resolvers:
//!
//! ```text
//! classify_error(agent, err)     → ModelDiagnosis (ModelBlocked { … } | NotAModelProblem)
//! plan_resolution(&blocked)      → a ModelProposal: RetryWithModel { … } or ConnectApiKey { … }
//! (consent happens at the call site — Bluey shows the proposal, the user approves)
//! fallback_retry_args(agent, err) → the argv to RE-DRIVE with (the model-flag pair),
//!                                   or None when the resolution is BYOT
//! ```
//!
//! ## Where this fits in the three-resolver split (coordination)
//!
//! Three resolvers classify a drive error into DISTINCT, non-overlapping causes:
//! - [`crate::runtime_resolve`] — the CLI is present but won't launch on the
//!   active runtime version ("requires Node ≥ 24").
//! - [`crate::auth_resolve`] — the CLI is installed but not signed in
//!   ("not signed in", "unauthorized", "invalid api key", …).
//! - **this module** — the CLI is signed in and runs, but the requested *model*
//!   is not allowed for the account/plan ("model is not supported", "not
//!   supported … with a ChatGPT account", "model_not_found", "does not have
//!   access to model").
//!
//! The boundary is enforced by construction: [`auth_resolve`]'s matcher already
//! hard-excludes "model not found" / "unknown model" / "no capacity" / "is
//! overloaded" (it defers them here), and [`looks_like_model_block`] below
//! requires an explicit *model-availability* phrase that the auth/runtime
//! matchers do not key on. The real Codex string — "… model is not supported
//! when using Codex with a ChatGPT account." — contains no auth phrase
//! (no "api key", "sign in", "unauthorized") and no runtime "requires vN", so
//! the other two resolvers correctly return their negative verdicts for it.
//!
//! ## Invariants
//!
//! - **Data-driven.** The fallback model list and the per-run model flag come
//!   from the registry row ([`crate::registry::AgentEntry::fallback_models`] /
//!   [`crate::registry::AgentEntry::model_flag`]) — never an `if agent == …`
//!   branch. Adding a fallback is editing a data row.
//! - **Fail-soft.** Every step degrades to a value; if no fallback model and no
//!   API-key path apply, the caller falls back to today's honest error. Nothing
//!   here panics.
//! - **Consent-gated.** This module never re-drives or stores a credential on
//!   its own; it produces a [`ModelProposal`] for the caller to confirm.
//! - **Bluey never handles the subscription auth.** The BYOT path ties into the
//!   existing cloud credential machinery ([`crate::cloud::keychain`]); the
//!   user's OpenAI API key is stored in the OS keychain, never logged.

use crate::registry::{self};
use crate::AgentKind;

/// What a drive error tells us about whether the failure is a **model-policy
/// block** (the requested model isn't allowed for this account/plan). Mirrors
/// the sibling resolvers' diagnosis enums ([`crate::auth_resolve::AuthDiagnosis`],
/// runtime's `Option<RuntimeReq>`): the model resolver only acts on
/// [`ModelDiagnosis::ModelBlocked`]; everything else is
/// [`ModelDiagnosis::NotAModelProblem`] so a non-model failure never triggers a
/// model swap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelDiagnosis {
    /// The failure is a model-availability/policy block — the agent is signed in
    /// and runs, but the requested model is not allowed for the account/plan.
    ModelBlocked(ModelBlocked),
    /// The failure is something other than a model block (auth, runtime version,
    /// missing binary, transient rate-limit, network, …). No model swap.
    NotAModelProblem,
}

/// A typed "this model isn't allowed for this account/plan" result.
///
/// `blocked_model` is the model the error named, when it could be extracted
/// (e.g. `gpt-5.1-codex-max` from the Codex message). `hint` is a short,
/// human-readable account/plan reason pulled from the message when present
/// (e.g. "ChatGPT account"), so the proposal can explain *why* the model was
/// blocked without re-parsing the raw error at the call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelBlocked {
    /// Which agent's drive hit the model block.
    pub agent: AgentKind,
    /// The model the error named as blocked, if it was extractable.
    pub blocked_model: Option<String>,
    /// A short human reason for the block (e.g. "ChatGPT account"), if the
    /// message carried one. Used only for the proposal text; never logic.
    pub hint: Option<String>,
}

/// The proposed resolution for a [`ModelBlocked`] failure — what Bluey would do
/// after the user approves. Two shapes, mirroring the product decision:
///
/// - [`RetryWithModel`](ModelProposal::RetryWithModel): a fallback model the
///   account is expected to support was found in the registry row; Bluey
///   proposes re-driving with it (via the agent's model flag).
/// - [`ConnectApiKey`](ModelProposal::ConnectApiKey): no usable subscription
///   model remains (or the agent has no model flag), so the only path is BYOT —
///   connect an OpenAI/vendor API key. Ties into [`crate::cloud::keychain`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelProposal {
    /// Retry the same drive under a different, account-supported model. Carries
    /// the chosen fallback model and the exact argv pair to append
    /// (`[model_flag, model]`, e.g. `["-m", "gpt-5.1-codex"]`).
    RetryWithModel {
        agent: AgentKind,
        /// The model that was blocked (echoed for the proposal text).
        blocked_model: Option<String>,
        /// The fallback model Bluey would retry with.
        fallback_model: &'static str,
        /// The argv to APPEND to the drive to force the fallback model. Two
        /// entries: the model flag and the model name.
        model_flag_args: Vec<String>,
    },
    /// No subscription model works (or the agent takes no model flag): the only
    /// remaining path is to connect an API key. Carries the keychain vendor +
    /// credential key the BYOT enrollment would write, and an env-var name the
    /// CLI reads, so the caller's enrollment UI is fully data-driven.
    ConnectApiKey(ByotProposal),
}

/// The BYOT (bring-your-own-token) alternative: where no subscription model
/// works, propose connecting an API key. This is a typed description of what the
/// enrollment would do — it does NOT store anything itself (the caller's consent
/// UI does, via [`crate::cloud::keychain`]).
///
/// For local Codex specifically, the "apply" is: the user provides an OpenAI API
/// key; Codex then drives in API-key auth mode (`OPENAI_API_KEY` / `codex login
/// --api-key`), under which the otherwise-blocked models ARE available. Fully
/// building that API-key drive for the local CLI is out of this slice's scope;
/// this proposal is the documented hook and the detection→proposal wiring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByotProposal {
    /// Which agent would switch to BYOT auth.
    pub agent: AgentKind,
    /// The model that was blocked (for the proposal text).
    pub blocked_model: Option<String>,
    /// The keychain vendor short-name the API key is stored under (the same
    /// `bluey_cloud_<vendor>` namespace [`crate::cloud::keychain`] uses). For
    /// Codex this is `"codex"`.
    pub keychain_vendor: &'static str,
    /// The credential key within that vendor's keychain service (e.g.
    /// `"api_key"`).
    pub keychain_key: &'static str,
    /// The environment variable the agent's CLI reads the API key from, when it
    /// supports one (e.g. `OPENAI_API_KEY`). Surfaced so the caller can set it
    /// per-spawn after the user enrolls. `None` if unknown.
    pub api_key_env: Option<&'static str>,
}

impl ModelProposal {
    /// A one-line, human-readable description of the fix for a consent prompt,
    /// mirroring [`crate::runtime_resolve::RuntimeResolution::proposal_line`].
    /// `agent_label` is the agent's display name (e.g. "Codex").
    pub fn proposal_line(&self, agent_label: &str) -> String {
        match self {
            ModelProposal::RetryWithModel {
                blocked_model,
                fallback_model,
                ..
            } => {
                let blocked = blocked_model.as_deref().unwrap_or("the configured model");
                format!(
                    "retry {agent_label} with the fallback model `{fallback_model}` \
                     (your account blocked `{blocked}`); your config.toml stays unchanged"
                )
            }
            ModelProposal::ConnectApiKey(byot) => {
                let blocked = byot
                    .blocked_model
                    .as_deref()
                    .unwrap_or("the configured model");
                let env = byot
                    .api_key_env
                    .map(|e| format!(" (set as {e})"))
                    .unwrap_or_default();
                format!(
                    "connect an API key for {agent_label}{env} — your account blocked \
                     `{blocked}` and no subscription model works, so Bluey would store \
                     your key in the OS keychain and drive under it"
                )
            }
        }
    }
}

/// Detector: classify a drive/launch error string as a **model-policy block**
/// (or not), for a known agent. Generic across agents — it matches on the
/// *shape* of the message (the phrases vendors use for "this model isn't allowed
/// for your account/plan"), never on a per-agent hardcoded string.
///
/// `error` is whatever the drive layer surfaced — typically the text of a
/// terminal [`crate::AnswerChunk::Error`] (which, for Codex, carries the JSON
/// `error`/`turn.failed` body). Case-insensitive.
pub fn classify_error(agent: &AgentKind, error: &str) -> ModelDiagnosis {
    if !looks_like_model_block(error) {
        return ModelDiagnosis::NotAModelProblem;
    }
    ModelDiagnosis::ModelBlocked(ModelBlocked {
        agent: agent.clone(),
        blocked_model: extract_blocked_model(error),
        hint: extract_account_hint(error),
    })
}

/// The generic model-block matcher. True when the message contains a phrase
/// vendors use to mean "the requested model is not available to your
/// account/plan". Deliberately specific so it does NOT fire on auth errors,
/// runtime-version errors, or transient rate-limits (those are the sibling
/// resolvers' domains).
///
/// Matched signals (case-insensitive substrings), drawn from REAL output and
/// vendor issue trackers (2026-06):
/// - "model is not supported" / "is not supported when using" — Codex's exact
///   ChatGPT-account block ("The '<model>' model is not supported when using
///   Codex with a ChatGPT account.").
/// - "not supported with a chatgpt account" / "with a chatgpt account" — the
///   account-scoped tail of the same family.
/// - "model_not_found" / "model not found" — OpenAI's API error code shape.
/// - "does not have access to model" / "do not have access to the model" /
///   "no access to model" — per-account model gating (OpenAI/others).
/// - "model is not available" / "is not available to your" — plan gating.
/// - "not entitled to" (model) / "your plan does not include" — plan gating.
///
/// Deliberately EXCLUDED (stay NotAModelProblem — sibling resolvers handle):
/// - auth phrasing ("invalid api key", "unauthorized", "sign in") — that string
///   would also match the auth resolver; a *pure* auth error names no model.
/// - "no capacity available" / "is overloaded" — TRANSIENT capacity, not a
///   policy block; retried by the caller, not resolved by a model swap.
/// - runtime-version "requires node vN".
fn looks_like_model_block(error: &str) -> bool {
    let e = error.to_ascii_lowercase();

    // ---- Hard exclusions first: shapes owned by sibling resolvers, even if a
    // stray "model" appears. A TRANSIENT capacity error ("no capacity",
    // "overloaded") is not a policy block — swapping models won't fix it and the
    // caller retries instead. ------------------------------------------------
    const NOT_MODEL_BLOCK: &[&str] = &[
        "no capacity available",
        "no capacity for",
        "is overloaded",
        "overloaded_error",
        "requires node",
        "requires node.js",
    ];
    if NOT_MODEL_BLOCK.iter().any(|p| e.contains(p)) {
        return false;
    }

    // ---- Positive model-block signals. -----------------------------------
    const MODEL_BLOCK_PHRASES: &[&str] = &[
        "model is not supported",
        "is not supported when using",
        "not supported when using",
        "with a chatgpt account", // the account-scoped tail
        "model_not_found",
        "model not found",
        "unknown model",
        "does not have access to model",
        "does not have access to the model",
        "do not have access to the model",
        "no access to model",
        "model is not available",
        "is not available to your",
        "not entitled to",
        "your plan does not include",
        "model is not enabled",
        "is not enabled for your",
    ];
    MODEL_BLOCK_PHRASES.iter().any(|p| e.contains(p))
}

/// Extract the blocked model id from a model-block message, when present.
///
/// The dominant real shape quotes the model: `The 'gpt-5.1-codex-max' model is
/// not supported …`. The OpenAI API error code shape uses backticks: ``the model
/// `gpt-5.9` does not exist``. We scan for a delimited token and return the first
/// that looks like a model id (contains a digit or a hyphen, to avoid grabbing
/// an unrelated quoted word). Falls back to `None` when no model-shaped token is
/// found — the resolver still works (it just can't echo the name nor skip it in
/// the fallback list).
///
/// We look for a model-id-shaped run that is *immediately wrapped* by a
/// delimiter on each side (`'gpt-5.1-codex-max'`, `` `gpt-5.9` ``, smart-quoted
/// variants). Scanning the wrapped RUN — not "everything until the next quote" —
/// is nesting-immune: it finds the single-quoted model even when the whole
/// thing sits inside a double-quoted JSON string (the real Codex case, where the
/// message is `"… 'gpt-5.1-codex-max' model is not supported …"`).
fn extract_blocked_model(error: &str) -> Option<String> {
    let chars: Vec<char> = error.chars().collect();
    let n = chars.len();
    let mut i = 0;
    while i < n {
        if is_open_delim(chars[i]) {
            // Collect the maximal run of model-id characters right after it.
            let mut j = i + 1;
            let mut token = String::new();
            while j < n && is_model_id_char(chars[j]) {
                token.push(chars[j]);
                j += 1;
            }
            // Wrapped iff the char that ended the run is a closing delimiter.
            let wrapped = j < n && is_close_delim(chars[j]);
            if wrapped && looks_like_model_id(&token) {
                return Some(token);
            }
        }
        i += 1;
    }
    None
}

/// Opening delimiters a model id may be wrapped in: straight `'`/`"`, backtick,
/// and the LEFT smart quotes some log pipelines substitute.
fn is_open_delim(c: char) -> bool {
    matches!(c, '\'' | '"' | '`' | '\u{2018}' | '\u{201C}')
}

/// Closing delimiters (the right smart quotes plus the self-closing straight
/// quotes/backtick).
fn is_close_delim(c: char) -> bool {
    matches!(c, '\'' | '"' | '`' | '\u{2019}' | '\u{201D}')
}

/// Whether `c` is a character that appears inside a model id.
fn is_model_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | ':')
}

/// Whether a wrapped token looks like a model id: non-trivial, bounded, and
/// containing a digit or hyphen (which every real model id does —
/// `gpt-5.1-codex-max`, `o4-mini`, `claude-4.5` — and a quoted English word
/// does not). The character set is already constrained by [`is_model_id_char`].
fn looks_like_model_id(token: &str) -> bool {
    !token.is_empty() && token.len() <= 64 && token.chars().any(|c| c.is_ascii_digit() || c == '-')
}

/// Extract a short account/plan hint from the message (e.g. "ChatGPT account"),
/// used only for the proposal text. Best-effort; `None` when nothing matches.
fn extract_account_hint(error: &str) -> Option<String> {
    let e = error.to_ascii_lowercase();
    if e.contains("chatgpt account") {
        Some("ChatGPT account".to_string())
    } else if e.contains("your plan") || e.contains("subscription") {
        Some("your current plan".to_string())
    } else {
        None
    }
}

/// The registry fallback-model list for an agent. Pure data — reads the row's
/// `fallback_models`. Empty slice for agents with no known fallback (or unknown).
pub fn fallback_models_for(agent: &AgentKind) -> &'static [&'static str] {
    registry::KindTag::from_agent_kind(agent)
        .and_then(registry::entry_for)
        .map(|e| e.fallback_models)
        .unwrap_or(&[])
}

/// The registry per-run model flag for an agent (e.g. Codex's `-m`). `None` when
/// the agent takes no model flag (or is unknown).
pub fn model_flag_for(agent: &AgentKind) -> Option<&'static str> {
    registry::KindTag::from_agent_kind(agent)
        .and_then(registry::entry_for)
        .and_then(|e| e.model_flag)
}

/// Build the [`ModelProposal`] for a [`ModelBlocked`] failure, reading the
/// agent's fallback list + model flag off the registry (data-driven). The
/// propose half of propose+approve; applying happens at the call site after
/// consent.
///
/// Decision (matches the product owner's autonomy model):
/// 1. If the agent has a model flag AND a fallback model that is NOT the
///    already-blocked one, propose [`ModelProposal::RetryWithModel`] with the
///    first such fallback.
/// 2. Otherwise (no model flag, or every fallback is exhausted/blocked), propose
///    [`ModelProposal::ConnectApiKey`] — the BYOT path.
///
/// `tried_models` lets the caller record models already attempted-and-blocked in
/// THIS resolution loop (so a retry that also returns ModelBlocked advances to
/// the next fallback, then to BYOT). Pass an empty slice for the first attempt;
/// the already-blocked model from the error is always treated as tried.
pub fn plan_resolution(blocked: &ModelBlocked, tried_models: &[String]) -> ModelProposal {
    let agent = &blocked.agent;
    let flag = model_flag_for(agent);
    let fallbacks = fallback_models_for(agent);

    // Models we must NOT propose again: the one the error just named, plus any
    // the caller already tried in this loop.
    let is_exhausted = |candidate: &str| -> bool {
        blocked
            .blocked_model
            .as_deref()
            .is_some_and(|b| b.eq_ignore_ascii_case(candidate))
            || tried_models
                .iter()
                .any(|t| t.eq_ignore_ascii_case(candidate))
    };

    if let Some(flag) = flag {
        if let Some(next) = fallbacks.iter().copied().find(|m| !is_exhausted(m)) {
            return ModelProposal::RetryWithModel {
                agent: agent.clone(),
                blocked_model: blocked.blocked_model.clone(),
                fallback_model: next,
                model_flag_args: vec![flag.to_string(), next.to_string()],
            };
        }
    }

    // No usable subscription model remains (or no model flag): BYOT.
    ModelProposal::ConnectApiKey(byot_proposal_for(blocked))
}

/// Build the [`ByotProposal`] for an agent's model block — the BYOT hook. The
/// vendor/keychain/env values are data for the known BYOT-capable local CLIs;
/// for an unknown agent the proposal still describes the generic "connect a key"
/// path with `None` env so the caller can guide the user.
fn byot_proposal_for(blocked: &ModelBlocked) -> ByotProposal {
    // Codex is the live case: a ChatGPT-account block is resolved by switching
    // Codex to API-key auth (`OPENAI_API_KEY`), under which the blocked models
    // are available. Data, not an `if agent ==` branch in the drive path —
    // this is the BYOT enrollment metadata, the model analogue of an install
    // recipe. Other agents fall through to a generic proposal.
    let (keychain_vendor, keychain_key, api_key_env) = match &blocked.agent {
        AgentKind::Codex => ("codex", "api_key", Some("OPENAI_API_KEY")),
        AgentKind::ClaudeCode | AgentKind::ClaudeCodeApp | AgentKind::ClaudeCodeAgent => {
            ("anthropic", "api_key", Some("ANTHROPIC_API_KEY"))
        }
        AgentKind::Gemini | AgentKind::Antigravity => ("gemini", "api_key", Some("GEMINI_API_KEY")),
        _ => ("byot", "api_key", None),
    };
    ByotProposal {
        agent: blocked.agent.clone(),
        blocked_model: blocked.blocked_model.clone(),
        keychain_vendor,
        keychain_key,
        api_key_env,
    }
}

/// Convenience for the drive layer: given an agent and a drive error, return the
/// argv to APPEND for a model-fallback retry, if one is proposed. `None` when the
/// error isn't a model block, or the resolution is BYOT (no in-place retry).
/// This is the single call a retrying drive loop needs; consent is still the
/// caller's responsibility (it should only re-drive after approval).
pub fn fallback_retry_args(agent: &AgentKind, error: &str) -> Option<Vec<String>> {
    match classify_error(agent, error) {
        ModelDiagnosis::ModelBlocked(blocked) => match plan_resolution(&blocked, &[]) {
            ModelProposal::RetryWithModel {
                model_flag_args, ..
            } => Some(model_flag_args),
            ModelProposal::ConnectApiKey(_) => None,
        },
        ModelDiagnosis::NotAModelProblem => None,
    }
}

/// The decision a **retrying drive loop** (the daemon's answer path, the matrix
/// Ask step) takes after one drive attempt failed. This is the single,
/// fully-testable step the loop repeats: classify the real error, then —
/// given the models already tried-and-blocked in THIS loop — say whether to
/// re-drive under a fallback model, to stop and surface the BYOT path, or that
/// the failure was never a model block at all (so the loop leaves it alone).
///
/// The loop drives it like this (no agent ever named — all data):
/// ```ignore
/// let mut tried: Vec<String> = Vec::new();
/// loop {
///     match decide_model_block(&agent, &err, &tried) {
///         ModelLoopStep::RetryWithModel { fallback_model, model_flag_args } => {
///             tried.push(fallback_model.to_string());
///             // re-drive with `model_flag_args` appended; on success break,
///             // on a new error set `err` and continue.
///         }
///         ModelLoopStep::ConnectApiKey(byot) => break surface_byot(&byot),
///         ModelLoopStep::NotModelBlock => break surface_original(&err),
///     }
/// }
/// ```
/// Bounded by construction: each retry records its `fallback_model` in `tried`,
/// so [`plan_resolution`] advances to the next fallback and, once all are
/// exhausted, returns [`ConnectApiKey`](ModelProposal::ConnectApiKey) — the loop
/// can never cycle forever.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelLoopStep {
    /// Re-drive the SAME question under a fallback model the account is expected
    /// to support. `model_flag_args` is the argv pair to append
    /// (`[model_flag, fallback_model]`); `fallback_model` is echoed so the loop
    /// can record it in its tried-list before retrying.
    RetryWithModel {
        fallback_model: &'static str,
        model_flag_args: Vec<String>,
    },
    /// No subscription model remains (or the agent takes no model flag): stop and
    /// surface the BYOT path. Carries the full [`ByotProposal`] so the caller can
    /// render an honest message ([`byot_guidance_line`]) and, later, drive an
    /// enrollment.
    ConnectApiKey(ByotProposal),
    /// The failure was not a model block — the loop must not swap models; surface
    /// the original error (the auth/runtime resolvers, or the raw text, own it).
    NotModelBlock,
}

/// One step of the model-fallback loop: classify `error` for `agent` and, with
/// the models already `tried` this loop, decide what to do next. Pure and
/// data-driven (reads only the registry via [`plan_resolution`]); the single
/// unit-testable brain of the "blocked → try fallbacks → exhausted → BYOT"
/// behavior, with no live agent required.
pub fn decide_model_block(agent: &AgentKind, error: &str, tried: &[String]) -> ModelLoopStep {
    match classify_error(agent, error) {
        ModelDiagnosis::NotAModelProblem => ModelLoopStep::NotModelBlock,
        ModelDiagnosis::ModelBlocked(blocked) => match plan_resolution(&blocked, tried) {
            ModelProposal::RetryWithModel {
                fallback_model,
                model_flag_args,
                ..
            } => ModelLoopStep::RetryWithModel {
                fallback_model,
                model_flag_args,
            },
            ModelProposal::ConnectApiKey(byot) => ModelLoopStep::ConnectApiKey(byot),
        },
    }
}

/// The honest, user-facing one-line guidance shown when every subscription model
/// is blocked for this account and the only remaining path is BYOT (connect an
/// API key). Centralizes the message so the daemon's answer card, the CLI, and
/// the matrix all say the same true thing. `agent_label` is the display name
/// (e.g. "Codex"). Data-driven: the env-var/vendor come from the
/// [`ByotProposal`] the resolver built off the registry.
///
/// Points at `bluey agent resolve-model <agent>` — the command that runs the
/// propose-and-apply BYOT flow (probe → detect the block → guide enrollment) —
/// and names the API-key env var the CLI reads, so the message is actionable.
pub fn byot_guidance_line(agent_label: &str, byot: &ByotProposal) -> String {
    let blocked = byot
        .blocked_model
        .as_deref()
        .unwrap_or("the configured model");
    let env = byot
        .api_key_env
        .map(|e| format!(" (set {e}, or run the agent's `--api-key` login)"))
        .unwrap_or_default();
    format!(
        "couldn't answer: your account blocked `{blocked}` for {agent_label}, and every \
         fallback model is also blocked for this account. Connect an API key to keep using \
         {agent_label} — run `bluey agent resolve-model {vendor}`{env}; Bluey stores the \
         key in your OS keychain and drives under it. (Your config was not changed.)",
        vendor = byot.keychain_vendor,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The EXACT error string captured live from the user's Codex CLI
    /// (`codex exec --json "say hi"`, ChatGPT-account auth, 2026-06). The Codex
    /// `error`/`turn.failed` events wrap the API body as a JSON string; this is
    /// that `message` field verbatim.
    const REAL_CODEX_BLOCK: &str = r#"{"type":"error","status":400,"error":{"type":"invalid_request_error","message":"The 'gpt-5.1-codex-max' model is not supported when using Codex with a ChatGPT account."}}"#;

    // ---- The detector on the REAL model-block string → ModelBlocked. -------

    #[test]
    fn classifies_real_codex_chatgpt_account_block_as_model_blocked() {
        let diag = classify_error(&AgentKind::Codex, REAL_CODEX_BLOCK);
        match diag {
            ModelDiagnosis::ModelBlocked(b) => {
                assert_eq!(b.agent, AgentKind::Codex);
                assert_eq!(b.blocked_model.as_deref(), Some("gpt-5.1-codex-max"));
                assert_eq!(b.hint.as_deref(), Some("ChatGPT account"));
            }
            other => panic!("expected ModelBlocked, got {other:?}"),
        }
    }

    #[test]
    fn classifies_plain_text_codex_block_message_too() {
        // Same message without the JSON envelope (some surfaces unwrap it).
        let err =
            "The 'gpt-5.1-codex' model is not supported when using Codex with a ChatGPT account.";
        match classify_error(&AgentKind::Codex, err) {
            ModelDiagnosis::ModelBlocked(b) => {
                assert_eq!(b.blocked_model.as_deref(), Some("gpt-5.1-codex"));
            }
            other => panic!("expected ModelBlocked, got {other:?}"),
        }
    }

    #[test]
    fn classifies_openai_model_not_found_and_no_access_shapes() {
        for err in [
            r#"{"error":{"code":"model_not_found","message":"The model `gpt-5.9` does not exist or you do not have access to the model."}}"#,
            "Error: does not have access to model gpt-5.9",
            "This model is not available to your current plan.",
        ] {
            assert!(
                matches!(
                    classify_error(&AgentKind::Codex, err),
                    ModelDiagnosis::ModelBlocked(_)
                ),
                "model-block shape not classified: {err:?}"
            );
        }
    }

    // ---- The detector must NOT misclassify the OTHER resolvers' errors. ----

    #[test]
    fn does_not_classify_auth_error_as_model_block() {
        // An auth error (auth_resolve's job) names no model and uses auth
        // phrasing — must stay NotAModelProblem here.
        for err in [
            "Error: not signed in. Please run `codex login`.",
            "401 Unauthorized: refresh token already used",
            "Authentication required. Please run 'agent login' first, or set CURSOR_API_KEY.",
            "Invalid API key · Please run /login",
        ] {
            assert_eq!(
                classify_error(&AgentKind::Codex, err),
                ModelDiagnosis::NotAModelProblem,
                "auth error wrongly classified as model block: {err:?}"
            );
        }
    }

    #[test]
    fn does_not_classify_runtime_version_error_as_model_block() {
        // runtime_resolve's job.
        let err = "The GitHub Copilot CLI requires Node.js version 24 or newer. \
                   You are running v20.11.0.";
        assert_eq!(
            classify_error(&AgentKind::Copilot, err),
            ModelDiagnosis::NotAModelProblem
        );
    }

    #[test]
    fn does_not_classify_missing_binary_as_model_block() {
        let err = "binary `codex` not found on PATH";
        assert_eq!(
            classify_error(&AgentKind::Codex, err),
            ModelDiagnosis::NotAModelProblem
        );
    }

    #[test]
    fn does_not_classify_transient_capacity_or_overload_as_model_block() {
        // Gemini's REAL capacity error mentions a model name but is TRANSIENT —
        // a model SWAP won't fix it, so it must NOT be a model-policy block.
        // (It is also excluded by auth_resolve; here we hold our own boundary.)
        let capacity = "Attempt 1 failed with status 429. _GaxiosError: No capacity \
                        available for model gemini-3.1-flash-lite on the server.";
        assert_eq!(
            classify_error(&AgentKind::Gemini, capacity),
            ModelDiagnosis::NotAModelProblem
        );
        let overload = "Error: model is overloaded (529)";
        assert_eq!(
            classify_error(&AgentKind::ClaudeCode, overload),
            ModelDiagnosis::NotAModelProblem
        );
    }

    // ---- Blocked-model extraction. ----------------------------------------

    #[test]
    fn extracts_model_from_smart_quotes_too() {
        // A log pipeline that swapped straight quotes for smart quotes.
        let err = "The \u{2018}gpt-5.1-codex-max\u{2019} model is not supported when using Codex with a ChatGPT account.";
        assert_eq!(
            extract_blocked_model(err).as_deref(),
            Some("gpt-5.1-codex-max")
        );
    }

    #[test]
    fn extract_skips_quoted_non_model_words() {
        // A quoted English word (no digit/hyphen) is not mistaken for a model;
        // the real model token is picked instead.
        let err = "The 'foo' setting blocked 'o4-mini' for your plan.";
        assert_eq!(extract_blocked_model(err).as_deref(), Some("o4-mini"));
    }

    #[test]
    fn extract_returns_none_when_no_model_quoted() {
        let err = "This model is not available to your current plan.";
        assert_eq!(extract_blocked_model(err), None);
    }

    // ---- plan_resolution: propose+approve, data-driven. -------------------

    #[test]
    fn codex_block_proposes_first_fallback_skipping_the_blocked_model() {
        // The registry Codex row starts its fallback list with "gpt-5.1-codex".
        // Blocked model is gpt-5.1-codex-max → first proposal is gpt-5.1-codex
        // with the `-m` flag pair.
        let blocked = ModelBlocked {
            agent: AgentKind::Codex,
            blocked_model: Some("gpt-5.1-codex-max".to_string()),
            hint: Some("ChatGPT account".to_string()),
        };
        match plan_resolution(&blocked, &[]) {
            ModelProposal::RetryWithModel {
                agent,
                fallback_model,
                model_flag_args,
                ..
            } => {
                assert_eq!(agent, AgentKind::Codex);
                assert_eq!(fallback_model, "gpt-5.1-codex");
                assert_eq!(model_flag_args, vec!["-m", "gpt-5.1-codex"]);
            }
            other => panic!("expected RetryWithModel, got {other:?}"),
        }
    }

    #[test]
    fn plan_resolution_does_not_repropose_the_blocked_model() {
        // If the blocked model IS the first fallback, propose the next one.
        let blocked = ModelBlocked {
            agent: AgentKind::Codex,
            blocked_model: Some("gpt-5.1-codex".to_string()),
            hint: None,
        };
        match plan_resolution(&blocked, &[]) {
            ModelProposal::RetryWithModel { fallback_model, .. } => {
                assert_ne!(fallback_model, "gpt-5.1-codex");
                assert_eq!(fallback_model, "gpt-5-codex");
            }
            other => panic!("expected RetryWithModel, got {other:?}"),
        }
    }

    #[test]
    fn exhausting_all_fallbacks_proposes_byot_api_key() {
        // The real verdict on the test machine: EVERY model was blocked. Once
        // the caller has tried them all, the resolver proposes the BYOT path.
        let blocked = ModelBlocked {
            agent: AgentKind::Codex,
            blocked_model: Some("gpt-5.1-codex-max".to_string()),
            hint: Some("ChatGPT account".to_string()),
        };
        let all_tried: Vec<String> = fallback_models_for(&AgentKind::Codex)
            .iter()
            .map(|s| s.to_string())
            .collect();
        match plan_resolution(&blocked, &all_tried) {
            ModelProposal::ConnectApiKey(byot) => {
                assert_eq!(byot.agent, AgentKind::Codex);
                assert_eq!(byot.keychain_vendor, "codex");
                assert_eq!(byot.keychain_key, "api_key");
                assert_eq!(byot.api_key_env, Some("OPENAI_API_KEY"));
            }
            other => panic!("expected ConnectApiKey (BYOT), got {other:?}"),
        }
    }

    #[test]
    fn agent_with_no_model_flag_proposes_byot_directly() {
        // Cursor has model_flag: None → even a (hypothetical) model block goes
        // straight to BYOT, never an unusable retry.
        let blocked = ModelBlocked {
            agent: AgentKind::Cursor,
            blocked_model: Some("some-model".to_string()),
            hint: None,
        };
        assert!(matches!(
            plan_resolution(&blocked, &[]),
            ModelProposal::ConnectApiKey(_)
        ));
    }

    // ---- fallback_retry_args: the drive-layer convenience. ----------------

    #[test]
    fn fallback_retry_args_returns_model_pair_for_real_codex_block() {
        let args = fallback_retry_args(&AgentKind::Codex, REAL_CODEX_BLOCK)
            .expect("a fallback retry is proposed for the real Codex block");
        // First fallback for Codex, with the `-m` flag.
        assert_eq!(args, vec!["-m", "gpt-5.1-codex"]);
    }

    #[test]
    fn fallback_retry_args_none_for_non_model_error() {
        assert_eq!(
            fallback_retry_args(&AgentKind::Codex, "401 Unauthorized"),
            None
        );
    }

    // ---- Registry wiring is data-driven (no agent named in logic). --------

    #[test]
    fn codex_registry_row_carries_flag_and_nonempty_fallbacks() {
        assert_eq!(model_flag_for(&AgentKind::Codex), Some("-m"));
        assert!(
            !fallback_models_for(&AgentKind::Codex).is_empty(),
            "Codex must carry a fallback ordering (the live model-block case)"
        );
    }

    #[test]
    fn proposal_line_is_human_and_names_the_models() {
        let blocked = ModelBlocked {
            agent: AgentKind::Codex,
            blocked_model: Some("gpt-5.1-codex-max".to_string()),
            hint: Some("ChatGPT account".to_string()),
        };
        let retry = plan_resolution(&blocked, &[]);
        let line = retry.proposal_line("Codex");
        assert!(
            line.contains("gpt-5.1-codex"),
            "should name the fallback: {line}"
        );
        assert!(
            line.contains("gpt-5.1-codex-max"),
            "should name the blocked model: {line}"
        );

        let all_tried: Vec<String> = fallback_models_for(&AgentKind::Codex)
            .iter()
            .map(|s| s.to_string())
            .collect();
        let byot = plan_resolution(&blocked, &all_tried);
        let line = byot.proposal_line("Codex");
        assert!(
            line.to_lowercase().contains("api key"),
            "BYOT line should mention an API key: {line}"
        );
        assert!(
            line.contains("OPENAI_API_KEY"),
            "BYOT line should name the env var: {line}"
        );
    }

    // ---- decide_model_block: the daemon/matrix retrying-loop brain. --------

    #[test]
    fn decide_model_block_returns_not_model_block_for_non_model_errors() {
        // Auth / runtime / transient errors must leave the loop alone.
        for err in [
            "401 Unauthorized",
            "binary `codex` not found on PATH",
            "Error: model is overloaded (529)",
        ] {
            assert_eq!(
                decide_model_block(&AgentKind::Codex, err, &[]),
                ModelLoopStep::NotModelBlock,
                "non-model error wrongly entered the model loop: {err:?}"
            );
        }
    }

    #[test]
    fn decide_model_block_first_step_retries_with_first_fallback() {
        // The real Codex block, nothing tried yet → retry with the first
        // fallback and the `-m` pair.
        match decide_model_block(&AgentKind::Codex, REAL_CODEX_BLOCK, &[]) {
            ModelLoopStep::RetryWithModel {
                fallback_model,
                model_flag_args,
            } => {
                assert_eq!(fallback_model, "gpt-5.1-codex");
                assert_eq!(model_flag_args, vec!["-m", "gpt-5.1-codex"]);
            }
            other => panic!("expected RetryWithModel, got {other:?}"),
        }
    }

    /// THE scenario the brief calls out: model blocked → try each fallback in
    /// turn (each also blocked on this account) → fallbacks exhausted → the loop
    /// lands on the BYOT guidance, in a BOUNDED number of steps (never forever).
    #[test]
    fn decide_model_block_loop_exhausts_fallbacks_then_yields_byot() {
        let fallbacks = fallback_models_for(&AgentKind::Codex);
        assert!(
            !fallbacks.is_empty(),
            "Codex must have fallbacks to exhaust"
        );

        // Simulate the daemon loop: every retry comes back ModelBlocked (the real
        // machine state — ALL subscription models blocked).
        let mut tried: Vec<String> = Vec::new();
        let mut retried_models: Vec<&'static str> = Vec::new();
        let mut byot_seen = false;
        // Hard upper bound so a regression that fails to advance can't hang the
        // test: at most one step per fallback, plus the terminal BYOT step.
        for _ in 0..(fallbacks.len() + 2) {
            match decide_model_block(&AgentKind::Codex, REAL_CODEX_BLOCK, &tried) {
                ModelLoopStep::RetryWithModel {
                    fallback_model,
                    model_flag_args,
                } => {
                    // The pair always leads with the registry model flag.
                    assert_eq!(model_flag_args.first().map(String::as_str), Some("-m"));
                    assert_eq!(
                        model_flag_args.get(1).map(String::as_str),
                        Some(fallback_model)
                    );
                    // Never re-propose a model already tried (loop is making progress).
                    assert!(
                        !tried.iter().any(|t| t == fallback_model),
                        "re-proposed an already-tried model: {fallback_model}"
                    );
                    retried_models.push(fallback_model);
                    tried.push(fallback_model.to_string());
                }
                ModelLoopStep::ConnectApiKey(byot) => {
                    // Exhausted → BYOT. Data-driven Codex enrollment metadata.
                    assert_eq!(byot.agent, AgentKind::Codex);
                    assert_eq!(byot.keychain_vendor, "codex");
                    assert_eq!(byot.api_key_env, Some("OPENAI_API_KEY"));
                    byot_seen = true;
                    break;
                }
                ModelLoopStep::NotModelBlock => panic!("real block misclassified mid-loop"),
            }
        }

        assert!(
            byot_seen,
            "loop never reached the BYOT step (possible infinite loop)"
        );
        // Every distinct fallback that isn't the already-blocked model was tried
        // exactly once before BYOT — bounded and complete.
        let expected: Vec<&str> = fallbacks
            .iter()
            .copied()
            .filter(|m| !m.eq_ignore_ascii_case("gpt-5.1-codex-max"))
            .collect();
        assert_eq!(
            retried_models, expected,
            "fallbacks were not tried in registry order, once each"
        );
    }

    #[test]
    fn byot_guidance_line_is_honest_and_actionable() {
        // Build the BYOT step the loop ends on, then render the guidance.
        let blocked = ModelBlocked {
            agent: AgentKind::Codex,
            blocked_model: Some("gpt-5.1-codex-max".to_string()),
            hint: Some("ChatGPT account".to_string()),
        };
        let all_tried: Vec<String> = fallback_models_for(&AgentKind::Codex)
            .iter()
            .map(|s| s.to_string())
            .collect();
        let ModelProposal::ConnectApiKey(byot) = plan_resolution(&blocked, &all_tried) else {
            panic!("all fallbacks tried should yield BYOT");
        };
        let line = byot_guidance_line("Codex", &byot);
        // Names the blocked model, the agent, the real command, and the env var —
        // and is NOT the raw "model is not supported" passthrough.
        assert!(
            line.contains("gpt-5.1-codex-max"),
            "names blocked model: {line}"
        );
        assert!(line.contains("Codex"), "names the agent: {line}");
        assert!(
            line.contains("bluey agent resolve-model codex"),
            "points at the resolve-model command: {line}"
        );
        assert!(line.contains("OPENAI_API_KEY"), "names the env var: {line}");
        assert!(
            !line.contains("is not supported when using"),
            "must not be the raw vendor error passthrough: {line}"
        );
    }
}
