//! PHASE 2 — map a discovered [`crate::AgentKind`] to its ACP entrypoint
//! ([`AcpAgentSpec`]: an executable + args we spawn as an ACP agent subprocess).
//!
//! We produce `program` + `args` directly rather than using the SDK's
//! convenience constructors (`AcpAgent::zed_claude_code()`, `google_gemini()`,
//! `zed_codex()`). Those return an opaque `AcpAgent` with no public path back to
//! `program`/`args`, AND they hard-code `npx -y …@latest` wrappers (and the older
//! `--experimental-acp` flag for Gemini) rather than the installed binaries we
//! discover. We cite the SDK strings in the docs below but map to the real
//! installed entrypoints.

use crate::acp::client::AcpAgentSpec;
use crate::{AgentKind, DiscoveredAgent};
use std::path::{Path, PathBuf};

/// Whether this agent's ACP entrypoint is its OWN discovered binary (native
/// ACP — Gemini/Cursor/Copilot) versus a SEPARATE adapter binary (Claude/Codex,
/// whose discovered binary is the base `claude`/`codex` CLI, not the adapter).
fn native_acp(agent: &AgentKind) -> bool {
    matches!(
        agent,
        AgentKind::Gemini | AgentKind::Cursor | AgentKind::Copilot
    )
}

/// Build the ACP entrypoint for a *discovered* agent, resolving the REAL
/// installed path so it works wherever the user installed the agent (not just
/// when it happens to be on the daemon's `PATH`).
///
/// - **Native ACP agents** (Gemini/Cursor/Copilot): use the agent's own
///   discovered executable (from [`DiscoveredAgent::install_evidence`]) as the
///   program, plus the ACP args from the bare-name mapping. If discovery only
///   found a footprint (no executable path), fall back to the bare name.
/// - **Adapter agents** (Claude/Codex): the discovered binary is the base CLI
///   (`claude`/`codex`), but ACP is spoken by a SEPARATE adapter
///   (`claude-agent-acp`/`codex-acp`). We keep the adapter program from the
///   bare-name mapping (resolved on `PATH` at spawn time); the discovered base
///   binary doesn't substitute for it.
///
/// Falls back to [`AcpAgentSpec::try_from`] (bare names) when no better path is
/// available, so behavior never regresses.
pub fn acp_spec_for_discovered(discovered: &DiscoveredAgent) -> anyhow::Result<AcpAgentSpec> {
    let base = AcpAgentSpec::try_from(&discovered.kind)?;
    if native_acp(&discovered.kind) {
        if let Some(exe) = first_executable(&discovered.install_evidence) {
            return Ok(AcpAgentSpec::new(
                exe.to_string_lossy().to_string(),
                base.args,
            ));
        }
    }
    // Adapter agents, or native agents with only a footprint: keep the
    // bare-name spec (the client resolves it on PATH, and the PATH-augment in
    // `to_acp_agent` puts its install dir first for Node-shebang adapters).
    Ok(base)
}

/// First evidence path that looks like a runnable executable (absolute path to
/// a file under a `bin`-like dir), if any.
fn first_executable(evidence: &[PathBuf]) -> Option<&Path> {
    evidence.iter().map(PathBuf::as_path).find(|p| {
        p.is_absolute()
            && p.is_file()
            && p.components().any(|c| {
                let s = c.as_os_str().to_string_lossy();
                s == "bin" || s == "sbin" || s.ends_with(".local")
            })
    })
}

impl TryFrom<&AgentKind> for AcpAgentSpec {
    type Error = anyhow::Error;

