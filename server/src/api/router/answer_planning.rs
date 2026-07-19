
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = text.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnswerIntent {
    Quick,
    Coding,
    CodingFollowUp,
    Behavioral,
    SystemDesign,
    Screen,
    Research,
    FollowUp,
    MissingContext,
    Writing,
    Meeting,
    General,
}

impl AnswerIntent {
    fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Coding => "coding",
            Self::CodingFollowUp => "coding_followup",
            Self::Behavioral => "behavioral",
            Self::SystemDesign => "system_design",
            Self::Screen => "screen",
            Self::Research => "research",
            Self::FollowUp => "follow_up",
            Self::MissingContext => "missing_context",
            Self::Writing => "writing",
            Self::Meeting => "meeting",
            Self::General => "general",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnswerOutput {
    Compact,
    CodeArtifact,
    SourceAnswer,
    CanvasDetail,
    InterviewAnswer,
}

impl AnswerOutput {
    fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::CodeArtifact => "code_artifact",
            Self::SourceAnswer => "source_answer",
            Self::CanvasDetail => "canvas_detail",
            Self::InterviewAnswer => "interview_answer",
        }
    }
}

#[derive(Debug, Clone)]
struct AnswerPlan {
    intent: AnswerIntent,
    output: AnswerOutput,
    recommended_lane: &'static str,
    confidence: f32,
    interview_context: bool,
    needs_screen: bool,
    needs_docs: bool,
    needs_transcript: bool,
    needs_memory: bool,
    needs_web_search: bool,
}

impl AnswerPlan {
    fn evidence_labels(&self) -> Vec<&'static str> {
        let mut labels = Vec::new();
        if self.needs_screen {
            labels.push("current screen");
        }
        if self.needs_docs {
            labels.push("attached documents");
        }
        if self.needs_transcript {
            labels.push("live transcript");
        }
        if self.needs_memory {
            labels.push("prior conversation context");
        }
        if self.needs_web_search {
            labels.push("managed web search");
        }
        if labels.is_empty() {
            labels.push("user question");
        }
        labels
    }
}

fn managed_vision_text_fallback_eligible(
    req: &CompleteRequest,
    effective_lane: &str,
    provider: &str,
    error: &anyhow::Error,
) -> bool {
    if effective_lane != "vision"
        || req.image_data_urls.is_empty()
        || internal_disclosure_error(req).is_some()
    {
        return false;
    }

    error
        .downcast_ref::<routing::dispatcher::UpstreamMediaRejectionError>()
        .is_some_and(|upstream| {
            upstream.provider == provider && matches!(upstream.status, 400 | 415 | 422)
        })
}

fn managed_vision_text_fallback_ready(
    vision_routes_exhausted: bool,
    explicit_media_rejection_seen: bool,
    fallback_routes_available: bool,
) -> bool {
    vision_routes_exhausted && explicit_media_rejection_seen && fallback_routes_available
}

fn managed_vision_text_fallback_lane(answer_plan: &AnswerPlan) -> &'static str {
    match answer_plan.recommended_lane {
        "instant" => "instant",
        "deep" => "deep",
        "balanced" => "balanced",
        _ if matches!(answer_plan.output, AnswerOutput::CodeArtifact) => "deep",
        _ => "balanced",
    }
}

fn managed_vision_text_fallback_prompt(system: &str, user: &str) -> (String, String) {
    (
        format!("{system}\n\n{MANAGED_VISION_TEXT_FALLBACK_INSTRUCTION}"),
        user.to_string(),
    )
}

#[derive(Debug, Clone)]
struct AnswerRequestDiagnostics {
    user_chars: usize,
    question_chars: usize,
    question_hash: String,
    context_chars: usize,
    context_hash: String,
    context_coding_signal: bool,
    transcript_chars: usize,
    transcript_hash: String,
    transcript_source_labels: usize,
    generic_live_transcript_prompt: bool,
}

fn answer_request_diagnostics(req: &CompleteRequest) -> AnswerRequestDiagnostics {
    let question = extract_search_question(&req.user);
    let normalized = normalize_guardrail_text(&question);
    let context = extract_planning_context(&req.user);
    let normalized_context = normalize_guardrail_text(&context);
    let transcript = transcript_diagnostic_text(&question);
    AnswerRequestDiagnostics {
        user_chars: req.user.chars().count(),
        question_chars: question.chars().count(),
        question_hash: stable_text_hash_prefix(&question),
        context_chars: context.chars().count(),
        context_hash: stable_text_hash_prefix(&context),
        context_coding_signal: looks_like_coding_question(&normalized_context)
            || has_code_shape(&normalized_context),
        transcript_chars: transcript.chars().count(),
        transcript_hash: stable_text_hash_prefix(&transcript),
        transcript_source_labels: transcript_source_label_count(&question),
        generic_live_transcript_prompt: is_generic_live_transcript_prompt(&normalized),
    }
}

fn stable_text_hash_prefix(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "none".to_string();
    }
    let digest = Sha256::digest(trimmed.as_bytes());
    hex::encode(&digest[..8])
}

fn transcript_diagnostic_text(question: &str) -> String {
    let lines: Vec<String> = question
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let (_, body) = split_transcript_source_line(trimmed)?;
            let body = body.trim();
            (!body.is_empty()).then(|| body.to_string())
        })
        .collect();
    if !lines.is_empty() {
        return lines.join("\n");
    }
    String::new()
}

fn transcript_source_label_count(question: &str) -> usize {
    question
        .lines()
        .filter(|line| split_transcript_source_line(line.trim()).is_some())
        .count()
}

fn split_transcript_source_line(line: &str) -> Option<(&str, &str)> {
    let (label, body) = line.split_once(':')?;
    let clean_label = label.trim().to_ascii_lowercase();
    matches!(
        clean_label.as_str(),
        "mic" | "microphone" | "system" | "speaker" | "audio"
    )
    .then_some((label.trim(), body))
}

