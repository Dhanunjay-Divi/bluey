//! Dev fixture: serve the MCP memory server with canned meeting data so a
//! REAL agent CLI can be pointed at it (the Batch-1 acceptance run).
//!
//! Usage: cargo run -p cue-mcp --example serve_fixture -- <port> <token>

use std::sync::Arc;

use cue_mcp::{
    AgentHistoryHitOut, FactHitOut, MeetingMemorySource, MeetingSummaryOut, TranscriptSliceOut,
};

struct Fixture;

#[async_trait::async_trait]
impl MeetingMemorySource for Fixture {
    async fn recent_transcript(
        &self,
        _max_turns: usize,
        _max_chars: usize,
    ) -> Option<TranscriptSliceOut> {
        Some(TranscriptSliceOut {
            transcript: "Alex: after the load test we moved the checkout SLA to 300ms\n\
                         Sam: noted, and Priya owns the payments migration now"
                .to_string(),
            turn_count: 2,
            truncated: false,
        })
    }
    async fn meeting_summary(&self) -> Option<MeetingSummaryOut> {
        Some(MeetingSummaryOut {
            title: "Platform sync (fixture)".to_string(),
            rolling_summary: Some("- SLA moved to 300ms\n- Priya owns payments".to_string()),
            decisions: vec![
                "[Constraint] Checkout SLA is 300ms (was 200ms)".to_string(),
                "[Owner] Priya — payments migration".to_string(),
            ],
        })
    }
    async fn search_decisions(&self, _query: &str, _limit: usize) -> Vec<FactHitOut> {
        vec![FactHitOut {
            text: "[Constraint] Checkout SLA is 300ms (was 200ms)".to_string(),
            meeting_id: "fixture-meeting".to_string(),
            relevance: 0.92,
        }]
    }
    async fn search_past_meetings(&self, _query: &str, _limit: usize) -> Vec<FactHitOut> {
        vec![FactHitOut {
            text: "[Decision] Shard the database by customer region".to_string(),
            meeting_id: "past-meeting".to_string(),
            relevance: 0.88,
        }]
    }
    async fn search_agent_history(&self, query: &str, _limit: usize) -> Vec<AgentHistoryHitOut> {
        vec![AgentHistoryHitOut {
            text: format!("prior reasoning about {query}: we chose advisory locks over table locks"),
            agent: "Claude Code".to_string(),
            session_id: "fixture-session".to_string(),
            when: "1750000000".to_string(),
            score: 0.81,
        }]
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let mut args = std::env::args().skip(1);
    let port: u16 = args.next().and_then(|p| p.parse().ok()).unwrap_or(47632);
    let token = args.next().unwrap_or_else(|| "fixture-token".to_string());
    let handle = cue_mcp::serve(Arc::new(Fixture), token, Some(port)).await?;
    println!("serving {} (ctrl-c to stop)", handle.url());
    tokio::signal::ctrl_c().await?;
    Ok(())
}
