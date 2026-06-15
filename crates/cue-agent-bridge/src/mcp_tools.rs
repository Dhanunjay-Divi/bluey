//! **MCP tools enumeration (Level 2)** — confirm, per agent, that the agent can
//! actually SEE and launch its own MCP connectors, by running the agent's OWN
//! "list my MCP servers/tools" command — not merely that the connectors sit in a
//! config file (Level 1, [`crate::read_connectors`]).
//!
//! The validation matrix's MCP step used to be a pure config read: it counted
//! the servers declared in the agent's connector config. That answers "are they
//! configured," not "can the agent see them." This module is the stronger,
//! truthful signal: it runs the agent's verified list command — e.g.
//! `claude mcp list`, `gemini mcp list`, `copilot mcp list`,
//! `cursor-agent mcp list-tools <server>` — and reports the servers (and, for
//! Cursor, the per-server TOOLS) the agent itself reports.
//!
//! ## Non-quota, read-only, bounded
//!
//! These commands LAUNCH the connector process / read its health; they do **not**
//! drive the model, so no account quota is spent and account limits are
//! irrelevant. Each command is bounded by [`LIST_TIMEOUT`] (killed on timeout via
//! `kill_on_drop`), captures bounded output, and is fully fail-soft: a missing
//! binary, a hanging server, or unparseable output degrades to [`McpToolsResult::Failed`]
//! / empty, never a panic and never a stall beyond the timeout.
//!
//! ## Secret-safe (CLAUDE.md §6)
//!
//! Only the `mcp list` / `mcp list-tools` forms are ever run (the registry's
//! [`crate::registry::AgentEntry::mcp_list_command`]). The dangerous
//! `claude mcp get <name>` form — which prints the connector's env (API keys) in
//! PLAINTEXT — is **never** invoked. As defense-in-depth, every captured line is
//! run through [`redact_line`] before parsing: any line that looks like it
//! carries a key/token (`pplx-…`, `sk-…`, `API_KEY`, `token=…`, a long
//! base64/hex run) is dropped, so a secret can never reach a result or a report.
//!
//! ## Data-driven, not `if agent ==`
//!
//! The per-agent command is registry DATA
//! ([`crate::registry::AgentEntry::mcp_list_command`] +
//! [`mcp_list_tools_per_server`](crate::registry::AgentEntry::mcp_list_tools_per_server)).
//! Adding an agent's MCP-list command is a registry edit, not a code branch here.

use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, BufReader};
use tokio::process::Command;

use crate::{discover_agents, registry, AgentKind};

/// Per-command wall-clock cap. An `mcp list` launches each configured connector
/// to health-check it, which is slower than a pure config read but still
/// completes well within this; past it we kill the child and record a failure
/// rather than let a hanging connector wedge the matrix.
const LIST_TIMEOUT: Duration = Duration::from_secs(15);

/// Hard cap on bytes read from a list command's stdout. An `mcp list` is a short
/// listing; this only guards against a runaway/hostile child.
const MAX_OUTPUT_BYTES: usize = 256 * 1024;

/// Max servers to scan in the per-server (Cursor) `list-tools` path. Bounds the
/// number of child spawns so a config with dozens of servers can't fan out
/// unboundedly; the first dozen are more than enough to prove the path works.
const MAX_SERVERS_SCANNED: usize = 12;

/// The honest outcome of one live MCP-enumeration attempt for an agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpToolsResult {
    /// The agent's list command ran and reported its MCP surface. `servers` are
    /// the live-reported server names; `tools` are per-server tool names when the
    /// agent enumerates them (Cursor's `list-tools`), else empty (the server-only
    /// `mcp list` agents). At least one of the two is non-empty.
    Enumerated {
        servers: Vec<String>,
        tools: Vec<String>,
    },
    /// This agent has no MCP-list command wired up (registry
    /// `mcp_list_command: None`). No process was spawned — the caller falls back
    /// to the Level-1 config read.
    NoCommand,
    /// The list command was attempted but failed (binary missing, non-zero exit,
    /// timeout, or it ran but reported nothing usable). `reason` is the real,
    /// redacted cause. The caller falls back to the Level-1 config read.
    Failed(String),
}

