//! MCP connector config reader: JSONC-tolerant, fail-soft, secret-free.
//!
//! Real configs are messy (Cursor `mcp.json` has trailing commas, VS Code
//! `settings.json` carries control characters), so this module strips comments
//! and trailing commas by hand before handing the text to `serde_json`. If a
//! parse still fails it logs a warning and returns an empty `Vec` — it never
//! panics and never blocks discovery.
//!
//! We read connector **shape** only — `command`/`args`/`url` and a coarse
//! [`AuthTier`]. We never read, copy, or log secret values from `env`.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// How a connector is reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    /// Spawned local process.
    Stdio { command: String, args: Vec<String> },
    /// Remote endpoint (http/sse).
    Http { url: String },
}

/// Coarse auth classification — never a secret, only the *tier*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthTier {
    /// Stdio server with a non-empty `env` block (keys respawnable directly).
    EnvAuth,
    /// Hosted http/sse endpoint that likely carries app-negotiated OAuth.
    HostedOauth,
    /// No auth shape detected.
    #[serde(rename = "none")]
    None_,
}

/// A single inherited MCP connector, shape + auth tier only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connector {
    pub name: String,
    pub transport: Transport,
    pub auth_tier: AuthTier,
}

/// Read an MCP config file and return its connectors. Fail-soft: a missing,
/// unreadable, or unparseable file yields an empty `Vec` (with a logged
/// warning), never an error or panic.
pub fn read_connectors(path: &Path) -> Vec<Connector> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "mcp config unreadable");
            return Vec::new();
        }
    };
    parse_connectors(&raw, &path.display().to_string())
}

/// Parse connectors from raw config text (JSONC-tolerant). Separated from IO
/// so it can be unit-tested directly against fixture strings.
pub fn parse_connectors(raw: &str, source_label: &str) -> Vec<Connector> {
    let cleaned = strip_jsonc(raw);
    let value: serde_json::Value = match serde_json::from_str(&cleaned) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(source = source_label, error = %e, "mcp config parse failed");
            return Vec::new();
        }
    };

    // Accept both `mcpServers` (Claude/Cursor) and `servers` (VS Code).
    let servers = value
        .get("mcpServers")
        .or_else(|| value.get("servers"))
        .and_then(|v| v.as_object());

    let Some(servers) = servers else {
        return Vec::new();
    };

    let mut out = Vec::with_capacity(servers.len());
    for (name, spec) in servers {
        if let Some(conn) = classify_server(name, spec) {
            out.push(conn);
        }
    }
    out
}

/// Build a [`Connector`] from one server spec object. Unknown shapes are
/// skipped (returns `None`) rather than guessed.
fn classify_server(name: &str, spec: &serde_json::Value) -> Option<Connector> {
    let obj = spec.as_object()?;

    // HTTP/SSE transport: presence of a `url` (or explicit http/sse type).
    if let Some(url) = obj.get("url").and_then(|u| u.as_str()) {
        return Some(Connector {
            name: name.to_string(),
            transport: Transport::Http {
                url: url.to_string(),
            },
            auth_tier: AuthTier::HostedOauth,
        });
    }

    // Stdio transport: a `command` plus optional `args` / `env`.
    if let Some(command) = obj.get("command").and_then(|c| c.as_str()) {
        let args = obj
            .get("args")
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        // Only the *presence* of a non-empty env block matters — never values.
        let has_env = obj
            .get("env")
            .and_then(|e| e.as_object())
            .map(|m| !m.is_empty())
            .unwrap_or(false);

        let auth_tier = if has_env {
            AuthTier::EnvAuth
        } else {
            AuthTier::None_
        };

        return Some(Connector {
            name: name.to_string(),
            transport: Transport::Stdio {
                command: command.to_string(),
                args,
            },
            auth_tier,
        });
    }

    None
}