fn answer_plan_for_request(
    req: &CompleteRequest,
    requested_lane: &str,
    rag_matches: &[sync::RagMatch],
) -> AnswerPlan {
    let question = extract_search_question(&req.user);
    let normalized = normalize_guardrail_text(&question);
    let generic_live_transcript_prompt = is_generic_live_transcript_prompt(&normalized);
    let flattened_planning_context = extract_planning_context(&req.user);
    let typed_planning_context = if generic_live_transcript_prompt {
        latest_typed_transcript_turn(req)
            .map(|turn| format!("{}\n{}", turn.question, turn.user_response))
            .unwrap_or_default()
    } else {
        req.context
            .iter()
            .map(|context| context.content.trim())
            .filter(|content| !content.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let planning_context = if typed_planning_context.is_empty() {
        flattened_planning_context
    } else if generic_live_transcript_prompt || flattened_planning_context.is_empty() {
        typed_planning_context
    } else {
        format!("{flattened_planning_context}\n\n{typed_planning_context}")
    };
    let normalized_context = normalize_guardrail_text(&planning_context);
    let interview_context = looks_like_interview_answer_context(&normalized, &normalized_context);
    let word_count = normalized.split_whitespace().count();
    let short_question = word_count <= 8;
    let topic_reset = looks_like_new_topic_request(&normalized);
    let transcript_placeholder = looks_like_transcript_placeholder(&normalized);
    let has_images = !req.image_data_urls.is_empty() || requested_lane == "vision";
    let has_planning_context = !planning_context.trim().is_empty();
    let generic_screen_capture_prompt = looks_like_generic_screen_capture_prompt(&normalized);
    let quick_conceptual = !has_images
        && !generic_live_transcript_prompt
        && !generic_screen_capture_prompt
        && looks_like_quick_conceptual_question(&normalized, word_count);
    let follow_up = !topic_reset
        && short_question
        && contains_any(
            &normalized,
            &[
                "that",
                "this",
                "those",
                "same",
                "above",
                "previous",
                "next",
                "long answer",
                "longer answer",
                "more detail",
                "more detailed",
                "expand",
                "elaborate",
            ],
        );
    let context_coding = has_planning_context
        && (looks_like_coding_question(&normalized_context) || has_code_shape(&normalized_context));
    let context_system_design =
        has_planning_context && looks_like_system_design_question(&normalized_context);
    let context_system_design_followup = context_system_design
        && !topic_reset
        && looks_like_system_design_followup_question(&normalized);
    let context_system_design_canvas_followup = context_system_design_followup
        && looks_like_system_design_canvas_followup_question(&normalized);
    let diagram_request = looks_like_diagram_request(&normalized);
    let explicit_code_generation = looks_like_explicit_code_generation_request(&normalized);
    let direct_technical_plan = looks_like_direct_technical_plan_question(&normalized);
    let direct_lived_story_followup = looks_like_lived_interview_story_followup(&normalized)
        && ((interview_context && !context_system_design)
            || direct_question_confirms_story_ownership(&question));
    let direct_behavioral = !direct_technical_plan
        && (looks_like_behavioral_question(&normalized) || direct_lived_story_followup);
    let context_behavioral = generic_live_transcript_prompt
        && has_planning_context
        && !context_system_design
        && (looks_like_behavioral_question(&normalized_context)
            || looks_like_interview_coaching_question(&normalized_context)
            || looks_like_interview_answer_context(&normalized_context, ""));
    let direct_system_design = !direct_technical_plan
        && !direct_behavioral
        && (diagram_request || looks_like_system_design_question(&normalized));
    let coding = !quick_conceptual
        && !direct_behavioral
        && !context_behavioral
        && !direct_system_design
        && (((!diagram_request || explicit_code_generation)
            && looks_like_direct_coding_request(&normalized))
            || (has_images && context_coding)
            || (generic_screen_capture_prompt && context_coding)
            || (generic_live_transcript_prompt && context_coding));
    let coding_followup = looks_like_coding_followup(&normalized, follow_up)
        || (has_planning_context
            && context_coding
            && looks_like_contextual_code_generation_followup(&normalized))
        || (has_images && context_coding && follow_up);
    let simple_coding = coding
        && !(context_coding && (has_images || generic_screen_capture_prompt))
        && looks_like_simple_coding_question(&normalized, short_question);
    let resume_intro = looks_like_resume_intro_request(&normalized)
        || (generic_live_transcript_prompt && looks_like_resume_intro_request(&normalized_context));
    let behavioral = direct_behavioral || context_behavioral || resume_intro;
    // A request the rule engine has classified as a behavioral answer is an
    // interview surface even when the interviewer omits literal words such as
    // "interview" or "candidate" (for example, "Why this role?" or a bare
    // "Tell me about a time..."). Keep the earlier value for disambiguating
    // lived follow-ups, then make the final plan self-consistent here.
    let interview_context = interview_context || behavioral;
    let system_design = !behavioral
        && (direct_system_design
            || context_system_design_canvas_followup
            || (generic_live_transcript_prompt && context_system_design));
    let screen = has_images
        || contains_any(
            &normalized,
            &["screen", "screenshot", "image", "canvas", "visible page"],
        );
    let docs_requested = contains_any(
        &normalized,
        &[
            "attached document",
            "attached docs",
            "attached file",
            "document",
            "pdf",
            "spreadsheet",
        ],
    );
    let docs = docs_requested
        && (!generic_screen_capture_prompt
            || planning_context_has_document_signal(&normalized_context));
    let meeting = contains_any(
        &normalized,
        &[
            "meeting",
            "call",
            "transcript",
            "what did they say",
            "what was decided",
            "action item",
            "follow up from the meeting",
        ],
    );
    let writing = contains_any(
        &normalized,
        &["rewrite", "write", "draft", "polish", "email", "message"],
    ) && !coding
        && !behavioral;
    let current_means_external = normalized.contains("current")
        && !contains_any(
            &normalized,
            &[
                "current session",
                "current context",
                "current screen",
                "current transcript",
            ],
        );
    let explicit_web = !generic_live_transcript_prompt
        && !transcript_placeholder
        && (current_means_external
            || contains_any(
                &normalized,
                &[
                    "search web",
                    "web search",
                    "look up",
                    "lookup",
                    "google",
                    "browse",
                    "search online",
                    "latest",
                    "today",
                    "news",
                    "price",
                    "stock",
                    "weather",
                    "schedule",
                    "recent",
                ],
            ));
    let public_lookup_phrase = looks_like_public_lookup_phrase(&normalized, word_count);
    let about_unknown = !quick_conceptual
        && rag_matches.is_empty()
        && !screen
        && !coding
        && !behavioral
        && !system_design
        && (public_lookup_phrase
            || normalized.starts_with("who is ")
            || normalized.starts_with("what is ")
            || normalized.starts_with("where is ")
            || normalized.starts_with("tell me about ")
            || normalized.starts_with("can you tell me about ")
            || normalized.contains(" information about "));
    let needs_web_search =
        !screen && !coding && !behavioral && !system_design && (explicit_web || about_unknown);
    let screen_without_image = screen && !has_images && !has_planning_context;
    let has_any_attached_evidence = has_images || has_planning_context || !rag_matches.is_empty();
    let missing_context = !needs_web_search
        && rag_matches.is_empty()
        && (((docs && !has_any_attached_evidence) || screen_without_image)
            || (generic_live_transcript_prompt && !has_planning_context)
            || (transcript_placeholder && !has_planning_context)
            || (!has_any_attached_evidence
                && contains_any(
                    &normalized,
                    &[
                        "attached",
                        "session context",
                        "current context",
                        "current session",
                    ],
                )));
    let explanation_only_coding =
        (coding || coding_followup) && looks_like_explanation_only_coding_question(&normalized);

    let intent = if needs_web_search {
        AnswerIntent::Research
    } else if missing_context {
        AnswerIntent::MissingContext
    } else if direct_technical_plan {
        AnswerIntent::General
    } else if behavioral {
        AnswerIntent::Behavioral
    } else if system_design {
        AnswerIntent::SystemDesign
    } else if coding_followup {
        AnswerIntent::CodingFollowUp
    } else if coding {
        AnswerIntent::Coding
    } else if screen {
        AnswerIntent::Screen
    } else if context_system_design_followup {
        AnswerIntent::FollowUp
    } else if quick_conceptual || (topic_reset && short_question) {
        AnswerIntent::Quick
    } else if meeting {
        AnswerIntent::Meeting
    } else if follow_up {
        AnswerIntent::FollowUp
    } else if writing {
        AnswerIntent::Writing
    } else if short_question {
        AnswerIntent::Quick
    } else {
        AnswerIntent::General
    };

    let recommended_lane = match intent {
        AnswerIntent::Quick => "instant",
        AnswerIntent::Coding if simple_coding || explanation_only_coding => "balanced",
        AnswerIntent::CodingFollowUp if explanation_only_coding => "balanced",
        AnswerIntent::Coding | AnswerIntent::CodingFollowUp | AnswerIntent::SystemDesign => "deep",
        AnswerIntent::Screen => "vision",
        AnswerIntent::Research
        | AnswerIntent::Behavioral
        | AnswerIntent::Meeting
        | AnswerIntent::MissingContext
        | AnswerIntent::Writing
        | AnswerIntent::FollowUp
        | AnswerIntent::General => "balanced",
    };
    let output = match intent {
        AnswerIntent::Coding | AnswerIntent::CodingFollowUp if explanation_only_coding => {
            AnswerOutput::Compact
        }
        AnswerIntent::Coding | AnswerIntent::CodingFollowUp => AnswerOutput::CodeArtifact,
        AnswerIntent::Research => AnswerOutput::SourceAnswer,
        AnswerIntent::Behavioral => AnswerOutput::InterviewAnswer,
        AnswerIntent::SystemDesign | AnswerIntent::Screen => AnswerOutput::CanvasDetail,
        _ => AnswerOutput::Compact,
    };
    let confidence = match intent {
        AnswerIntent::Screen if has_images => 0.95,
        AnswerIntent::Behavioral | AnswerIntent::Coding | AnswerIntent::Research => 0.90,
        AnswerIntent::CodingFollowUp
        | AnswerIntent::SystemDesign
        | AnswerIntent::MissingContext => 0.86,
        AnswerIntent::Meeting | AnswerIntent::Writing => 0.80,
        AnswerIntent::Quick => 0.72,
        AnswerIntent::FollowUp | AnswerIntent::General | AnswerIntent::Screen => 0.68,
    };

    AnswerPlan {
        intent,
        output,
        recommended_lane,
        confidence,
        interview_context,
        needs_screen: screen,
        needs_docs: docs,
        needs_transcript: meeting,
        needs_memory: !rag_matches.is_empty(),
        needs_web_search: matches!(intent, AnswerIntent::Research) && needs_web_search,
    }
}

fn should_lookup_completion_memory(req: &CompleteRequest, requested_lane: &str) -> bool {
    let question = extract_search_question(&req.user);
    let normalized = normalize_guardrail_text(&question);
    if normalized.trim().chars().count() < 8 {
        return false;
    }
    if !req.image_data_urls.is_empty() || requested_lane == "vision" {
        return false;
    }
    let planning_context = extract_planning_context(&req.user);
    if !planning_context.trim().is_empty()
        && (is_generic_live_transcript_prompt(&normalized)
            || looks_like_transcript_placeholder(&normalized))
    {
        return false;
    }
    if contains_any(
        &normalized,
        &[
            "saved memory",
            "bluey memory",
            "conversation context",
            "session context",
            "current session",
            "previous session",
            "use memory",
            "use the memory",
            "from memory",
            "what did we",
            "what was decided",
            "action item",
            "meeting notes",
            "continue",
            "the previous",
            "previous answer",
            "previous code",
            "previous design",
            "earlier answer",
            "earlier code",
            "same answer",
            "same code",
            "same design",
            "above answer",
            "above code",
            "long answer",
            "longer answer",
            "more detail",
            "more detailed",
            "expand on that",
            "elaborate on that",
        ],
    ) {
        return true;
    }

    let word_count = normalized.split_whitespace().count();
    let short_follow_up = word_count <= 8
        && !looks_like_new_topic_request(&normalized)
        && contains_any(
            &normalized,
            &[
                "that",
                "this",
                "those",
                "same",
                "above",
                "previous",
                "next",
                "long answer",
                "longer answer",
                "more detail",
                "more detailed",
                "expand",
                "elaborate",
            ],
        );
    if short_follow_up {
        return true;
    }

    false
}

fn answer_plan_allows_memory_lookup(plan: &AnswerPlan) -> bool {
    !matches!(
        plan.intent,
        AnswerIntent::Quick
            | AnswerIntent::Coding
            | AnswerIntent::Screen
            | AnswerIntent::Research
            | AnswerIntent::MissingContext
    )
}

fn answer_plan_routing_enabled() -> bool {
    !env_flag_is_false("BLUEY_ANSWER_PLAN_ROUTING")
}

fn answer_plan_ai_fallback_enabled() -> bool {
    env_flag_is_true("BLUEY_ANSWER_PLAN_AI_FALLBACK")
}

const DEFAULT_ANSWER_PLAN_AI_CONFIDENCE_THRESHOLD: f32 = 0.70;
const DEFAULT_ANSWER_PLAN_AI_TIMEOUT_MS: u64 = 900;
const DEFAULT_ANSWER_PLAN_AI_MAX_TOKENS: u32 = 180;

#[derive(Debug, Clone)]
struct ResolvedAnswerPlan {
    plan: AnswerPlan,
    source: &'static str,
    ai_attempted: bool,
    ai_reason: &'static str,
    provider_accounting_pending: bool,
}

impl ResolvedAnswerPlan {
    fn rules(plan: AnswerPlan, reason: &'static str) -> Self {
        Self {
            plan,
            source: "rules",
            ai_attempted: false,
            ai_reason: reason,
            provider_accounting_pending: false,
        }
    }
}

enum AiAnswerPlanRefinement {
    Refined(AnswerPlan),
    Unavailable,
    ProviderAccountingPending,
}

fn lane_for_answer_plan(requested_lane: &str, plan: &AnswerPlan, enabled: bool) -> String {
    let requested = requested_lane.trim();
    if !enabled || requested == "local" {
        return requested.to_string();
    }
    if requested == "vision" {
        return "vision".to_string();
    }
    // A user-selected performance mode is a contract. Classification may
    // shape the answer and artifacts, but must not silently turn a balanced
    // or instant request into a slower, more expensive deep request.
    if matches!(requested, "instant" | "balanced" | "deep") {
        return requested.to_string();
    }
    plan.recommended_lane.to_string()
}

fn max_tokens_for_answer_plan(requested: Option<u32>, output: AnswerOutput) -> Option<u32> {
    if requested.is_some() {
        return requested;
    }
    match output {
        AnswerOutput::Compact => Some(512),
        AnswerOutput::CodeArtifact => Some(CODE_ARTIFACT_DEFAULT_OUTPUT_TOKENS),
        AnswerOutput::CanvasDetail => Some(CANVAS_DETAIL_DEFAULT_OUTPUT_TOKENS),
        AnswerOutput::InterviewAnswer => Some(700),
        AnswerOutput::SourceAnswer => Some(900),
    }
}

fn estimate_max_output_tokens_for_answer_plan(
    requested: Option<u32>,
    thinking: routing::ThinkingBudget,
    output: AnswerOutput,
) -> u32 {
    let planned = max_tokens_for_answer_plan(requested, output);
    routing::effective_max_output_tokens(planned, thinking)
}

fn generated_answer_quality_failure(
    text: &str,
    output_tokens: i64,
    max_tokens: Option<u32>,
    plan: &AnswerPlan,
    normalized_question: &str,
) -> Option<&'static str> {
    if likely_truncated_at_budget(text, output_tokens, max_tokens) {
        return Some("upstream_output_truncated");
    }
    if lru_code_uses_library_cache(text, plan, normalized_question) {
        return Some("upstream_code_contract_failed");
    }
    if requires_first_principles_lru_code(plan, normalized_question)
        && !lru_code_has_obvious_first_principles_structure(text)
    {
        return Some("upstream_code_contract_failed");
    }
    let substantive_words = text
        .split_whitespace()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .count();
    if plan.output == AnswerOutput::InterviewAnswer && substantive_words < 30 {
        return Some("upstream_answer_too_short");
    }
    None
}

fn lru_code_uses_library_cache(text: &str, plan: &AnswerPlan, normalized_question: &str) -> bool {
    if !requires_first_principles_lru_code(plan, normalized_question) {
        return false;
    }

    let mut inside_fence = false;
    let lru_cache_aliases = ["lru_cache"];
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            inside_fence = !inside_fence;
            continue;
        }
        if !inside_fence || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        let code = trimmed.to_ascii_lowercase();
        if code.starts_with("import collections") || code.starts_with("from collections import") {
            return true;
        }
        if let Some(imports) = code.strip_prefix("from functools import") {
            for imported in imports.split(',') {
                let imported = imported.trim();
                let mut words = imported.split_whitespace();
                if words.next() == Some("lru_cache") {
                    return true;
                }
            }
        }
        if code.contains("functools.lru_cache")
            || code.contains(".lru_cache")
            || code.strip_prefix('@').is_some_and(|decorator| {
                let decorator = decorator
                    .trim_start()
                    .split(['(', ' ', '\t'])
                    .next()
                    .unwrap_or_default();
                lru_cache_aliases.contains(&decorator)
            })
        {
            return true;
        }
    }
    false
}

