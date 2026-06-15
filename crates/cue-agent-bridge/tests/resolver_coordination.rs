//! Cross-resolver coordination: the runtime, auth, and model resolvers must
//! classify a drive error into DISTINCT, non-overlapping causes. This guards the
//! boundary between the three self-resolvers (each owned by a different module)
//! against future drift — if someone widens one matcher so it starts claiming
//! another's errors, one of these assertions fails on purpose.
//!
//! The anchor is the REAL error captured live from the user's Codex CLI on
//! 2026-06 (`codex exec --json` under a ChatGPT-account login): the model is
//! blocked for the account/plan. Exactly ONE resolver (model) should claim it.

use cue_agent_bridge::{auth_resolve, model_resolve, runtime_resolve, AgentKind};

/// The verbatim `message` body of the Codex `error`/`turn.failed` event,
/// captured live from `codex exec --json "say hi"` on a ChatGPT-account login.
const REAL_CODEX_MODEL_BLOCK: &str = r#"{"type":"error","status":400,"error":{"type":"invalid_request_error","message":"The 'gpt-5.1-codex-max' model is not supported when using Codex with a ChatGPT account."}}"#;

/// A real auth failure (codex not signed in) — only the AUTH resolver claims it.
const AUTH_ERROR: &str = "Error: not signed in. Please run `codex login`.";

/// A real runtime-version failure (Copilot on too-old Node) — only the RUNTIME
/// resolver claims it.
const RUNTIME_ERROR: &str =
    "The GitHub Copilot CLI requires Node.js version 24 or newer. You are running v20.11.0.";

#[test]
fn real_codex_model_block_is_claimed_only_by_the_model_resolver() {
    // MODEL resolver: YES — this is its job.
    assert!(
        matches!(
            model_resolve::classify_error(&AgentKind::Codex, REAL_CODEX_MODEL_BLOCK),
            model_resolve::ModelDiagnosis::ModelBlocked(_)
        ),
        "model resolver must claim the real Codex model block"
    );

    // AUTH resolver: NO — a model block is not an auth problem (the string has
    // no auth phrasing; the auth matcher must defer it).
    assert_eq!(
        auth_resolve::classify_error(&AgentKind::Codex, REAL_CODEX_MODEL_BLOCK),
        auth_resolve::AuthDiagnosis::NotAnAuthProblem,
        "auth resolver must NOT claim a model block"
    );

    // RUNTIME resolver: NO — there is no runtime-version requirement here.
    assert!(
        runtime_resolve::resolve_for_launch_error(REAL_CODEX_MODEL_BLOCK).is_none(),
        "runtime resolver must NOT claim a model block"
    );
}

#[test]
fn real_auth_error_is_claimed_only_by_the_auth_resolver() {
    // AUTH: yes.
    assert!(
        matches!(
            auth_resolve::classify_error(&AgentKind::Codex, AUTH_ERROR),
            auth_resolve::AuthDiagnosis::NeedsLogin(_)
        ),
        "auth resolver must claim a not-signed-in error"
    );
    // MODEL: no.
    assert_eq!(
        model_resolve::classify_error(&AgentKind::Codex, AUTH_ERROR),
        model_resolve::ModelDiagnosis::NotAModelProblem,
        "model resolver must NOT claim an auth error"
    );
    // RUNTIME: no.
    assert!(
        runtime_resolve::resolve_for_launch_error(AUTH_ERROR).is_none(),
        "runtime resolver must NOT claim an auth error"
    );
}

#[test]
fn real_runtime_error_is_not_claimed_by_auth_or_model_resolvers() {
    // MODEL: no — a runtime-version failure is not a model block.
    assert_eq!(
        model_resolve::classify_error(&AgentKind::Copilot, RUNTIME_ERROR),
        model_resolve::ModelDiagnosis::NotAModelProblem,
        "model resolver must NOT claim a runtime-version error"
    );
    // AUTH: no — already covered by the auth module's own tests, asserted here
    // too so the three-way boundary is checked in one place.
    assert_eq!(
        auth_resolve::classify_error(&AgentKind::Copilot, RUNTIME_ERROR),
        auth_resolve::AuthDiagnosis::NotAnAuthProblem,
        "auth resolver must NOT claim a runtime-version error"
    );
    // (The runtime resolver's positive claim on this string is asserted in its
    // own module tests, which can locate a satisfying runtime fixture; here we
    // only assert the *other two* correctly decline.)
}

#[test]
fn model_resolver_extracts_the_blocked_model_from_the_real_string() {
    // The whole flow depends on pulling the model id out of the live message.
    match model_resolve::classify_error(&AgentKind::Codex, REAL_CODEX_MODEL_BLOCK) {
        model_resolve::ModelDiagnosis::ModelBlocked(b) => {
            assert_eq!(b.blocked_model.as_deref(), Some("gpt-5.1-codex-max"));
            assert_eq!(b.hint.as_deref(), Some("ChatGPT account"));
        }
        other => panic!("expected ModelBlocked, got {other:?}"),
    }
}
