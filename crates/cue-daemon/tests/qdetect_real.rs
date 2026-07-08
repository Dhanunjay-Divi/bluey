//! REAL end-to-end test of the stage-2 question classifier — the actual
//! exported int8 ONNX model through the actual Rust inference path (no mocks).
//!
//! Needs the exported model on disk (`scripts/export-qdetect-onnx.sh` →
//! `dist/models/qdetect-en/`), so it is `#[ignore]`d in the default suite:
//!
//! ```sh
//! cargo test -p cue-daemon --features local-memory --target aarch64-apple-darwin \
//!   --test qdetect_real -- --ignored --nocapture
//! ```
#![cfg(feature = "local-memory")]

use cue_daemon::qdetect::QuestionClassifier;

fn model_dir() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR = crates/cue-daemon → repo root is two up.
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dist/models/qdetect-en")
        .canonicalize()
        .expect("dist/models/qdetect-en missing — run scripts/export-qdetect-onnx.sh")
}

#[tokio::test]
#[ignore = "needs the exported ONNX model (scripts/export-qdetect-onnx.sh)"]
async fn classifier_catches_regex_rejects_on_real_meeting_lines() {
    let dir = model_dir();
    let classifier =
        QuestionClassifier::load(&dir.join("model_int8.onnx"), &dir.join("tokenizer.json"))
            .expect("load exported classifier");

    // The two-stage contract: these are all lines the LEXICAL check REJECTS
    // (no '?', no leading wh/aux word) — stage 2 exists precisely for them.
    // Measured (Appendix D): direct disfluent questions land in the strong
    // band; INDIRECT/declarative forms ("i'm wondering if...", "you think...")
    // are the model's known ~51% weak band — verified live 2026-07: both
    // indirect lines below classify false. The assertion therefore requires
    // only >=half caught; the hard gate is ZERO false positives (a false fire
    // auto-drives the agent — precision over recall).
    let disfluent_questions = [
        "so um do we need the feature flag for this or not",
        "wait is this thread safe",
        "sarah i'm wondering if the migration plan covers the replica",
        "and you think the cache will hold up under that load",
    ];
    // Plain statements stage 2 must NOT flip to questions (false positives
    // auto-drive the agent — precision matters more than recall here).
    let statements = [
        "i pushed the fix it's in review now",
        "nothing blocking on my end",
        "the build is green we're good to merge",
        "let's move on to the next item",
    ];

    let mut caught = 0;
    for line in disfluent_questions {
        assert!(
            !cue_core::is_question_shaped(line),
            "test premise broken — regex already accepts: {line}"
        );
        let verdict = classifier.classify(line).await.expect("classify");
        println!("question line -> {verdict}: {line}");
        if verdict {
            caught += 1;
        }
    }
    // A broken export (wrong labels, dead graph) catches ZERO of these; a
    // healthy one catches the direct forms. >=half separates those cleanly
    // without being flaky about the measured-weak indirect band.
    assert!(
        caught >= disfluent_questions.len() / 2,
        "classifier caught only {caught}/{} regex-rejected questions",
        disfluent_questions.len()
    );

    for line in statements {
        let verdict = classifier.classify(line).await.expect("classify");
        println!("statement line -> {verdict}: {line}");
        assert!(!verdict, "false positive on statement: {line}");
    }
}