/// Run the given agent's verified MCP-list command and report what it sees.
///
/// **Read-only and NON-quota**: the command lists/health-checks connectors; it
/// never drives the model. Bounded by [`LIST_TIMEOUT`] and fully fail-soft.
///
/// Dispatch is data-driven off the registry row:
/// - no [`mcp_list_command`](registry::AgentEntry::mcp_list_command) →
///   [`McpToolsResult::NoCommand`];
/// - [`mcp_list_tools_per_server`](registry::AgentEntry::mcp_list_tools_per_server)
///   set (Cursor) → read the configured server names, then run
///   `<bin> <cmd> <server>` per server (bounded) and collect TOOL names;
/// - otherwise (claude/gemini/copilot/antigravity) → run `<bin> <cmd>` once and
///   parse the SERVER names from its output.
pub async fn enumerate_mcp_tools(kind: &AgentKind) -> McpToolsResult {
    let Some(tag) = registry::KindTag::from_agent_kind(kind) else {
        return McpToolsResult::NoCommand;
    };
    let Some(entry) = registry::entry_for(tag) else {
        return McpToolsResult::NoCommand;
    };
    let Some(cmd_args) = entry.mcp_list_command else {
        return McpToolsResult::NoCommand;
    };
    // The binary to run the list under is the one the agent DRIVES with — for
    // Antigravity that is `gemini` (its `drive_command[0]`), not its first
    // binary candidate (`agy`). Fail-soft if a row somehow has no drive command.
    let Some(binary) = entry.drive_command.first().copied() else {
        return McpToolsResult::NoCommand;
    };

    if entry.mcp_list_tools_per_server {
        enumerate_per_server_tools(kind, binary, cmd_args).await
    } else {
        enumerate_server_list(binary, cmd_args).await
    }
}

/// Server-level path: run `<binary> <cmd_args>` once and parse the server names
/// from its output (claude/gemini/copilot/antigravity).
async fn enumerate_server_list(binary: &str, cmd_args: &[&str]) -> McpToolsResult {
    let argv: Vec<String> = cmd_args.iter().map(|s| s.to_string()).collect();
    match run_list_command(binary, &argv).await {
        Ok(output) => {
            let servers = parse_server_list(&output);
            if servers.is_empty() {
                McpToolsResult::Failed("list command produced no server names".to_string())
            } else {
                McpToolsResult::Enumerated {
                    servers,
                    tools: Vec::new(),
                }
            }
        }
        Err(reason) => McpToolsResult::Failed(reason),
    }
}

/// Per-server path (Cursor): read the configured server names (the Level-1
/// config path), then run `<binary> <cmd_args> <server>` for each (bounded to
/// [`MAX_SERVERS_SCANNED`]) and collect the TOOL names it reports.
async fn enumerate_per_server_tools(
    kind: &AgentKind,
    binary: &str,
    cmd_args: &[&str],
) -> McpToolsResult {
    // Reuse the existing config-read path to learn which servers to scan.
    let configured: Vec<String> = discover_agents()
        .into_iter()
        .find(|d| &d.kind == kind)
        .and_then(|d| d.connector_config_path)
        .map(|cfg| {
            crate::read_connectors(&cfg)
                .into_iter()
                .map(|c| c.name)
                .collect()
        })
        .unwrap_or_default();

    if configured.is_empty() {
        return McpToolsResult::Failed("no configured MCP servers to enumerate".to_string());
    }

    let mut servers: Vec<String> = Vec::new();
    let mut tools: Vec<String> = Vec::new();
    let mut last_err: Option<String> = None;
    for name in configured.into_iter().take(MAX_SERVERS_SCANNED) {
        // argv = [..cmd_args, <server>], server is a config-derived name (not
        // attacker prompt text) and is passed as its own argv entry.
        let mut argv: Vec<String> = cmd_args.iter().map(|s| s.to_string()).collect();
        argv.push(name.clone());
        match run_list_command(binary, &argv).await {
            Ok(output) => {
                let server_tools = parse_tools_list(&output);
                if !server_tools.is_empty() {
                    servers.push(name);
                    tools.extend(server_tools);
                }
            }
            Err(e) => last_err = Some(e),
        }
    }

    if tools.is_empty() {
        return McpToolsResult::Failed(
            last_err.unwrap_or_else(|| "no tools enumerated for any server".to_string()),
        );
    }
    McpToolsResult::Enumerated { servers, tools }
}

