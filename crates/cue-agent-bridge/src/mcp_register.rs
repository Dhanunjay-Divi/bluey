//! Register Bluey's OWN memory MCP server into the attached agent — the
//! write-side counterpart of [`crate::connectors`] (which only reads).
//!
//! Every recipe below was LIVE-VERIFIED in the Batch-0 spike (2026-07-08):
//! the agent's own scriptable `mcp add` where one exists (claude, copilot,
//! codex), a project-scope config write where that is the surface (gemini,
//! cursor), plus the per-agent spawn env/args that make the tools fire in a
//! HEADLESS drive with zero prompts. Bluey never holds agent credentials —
//! this writes only Bluey's own loopback URL + bearer token.

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::json;

use crate::AgentKind;

/// The server name agents see. The drive layer's allow-args derive from it
/// (Claude approves `mcp__bluey-memory`, gemini/copilot approve by name).
pub const BLUEY_SERVER_NAME: &str = "bluey-memory";

/// Env var Codex reads the bearer token from (`--bearer-token-env-var`).
pub const BLUEY_TOKEN_ENV: &str = "BLUEY_MCP_TOKEN";

/// Bluey's server coordinates, as written into the agent's config.
#[derive(Debug, Clone)]
pub struct BlueyServerReg {
    pub url: String,
    pub token: String,
}

/// What happened when registering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegisterOutcome {
    Registered,
    /// This agent has no scriptable registration surface (GUI-only rows).
    Unsupported(String),
}

/// Register Bluey's memory server into `kind`'s MCP config. `cwd` is the
/// directory project-scope configs are written under (the same cwd the warm
/// drive runs from).
pub async fn register_bluey_memory(
    kind: &AgentKind,
    reg: &BlueyServerReg,
    cwd: &Path,
) -> Result<RegisterOutcome> {
    match kind {
        AgentKind::ClaudeCode | AgentKind::ClaudeCodeApp | AgentKind::ClaudeCodeAgent => {
            // User scope: valid regardless of drive cwd; removed on close.
            run(
                cwd,
                "claude",
                &[
                    "mcp",
                    "add",
                    "--scope",
                    "user",
                    "--transport",
                    "http",
                    BLUEY_SERVER_NAME,
                    &reg.url,
                    "--header",
                    &format!("Authorization: Bearer {}", reg.token),
                ],
            )
            .await?;
            Ok(RegisterOutcome::Registered)
        }
        AgentKind::Copilot => {
            run(
                cwd,
                "copilot",
                &[
                    "mcp",
                    "add",
                    "--transport",
                    "http",
                    BLUEY_SERVER_NAME,
                    &reg.url,
                    "--header",
                    &format!("Authorization: Bearer {}", reg.token),
                ],
            )
            .await?;
            Ok(RegisterOutcome::Registered)
        }
        AgentKind::Codex => {
            // Token via env (never plaintext in config) — codex's own
            // `--bearer-token-env-var` mechanism; the drive sets the env.
            run(
                cwd,
                "codex",
                &[
                    "mcp",
                    "add",
                    BLUEY_SERVER_NAME,
                    "--url",
                    &reg.url,
                    "--bearer-token-env-var",
                    BLUEY_TOKEN_ENV,
                ],
            )
            .await?;
            Ok(RegisterOutcome::Registered)
        }
        AgentKind::Gemini => {
            // Project-scope settings.json under the drive cwd; `httpUrl` is
            // gemini's streamable-HTTP field (verified: `url` is SSE-only).
            merge_json_config(
                &cwd.join(".gemini").join("settings.json"),
                "mcpServers",
                json!({
                    "httpUrl": reg.url,
                    "headers": { "Authorization": format!("Bearer {}", reg.token) }
                }),
            )?;
            Ok(RegisterOutcome::Registered)
        }
        AgentKind::Cursor => {
            merge_json_config(
                &cwd.join(".cursor").join("mcp.json"),
                "mcpServers",
                json!({
                    "url": reg.url,
                    "headers": { "Authorization": format!("Bearer {}", reg.token) }
                }),
            )?;
            // One-time scriptable approval (persisted by cursor globally).
            run(cwd, "cursor-agent", &["mcp", "enable", BLUEY_SERVER_NAME]).await?;
            Ok(RegisterOutcome::Registered)
        }
        other => Ok(RegisterOutcome::Unsupported(format!(
            "{other:?} has no scriptable MCP registration surface"
        ))),
    }
}

/// Remove Bluey's server from `kind`'s MCP config (meeting end / token burn).
/// Best-effort by design: a failed removal must never block meeting teardown —
/// the rotated token already makes any stale registration useless.
pub async fn deregister_bluey_memory(kind: &AgentKind, cwd: &Path) -> Result<()> {
    match kind {
        AgentKind::ClaudeCode | AgentKind::ClaudeCodeApp | AgentKind::ClaudeCodeAgent => {
            run(
                cwd,
                "claude",
                &["mcp", "remove", "--scope", "user", BLUEY_SERVER_NAME],
            )
            .await
        }
        AgentKind::Copilot => run(cwd, "copilot", &["mcp", "remove", BLUEY_SERVER_NAME]).await,
        AgentKind::Codex => run(cwd, "codex", &["mcp", "remove", BLUEY_SERVER_NAME]).await,
        AgentKind::Gemini => {
            remove_json_config_key(&cwd.join(".gemini").join("settings.json"), "mcpServers")
        }
        AgentKind::Cursor => {
            let _ = run(cwd, "cursor-agent", &["mcp", "disable", BLUEY_SERVER_NAME]).await;
            remove_json_config_key(&cwd.join(".cursor").join("mcp.json"), "mcpServers")
        }
        _ => Ok(()),
    }
}

