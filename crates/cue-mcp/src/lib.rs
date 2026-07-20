//! Bluey's MCP server — the meeting memory the attached agent PULLS from.
//!
//! The pivot (PLAN: MCP-backend, 2026-07): Bluey stops pushing context blobs
//! into agent prompts. Instead the daemon serves four READ-ONLY tools over
//! loopback Streamable HTTP, and the agent retrieves exactly what its own
//! reasoning needs:
//!
//! - `get_recent_transcript`   — the newest live-transcript slice
//! - `get_meeting_summary`     — title + rolling summary + verified decisions
//! - `search_meeting_decisions`— hybrid search over decision-type facts
//! - `search_past_meetings`    — cross-meeting facts recall (hybrid)
//!
//! Design rules (live-verified in the Batch-0 spike against claude, gemini,
//! copilot, cursor and codex):
//! - **Loopback Streamable HTTP**, not stdio: stdio is process-per-connection
//!   (the agent would spawn a fresh child that owns no live meeting state).
//!   One long-lived daemon serves every agent session.
//! - **Bearer token per meeting** + Host allow-list: private meeting data
//!   never listens beyond 127.0.0.1, and a leaked token rotates out at the
//!   next meeting.
//! - **Read-only tool surface**: no write/exec/network tool exists here, so a
//!   prompt injection spoken INTO a meeting has no exfiltration vector.
//! - The daemon implements [`MeetingMemorySource`] with SHORT lock holds
//!   (clone-then-release, never `.await` under the meeting lock) — an MCP
//!   tool call must never stall the live STT sink.

mod protocol;
mod tools;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{info, warn};

pub use tools::{AgentHistoryHitOut, FactHitOut, MeetingSummaryOut, TranscriptSliceOut};

/// What the daemon exposes to the MCP layer. Tool-shaped on purpose: cue-mcp
/// holds no meeting-model logic and no cue-core dependency — the daemon does
/// the store access (under its own locking discipline) and returns plain data.
#[async_trait::async_trait]
pub trait MeetingMemorySource: Send + Sync + 'static {
    /// Newest transcript slice of the ACTIVE meeting (`None` = no meeting).
    async fn recent_transcript(
        &self,
        max_turns: usize,
        max_chars: usize,
    ) -> Option<TranscriptSliceOut>;
    /// Rolling summary + verified decisions of the ACTIVE meeting.
    async fn meeting_summary(&self) -> Option<MeetingSummaryOut>;
    /// Hybrid search over decision-type facts (current + past meetings).
    async fn search_decisions(&self, query: &str, limit: usize) -> Vec<FactHitOut>;
    /// Cross-meeting facts recall, excluding the active meeting.
    async fn search_past_meetings(&self, query: &str, limit: usize) -> Vec<FactHitOut>;
    /// Cross-AGENT session-history recall: the driven agent's OTHER coding-agent
    /// sessions' past prose reasoning, relevant to this meeting question. Read-
    /// only. Returns `[]` when the feature/consent is off or the index is empty.
    async fn search_agent_history(&self, query: &str, limit: usize) -> Vec<AgentHistoryHitOut>;
}