/// Spawn `<binary> <argv...>` with a bounded timeout, kill-on-drop, and the
/// shared per-spawn runtime PATH (so `copilot mcp list` runs under Node ≥ 24).
/// Returns the captured stdout with every secret-ish line redacted, or an
/// `Err(reason)` (also redacted) for spawn failure / non-zero exit / timeout.
///
/// Mirrors the spawn + timeout posture of [`crate::drive`]'s CLI runner: argv is
/// an array (prompt-free here — only the binary + list args + a config-derived
/// server name), the child is killed on timeout, and output is size-capped.
async fn run_list_command(binary: &str, argv: &[String]) -> Result<String, String> {
    let mut cmd = Command::new(binary);
    cmd.args(argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    // Self-resolve a runtime-version mismatch (Copilot needs Node ≥ 24): drive
    // the list under a satisfying runtime by PREPENDING its bin dir to THIS
    // CHILD's PATH only — the same per-spawn mechanism the drive layer uses.
    // Fail-soft: when nothing applies the child inherits the parent PATH.
    if let Some(child_path) = crate::runtime_resolve::runtime_path_for_program(binary) {
        cmd.env("PATH", child_path);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let msg = if e.kind() == std::io::ErrorKind::NotFound {
                format!("binary `{binary}` not found on PATH")
            } else {
                format!("failed to spawn `{binary}`: {e}")
            };
            return Err(redact(&msg));
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Read BOTH stdout and stderr (bounded) concurrently and await exit, all
    // under one wall-clock deadline. Capturing both is required: some CLIs print
    // the listing to stdout (claude/cursor/copilot) while others print it to
    // STDERR (gemini's `mcp list` writes the server list to stderr with a 0 exit)
    // — VERIFIED live. Parsing the combined text means the stream choice doesn't
    // matter and the signal is never lost to the wrong pipe.
    let collect = async {
        let (out_bytes, err_bytes) = tokio::join!(read_bounded(stdout), read_bounded(stderr));
        let status = child.wait().await;
        (out_bytes, err_bytes, status)
    };

    let (out_bytes, err_bytes, status) = match tokio::time::timeout(LIST_TIMEOUT, collect).await {
        Ok(triple) => triple,
        Err(_) => {
            // Timed out: the child is killed on drop (kill_on_drop) when `child`
            // leaves scope at function return.
            return Err(format!(
                "mcp list timed out after {}s",
                LIST_TIMEOUT.as_secs()
            ));
        }
    };

    // Combine the two streams (stdout first), then redact once. The parsers skip
    // header/noise lines, so concatenating a stdout banner with a stderr listing
    // is harmless.
    let combined = {
        let mut s = String::from_utf8_lossy(&out_bytes).into_owned();
        let err = String::from_utf8_lossy(&err_bytes);
        if !err.trim().is_empty() {
            if !s.is_empty() && !s.ends_with('\n') {
                s.push('\n');
            }
            s.push_str(&err);
        }
        redact(&s)
    };

    match status {
        // Success, or a non-zero/odd exit that still produced a listing (e.g.
        // copilot's runtime-version notice): if we captured usable text, return
        // it — the parser will keep only real server/tool lines.
        Ok(s) if s.success() || !combined.trim().is_empty() => Ok(combined),
        Ok(s) => {
            let code = s
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string());
            Err(redact(&format!("exited with status {code} and no output")))
        }
        Err(e) => Err(redact(&format!("failed to await child: {e}"))),
    }
}

/// Read an optional async stream into a bounded byte buffer, fail-soft. Returns
/// an empty buffer for `None` or any read error. Caps at [`MAX_OUTPUT_BYTES`] so
/// a runaway/hostile child cannot exhaust memory.
async fn read_bounded<R>(stream: Option<R>) -> Vec<u8>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let Some(stream) = stream else {
        return Vec::new();
    };
    let mut buf = Vec::with_capacity(8 * 1024);
    let mut reader = BufReader::new(stream);
    let mut chunk = [0u8; 8 * 1024];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                if buf.len() + n > MAX_OUTPUT_BYTES {
                    buf.extend_from_slice(&chunk[..(MAX_OUTPUT_BYTES - buf.len())]);
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            Err(_) => break,
        }
    }
    buf
}

// ---------------------------------------------------------------------------
// Output parsers (pure — unit-tested against sampled real output)
// ---------------------------------------------------------------------------