fn requires_first_principles_lru_code(plan: &AnswerPlan, normalized_question: &str) -> bool {
    plan.output == AnswerOutput::CodeArtifact
        && contains_any(normalized_question, &["lru", "least recently used"])
        && contains_any(
            normalized_question,
            &[
                "from first principles",
                "without a library cache",
                "without replacing it with a library cache",
            ],
        )
        && !contains_any(
            normalized_question,
            &[
                "using collections ordereddict",
                "use collections ordereddict",
                "with collections ordereddict",
                "using functools lru cache",
                "use functools lru cache",
                "with functools lru cache",
            ],
        )
}

/// This is deliberately an obvious-shape gate, not a proof of LRU correctness:
/// it stops empty placeholders from reaching a streaming client before the
/// deeper evaluator can inspect behavior and invariants.
fn lru_code_has_obvious_first_principles_structure(text: &str) -> bool {
    let mut inside_fence = false;
    let mut saw_opening_fence = false;
    let mut saw_closing_fence = false;
    let mut code = String::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            if inside_fence {
                saw_closing_fence = true;
            } else {
                saw_opening_fence = true;
            }
            inside_fence = !inside_fence;
            continue;
        }
        if inside_fence {
            code.push_str(line);
            code.push('\n');
        }
    }
    if !saw_opening_fence || !saw_closing_fence {
        return false;
    }

    let code = code.to_ascii_lowercase();
    let has_sentinel_pair = (code.contains("head") && code.contains("tail"))
        || (code.contains("left") && code.contains("right"))
        || (code.contains("front") && code.contains("back"));
    code.contains("class lrucache")
        && code.contains("def get")
        && code.contains("def put")
        && code.contains("prev")
        && code.contains("next")
        && has_sentinel_pair
        && ["hashmap", "cache", "map", "nodes"]
            .iter()
            .any(|signal| code.contains(signal))
}