/// Extra spawn ENV a headless drive needs for this agent to reach + trust
/// Bluey's server (live-verified per agent in the Batch-0 spike).
pub fn drive_env_for(kind: &AgentKind, reg: &BlueyServerReg) -> Vec<(String, String)> {
    match kind {
        // Gemini refuses headless MCP in an untrusted workspace.
        AgentKind::Gemini => vec![("GEMINI_CLI_TRUST_WORKSPACE".into(), "true".into())],
        // Codex reads the bearer token from this env var.
        AgentKind::Codex => vec![(BLUEY_TOKEN_ENV.into(), reg.token.clone())],
        _ => Vec::new(),
    }
}

/// Extra ARGV a headless drive needs so Bluey's tools actually fire.
/// Cursor's `--approve-mcps` does NOT approve tool CALLS (verified live);
/// only `--force` does. Scoping that grant tighter is tracked follow-up work —
/// until then cursor warm drives carry the broad flag, documented.
pub fn drive_args_for(kind: &AgentKind) -> Vec<String> {
    match kind {
        AgentKind::Cursor => vec!["--force".to_string()],
        _ => Vec::new(),
    }
}

/// Run one registration command with a bounded wait; non-zero exit → error
/// with stderr attached (surfaced, never swallowed).
async fn run(cwd: &Path, binary: &str, args: &[&str]) -> Result<()> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        tokio::process::Command::new(binary)
            .args(args)
            .current_dir(cwd)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .with_context(|| format!("{binary} {} timed out", args.join(" ")))?
    .with_context(|| format!("spawn {binary}"))?;
    if !output.status.success() {
        bail!(
            "{binary} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// Merge `{ <section>: { bluey-memory: <entry> } }` into a JSON config file,
/// preserving every other key (the file may be user-owned).
fn merge_json_config(path: &Path, section: &str, entry: serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let mut root: serde_json::Value = match std::fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw).with_context(|| {
            format!(
                "{} is not valid JSON; refusing to overwrite",
                path.display()
            )
        })?,
        Err(_) => json!({}),
    };
    let obj = root
        .as_object_mut()
        .with_context(|| format!("{} root is not an object", path.display()))?;
    let servers = obj
        .entry(section)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .with_context(|| format!("{}.{section} is not an object", path.display()))?;
    servers.insert(BLUEY_SERVER_NAME.to_string(), entry);
    std::fs::write(path, serde_json::to_string_pretty(&root)?)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Remove ONLY Bluey's entry from a JSON config file (other keys untouched).
fn remove_json_config_key(path: &Path, section: &str) -> Result<()> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Ok(()); // nothing to remove
    };
    let Ok(mut root) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Ok(()); // not ours to fix
    };
    if let Some(servers) = root.get_mut(section).and_then(|v| v.as_object_mut()) {
        servers.remove(BLUEY_SERVER_NAME);
    }
    std::fs::write(path, serde_json::to_string_pretty(&root)?)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_merge_preserves_user_keys_and_removal_is_surgical() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join(".gemini").join("settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{ "theme": "dark", "mcpServers": { "user-server": { "httpUrl": "http://x" } } }"#,
        )
        .unwrap();

        merge_json_config(
            &path,
            "mcpServers",
            json!({"httpUrl": "http://127.0.0.1:1/mcp"}),
        )
        .expect("merge");
        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["theme"], "dark", "user keys preserved");
        assert!(root["mcpServers"]["user-server"].is_object());
        assert_eq!(
            root["mcpServers"][BLUEY_SERVER_NAME]["httpUrl"],
            "http://127.0.0.1:1/mcp"
        );

        remove_json_config_key(&path, "mcpServers").expect("remove");
        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(root["mcpServers"][BLUEY_SERVER_NAME].is_null());
        assert!(root["mcpServers"]["user-server"].is_object(), "surgical");
    }

    #[test]
    fn corrupt_user_config_is_never_overwritten() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("mcp.json");
        std::fs::write(&path, "{ not json").unwrap();
        let err = merge_json_config(&path, "mcpServers", json!({})).unwrap_err();
        assert!(err.to_string().contains("refusing to overwrite"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
    }

    #[test]
    fn env_and_args_recipes_match_the_spike() {
        let reg = BlueyServerReg {
            url: "http://127.0.0.1:1/mcp".into(),
            token: "tok".into(),
        };
        assert_eq!(
            drive_env_for(&AgentKind::Gemini, &reg),
            vec![("GEMINI_CLI_TRUST_WORKSPACE".to_string(), "true".to_string())]
        );
        assert_eq!(
            drive_env_for(&AgentKind::Codex, &reg),
            vec![(BLUEY_TOKEN_ENV.to_string(), "tok".to_string())]
        );
        assert!(drive_env_for(&AgentKind::ClaudeCode, &reg).is_empty());
        assert_eq!(drive_args_for(&AgentKind::Cursor), vec!["--force"]);
        assert!(drive_args_for(&AgentKind::ClaudeCode).is_empty());
    }
}
