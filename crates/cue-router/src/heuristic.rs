//! Local heuristic classifier.
//!
//! Pattern-matches the prompt + attachments + session context to produce a
//! reasonable `TaskClassification` without any network round-trip. Fast,
//! deterministic, runs in-process. The single source of routing truth on
//! offline / privacy-only runs.
//!
//! The heuristic uses three signals:
//!
//! 1. Lexical task-type cues (keywords + code fences).
//! 2. Length + structure (one-liner vs multi-line vs has-code).
//! 3. Attachment + context surfaces.
//!
//! Confidence is computed from how unambiguous the cues are: a prompt that
//! matches exactly one task-type keyword set with a clear length signal scores
//! ~0.9; a vague short prompt scores ~0.5; a totally generic prompt with no
//! signals defaults to General/Medium/Balanced at ~0.4.

use async_trait::async_trait;

use crate::classifier::{ClassifierInput, TaskClassifier};
use crate::model::{ContextNeeds, Difficulty, LatencyLane, TaskClassification, TaskType};

/// Default keyword sets, exposed so callers / tests can extend or override.
const CODE_KEYWORDS: &[&str] = &[
    "function",
    "method",
    "class",
    "struct",
    "trait",
    "impl",
    "implement",
    "rewrite",
    "refactor",
    "debug",
    "stack trace",
    "compile",
    "compiler",
    "linter",
    "test",
    "unit test",
    "fix this",
    "panic",
    "error[",
    "lifetime",
    "borrow checker",
    "type error",
    "rust ",
    "python ",
    "typescript ",
    "javascript ",
    "swift ",
    "golang",
    " go ",
    " js ",
    " ts ",
    " sql ",
];

const DESIGN_KEYWORDS: &[&str] = &[
    "design",
    "architecture",
    "system design",
    "scalability",
    "tradeoff",
    "trade-off",
    "throughput",
    "latency",
    "consistency",
    "availability",
    "load balanc",
    "shard",
    "replica",
    "queue",
    "high-level",
    "diagram",
    "service-oriented",
    "microservice",
    "monolith",
    "event-driven",
    "design a ",
    "how would you build",
    "build a system",
];

const MEETING_KEYWORDS: &[&str] = &[
    "in the meeting",
    "what did",
    "they said",
    "decision",
    "action item",
    "transcript",
    "the call",
    "the discussion",
    "talked about",
    "mentioned",
    "agreed",
    "follow-up",
    "follow up",
    "what was decided",
];

const WRITING_KEYWORDS: &[&str] = &[
    "draft",
    "write an email",
    "summarize",
    "rewrite this",
    "polish",
    "tone",
    "rephrase",
    "make it shorter",
    "make it longer",
    "edit this",
    "blog post",
    "release note",
    "changelog",
    "documentation",
];

const VISION_KEYWORDS: &[&str] = &[
    // Only unambiguous vision markers. We deliberately do NOT include
    // "diagram" / "the chart" / "the graph" / "the figure" because those
    // routinely appear in text-only design discussions ("include a diagram of
    // the data flow") and would misroute a SystemDesign question to Vision.
    // Vision still wins by attachment (has_screenshot=true) and by these
    // phrases that only make sense when a real image is in scope.
    "screenshot",
    "this screen",
    "on the screen",
    "what is in the image",
    "what'\''s in the image",
    "in the picture",
    "this picture",
];

/// Heuristic-only classifier.
pub struct HeuristicClassifier;

impl HeuristicClassifier {
    /// Construct a default heuristic classifier. Stateless.
    pub fn new() -> Self {
        Self
    }
}