/// Parse SERVER names from a server-level `mcp list` output, across the verified
/// CLI shapes (all secret-free; the caller feeds combined stdout+stderr because
/// Gemini prints its listing to STDERR):
/// - Claude:   `perplexity: npx -y @perplexity-ai/mcp-server - ✓ Connected`
/// - Gemini:   `✓ github: npx -y @modelcontextprotocol/server-github (stdio) - Connected`
/// - Cursor:   `context7: not loaded (needs approval)` (the `mcp list` form)
/// - Copilot:  `User servers:` then `  perplexity (local)`
///
/// Strategy (tolerant, never fabricates): a server line is one of
/// 1. `<name>: <rest>` — take the token before the first colon (Claude/Gemini/
///    Cursor), stripping a leading status glyph (`✓`/`✗`/`-`/`•`); or
/// 2. an indented `  <name> (local|http|stdio|…)` under a `… servers:` header
///    (Copilot), where there is no colon — take the token before ` (`.
///
/// Header/blank/noise lines (`Checking MCP server health...`, `Configured MCP
/// servers:`, `User servers:`) are skipped. Input is assumed already redacted.
pub fn parse_server_list(output: &str) -> Vec<String> {
    let mut names = Vec::new();
    for raw in output.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Skip obvious header / status lines.
        if is_header_line(trimmed) {
            continue;
        }

        // Shape 1: "<name>: <rest>" — strip a leading status glyph first.
        if let Some((before_colon, _)) = trimmed.split_once(':') {
            let name = strip_status_prefix(before_colon).trim();
            if is_plausible_server_name(name) {
                push_unique(&mut names, name);
                continue;
            }
        }

        // Shape 2 (Copilot): an INDENTED "  <name> (local)" with no colon. Only
        // accept indented lines here so a stray top-level sentence isn't read as
        // a server.
        let indented = line.starts_with(' ') || line.starts_with('\t');
        if indented {
            let body = trimmed;
            let candidate = match body.split_once(" (") {
                Some((n, _)) => n.trim(),
                None => body,
            };
            if is_plausible_server_name(candidate) {
                push_unique(&mut names, candidate);
            }
        }
    }
    names
}

/// Parse TOOL names from Cursor's `mcp list-tools <server>` output:
///
/// ```text
/// Tools for perplexity (4):
/// - perplexity_ask (messages, search_recency_filter, …)
/// - perplexity_reason (messages, strip_thinking, …)
/// ```
///
/// Each tool is a line beginning with `- ` (or `* `); the tool name is the token
/// before the first ` (` (the parenthesized params, which are field NAMES not
/// values — still, we keep only the tool name). The `Tools for …` header is
/// skipped. Input is assumed already redacted.
pub fn parse_tools_list(output: &str) -> Vec<String> {
    let mut tools = Vec::new();
    for raw in output.lines() {
        let trimmed = raw.trim();
        let body = if let Some(rest) = trimmed.strip_prefix("- ") {
            rest
        } else if let Some(rest) = trimmed.strip_prefix("* ") {
            rest
        } else {
            continue;
        };
        // Tool name is the token before the first " (" (params) or whitespace.
        let name = match body.split_once(" (") {
            Some((n, _)) => n.trim(),
            None => body.split_whitespace().next().unwrap_or("").trim(),
        };
        if is_plausible_server_name(name) {
            push_unique(&mut tools, name);
        }
    }
    tools
}

/// Header / status lines that an `mcp list` may print but that are NOT servers.
fn is_header_line(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.ends_with("servers:")            // "Configured MCP servers:", "User servers:"
        || l.starts_with("checking")   // "Checking MCP server health..."
        || l.starts_with("no mcp")     // "No MCP servers configured"
        || l == "tools:"
}

/// Strip a leading status glyph / bullet from a server token: `✓ github` →
/// `github`, `- context7` → `context7`, `• x` → `x`.
fn strip_status_prefix(s: &str) -> &str {
    let s = s.trim_start();
    for g in ['✓', '✗', '✔', '✘', '-', '•', '*'] {
        if let Some(rest) = s.strip_prefix(g) {
            return rest.trim_start();
        }
    }
    s
}