    fn try_from(agent: &AgentKind) -> Result<Self, Self::Error> {
        let spec = match agent {
            // ---- Native ACP agents (the agent's own CLI speaks ACP) ----
            // Gemini CLI: `gemini --acp` (Google's reference ACP impl).
            // NOTE the SDK ctor uses the older `--experimental-acp`; the current
            // flag is `--acp`.
            AgentKind::Gemini => AcpAgentSpec::new("gemini", vec!["--acp".to_string()]),
            // Cursor: `cursor-agent acp` (native). NOTE: the ACP command is
            // TOP-LEVEL in current builds (verified on 2026.06.16). An older
            // build used `cursor-agent agent acp`, but that now parses "acp" as a
            // prompt arg to `agent` and hangs on a TTY (no stdio handshake) — so
            // the daemon timed out. The subcommand is `acp`, not `agent acp`.
            AgentKind::Cursor => AcpAgentSpec::new("cursor-agent", vec!["acp".to_string()]),
            // GitHub Copilot CLI: `copilot --acp --stdio` (native, public preview).
            AgentKind::Copilot => {
                AcpAgentSpec::new("copilot", vec!["--acp".to_string(), "--stdio".to_string()])
            }

            // ---- Via Zed's open-source ACP adapters (Apache-2.0) ----
            // Claude Code (and the desktop App/Agent surfaces — same engine):
            // `claude-agent-acp` (the @zed-industries/claude-agent-acp binary;
            // SDK ctor: `npx -y @zed-industries/claude-code-acp@latest`).
            AgentKind::ClaudeCode | AgentKind::ClaudeCodeApp | AgentKind::ClaudeCodeAgent => {
                AcpAgentSpec::new("claude-agent-acp", Vec::new())
            }
            // Codex: a codex ACP adapter (SDK ctor: `npx -y @zed-industries/
            // codex-acp@latest`).
            // NEEDS VERIFICATION: the exact installed adapter binary name. We use
            // `codex-acp` as the entrypoint; confirm at integration time.
            AgentKind::Codex => AcpAgentSpec::new("codex-acp", Vec::new()),

            // Antigravity: ACP via the sibling `gemini --acp`. Antigravity DOES
            // ship a real CLI (`agy`, a Go binary, v1.0.10 — verified 2026-06-19,
            // correcting the earlier "only agy-node runtime" note), but `agy`
            // itself has NO ACP mode (only --print / --conversation / a TUI). Its
            // registry row already drives through `gemini` (binary_candidates
            // include gemini; drive_command `gemini -p {prompt}`), and `gemini`
            // speaks ACP — so Antigravity's ACP entrypoint is `gemini --acp`,
            // identical to the Gemini arm. (Continuation stays Replay: `agy
            // --conversation=<uuid>` could resume natively but isn't assumed
            // installed; the gemini-driven path replays.)
            AgentKind::Antigravity => AcpAgentSpec::new("gemini", vec!["--acp".to_string()]),

            // ---- No (known) local ACP entrypoint ----
            AgentKind::Aider => {
                anyhow::bail!("Aider ACP support is unconfirmed; no known local ACP entrypoint")
            }
            AgentKind::CursorCloud
            | AgentKind::CopilotCloud
            | AgentKind::CodexCloud
            | AgentKind::AnthropicCloud
            | AgentKind::AntigravityCloud
            | AgentKind::GeminiCloud => {
                anyhow::bail!("{agent:?} is a cloud agent, not a local ACP subprocess")
            }
            AgentKind::VsCodeFork | AgentKind::Windsurf => {
                anyhow::bail!("{agent:?} has no known local ACP entrypoint")
            }
            AgentKind::Other(label) => {
                anyhow::bail!("unrecognized agent {label:?} has no ACP entrypoint")
            }
            AgentKind::Unknown => {
                anyhow::bail!("unclassified agent has no ACP entrypoint")
            }
        };
        Ok(spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(agent: AgentKind) -> AcpAgentSpec {
        AcpAgentSpec::try_from(&agent).expect("agent should map to an ACP spec")
    }

    #[test]
    fn native_agents_map_to_expected_entrypoints() {
        let g = spec(AgentKind::Gemini);
        assert_eq!(g.program, "gemini");
        assert_eq!(g.args, vec!["--acp".to_string()]);

        let c = spec(AgentKind::Cursor);
        assert_eq!(c.program, "cursor-agent");
        assert_eq!(c.args, vec!["acp".to_string()]);

        let cp = spec(AgentKind::Copilot);
        assert_eq!(cp.program, "copilot");
        assert_eq!(cp.args, vec!["--acp".to_string(), "--stdio".to_string()]);

        // Antigravity drives its ACP through the sibling `gemini --acp` (its own
        // `agy` CLI has no ACP mode).
        let a = spec(AgentKind::Antigravity);
        assert_eq!(a.program, "gemini");
        assert_eq!(a.args, vec!["--acp".to_string()]);
    }

    #[test]
    fn claude_surfaces_share_the_adapter() {
        for k in [
            AgentKind::ClaudeCode,
            AgentKind::ClaudeCodeApp,
            AgentKind::ClaudeCodeAgent,
        ] {
            let s = spec(k);
            assert_eq!(s.program, "claude-agent-acp");
            assert!(s.args.is_empty());
        }
    }

    #[test]
    fn unsupported_kinds_return_err() {
        for k in [
            AgentKind::Aider,
            AgentKind::CursorCloud,
            AgentKind::CopilotCloud,
            AgentKind::CodexCloud,
            AgentKind::AnthropicCloud,
            AgentKind::AntigravityCloud,
            AgentKind::GeminiCloud,
            AgentKind::VsCodeFork,
            AgentKind::Windsurf,
            AgentKind::Other("zed".to_string()),
            AgentKind::Unknown,
        ] {
            assert!(
                AcpAgentSpec::try_from(&k).is_err(),
                "{k:?} should have no ACP entrypoint"
            );
        }
    }
}