impl Default for HeuristicClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TaskClassifier for HeuristicClassifier {
    async fn classify(&self, input: &ClassifierInput<'_>) -> TaskClassification {
        let prompt = input.prompt;
        let prompt_lower = prompt.to_lowercase();
        let trimmed = prompt.trim();
        let length = trimmed.chars().count();
        let line_count = trimmed.lines().count();
        let has_code_fence = prompt.contains("```");

        // Vision wins if a screenshot is attached or the prompt explicitly
        // mentions vision. Vision overrides other classifications because the
        // routing target (vision-capable provider) is different.
        let vision_signal_count = VISION_KEYWORDS
            .iter()
            .filter(|k| prompt_lower.contains(*k))
            .count();
        let vision_by_attachment = input.has_screenshot;
        let is_vision = vision_by_attachment || vision_signal_count > 0;

        // Score each non-vision task type by keyword hits.
        let algorithmic_challenge = looks_like_algorithmic_challenge_prompt(&prompt_lower);
        let code_hits = count_hits(&prompt_lower, CODE_KEYWORDS)
            + if has_code_fence { 2 } else { 0 }
            + if algorithmic_challenge { 3 } else { 0 };
        let design_hits = count_hits(&prompt_lower, DESIGN_KEYWORDS);
        let meeting_hits =
            count_hits(&prompt_lower, MEETING_KEYWORDS) + if input.has_transcript { 1 } else { 0 };
        let writing_hits = count_hits(&prompt_lower, WRITING_KEYWORDS);

        let (task_type, top_hits, runner_up) = if is_vision {
            (
                TaskType::Vision,
                vision_signal_count + if vision_by_attachment { 2 } else { 0 },
                0,
            )
        } else {
            // Pick the highest-scoring non-vision bucket; default to General if all zero.
            let mut buckets: [(TaskType, usize); 4] = [
                (TaskType::Code, code_hits),
                (TaskType::SystemDesign, design_hits),
                (TaskType::Meeting, meeting_hits),
                (TaskType::Writing, writing_hits),
            ];
            buckets.sort_by_key(|x| std::cmp::Reverse(x.1));
            if buckets[0].1 == 0 {
                (TaskType::General, 0, 0)
            } else {
                (buckets[0].0, buckets[0].1, buckets[1].1)
            }
        };

        // Difficulty heuristic:
        //   - long prompt (>500 chars) OR design keyword present OR multiple
        //     code fences -> Hard
        //   - very short prompt (<60 chars) and no design/code cues -> Easy
        //   - otherwise Medium
        let difficulty = if length > 500
            || matches!(task_type, TaskType::SystemDesign)
            || (matches!(task_type, TaskType::Code) && (has_code_fence || algorithmic_challenge))
        {
            Difficulty::Hard
        } else if length < 60 && code_hits == 0 && design_hits == 0 {
            Difficulty::Easy
        } else {
            Difficulty::Medium
        };

        // Latency lane.
        //   - Easy -> Instant
        //   - Hard -> Deep
        //   - Medium -> Balanced
        // Vision always uses Balanced unless the prompt is short (Instant).
        let latency_lane = match (task_type, difficulty) {
            (TaskType::Vision, _) if length < 80 => LatencyLane::Instant,
            (TaskType::Vision, _) => LatencyLane::Balanced,
            (_, Difficulty::Easy) => LatencyLane::Instant,
            (_, Difficulty::Hard) => LatencyLane::Deep,
            _ => LatencyLane::Balanced,
        };

        // Context needs.
        let needed_context = ContextNeeds {
            transcript: input.has_transcript || meeting_hits > 0,
            page: input.has_page,
            files: input.file_attachment_count > 0,
            screenshot: input.has_screenshot,
            memory: matches!(task_type, TaskType::Meeting | TaskType::SystemDesign),
        };

        // Confidence:
        //   - 0.9 when top bucket is well above runner-up
        //   - 0.75 when top bucket is at least 2 hits and not tied
        //   - 0.55 when low signal but not zero
        //   - 0.4 when no cues at all
        // Vision-by-attachment is high confidence regardless of word count.
        let confidence = if vision_by_attachment {
            0.95
        } else if matches!(task_type, TaskType::General) {
            // No keyword hit. Maybe still some signal from line count / length.
            if line_count > 3 || length > 200 {
                0.55
            } else {
                0.4
            }
        } else if top_hits >= 3 && top_hits.saturating_sub(runner_up) >= 2 {
            0.9
        } else if top_hits >= 2 {
            0.75
        } else {
            0.55
        };

        TaskClassification {
            task_type,
            difficulty,
            needed_context,
            latency_lane,
            confidence,
        }
    }
}

fn count_hits(prompt_lower: &str, keywords: &[&str]) -> usize {
    keywords
        .iter()
        .filter(|k| prompt_lower.contains(*k))
        .count()
}