fn flush_interrupted_role_anchor(
    role_anchor: &mut interview_contracts::EvidenceBoundRoleAnchor,
    output: &mut BufferedDisclosureOutput,
) -> String {
    let mut safe_partial = role_anchor
        .finish()
        .and_then(|raw_opening| output.push(&raw_opening))
        .unwrap_or_default();
    safe_partial.push_str(&output.take_safe());
    safe_partial
}

fn upstream_stream_failure_reason(error: &anyhow::Error) -> &'static str {
    let Some(reason) = routing::upstream_terminal_reason(error) else {
        return "upstream_stream_error";
    };
    let normalized = reason.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    if matches!(
        normalized.as_str(),
        "length" | "max_tokens" | "max_output_tokens" | "token_limit"
    ) {
        "upstream_output_truncated"
    } else if matches!(
        normalized.as_str(),
        "content_filter" | "safety" | "blocked" | "refusal" | "recitation" | "prohibited_content"
    ) {
        "upstream_output_blocked"
    } else {
        "upstream_output_incomplete"
    }
}

fn likely_truncated_at_budget(text: &str, output_tokens: i64, max_tokens: Option<u32>) -> bool {
    let Some(max_tokens) = max_tokens else {
        return false;
    };
    if output_tokens < i64::from(max_tokens.saturating_sub(2)) {
        return false;
    }

    let trimmed = text.trim_end();
    if trimmed.is_empty() || trimmed.matches("```").count() % 2 == 1 {
        return true;
    }
    let last_line = trimmed.lines().last().unwrap_or_default().trim();
    if last_line.is_empty()
        || last_line.ends_with(':')
        || (last_line.starts_with('#') && !last_line.contains(['.', '!', '?']))
    {
        return true;
    }
    !matches!(
        trimmed.chars().last(),
        Some('.' | '!' | '?' | ')' | ']' | '}' | '`' | '"' | '\'')
    )
}

fn code_artifact_missing_for_plan(plan: &AnswerPlan, artifact: Option<&ResponseArtifact>) -> bool {
    plan.output == AnswerOutput::CodeArtifact
        && !matches!(
            artifact.map(|artifact| artifact.artifact_type),
            Some("code")
        )
}

fn code_artifact_missing_error() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(ApiError {
            error: "Bluey expected code for this answer, but the provider returned only prose. Please retry.".into(),
            reason: Some("code_artifact_missing".into()),
            retry_after_secs: Some(1),
            ..Default::default()
        }),
    )
}

async fn resolve_answer_plan_for_request(
    state: &AppState,
    account: &Account,
    req: &CompleteRequest,
    requested_lane: &str,
    rag_matches: &[sync::RagMatch],
) -> ResolvedAnswerPlan {
    let rule_plan = answer_plan_for_request(req, requested_lane, rag_matches);
    let Some(reason) =
        should_run_ai_answer_plan_classifier(req, requested_lane, rag_matches, &rule_plan)
    else {
        return ResolvedAnswerPlan::rules(rule_plan, "rule_confident");
    };

    match refine_answer_plan_with_ai_classifier(state, account, req, requested_lane, &rule_plan)
        .await
    {
        AiAnswerPlanRefinement::Refined(plan) => ResolvedAnswerPlan {
            plan,
            source: "ai_refined",
            ai_attempted: true,
            ai_reason: reason,
            provider_accounting_pending: false,
        },
        AiAnswerPlanRefinement::Unavailable => ResolvedAnswerPlan {
            plan: rule_plan,
            source: "rules",
            ai_attempted: true,
            ai_reason: "ai_unavailable_or_invalid",
            provider_accounting_pending: false,
        },
        AiAnswerPlanRefinement::ProviderAccountingPending => ResolvedAnswerPlan {
            plan: rule_plan,
            source: "rules",
            ai_attempted: true,
            ai_reason: "provider_accounting_pending",
            provider_accounting_pending: true,
        },
    }
}