/// A running MCP server: bound address + rotating bearer token + shutdown.
pub struct McpServerHandle {
    addr: SocketAddr,
    token: Arc<RwLock<String>>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl McpServerHandle {
    /// The URL agents connect to (the value written into their MCP config).
    pub fn url(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }

    /// The current bearer token (written into the agent's config/header).
    pub async fn token(&self) -> String {
        self.token.read().await.clone()
    }

    /// Rotate the bearer token (called per meeting so a stale registration
    /// cannot read a later meeting's memory).
    pub async fn rotate_token(&self, new_token: String) {
        *self.token.write().await = new_token;
    }

    /// Stop accepting connections. Idempotent.
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for McpServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Bind the MCP server on `127.0.0.1` (ephemeral port unless `port` is given)
/// and serve until the handle shuts down. Fail-soft is the CALLER's choice —
/// this returns an error rather than panicking.
pub async fn serve(
    source: Arc<dyn MeetingMemorySource>,
    initial_token: String,
    port: Option<u16>,
) -> Result<McpServerHandle> {
    let addr: SocketAddr = ([127, 0, 0, 1], port.unwrap_or(0)).into();
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind MCP server on {addr}"))?;
    let addr = listener.local_addr().context("read bound MCP addr")?;
    let token = Arc::new(RwLock::new(initial_token));
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let accept_token = Arc::clone(&token);
    tokio::spawn(async move {
        info!("bluey MCP memory server on http://{addr}/mcp (loopback only)");
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    info!("bluey MCP server shutting down");
                    break;
                }
                accepted = listener.accept() => {
                    let (stream, _peer) = match accepted {
                        Ok(pair) => pair,
                        Err(e) => {
                            warn!("MCP accept error: {e}");
                            continue;
                        }
                    };
                    let source = Arc::clone(&source);
                    let token = Arc::clone(&accept_token);
                    tokio::spawn(async move {
                        if let Err(e) = protocol::serve_connection(stream, source, token).await {
                            tracing::debug!("MCP connection ended: {e:#}");
                        }
                    });
                }
            }
        }
    });

    Ok(McpServerHandle {
        addr,
        token,
        shutdown: Some(shutdown_tx),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory fake source for protocol-level tests.
    pub(crate) struct FakeSource;

    #[async_trait::async_trait]
    impl MeetingMemorySource for FakeSource {
        async fn recent_transcript(
            &self,
            max_turns: usize,
            _max_chars: usize,
        ) -> Option<TranscriptSliceOut> {
            Some(TranscriptSliceOut {
                transcript: "Alex: we ship friday\nSam: agreed".to_string(),
                turn_count: max_turns.min(2) as u32,
                truncated: false,
            })
        }
        async fn meeting_summary(&self) -> Option<MeetingSummaryOut> {
            Some(MeetingSummaryOut {
                title: "Platform sync".to_string(),
                rolling_summary: Some("- shipping friday".to_string()),
                decisions: vec!["[Decision] Ship on Friday".to_string()],
            })
        }
        async fn search_decisions(&self, query: &str, _limit: usize) -> Vec<FactHitOut> {
            vec![FactHitOut {
                text: format!("[Decision] about {query}"),
                meeting_id: "m1".to_string(),
                relevance: 0.9,
            }]
        }
        async fn search_past_meetings(&self, _query: &str, _limit: usize) -> Vec<FactHitOut> {
            vec![FactHitOut {
                text: "[Owner] Priya — payments migration".to_string(),
                meeting_id: "m0".to_string(),
                relevance: 0.8,
            }]
        }
        async fn search_agent_history(
            &self,
            query: &str,
            _limit: usize,
        ) -> Vec<AgentHistoryHitOut> {
            vec![
                AgentHistoryHitOut {
                    text: format!("prior reasoning about {query}: we chose advisory locks"),
                    agent: "Claude Code".to_string(),
                    session_id: "sess-abc".to_string(),
                    when: "1700000000".to_string(),
                    score: 0.91,
                },
                AgentHistoryHitOut {
                    text: "the retry budget for the webhook is three attempts".to_string(),
                    agent: "Codex".to_string(),
                    session_id: "sess-def".to_string(),
                    when: "1699990000".to_string(),
                    score: 0.72,
                },
            ]
        }
    }

    async fn start() -> McpServerHandle {
        serve(Arc::new(FakeSource), "test-token".to_string(), None)
            .await
            .expect("serve")
    }

    async fn rpc(
        handle: &McpServerHandle,
        token: &str,
        body: serde_json::Value,
    ) -> (u16, serde_json::Value) {
        let client = reqwest::Client::new();
        let resp = client
            .post(handle.url())
            .header("Authorization", format!("Bearer {token}"))
            .json(&body)
            .send()
            .await
            .expect("send");
        let status = resp.status().as_u16();
        let value = resp.json().await.unwrap_or(serde_json::json!({}));
        (status, value)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn initialize_lists_and_calls_tools() {
        let handle = start().await;

        let (status, init) = rpc(
            &handle,
            "test-token",
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize",
                "params":{"protocolVersion":"2025-03-26","capabilities":{},
                          "clientInfo":{"name":"test","version":"0"}}}),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(init["result"]["serverInfo"]["name"], "bluey-memory");

        let (_, tools) = rpc(
            &handle,
            "test-token",
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        )
        .await;
        let names: Vec<&str> = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec![
                "get_recent_transcript",
                "get_meeting_summary",
                "search_meeting_decisions",
                "search_past_meetings",
                "search_agent_history"
            ]
        );

        let (_, called) = rpc(
            &handle,
            "test-token",
            serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
                "params":{"name":"get_meeting_summary","arguments":{}}}),
        )
        .await;
        let text = called["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Platform sync"), "{text}");
        assert_eq!(called["result"]["isError"], false);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn auth_is_enforced_and_token_rotates() {
        let handle = start().await;
        let ping = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"ping"});

        let (status, _) = rpc(&handle, "wrong-token", ping.clone()).await;
        assert_eq!(status, 401);

        let (status, _) = rpc(&handle, "test-token", ping.clone()).await;
        assert_eq!(status, 200);

        handle.rotate_token("next-meeting-token".to_string()).await;
        let (status, _) = rpc(&handle, "test-token", ping.clone()).await;
        assert_eq!(status, 401, "old token must die on rotation");
        let (status, _) = rpc(&handle, "next-meeting-token", ping).await;
        assert_eq!(status, 200);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unknown_tool_and_bad_args_are_clean_errors() {
        let handle = start().await;
        let (_, resp) = rpc(
            &handle,
            "test-token",
            serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call",
                "params":{"name":"drop_all_tables","arguments":{}}}),
        )
        .await;
        assert_eq!(resp["result"]["isError"], true);

        // Unknown argument on a strict schema → tool-level error, not a panic.
        let (_, resp) = rpc(
            &handle,
            "test-token",
            serde_json::json!({"jsonrpc":"2.0","id":5,"method":"tools/call",
                "params":{"name":"search_past_meetings",
                          "arguments":{"query":"x","bogus_field":1}}}),
        )
        .await;
        assert_eq!(resp["result"]["isError"], true);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shutdown_stops_accepting() {
        let mut handle = start().await;
        let url = handle.url();
        handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let err = reqwest::Client::new()
            .post(url)
            .header("Authorization", "Bearer test-token")
            .json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"ping"}))
            .send()
            .await;
        assert!(err.is_err(), "server must stop accepting after shutdown");
    }
}