/// Strip JSONC features that `serde_json` rejects: `//` line comments,
/// `/* */` block comments, trailing commas, and stray control characters —
/// while respecting string literals (so `"http://..."` and `"a, b"` survive).
fn strip_jsonc(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut in_string = false;
    let mut escaped = false;

    while i < bytes.len() {
        let c = bytes[i];

        if in_string {
            out.push(c as char);
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        match c {
            b'"' => {
                in_string = true;
                out.push('"');
                i += 1;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                // Line comment: skip to end of line.
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                // Block comment: skip to closing `*/`.
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            // Drop raw control chars (except whitespace serde tolerates).
            0x00..=0x08 | 0x0b | 0x0c | 0x0e..=0x1f => {
                i += 1;
            }
            _ => {
                out.push(c as char);
                i += 1;
            }
        }
    }

    strip_trailing_commas(&out)
}

/// Remove trailing commas (`,` followed only by whitespace then `}` or `]`).
/// Operates outside string literals only.
fn strip_trailing_commas(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut in_string = false;
    let mut escaped = false;

    for (idx, &c) in chars.iter().enumerate() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        if c == '"' {
            in_string = true;
            out.push(c);
            continue;
        }

        if c == ',' {
            // Look ahead past whitespace for a closing bracket.
            let next = chars[idx + 1..]
                .iter()
                .find(|ch| !ch.is_whitespace())
                .copied();
            if matches!(next, Some('}') | Some(']')) {
                continue; // drop the trailing comma
            }
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parses_trailing_commas() {
        let raw = r#"{
            "mcpServers": {
                "fs": { "command": "node", "args": ["server.js"], },
            },
        }"#;
        let conns = parse_connectors(raw, "fixture");
        assert_eq!(conns.len(), 1);
        assert_eq!(conns[0].name, "fs");
    }

    #[test]
    fn test_parses_line_and_block_comments() {
        let raw = r#"{
            // top comment
            "mcpServers": {
                /* the filesystem server */
                "fs": { "command": "node" }
            }
        }"#;
        let conns = parse_connectors(raw, "fixture");
        assert_eq!(conns.len(), 1);
    }

    #[test]
    fn test_url_in_string_survives_comment_stripping() {
        let raw = r#"{ "servers": { "remote": { "url": "https://api.example.com/mcp" } } }"#;
        let conns = parse_connectors(raw, "fixture");
        assert_eq!(conns.len(), 1);
        match &conns[0].transport {
            Transport::Http { url } => assert_eq!(url, "https://api.example.com/mcp"),
            other => panic!("expected http transport, got {other:?}"),
        }
        assert_eq!(conns[0].auth_tier, AuthTier::HostedOauth);
    }

    #[test]
    fn test_env_auth_classification() {
        let raw = r#"{ "mcpServers": {
            "db": { "command": "pg-mcp", "env": { "PG_URL": "secret" } }
        } }"#;
        let conns = parse_connectors(raw, "fixture");
        assert_eq!(conns[0].auth_tier, AuthTier::EnvAuth);
    }

    #[test]
    fn test_no_auth_when_env_empty_or_absent() {
        let raw = r#"{ "mcpServers": {
            "plain": { "command": "thing", "args": [] },
            "empty_env": { "command": "thing2", "env": {} }
        } }"#;
        let conns = parse_connectors(raw, "fixture");
        assert!(conns.iter().all(|c| c.auth_tier == AuthTier::None_));
    }

    #[test]
    fn test_control_chars_are_stripped() {
        // Embedded NUL and bell between tokens.
        let raw = "{\u{0007}\"mcpServers\":\u{0000}{\"x\":{\"command\":\"y\"}}}";
        let conns = parse_connectors(raw, "fixture");
        assert_eq!(conns.len(), 1);
    }

    #[test]
    fn test_servers_top_level_key() {
        let raw = r#"{ "servers": { "a": { "command": "x" } } }"#;
        assert_eq!(parse_connectors(raw, "fixture").len(), 1);
    }

    #[test]
    fn test_malformed_returns_empty_not_panic() {
        let raw = "{ this is : not json at all ][ ";
        assert!(parse_connectors(raw, "fixture").is_empty());
    }

    #[test]
    fn test_comma_inside_string_preserved() {
        let raw = r#"{ "mcpServers": { "a": { "command": "echo", "args": ["x, y"] } } }"#;
        let conns = parse_connectors(raw, "fixture");
        match &conns[0].transport {
            Transport::Stdio { args, .. } => assert_eq!(args, &vec!["x, y".to_string()]),
            other => panic!("expected stdio, got {other:?}"),
        }
    }

    #[test]
    fn test_does_not_expose_env_values() {
        let raw = r#"{ "mcpServers": {
            "db": { "command": "pg", "env": { "TOKEN": "super-secret" } }
        } }"#;
        let conns = parse_connectors(raw, "fixture");
        // Connector carries only the tier, never the secret string.
        let serialized = serde_json::to_string(&conns).unwrap();
        assert!(!serialized.contains("super-secret"));
        assert!(!serialized.contains("TOKEN"));
    }
}