fn should_run_ai_answer_plan_classifier(
    req: &CompleteRequest,
    requested_lane: &str,
    rag_matches: &[sync::RagMatch],
    plan: &AnswerPlan,
) -> Option<&'static str> {
    if !answer_plan_ai_fallback_enabled() {
        return None;
    }
    if requested_lane == "local" || requested_lane == "vision" || !req.image_data_urls.is_empty() {
        return None;
    }
    let question = extract_search_question(&req.user);
    let normalized = normalize_guardrail_text(&question);
    if normalized.is_empty() || contains_sensitive_classifier_text(&normalized) {
        return None;
    }
    if is_hard_answer_plan_signal(&normalized) {
        return None;
    }

    let threshold = answer_plan_ai_confidence_threshold();
    if plan.confidence < threshold {
        return Some("low_confidence");
    }

    let signal_count = [
        looks_like_coding_question(&normalized),
        looks_like_behavioral_question(&normalized),
        looks_like_system_design_question(&normalized),
        contains_any(
            &normalized,
            &["screen", "screenshot", "image", "visible page"],
        ),
        contains_any(
            &normalized,
            &["rewrite", "draft", "polish", "email", "message"],
        ),
        !rag_matches.is_empty(),
    ]
    .into_iter()
    .filter(|value| *value)
    .count();
    if signal_count >= 2 && plan.confidence < 0.86 {
        return Some("conflicting_signals");
    }

    None
}

fn answer_plan_ai_confidence_threshold() -> f32 {
    std::env::var("BLUEY_ANSWER_PLAN_AI_CONFIDENCE_THRESHOLD")
        .ok()
        .and_then(|value| value.trim().parse::<f32>().ok())
        .filter(|value| (0.50..=0.95).contains(value))
        .unwrap_or(DEFAULT_ANSWER_PLAN_AI_CONFIDENCE_THRESHOLD)
}

fn answer_plan_ai_timeout() -> Duration {
    let ms = std::env::var("BLUEY_ANSWER_PLAN_AI_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| (100..=3_000).contains(value))
        .unwrap_or(DEFAULT_ANSWER_PLAN_AI_TIMEOUT_MS);
    Duration::from_millis(ms)
}

fn answer_plan_ai_max_tokens() -> u32 {
    std::env::var("BLUEY_ANSWER_PLAN_AI_MAX_TOKENS")
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|value| (64..=512).contains(value))
        .unwrap_or(DEFAULT_ANSWER_PLAN_AI_MAX_TOKENS)
}

fn contains_sensitive_classifier_text(normalized: &str) -> bool {
    contains_any(
        normalized,
        &[
            "api key",
            "secret key",
            "password",
            "bearer ",
            "authorization:",
            "private key",
            "access token",
            "refresh token",
        ],
    ) || normalized.contains("sk-")
        || normalized.contains('@')
}

fn is_hard_answer_plan_signal(normalized: &str) -> bool {
    looks_like_behavioral_question(normalized)
        || looks_like_coding_question(normalized)
        || looks_like_system_design_question(normalized)
        || looks_like_direct_technical_plan_question(normalized)
}

#[derive(Debug, Deserialize)]
struct AiAnswerPlanPayload {
    intent: Option<String>,
    lane: Option<String>,
    output: Option<String>,
    needs_web_search: Option<bool>,
    confidence: Option<f32>,
}

async fn refine_answer_plan_with_ai_classifier(
    state: &AppState,
    account: &Account,
    req: &CompleteRequest,
    requested_lane: &str,
    rule_plan: &AnswerPlan,
) -> AiAnswerPlanRefinement {
    let question = truncate_chars(&extract_search_question(&req.user), 1_200);
    let system = "You are Bluey's fast routing classifier. Return only one JSON object. Do not answer the user. Valid intent values: quick, coding, coding_followup, behavioral, system_design, screen, research, follow_up, missing_context, writing, meeting, general. Valid lane values: instant, balanced, deep, vision. Valid output values: compact, code_artifact, source_answer, canvas_detail, interview_answer.";
    let user = format!(
        "Classify this Bluey request for routing.\n\
         User question:\n{question}\n\n\
         Signals:\n\
         requested_lane={requested_lane}\n\
         image_count={}\n\
         rule_intent={}\n\
         rule_output={}\n\
         rule_lane={}\n\
         rule_confidence={:.2}\n\n\
         Return JSON with keys: intent, lane, output, needs_web_search, confidence.",
        req.image_data_urls.len(),
        rule_plan.intent.as_str(),
        rule_plan.output.as_str(),
        rule_plan.recommended_lane,
        rule_plan.confidence
    );
    let max_tokens = answer_plan_ai_max_tokens();
    let fallback_input_tokens = pricing::utf8_input_token_upper_bound([system, user.as_str()]);
    let routes = priced_routes_for(
        "instant",
        fallback_input_tokens,
        i64::from(max_tokens),
        &format!("{}:answer-plan", req.request_id),
    );
    if routes.is_empty() {
        return AiAnswerPlanRefinement::Unavailable;
    }

    let started = Instant::now();
    let mut dispatch_index = 0usize;
    for route in routes {
        let key_candidates = state.config.upstream.key_candidates(
            route.provider,
            &format!(
                "answer-plan:{}:{}:{}",
                req.request_id, route.provider, route.model
            ),
        );
        if key_candidates.is_empty() {
            continue;
        }

        loop {
            let selected_key = match state
                .provider_health
                .choose_key(route.provider, route.model, &key_candidates)
                .await
            {
                Ok(key) => key,
                Err(_) => break,
            };
            if state
                .rate_limiters
                .check_provider_llm(route.provider, route.model)
                .await
                .is_err()
            {
                break;
            }

            let hold_request_id = format!("{}:answer-plan:{dispatch_index}", req.request_id);
            dispatch_index = dispatch_index.saturating_add(1);
            let mut cost_guard = match provider_cost_guard::reserve(
                &state.pool,
                state.config.upstream_spend_guard,
                &account.id,
                &format!("router:{}:answer-plan", req.request_id),
                &hold_request_id,
                route.provider,
                route.model,
                route.estimated_bluey_cost_cents,
                "answer_plan_classifier_attempt",
                "answer_plan_classifier",
            ) {
                Ok(provider_cost_guard::Admission::Held(guard)) => guard,
                Ok(provider_cost_guard::Admission::Unconfigured)
                | Ok(provider_cost_guard::Admission::GlobalLimit)
                | Err(_) => return AiAnswerPlanRefinement::Unavailable,
            };

            let completion = tokio::time::timeout(
                answer_plan_ai_timeout(),
                routing::complete_with_key(
                    &selected_key.secret,
                    route.provider,
                    route.model,
                    system,
                    &user,
                    Some(max_tokens),
                    Some(0.0),
                    routing::ThinkingBudget::off(),
                    Some(fallback_input_tokens),
                    &[],
                ),
            )
            .await;

            match completion {
                Ok(Ok(comp)) => {
                    let route_matches =
                        comp.provider == route.provider && comp.model == route.model;
                    let event = answer_plan_classifier_usage_event(
                        &hold_request_id,
                        &comp,
                        started.elapsed().as_millis() as i64,
                    );
                    let actual_cost = event.cost_cents_to_bluey;
                    if settle_provider_attempt_before_customer(
                        &state.pool,
                        &account.id,
                        &req.request_id,
                        &mut cost_guard,
                        event,
                        actual_cost,
                        comp.usage_provenance,
                    )
                    .is_err()
                    {
                        return AiAnswerPlanRefinement::ProviderAccountingPending;
                    }
                    if !route_matches {
                        tracing::error!(
                            request_id = %req.request_id,
                            requested_provider = route.provider,
                            requested_model = route.model,
                            completed_provider = %comp.provider,
                            completed_model = %comp.model,
                            "answer-plan classifier crossed its routed provider/model boundary"
                        );
                        return AiAnswerPlanRefinement::Unavailable;
                    }
                    if let Some(plan) = parse_ai_answer_plan(&comp.text).and_then(|payload| {
                        merge_ai_answer_plan(rule_plan, payload, req, requested_lane)
                    }) {
                        tracing::info!(
                            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                            request_id = %req.request_id,
                            provider = %comp.provider,
                            model = %comp.model,
                            answer_intent = %plan.intent.as_str(),
                            answer_output = %plan.output.as_str(),
                            answer_lane = %plan.recommended_lane,
                            answer_confidence = plan.confidence,
                            "answer plan AI classifier refined route"
                        );
                        return AiAnswerPlanRefinement::Refined(plan);
                    }
                    return AiAnswerPlanRefinement::Unavailable;
                }
                Ok(Err(err)) => {
                    if cost_guard.settle_conservative().is_err() {
                        return AiAnswerPlanRefinement::ProviderAccountingPending;
                    }
                    if let Some(retry_after_secs) = routing::upstream_retry_after(&err) {
                        let _ = state
                            .provider_health
                            .record_cooldown(
                                route.provider,
                                route.model,
                                &selected_key.fingerprint,
                                retry_after_secs,
                            )
                            .await;
                        continue;
                    }
                    break;
                }
                Err(_) => {
                    if cost_guard.settle_conservative().is_err() {
                        return AiAnswerPlanRefinement::ProviderAccountingPending;
                    }
                    break;
                }
            }
        }
    }

    AiAnswerPlanRefinement::Unavailable
}