fn looks_like_algorithmic_challenge_prompt(prompt_lower: &str) -> bool {
    let has_problem_intro = [
        "you are given",
        "given an array",
        "given a string",
        "given a list",
        "given a matrix",
        "given two",
        "given n",
        "given the root",
    ]
    .iter()
    .any(|signal| prompt_lower.contains(signal));
    let has_return_or_output = [
        "return true",
        "return false",
        "return the",
        "return a",
        "return an",
        "output",
        "find the",
        "determine if",
        "calculate the",
    ]
    .iter()
    .any(|signal| prompt_lower.contains(signal));
    let has_data_signal = [
        "array",
        "integer",
        "integers",
        "nums",
        "string",
        "matrix",
        "list",
        "linked list",
        "tree",
        "graph",
        "positive integers",
    ]
    .iter()
    .any(|signal| prompt_lower.contains(signal));

    (has_problem_intro && has_return_or_output && has_data_signal)
        || (prompt_lower.contains("return true if")
            && prompt_lower.contains("otherwise return false"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(prompt: &str) -> ClassifierInput<'_> {
        ClassifierInput {
            prompt,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn easy_one_liner_general() {
        let c = HeuristicClassifier::new();
        let r = c.classify(&make_input("what year is it")).await;
        assert_eq!(r.task_type, TaskType::General);
        assert_eq!(r.difficulty, Difficulty::Easy);
        assert_eq!(r.latency_lane, LatencyLane::Instant);
    }

    #[tokio::test]
    async fn hard_design_question() {
        let c = HeuristicClassifier::new();
        let r = c
            .classify(&make_input(
                "Design a system to handle 10k qps writes with global consistency and high availability. Discuss tradeoffs between sharding strategies.",
            ))
            .await;
        assert_eq!(r.task_type, TaskType::SystemDesign);
        assert_eq!(r.difficulty, Difficulty::Hard);
        assert_eq!(r.latency_lane, LatencyLane::Deep);
        assert!(r.confidence >= 0.75);
    }

    #[tokio::test]
    async fn code_with_fence_is_hard() {
        let c = HeuristicClassifier::new();
        let prompt = "Why does this Rust function panic?\n```rust\nfn foo() { let x: u8 = 300; }\n```\nstack trace shows overflow.";
        let r = c.classify(&make_input(prompt)).await;
        assert_eq!(r.task_type, TaskType::Code);
        assert_eq!(r.difficulty, Difficulty::Hard);
        assert_eq!(r.latency_lane, LatencyLane::Deep);
    }

    #[tokio::test]
    async fn leetcode_statement_is_code_and_deep() {
        let c = HeuristicClassifier::new();
        let prompt = "You are given an array of positive integers nums. Alice and Bob are playing a game. Return true if Alice can win this game, otherwise return false.";
        let r = c.classify(&make_input(prompt)).await;
        assert_eq!(r.task_type, TaskType::Code);
        assert_eq!(r.difficulty, Difficulty::Hard);
        assert_eq!(r.latency_lane, LatencyLane::Deep);
    }

    #[tokio::test]
    async fn vision_by_attachment_overrides_keywords() {
        let c = HeuristicClassifier::new();
        let mut input = make_input("write me a haiku");
        input.has_screenshot = true;
        let r = c.classify(&input).await;
        assert_eq!(r.task_type, TaskType::Vision);
        assert!(r.confidence >= 0.9);
        assert!(r.needed_context.screenshot);
    }

    #[tokio::test]
    async fn meeting_followup_with_transcript() {
        let c = HeuristicClassifier::new();
        let mut input = make_input("what did they decide about the rollout date?");
        input.has_transcript = true;
        let r = c.classify(&input).await;
        assert_eq!(r.task_type, TaskType::Meeting);
        assert!(r.needed_context.transcript);
        assert!(r.needed_context.memory);
    }

    #[tokio::test]
    async fn writing_polish_request() {
        let c = HeuristicClassifier::new();
        let r = c
            .classify(&make_input(
                "polish this email to make it shorter and more direct",
            ))
            .await;
        assert_eq!(r.task_type, TaskType::Writing);
    }

    #[tokio::test]
    async fn empty_prompt_general_low_confidence() {
        let c = HeuristicClassifier::new();
        let r = c.classify(&make_input("")).await;
        assert_eq!(r.task_type, TaskType::General);
        assert!(r.confidence <= 0.5);
    }

    #[tokio::test]
    async fn ambiguous_short_prompt_falls_back() {
        let c = HeuristicClassifier::new();
        let r = c.classify(&make_input("hello")).await;
        assert_eq!(r.task_type, TaskType::General);
        assert_eq!(r.difficulty, Difficulty::Easy);
        assert!(r.confidence <= 0.5);
    }

    #[tokio::test]
    async fn followup_routing_brings_in_transcript_context() {
        // A two-word follow-up during an active meeting session should still
        // pick up the transcript context surface.
        let c = HeuristicClassifier::new();
        let mut input = make_input("more details?");
        input.has_transcript = true;
        let r = c.classify(&input).await;
        assert!(
            r.needed_context.transcript,
            "transcript context should be needed"
        );
        // Confidence is allowed to be moderate — the prompt itself is vague.
        assert!(r.confidence < 0.9);
    }

    // local_only is now a routing-policy concern (it does not appear on
    // ClassifierInput). The end-to-end test that AutoRouter honors local_only
    // lives in `auto.rs::tests::auto_router_local_only_forces_local_lane`.

    #[tokio::test]
    async fn text_only_diagram_in_design_question_is_not_vision() {
        // Regression for the codex-flagged bug: VISION_KEYWORDS used to include
        // "diagram", "the chart", etc, so a normal text-only design question
        // mentioning a diagram of the data flow was being misrouted to Vision.
        // Vision should only fire on actual visual attachments or unambiguous
        // wording like "the screenshot" / "in the image".
        let c = HeuristicClassifier::new();
        let r = c
            .classify(&make_input(
                "design a system for 10k qps writes; include a diagram of the data flow",
            ))
            .await;
        assert_eq!(
            r.task_type,
            TaskType::SystemDesign,
            "diagram should not flip to Vision"
        );
    }

    #[tokio::test]
    async fn text_only_chart_mention_is_not_vision() {
        let c = HeuristicClassifier::new();
        let r = c
            .classify(&make_input(
                "write a blog post about the unit-economics chart",
            ))
            .await;
        assert_ne!(r.task_type, TaskType::Vision);
    }

    #[tokio::test]
    async fn vision_still_wins_on_explicit_screen_wording() {
        // The narrowed keyword set still catches unambiguous phrases.
        let c = HeuristicClassifier::new();
        let r = c
            .classify(&make_input("what is in the image i just sent"))
            .await;
        assert_eq!(r.task_type, TaskType::Vision);
    }
}