/// True when `name` is a plausible MCP server/tool identifier: non-empty, not
/// too long, and made only of the characters real names use (letters, digits,
/// `_`, `-`, `.`, and a space — Cursor allows `supabase DIVINI MCP`). This keeps
/// a stray prose line or a redaction marker from being recorded as a name.
fn is_plausible_server_name(name: &str) -> bool {
    if name.is_empty() || name.chars().count() > 64 {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
        && name.chars().any(|c| c.is_ascii_alphanumeric())
}

/// Append `name` if not already present (case-sensitive; names are stable IDs).
fn push_unique(out: &mut Vec<String>, name: &str) {
    let owned = name.to_string();
    if !out.contains(&owned) {
        out.push(owned);
    }
}

// ---------------------------------------------------------------------------
// Redaction (defense-in-depth — the captured output must NEVER carry a secret)
// ---------------------------------------------------------------------------

/// Redact any secret-bearing lines from captured command output.
///
/// Splits on lines and drops/masks any line that [`line_carries_secret`] flags.
/// A dropped line is replaced with a `[redacted: possible secret]` marker so the
/// shape of the output is preserved for debugging without leaking the value.
/// Applied to BOTH stdout and any stderr before either is parsed or surfaced.
pub fn redact(text: &str) -> String {
    text.lines()
        .map(|line| {
            if line_carries_secret(line) {
                "[redacted: possible secret]"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Convenience alias matching the brief's name: redact a single line, returning
/// either the line unchanged or the redaction marker. Used by tests and as a
/// readable single-line entry point.
pub fn redact_line(line: &str) -> &str {
    if line_carries_secret(line) {
        "[redacted: possible secret]"
    } else {
        line
    }
}

/// Heuristic: does this line look like it carries a credential? Conservative but
/// broad — it is cheaper to redact a benign line than to leak a key. Flags:
/// - known key prefixes embedded as tokens: `pplx-…`, `sk-…`, `sk-ant-…`,
///   `ghp_…`, `gho_…`, `github_pat_…`, `AIza…`, `xoxb-…`;
/// - the literals `API_KEY` / `APIKEY` / `SECRET` / `TOKEN=` / `PASSWORD`
///   (case-insensitive), or a `key=`/`token=`/`secret=` assignment;
/// - a long unbroken base64/hex-ish run (≥ 32 chars) that is characteristic of a
///   raw key dumped inline.
fn line_carries_secret(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();

    // Assignment-style or named-secret markers.
    const NAMED: &[&str] = &[
        "api_key", "apikey", "secret", "password", "passwd", "bearer ",
    ];
    if NAMED.iter().any(|m| lower.contains(m)) {
        return true;
    }
    // `token=`, `key=`, `secret=`, `apikey=` assignments (value present).
    for k in ["token=", "key=", "secret=", "apikey=", "api_key="] {
        if let Some(idx) = lower.find(k) {
            let after = &lower[idx + k.len()..];
            if after.chars().next().is_some_and(|c| !c.is_whitespace()) {
                return true;
            }
        }
    }

    // Known credential token prefixes, matched per whitespace-separated token so
    // a benign mention of the word elsewhere doesn't over-match the whole check
    // (the prefix must START a token).
    const TOKEN_PREFIXES: &[&str] = &[
        "pplx-",
        "sk-",
        "sk-ant-",
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "aiza",
    ];
    for tok in line.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | '(' | ')'))
    {
        let lt = tok.to_ascii_lowercase();
        for p in TOKEN_PREFIXES {
            // A real key is the prefix PLUS a body; the bare word (e.g. "sk-" or
            // an "aiza"-less mention) is not enough.
            if lt.starts_with(p) && lt.len() > p.len() + 6 {
                return true;
            }
        }
        // A long base64/hex-ish run characteristic of a raw key.
        if is_long_keyish_run(tok) {
            return true;
        }
    }
    false
}

/// True when `tok` is a single long run (≥ 32 chars) of base64/hex key
/// characters (`A–Z a–z 0–9 _ - + / =`). Plain English words and command lines
/// are broken up by spaces/punctuation and won't reach this length as one token;
/// an npm package spec (`@scope/pkg`) contains `/`/`@` but is far shorter.
fn is_long_keyish_run(tok: &str) -> bool {
    let len = tok.chars().count();
    if len < 32 {
        return false;
    }
    tok.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '/' | '='))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- per-agent command lookup (registry data) -----------------------

    #[test]
    fn cli_agents_have_their_verified_list_command() {
        use registry::KindTag;
        // claude / gemini / copilot: server-level `mcp list`.
        for tag in [KindTag::ClaudeCode, KindTag::Gemini, KindTag::Copilot] {
            let e = registry::entry_for(tag).unwrap();
            assert_eq!(
                e.mcp_list_command,
                Some(&["mcp", "list"][..]),
                "{} should run `mcp list`",
                e.display_name
            );
            assert!(
                !e.mcp_list_tools_per_server,
                "{} is server-level, not per-server",
                e.display_name
            );
        }
        // Antigravity drives through gemini and shares `mcp list`.
        let antigravity = registry::entry_for(KindTag::Antigravity).unwrap();
        assert_eq!(antigravity.mcp_list_command, Some(&["mcp", "list"][..]));
        // Antigravity's list binary is `gemini` (its drive_command), not `agy`.
        assert_eq!(antigravity.drive_command.first().copied(), Some("gemini"));

        // Cursor is the per-server `mcp list-tools` path.
        let cursor = registry::entry_for(KindTag::Cursor).unwrap();
        assert_eq!(cursor.mcp_list_command, Some(&["mcp", "list-tools"][..]));
        assert!(cursor.mcp_list_tools_per_server);
    }

    #[test]
    fn non_cli_agents_have_no_list_command() {
        use registry::KindTag;
        // Codex (no verified non-quota list), Aider/Windsurf/VS Code (no CLI),
        // and the Claude-app index rows are all config-only.
        for tag in [
            KindTag::Codex,
            KindTag::Aider,
            KindTag::Windsurf,
            KindTag::VsCode,
            KindTag::ClaudeCodeApp,
            KindTag::ClaudeCodeAgent,
        ] {
            let e = registry::entry_for(tag).unwrap();
            assert!(
                e.mcp_list_command.is_none(),
                "{} must have no mcp_list_command (config-only)",
                e.display_name
            );
        }
    }

    #[test]
    fn per_server_marker_only_set_for_cursor() {
        // Exactly one row uses the per-server tool-enumeration path; the rest are
        // either server-level or None. Guards against a future copy/paste setting
        // the marker on a server-level row (which would mis-run `mcp list <x>`).
        let per_server: Vec<_> = registry::REGISTRY
            .iter()
            .filter(|e| e.mcp_list_tools_per_server)
            .map(|e| e.display_name)
            .collect();
        assert_eq!(per_server, vec!["Cursor"]);
    }

    // ---- server-list parsing (sampled REAL output) ----------------------

    #[test]
    fn parses_claude_mcp_list() {
        // Exact shape from `claude mcp list` on this machine.
        let out = "Checking MCP server health...\n\n\
                   perplexity: npx -y @perplexity-ai/mcp-server - ✓ Connected\n";
        let servers = parse_server_list(out);
        assert_eq!(servers, vec!["perplexity"]);
    }

    #[test]
    fn parses_gemini_mcp_list_with_status_glyph() {
        // `gemini mcp list`: a leading ✓ glyph before the name.
        let out = "Configured MCP servers:\n\n\
                   ✓ github: npx -y @modelcontextprotocol/server-github (stdio) - Connected\n";
        let servers = parse_server_list(out);
        assert_eq!(servers, vec!["github"]);
    }

    #[test]
    fn parses_cursor_mcp_list_servers() {
        // `cursor-agent mcp list` (the server listing form, not list-tools).
        let out = "context7: not loaded (needs approval)\n\
                   supabase_fairhire: not loaded (needs approval)\n\
                   perplexity: not loaded (needs approval)\n\
                   supabase DIVINI MCP: not loaded (needs approval)\n";
        let servers = parse_server_list(out);
        assert_eq!(
            servers,
            vec![
                "context7",
                "supabase_fairhire",
                "perplexity",
                "supabase DIVINI MCP"
            ]
        );
    }

    #[test]
    fn parses_copilot_user_servers_block() {
        // `copilot mcp list`: an indented "<name> (local)" under a header.
        let out = "User servers:\n  perplexity (local)\n  github (local)\n";
        let servers = parse_server_list(out);
        assert_eq!(servers, vec!["perplexity", "github"]);
    }

    #[test]
    fn header_and_blank_lines_are_skipped() {
        let out = "Configured MCP servers:\n\nChecking MCP server health...\n";
        assert!(parse_server_list(out).is_empty());
    }

    // ---- tool-list parsing (Cursor list-tools, sampled REAL output) -----

    #[test]
    fn parses_cursor_list_tools_for_perplexity() {
        // EXACT shape from `cursor-agent mcp list-tools perplexity`.
        let out = "Tools for perplexity (4):\n\
                   - perplexity_ask (messages, search_recency_filter, search_domain_filter, search_context_size)\n\
                   - perplexity_reason (messages, strip_thinking, search_recency_filter)\n\
                   - perplexity_research (messages, strip_thinking, reasoning_effort)\n\
                   - perplexity_search (query, max_results, max_tokens_per_page, country)\n";
        let tools = parse_tools_list(out);
        assert_eq!(
            tools,
            vec![
                "perplexity_ask",
                "perplexity_reason",
                "perplexity_research",
                "perplexity_search"
            ]
        );
        // The `Tools for …` header is not a tool.
        assert!(!tools.iter().any(|t| t.contains("Tools")));
    }

    #[test]
    fn tool_list_ignores_non_bullet_lines() {
        let out = "Tools for x (1):\nsome prose line\n- only_tool (a, b)\n";
        assert_eq!(parse_tools_list(out), vec!["only_tool"]);
    }

    // ---- redaction (the hard secret guarantee) --------------------------

    #[test]
    fn redacts_a_line_with_a_perplexity_key() {
        // A line that (hypothetically) leaked a pplx- key must be masked.
        let leaked = "perplexity: npx -y server --key pplx-abcdef0123456789abcdef0123456789";
        assert_eq!(redact_line(leaked), "[redacted: possible secret]");
        // And it must not survive into a parsed server list either.
        let servers = parse_server_list(&redact(leaked));
        assert!(
            !servers.iter().any(|s| s.contains("pplx")),
            "redacted key leaked into parse: {servers:?}"
        );
    }

    #[test]
    fn redacts_assorted_key_shapes_and_assignments() {
        for leaked in [
            "API_KEY=super-secret-value",
            "env TOKEN=ghp_0123456789abcdef0123456789abcdef0123",
            "Authorization: Bearer sk-ant-0123456789abcdef0123456789",
            "OPENAI key sk-proj-ABCDEFGHIJKLMNOPQRSTUVWX",
            "x-goog-api-key: AIzaSyA1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6",
            "password = hunter2hunter2",
            "blob 0123456789abcdef0123456789abcdef0123456789abcd",
        ] {
            assert_eq!(
                redact_line(leaked),
                "[redacted: possible secret]",
                "should redact: {leaked:?}"
            );
        }
    }

    #[test]
    fn redaction_keeps_benign_list_lines_intact() {
        // The real, secret-free `mcp list` lines must pass through untouched —
        // otherwise we'd redact away the very signal we're capturing.
        for benign in [
            "perplexity: npx -y @perplexity-ai/mcp-server - ✓ Connected",
            "✓ github: npx -y @modelcontextprotocol/server-github (stdio) - Connected",
            "  perplexity (local)",
            "- perplexity_ask (messages, search_recency_filter)",
            "context7: not loaded (needs approval)",
        ] {
            assert_eq!(redact_line(benign), benign, "should NOT redact: {benign:?}");
        }
        // A full multi-line redact pass preserves benign content verbatim.
        let block = "Checking MCP server health...\n\nperplexity: npx -y x - ✓ Connected";
        assert_eq!(redact(block), block);
    }

    #[test]
    fn long_keyish_run_detector_is_specific() {
        // A raw 40-char hex run is flagged…
        assert!(is_long_keyish_run(
            "0123456789abcdef0123456789abcdef01234567"
        ));
        // …but ordinary words / short tokens / package specs are not.
        assert!(!is_long_keyish_run("perplexity"));
        assert!(!is_long_keyish_run("@perplexity-ai/mcp-server"));
        assert!(!is_long_keyish_run("Connected"));
    }

    // ---- result variants -------------------------------------------------

    #[test]
    fn result_equality_distinguishes_variants() {
        let a = McpToolsResult::Enumerated {
            servers: vec!["perplexity".into()],
            tools: vec![],
        };
        let b = McpToolsResult::NoCommand;
        let c = McpToolsResult::Failed("x".into());
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_eq!(b, McpToolsResult::NoCommand);
    }

    // ---- live enumeration is fail-soft for absent agents ----------------

    #[tokio::test]
    async fn enumerate_is_nocommand_for_agent_without_a_list_command() {
        // Codex has no list command → NoCommand, no spawn, no panic.
        let r = enumerate_mcp_tools(&AgentKind::Codex).await;
        assert_eq!(r, McpToolsResult::NoCommand);
        // Aider too.
        assert_eq!(
            enumerate_mcp_tools(&AgentKind::Aider).await,
            McpToolsResult::NoCommand
        );
    }
}