fn parse_ai_answer_plan(text: &str) -> Option<AiAnswerPlanPayload> {
    let trimmed = text.trim();
    serde_json::from_str::<AiAnswerPlanPayload>(trimmed)
        .ok()
        .or_else(|| {
            let start = trimmed.find('{')?;
            let end = trimmed.rfind('}')?;
            if end <= start {
                return None;
            }
            serde_json::from_str::<AiAnswerPlanPayload>(&trimmed[start..=end]).ok()
        })
}

fn merge_ai_answer_plan(
    rule_plan: &AnswerPlan,
    payload: AiAnswerPlanPayload,
    req: &CompleteRequest,
    requested_lane: &str,
) -> Option<AnswerPlan> {
    let question = extract_search_question(&req.user);
    let normalized = normalize_guardrail_text(&question);
    let mut intent = payload
        .intent
        .as_deref()
        .and_then(parse_answer_intent)
        .unwrap_or(rule_plan.intent);
    let mut hard_override_applied = false;

    if !req.image_data_urls.is_empty() || requested_lane == "vision" {
        intent = AnswerIntent::Screen;
        hard_override_applied = true;
    } else if looks_like_behavioral_question(&normalized) {
        intent = AnswerIntent::Behavioral;
        hard_override_applied = true;
    } else if looks_like_coding_followup(&normalized, true) {
        intent = AnswerIntent::CodingFollowUp;
        hard_override_applied = true;
    } else if looks_like_coding_question(&normalized) {
        intent = AnswerIntent::Coding;
        hard_override_applied = true;
    } else if looks_like_system_design_question(&normalized) && intent == AnswerIntent::Behavioral {
        return None;
    }

    let mut output = payload
        .output
        .as_deref()
        .and_then(parse_answer_output)
        .unwrap_or_else(|| default_output_for_intent(intent));
    if !output_matches_intent(intent, output) {
        output = default_output_for_intent(intent);
    }
    let recommended_lane = default_lane_for_intent(intent);
    if let Some(lane) = payload.lane.as_deref().and_then(valid_answer_lane) {
        if !lane_matches_intent(intent, lane) && !hard_override_applied {
            return None;
        }
    }
    let mut needs_web_search = payload
        .needs_web_search
        .unwrap_or(rule_plan.needs_web_search);
    if matches!(
        intent,
        AnswerIntent::Coding
            | AnswerIntent::CodingFollowUp
            | AnswerIntent::Behavioral
            | AnswerIntent::SystemDesign
            | AnswerIntent::Screen
            | AnswerIntent::MissingContext
    ) {
        needs_web_search = false;
    }
    if intent == AnswerIntent::Research {
        needs_web_search = true;
    }

    let confidence = payload
        .confidence
        .map(|value| value.clamp(0.0, 0.99))
        .unwrap_or(rule_plan.confidence)
        .max(rule_plan.confidence.min(0.80));

    Some(AnswerPlan {
        intent,
        output,
        recommended_lane,
        confidence,
        interview_context: rule_plan.interview_context,
        needs_screen: intent == AnswerIntent::Screen || rule_plan.needs_screen,
        needs_docs: rule_plan.needs_docs,
        needs_transcript: intent == AnswerIntent::Meeting || rule_plan.needs_transcript,
        needs_memory: rule_plan.needs_memory,
        needs_web_search,
    })
}

fn parse_answer_intent(value: &str) -> Option<AnswerIntent> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "quick" => Some(AnswerIntent::Quick),
        "coding" | "code" => Some(AnswerIntent::Coding),
        "coding_followup" | "code_followup" => Some(AnswerIntent::CodingFollowUp),
        "behavioral" | "interview_behavioral" => Some(AnswerIntent::Behavioral),
        "system_design" => Some(AnswerIntent::SystemDesign),
        "screen" | "vision" => Some(AnswerIntent::Screen),
        "research" | "web_research" => Some(AnswerIntent::Research),
        "follow_up" | "followup" => Some(AnswerIntent::FollowUp),
        "missing_context" => Some(AnswerIntent::MissingContext),
        "writing" => Some(AnswerIntent::Writing),
        "meeting" => Some(AnswerIntent::Meeting),
        "general" => Some(AnswerIntent::General),
        _ => None,
    }
}

fn parse_answer_output(value: &str) -> Option<AnswerOutput> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "compact" => Some(AnswerOutput::Compact),
        "code_artifact" | "code" => Some(AnswerOutput::CodeArtifact),
        "source_answer" | "sources" => Some(AnswerOutput::SourceAnswer),
        "canvas_detail" | "canvas" => Some(AnswerOutput::CanvasDetail),
        "interview_answer" | "interview" | "behavioral_answer" => {
            Some(AnswerOutput::InterviewAnswer)
        }
        _ => None,
    }
}

fn default_lane_for_intent(intent: AnswerIntent) -> &'static str {
    match intent {
        AnswerIntent::Quick => "instant",
        AnswerIntent::Coding | AnswerIntent::CodingFollowUp | AnswerIntent::SystemDesign => "deep",
        AnswerIntent::Screen => "vision",
        _ => "balanced",
    }
}

fn default_output_for_intent(intent: AnswerIntent) -> AnswerOutput {
    match intent {
        AnswerIntent::Coding | AnswerIntent::CodingFollowUp => AnswerOutput::CodeArtifact,
        AnswerIntent::Research => AnswerOutput::SourceAnswer,
        AnswerIntent::Behavioral => AnswerOutput::InterviewAnswer,
        AnswerIntent::SystemDesign | AnswerIntent::Screen => AnswerOutput::CanvasDetail,
        _ => AnswerOutput::Compact,
    }
}

