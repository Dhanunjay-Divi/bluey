//! REAL ACP stream-shape probe: drive the actual claude CLI over ACP and
//! print every chunk, fresh and resumed — the evidence base for the
//! replay-suppression fix. `#[ignore]`: needs the claude CLI + network.
//!
//! `cargo test -p cue-agent-bridge --test acp_stream_real -- --ignored --nocapture`

use cue_agent_bridge::{AgentKind, AnswerChunk, Question, Role, Transcript, Turn};
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
        images: Vec::new(),
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
        images: Vec::new(),
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

/// REAL end-to-end proof that an attached IMAGE is transported to the agent and
/// read as pixels. We render a unique token into a PNG, attach it via
/// `Question.images`, drive the real claude agent over ACP, and assert the
/// answer contains the token — which the agent could ONLY have obtained by
/// decoding the image bytes we sent (it is nowhere in the prompt text).
///
/// This is the definitive check that the "+"-menu image path is not a no-op:
/// `image_block_from_path` → `ContentBlock::Image` → the multi-block
/// `send_request_to` path → the agent's own vision. No separate vision provider.
///
/// `cargo test -p cue-agent-bridge --test acp_stream_real \
///   real_attached_image_reaches_agent_vision -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "drives the real claude CLI over ACP + needs the Python PIL probe image"]
async fn real_attached_image_reaches_agent_vision() {
    // The token is rendered into the image ONLY — never written into the prompt.
    let token = "BLUEY-VISION-PROBE-7788";
    let img = render_probe_png(token);
    assert!(img.exists(), "probe image was not generated at {img:?}");

    let (_, chunks) = collect(Question {
        // The prompt must NOT contain the token — the agent has to read it from
        // the attached image. We ask it to transcribe exactly what it sees.
        prompt: "An image is attached. Read the exact text shown in it and reply \
                 with ONLY that text, character for character, nothing else."
            .to_string(),
        context: None,
        resume: None,
        cwd: None,
        images: vec![img.clone()],
    })
    .await;

    let answer: String = chunks
        .iter()
        .filter_map(|c| c.strip_prefix("DELTA["))
        .map(|c| c.trim_end_matches(']'))
        .collect::<Vec<_>>()
        .join("");
    println!("== IMAGE-ATTACH ANSWER ==\n{answer}");
    for c in &chunks {
        println!("  {c}");
    }
    let _ = std::fs::remove_file(&img);

    // The token can ONLY appear if the image bytes reached the agent's vision.
    assert!(
        answer.contains(token),
        "agent did not read the token from the attached image — image attach is a \
         no-op. Answer was: {answer}"
    );
}

/// REAL end-to-end proof that an attached TEXT/CODE file's CONTENT reaches the
/// agent. Non-image files are not sent as pixels — the daemon reads their text
/// and embeds it as a context turn (`Question.context`), which the ACP client
/// prepends to the prompt. We put a unique token in that context turn (as a
/// file's content would arrive) and assert the agent reads it back. The token is
/// NOT in the prompt itself — only in the attached file content.
///
/// `cargo test -p cue-agent-bridge --test acp_stream_real \
///   real_attached_text_file_content_reaches_agent -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "drives the real claude CLI over ACP"]
async fn real_attached_text_file_content_reaches_agent() {
    let token = "BLUEY-DOC-PROBE-4457";
    // This is exactly what the daemon builds for an attached .txt/.md/code file:
    // an `Other`-role context turn carrying the file's extracted text. Neutral,
    // non-confidential content so the agent just transcribes the token back.
    let file_content = format!(
        "Attached file (build-id.txt):\n\
         The build identifier for this release is {token}."
    );

    let (_, chunks) = collect(Question {
        // The prompt does NOT contain the token — the agent must read it from the
        // attached file content in the context.
        prompt: "What is the build identifier stated in the attached file? \
                 Reply with ONLY that identifier."
            .to_string(),
        context: Some(Transcript {
            turns: vec![Turn {
                role: Role::Other,
                text: file_content,
            }],
        }),
        resume: None,
        cwd: None,
        images: Vec::new(),
    })
    .await;

    let answer: String = chunks
        .iter()
        .filter_map(|c| c.strip_prefix("DELTA["))
        .map(|c| c.trim_end_matches(']'))
        .collect::<Vec<_>>()
        .join("");
    println!("== TEXT-FILE ANSWER ==\n{answer}");
    for c in &chunks {
        println!("  {c}");
    }

    assert!(
        answer.contains(token),
        "agent did not read the token from the attached file content — text-file \
         attach is a no-op. Answer was: {answer}"
    );
}

/// Render `token` into a high-contrast PNG under the test temp dir using Python
/// PIL (present on the dev machine) and return its path. Panics if generation
/// fails — this test's whole premise is a readable attached image.
fn render_probe_png(token: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("bluey-acp-vision-probe");
    std::fs::create_dir_all(&dir).expect("create probe dir");
    let path = dir.join(format!("probe-{token}.png"));
    let script = format!(
        r#"
from PIL import Image, ImageDraw, ImageFont
img = Image.new("RGB", (900, 300), "white")
d = ImageDraw.Draw(img)
try:
    font = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial Bold.ttf", 64)
except Exception:
    font = ImageFont.load_default()
d.text((40, 110), "{token}", fill="black", font=font)
img.save(r"{}")
"#,
        path.display()
    );
    let status = std::process::Command::new("python3")
        .arg("-c")
        .arg(script)
        .status()
        .expect("run python3 to render probe image");
    assert!(status.success(), "python3 failed to render the probe image");
    path
}
