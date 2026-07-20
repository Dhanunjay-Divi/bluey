//! REAL ACP stream-shape probe: drive the actual claude CLI over ACP and
//! print every chunk, fresh and resumed — the evidence base for the
//! replay-suppression fix. `#[ignore]`: needs the claude CLI + network.
//!
//! `cargo test -p cue-agent-bridge --test acp_stream_real -- --ignored --nocapture`

use cue_agent_bridge::{AgentKind, AnswerChunk, Question};
use futures_util::StreamExt;

async fn collect(question: Question) -> (Option<String>, Vec<String>) {
    let mut stream = cue_agent_bridge::acp::drive_acp(AgentKind::ClaudeCode, question, false)
        .await
        .expect("acp drive");
    let mut session = None;
    let mut chunks = Vec::new();
    while let Some(chunk) = stream.next().await {
        match chunk {
            AnswerChunk::Started { session_id } => {
                session = session_id;
                chunks.push("STARTED".to_string());
            }
            AnswerChunk::Delta(text) => {
                chunks.push(format!("DELTA[{}]", text.replace('\n', "\\n")))
            }
            AnswerChunk::ToolCall { title, .. } => chunks.push(format!("TOOL[{title}]")),
            AnswerChunk::Done { .. } => chunks.push("DONE".to_string()),
            AnswerChunk::Error(e) => chunks.push(format!("ERROR[{e}]")),
            other => chunks.push(format!("{other:?}")),
        }
    }
    (session, chunks)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "drives the real claude CLI over ACP"]
async fn acp_fresh_and_resumed_stream_shapes() {
    // Fresh turn.
    let (session, fresh) = collect(Question {
        prompt: "Reply with exactly: ACP-ALPHA".to_string(),
        context: None,
        resume: None,
        cwd: None,
    })
    .await;
    println!("== FRESH ==");
    for c in &fresh {
        println!("  {c}");
    }
    let session = session.expect("fresh session id");

    // Resumed turn — the replay-contamination scenario.
    let (_, resumed) = collect(Question {
        prompt: "Reply with exactly: ACP-BRAVO".to_string(),
        context: None,
        resume: Some(session),
        cwd: None,
    })
    .await;
    println!("== RESUMED ==");
    for c in &resumed {
        println!("  {c}");
    }

    // The assertion of the BUG (to be inverted by the fix): resumed deltas
    // must NOT contain the prior turn's answer text.
    let resumed_text: String = resumed
        .iter()
        .filter_map(|c| c.strip_prefix("DELTA["))
        .collect();
    println!("resumed concatenated deltas: {resumed_text}");
    assert!(
        !resumed_text.contains("ACP-ALPHA"),
        "REPLAY CONTAMINATION: resumed stream re-emitted the prior turn"
    );
    assert!(resumed_text.contains("ACP-BRAVO"), "new answer must stream");
}