fn output_matches_intent(intent: AnswerIntent, output: AnswerOutput) -> bool {
    default_output_for_intent(intent) == output
        || matches!(
            (intent, output),
            (AnswerIntent::General, AnswerOutput::CanvasDetail)
                | (AnswerIntent::Writing, AnswerOutput::CanvasDetail)
                | (AnswerIntent::Meeting, AnswerOutput::CanvasDetail)
        )
}

fn valid_answer_lane(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "instant" => Some("instant"),
        "balanced" => Some("balanced"),
        "deep" => Some("deep"),
        "vision" => Some("vision"),
        _ => None,
    }
}

fn lane_matches_intent(intent: AnswerIntent, lane: &str) -> bool {
    default_lane_for_intent(intent) == lane
        || matches!(
            (intent, lane),
            (AnswerIntent::General, "instant")
                | (AnswerIntent::Quick, "balanced")
                | (AnswerIntent::Writing, "instant")
        )
}

fn answer_plan_classifier_usage_event(
    request_id: &str,
    comp: &routing::Completion,
    latency_ms: i64,
) -> UsageEvent {
    let bluey_cost = pricing::lookup(&comp.provider, &comp.model)
        .map(|entry| pricing::compute_cost(entry, comp.input_tokens, comp.output_tokens).0)
        .unwrap_or(0);
    UsageEvent {
        request_id: request_id.to_string(),
        kind: "answer_plan_classifier_attempt".into(),
        task_type: Some("answer_plan_classifier".into()),
        lane: Some("instant".into()),
        provider: Some(comp.provider.clone()),
        model: Some(comp.model.clone()),
        input_tokens: comp.input_tokens,
        output_tokens: comp.output_tokens,
        latency_ms,
        cost_cents_to_bluey: bluey_cost,
        cost_cents_to_customer: 0,
        was_speculative: false,
        was_fallback: false,
    }
}

fn looks_like_coding_question(normalized: &str) -> bool {
    if looks_like_algorithmic_challenge_prompt(normalized) {
        return true;
    }

    contains_any(
        normalized,
        &[
            "code",
            "coding",
            "write a code",
            "write code",
            "give me code",
            "full code",
            "implementation",
            "implement",
            "function",
            "class",
            "test",
            "unit test",
            "test case",
            "traceback",
            "stack trace",
            "compile",
            "build error",
            "exception",
            "jsonresponse",
            "sql",
            "typescript",
            "javascript",
            "python",
            "java",
            "c++",
            "c#",
            "golang",
            "rust",
            "backend",
            "frontend",
            "database",
            "api",
            "endpoint",
            "algorithm",
            "leetcode",
            "lru",
            "cache",
            "fibonacci",
            "series",
            "swap two numbers",
            "time complexity",
            "space complexity",
            "binary search",
            "linked list",
            "doubly linked",
            "stack",
            "queue",
            "heap",
            "tree",
            "graph",
            "dfs",
            "bfs",
            "dynamic programming",
            "memoization",
        ],
    ) || normalized.contains("```")
        || normalized.contains(".rs")
        || normalized.contains(".py")
        || normalized.contains(".ts")
        || normalized.contains(".tsx")
}

fn looks_like_direct_coding_request(normalized: &str) -> bool {
    looks_like_algorithmic_challenge_prompt(normalized)
        || looks_like_explicit_code_generation_request(normalized)
        || looks_like_concrete_code_debug_request(normalized)
        || looks_like_code_explanation_request(normalized)
}

fn looks_like_concrete_code_debug_request(normalized: &str) -> bool {
    normalized.contains("```")
        || normalized.contains(".rs")
        || normalized.contains(".py")
        || normalized.contains(".ts")
        || normalized.contains(".tsx")
        || contains_any(
            normalized,
            &[
                "traceback",
                "stack trace",
                "compile error",
                "compiler error",
                "syntax error",
                "runtime error",
                "failing test",
                "test is failing",
                "exception in",
                "bug in this code",
                "debug this code",
                "fix this code",
                "fix the code",
            ],
        )
}

fn looks_like_code_explanation_request(normalized: &str) -> bool {
    looks_like_explanation_only_coding_question(normalized)
        && contains_any(
            normalized,
            &[
                "this code",
                "the code",
                "function",
                "class ",
                "algorithm",
                "data structure",
                "lru",
                "linked list",
                "pointer",
                "binary search",
                "stack",
                "queue",
                "heap",
                "tree traversal",
                "dfs",
                "bfs",
                "dynamic programming",
                "memoization",
                "time complexity",
                "space complexity",
            ],
        )
}

fn looks_like_algorithmic_challenge_prompt(normalized: &str) -> bool {
    let has_problem_intro = contains_any(
        normalized,
        &[
            "you are given",
            "given an array",
            "given a string",
            "given a list",
            "given a matrix",
            "given two",
            "given n",
            "given the root",
        ],
    );
    let has_return_or_output = contains_any(
        normalized,
        &[
            "return true",
            "return false",
            "return the",
            "return a",
            "return an",
            "output",
            "find the",
            "determine if",
            "calculate the",
        ],
    );
    let has_data_signal = contains_any(
        normalized,
        &[
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
        ],
    );

    (has_problem_intro && has_return_or_output && has_data_signal)
        || (normalized.contains("return true if") && normalized.contains("otherwise return false"))
}

fn looks_like_explicit_code_generation_request(normalized: &str) -> bool {
    if looks_like_algorithmic_challenge_prompt(normalized) {
        return true;
    }

    let code_subject = contains_any(
        normalized,
        &[
            "code",
            "implementation",
            "function",
            "class ",
            "python",
            "java",
            "typescript",
            "javascript",
            "rust",
            "c++",
            "c#",
            "golang",
            "algorithm",
            "leetcode",
            "lru",
            "fibonacci",
            "sudoku",
            "cache",
        ],
    );
    let generation_verb = contains_any(
        normalized,
        &[
            "write ",
            "implement",
            "build me",
            "create a function",
            "create the function",
            "create a class",
            "generate ",
            "solve this in",
            "provide ",
            "show me",
            "give me",
            "i want the code",
            "return the complete",
            "return complete",
            "convert this to",
            "translate this to",
            "update the code",
            "modify the code",
        ],
    );

    code_subject && generation_verb
}

fn looks_like_simple_coding_question(normalized: &str, short_question: bool) -> bool {
    if looks_like_algorithmic_challenge_prompt(normalized) {
        return false;
    }

    if contains_any(
        normalized,
        &[
            "lru",
            "cache",
            "system design",
            "architecture",
            "backend",
            "frontend",
            "database",
            "api",
            "endpoint",
            "full code",
            "full implementation",
            "production",
            "debug",
            "traceback",
            "stack trace",
            "unit test",
            "test case",
            "optimize",
            "algorithm",
            "leetcode",
            "solver",
            "sudoku",
            "backtracking",
            "binary search",
            "dfs",
            "bfs",
            "concurrency",
            "thread",
            "async",
            "distributed",
            "graph",
            "tree",
            "heap",
            "stack",
            "queue",
            "dynamic programming",
            "memoization",
            "doubly linked",
            "linked list",
        ],
    ) {
        return false;
    }
    short_question
        || contains_any(
            normalized,
            &[
                "tiny",
                "simple",
                "small",
                "basic",
                "swap two numbers",
                "fibonacci",
                "series",
                "function",
            ],
        )
}

fn looks_like_quick_conceptual_question(normalized: &str, word_count: usize) -> bool {
    if word_count == 0 || word_count > 16 || normalized.chars().count() > 180 {
        return false;
    }

    if looks_like_algorithmic_challenge_prompt(normalized)
        || looks_like_explicit_code_generation_request(normalized)
        || contains_any(
            normalized,
            &[
                "system design",
                "design a system",
                "architecture",
                "scale this",
                "scalability",
                "screenshot",
                "screen context",
                "attached",
                "current session",
                "current context",
                "transcript",
                "search web",
                "web search",
                "look up",
                "latest",
            ],
        )
    {
        return false;
    }

    contains_any(
        normalized,
        &[
            "difference between",
            "compare",
            " vs ",
            " versus ",
            "what is",
            "what are",
            "why is",
            "why does",
            "why can",
            "how does",
            "how do",
            "how would you approach",
            "can you explain",
            "explain me",
            "explain the difference",
            "when would",
        ],
    )
}

fn looks_like_coding_followup(normalized: &str, follow_up: bool) -> bool {
    contains_any(
        normalized,
        &[
            "this code",
            "above code",
            "previous code",
            "existing code",
            "fix the code",
            "optimize this",
            "reduce time complexity",
            "time complexity for this",
            "explain the code",
            "explain this logic",
            "i want the code",
            "give full code",
            "give me full code",
            "send full code",
            "convert this to",
            "translate this to",
            "add comments",
            "comment this",
            "dry run",
            "walk through this",
            "update the code",
            "modify the code",
            "only change",
            "smallest change",
        ],
    ) || (follow_up
        && contains_any(
            normalized,
            &[
                "code",
                "logic",
                "complexity",
                "optimize",
                "python",
                "java",
                "typescript",
                "rust",
            ],
        ))
}

fn looks_like_explanation_only_coding_question(normalized: &str) -> bool {
    let explanation_signal = contains_any(
        normalized,
        &[
            "explain",
            "why",
            "logic",
            "walk through",
            "walk me through",
            "how does",
            "how do",
            "how it works",
            "what is the idea",
            "core idea",
            "intuition",
            "dry run",
        ],
    );
    if !explanation_signal {
        return false;
    }

    !contains_any(
        normalized,
        &[
            "write code",
            "write a code",
            "give me code",
            "give code",
            "i want the code",
            "full code",
            "complete code",
            "can you write",
            "write ",
            "code for",
            "python code",
            "java code",
            "typescript code",
            "javascript code",
            "implementation",
            "implement",
            "build",
            "fix",
            "patch",
            "update the code",
            "modify the code",
            "convert this to",
            "translate this to",
            "add comments",
            "comment this",
            "test case",
            "unit test",
        ],
    )
}

fn looks_like_direct_technical_plan_question(normalized: &str) -> bool {
    let explicit_named_plan_request = contains_any(
        normalized,
        &[
            "design an evaluation plan",
            "design a test plan",
            "create an evaluation plan",
            "create a test plan",
            "propose an evaluation plan",
            "evaluation plan for",
            "test plan for",
        ],
    );
    let strategy_request = contains_any(
        normalized,
        &[
            "evaluation strategy",
            "test strategy",
            "metrics and launch gates",
        ],
    ) && contains_any(
        normalized,
        &[
            "design",
            "create",
            "propose",
            "develop",
            "build",
            "outline",
            "draft",
            "give me",
            "recommend",
            "would you use",
            "should we use",
            "what metrics",
            "how would",
            "how do",
            "how should",
            "how can",
            "what should",
        ],
    );
    let future_evaluation_request = contains_any(
        normalized,
        &[
            "how would you evaluate",
            "how do you evaluate",
            "how should you evaluate",
            "how can you evaluate",
            "how should we evaluate",
            "how can we evaluate",
            "how would you assess",
            "how do you assess",
            "how should you assess",
            "how can you assess",
            "how should we assess",
            "how can we assess",
            "how should i assess",
        ],
    ) && contains_any(
        normalized,
        &[
            "before production",
            "production launch",
            "launch readiness",
            "before launch",
        ],
    );
    let non_plan_request = contains_any(
        normalized,
        &[
            "summarize",
            "summary",
            "draft an email",
            "write an email",
            "meeting notes",
            "status update",
            "postmortem",
        ],
    );
    let evaluation_plan_request =
        (explicit_named_plan_request || strategy_request || future_evaluation_request)
            && !non_plan_request;
    let technical_target = contains_any(
        normalized,
        &[
            "rag",
            "retrieval",
            "assistant",
            "model",
            "system",
            "service",
            "api",
            "pipeline",
            "production",
            "launch",
        ],
    );

    evaluation_plan_request && technical_target
}

fn has_explicit_response_length(normalized: &str) -> bool {
    if contains_any(
        normalized,
        &[
            "one sentence",
            "single sentence",
            "two sentences",
            "three sentences",
            "30 second",
            "thirty second",
            "60 second",
            "sixty second",
            "short answer",
            "brief answer",
            "answer briefly",
            "in brief",
            "one paragraph",
            "two paragraphs",
            "three paragraphs",
            "one bullet",
            "two bullets",
            "three bullets",
            "bullet point",
            "bullet points",
            "numbered list",
            "in a table",
            "as a table",
        ],
    ) {
        return true;
    }

    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    tokens.windows(2).any(|pair| {
        pair[0].parse::<u32>().is_ok()
            && matches!(
                pair[1],
                "word"
                    | "words"
                    | "sentence"
                    | "sentences"
                    | "second"
                    | "seconds"
                    | "paragraph"
                    | "paragraphs"
                    | "bullet"
                    | "bullets"
            )
    })
}

fn looks_like_new_topic_request(normalized: &str) -> bool {
    contains_any(
        normalized,
        &[
            "new question",
            "different question",
            "separate question",
            "unrelated question",
            "ignore previous",
            "forget previous",
            "forget the above",
            "start fresh",
            "start over",
            "fresh question",
            "now answer this",
        ],
    )
}

fn is_generic_live_transcript_prompt(normalized: &str) -> bool {
    normalized.starts_with("answer the latest ")
        && normalized.contains("live captions from the current session transcript")
}

fn looks_like_transcript_placeholder(normalized: &str) -> bool {
    normalized.is_empty()
        || contains_any(
            normalized,
            &[
                "captions appear here",
                "live captions preview",
                "starting audio",
                "audio is live",
                "listening for follow-up",
                "listening for follow up",
                "no captions yet",
                "nothing was transcribed",
            ],
        )
}

fn looks_like_behavioral_question(normalized: &str) -> bool {
    let explicitly_personal_challenge = contains_any(
        normalized,
        &[
            "your biggest challenge",
            "biggest challenge you faced",
            "tell me about a challenge",
            "tell me about your challenge",
        ],
    );
    let explicitly_personal_conflict = contains_any(
        normalized,
        &[
            "conflict you faced",
            "conflict you handled",
            "tell me about a conflict",
            "tell me about your conflict",
        ],
    );
    contains_any(
        normalized,
        &[
            "tell me about yourself",
            "introduce yourself",
            "walk me through your background",
            "walk me through your resume",
            "why should we hire you",
            "why are you interested",
            "why this role",
            "your strengths",
            "your weakness",
            "leadership style",
            "behavioral",
        ],
    ) || explicitly_personal_challenge
        || explicitly_personal_conflict
        || looks_like_resume_intro_request(normalized)
        || looks_like_interview_story_question(normalized)
        || looks_like_interview_coaching_question(normalized)
}
