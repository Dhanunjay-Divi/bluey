
fn looks_like_interview_story_question(normalized: &str) -> bool {
    let leadership_scenario = contains_any(
        normalized,
        &[
            "both claim top priority",
            "different directors",
            "coach them without taking over",
            "coach a struggling engineer",
            "ownership beyond your assigned task",
        ],
    );

    looks_like_lived_interview_story_request(normalized) || leadership_scenario
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BehavioralStoryGrounding {
    NotRequired,
    Complete { provider_user: String },
    Missing { fields: Vec<&'static str> },
}

const BEHAVIORAL_STORY_FIELDS: [&str; 4] = ["Situation", "Task", "Action", "Result"];

fn looks_like_lived_interview_story_request(normalized: &str) -> bool {
    let explicit_past = contains_any(
        normalized,
        &[
            "tell me about a time",
            "tell us about a time",
            "tell me about a failure",
            "tell me about a mistake",
            "describe a time",
            "share a time",
            "share a story about a time",
            "give me a time when",
            "example from your experience",
        ],
    );
    let described_lived_situation = normalized.contains("describe a situation")
        && contains_any(
            normalized,
            &[
                "where you",
                "when you",
                "you faced",
                "you handled",
                "you had",
            ],
        );
    let walkthrough = contains_any(normalized, &["walk me through", "talk me through"]);
    let walkthrough_is_intro = contains_any(
        normalized,
        &[
            "your resume",
            "your background",
            "your experience",
            "about yourself",
        ],
    );
    let walkthrough_has_past_object = contains_any(
        normalized,
        &[
            "a time",
            "when you",
            "you built",
            "you owned",
            "you led",
            "you handled",
            "production issue",
            "outage",
            "incident",
            "project you",
            "system you",
            "pipeline you",
            "challenge you",
            "conflict you",
        ],
    );
    let have_you_ever_had_to = normalized.contains("have you ever had to")
        && contains_any(
            normalized,
            &[
                "had to persuade",
                "had to convince",
                "had to resolve",
                "had to handle",
                "had to recover",
                "had to lead",
                "had to own",
                "had to adapt",
                "had to improve",
                "had to make a difficult",
                "had to deal with",
                "had to challenge",
                "had to deliver difficult feedback",
                "had to terminate",
                "had to fire",
            ],
        );
    let personal_episode_frame = have_you_ever_had_to
        || (normalized.contains("have you ever")
            && contains_any(
                normalized,
                &[
                    "you ever led",
                    "you ever handled",
                    "you ever resolved",
                    "you ever faced",
                    "you ever owned",
                    "you ever failed",
                    "you ever made a mistake",
                    "production issue",
                    "outage",
                    "incident",
                    "conflict",
                    "challenge",
                ],
            ))
        || (contains_any(
            normalized,
            &[
                "describe an instance where",
                "describe an instance when",
                "describe an example where",
                "describe an example when",
            ],
        ) && contains_any(
            normalized,
            &[
                "where you",
                "when you",
                "you led",
                "you handled",
                "you resolved",
                "you faced",
                "you owned",
                "you persuaded",
                "you changed",
                "you improved",
            ],
        ));
    let unmistakably_past = explicit_past
        || described_lived_situation
        || personal_episode_frame
        || (walkthrough && walkthrough_has_past_object && !walkthrough_is_intro);
    let hypothetical = contains_any(
        normalized,
        &[
            "what would you do",
            "how would you",
            "what do you do",
            "how do you handle",
            "how do you deal with",
            "how do you approach",
            "how do you manage",
            "how do you resolve",
            "how do you coach",
            "suppose ",
            "imagine ",
            "if you were",
            "if requirements",
            "if a ",
        ],
    );
    if hypothetical && !unmistakably_past {
        return false;
    }

    let explicit_story = unmistakably_past
        || contains_any(
            normalized,
            &[
                "worked under pressure",
                "requirements were ambiguous",
                "challenged a decision",
                "disagreed with",
                "biggest challenge you faced",
                "your biggest challenge",
                "conflict you faced",
                "conflict you handled",
                "ownership beyond your assigned task",
                "production issue you",
                "outage you",
                "incident you",
                "project you",
                "system you",
                "pipeline you",
                "dashboard you",
                "rag system you",
            ],
        );
    let lived_example = contains_any(
        normalized,
        &[
            "give me an example",
            "give an example",
            "share an example",
            "share a real example",
        ],
    ) && contains_any(
        normalized,
        &[
            "ownership",
            "leadership",
            "failure",
            "mistake",
            "conflict",
            "disagreement",
            "your experience",
            "you led",
            "you owned",
            "you handled",
            "you built",
            "outage",
            "incident",
            "production issue",
            "project you",
            "system you",
            "pipeline you",
        ],
    );
    let past_work_walkthrough = walkthrough && walkthrough_has_past_object && !walkthrough_is_intro;
    let direct_past_work = (normalized.contains("tell me about")
        && contains_any(
            normalized,
            &[
                "production issue",
                "outage",
                "incident",
                "challenge",
                "conflict",
                "project you",
                "pipeline you",
                "dashboard you",
                "system you",
            ],
        ))
        || (normalized.contains("how did you")
            && contains_any(
                normalized,
                &[
                    "recover from a production",
                    "recover from the production",
                    "recover from an outage",
                    "recover from the outage",
                    "resolve a production issue",
                    "resolve the production issue",
                    "handle a production incident",
                    "handle the production incident",
                ],
            ))
        || (normalized.contains("what was your")
            && contains_any(
                normalized,
                &[
                    "hardest debugging incident",
                    "biggest failure",
                    "biggest mistake",
                    "biggest challenge",
                    "most difficult conflict",
                ],
            ));
    if explicit_story || lived_example || past_work_walkthrough || direct_past_work {
        return true;
    }

    normalized.contains("example from your experience")
}

fn looks_like_lived_interview_story_followup(normalized: &str) -> bool {
    let coaching_or_hypothetical_frame = contains_any(
        normalized,
        &[
            "what should i say",
            "how should i answer",
            "how do i answer",
            "if they ask",
            "if an interviewer",
            "if interviewer",
            "interviewer asks",
            "answer this like",
            "if i designed",
            "if i had designed",
            "if i built",
            "if i had built",
            "suppose ",
            "imagine ",
            "hypothetically",
        ],
    );
    if coaching_or_hypothetical_frame {
        return false;
    }

    (contains_any(
        normalized,
        &["why did you choose", "tradeoff did you accept"],
    ) && contains_any(normalized, &["architecture", "design", "tradeoff"]))
        || (contains_any(normalized, &["how did you prove", "how did you verify"])
            && contains_any(
                normalized,
                &[
                    "data was correct",
                    "data correctness",
                    "trusted it",
                    "downstream",
                ],
            ))
        || (contains_any(
            normalized,
            &["what did you put in place", "what did you implement"],
        ) && contains_any(
            normalized,
            &["that system", "rag system", "hallucinat", "project"],
        ))
}

fn looks_like_lived_interview_story_or_followup(normalized: &str) -> bool {
    looks_like_lived_interview_story_request(normalized)
        || looks_like_lived_interview_story_followup(normalized)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct LiveTranscriptTurn {
    question: String,
    user_response: String,
}

fn latest_typed_transcript_turn(req: &CompleteRequest) -> Option<LiveTranscriptTurn> {
    let mut latest = None;
    for context in req
        .context
        .iter()
        .filter(|context| context.kind == cue_core::AnswerContextKind::Transcript)
    {
        let mut current: Option<LiveTranscriptTurn> = None;
        let mut accepting_user_continuation = false;
        for line in context.content.lines() {
            let trimmed = line.trim();
            let lower = trimmed.to_ascii_lowercase();
            let question_prefix = if lower.starts_with("interviewer:") {
                Some("interviewer:")
            } else if lower.starts_with("system:") {
                Some("system:")
            } else {
                None
            };
            if let Some(prefix) = question_prefix {
                let fragment = trimmed[prefix.len()..].trim();
                if let Some(turn) = current.as_mut() {
                    if turn.user_response.is_empty() {
                        if !fragment.is_empty() {
                            if !turn.question.is_empty() {
                                turn.question.push(' ');
                            }
                            turn.question.push_str(fragment);
                        }
                        accepting_user_continuation = false;
                        continue;
                    }
                }
                if let Some(turn) = current.take() {
                    latest = Some(turn);
                }
                current = Some(LiveTranscriptTurn {
                    question: fragment.to_string(),
                    user_response: String::new(),
                });
                accepting_user_continuation = false;
                continue;
            }

            let user_prefix = if lower.starts_with("mic:") {
                Some("mic:")
            } else if lower.starts_with("user:") {
                Some("user:")
            } else {
                None
            };
            if let (Some(prefix), Some(turn)) = (user_prefix, current.as_mut()) {
                let value = trimmed[prefix.len()..].trim();
                if !value.is_empty() {
                    if !turn.user_response.is_empty() {
                        turn.user_response.push('\n');
                    }
                    turn.user_response.push_str(value);
                }
                accepting_user_continuation = true;
                continue;
            }

            if lower.starts_with("speaker:")
                || lower.starts_with("other:")
                || lower.starts_with("unknown:")
                || lower.starts_with("screen:")
                || lower.starts_with("assistant:")
            {
                accepting_user_continuation = false;
                continue;
            }

            if let Some(turn) = current.as_mut() {
                if !trimmed.is_empty()
                    && accepting_user_continuation
                    && !turn.user_response.is_empty()
                {
                    turn.user_response.push('\n');
                    turn.user_response.push_str(trimmed);
                }
            }
        }
        if let Some(turn) = current.take() {
            latest = Some(turn);
        }
    }
    latest.filter(|turn| !turn.question.trim().is_empty())
}

fn direct_question_confirms_story_ownership(question: &str) -> bool {
    contains_any(
        &normalize_guardrail_text(question),
        &[
            "my real example",
            "here are my facts",
            "candidate facts",
            "user confirmed story",
        ],
    )
}

fn confirmed_story_matches_question(question: &str, story: &str) -> bool {
    let question = format!(" {} ", normalize_guardrail_text(question));
    let story = format!(" {} ", normalize_guardrail_text(story));
    let contains_term = |text: &str, term: &str| text.contains(&format!(" {term} "));
    let contains_terms =
        |text: &str, terms: &[&str]| terms.iter().any(|term| contains_term(text, term));
    let categories: &[(&[&str], &[&str])] = &[
        (
            &["conflict", "disagreed", "disagreement", "stakeholder"],
            &[
                "conflict",
                "disagreed",
                "disagreement",
                "stakeholder",
                "stakeholders",
                "competing priority",
                "competing priorities",
                "alignment",
                "negotiated",
                "negotiation",
            ],
        ),
        (
            &["ownership", "owned", "beyond your assigned"],
            &[
                "owned",
                "ownership",
                "responsible",
                "responsibility",
                "accountable",
                "took over",
            ],
        ),
        (
            &[
                "failure",
                "failed",
                "mistake",
                "outage",
                "incident",
                "production issue",
            ],
            &[
                "failure",
                "failed",
                "mistake",
                "error",
                "outage",
                "incident",
                "production issue",
                "production failure",
                "recovered",
                "recovery",
                "rollback",
                "rolled back",
            ],
        ),
        (
            &["ambiguous", "requirements", "unclear"],
            &[
                "ambiguous",
                "requirement",
                "requirements",
                "unclear",
                "clarified",
                "clarification",
                "scope",
            ],
        ),
        (
            &["coach", "coached", "mentor", "mentored", "mentoring"],
            &[
                "coach",
                "coached",
                "mentor",
                "mentored",
                "mentoring",
                "feedback",
                "developed",
            ],
        ),
        (
            &["leadership", "led", "influence", "influenced"],
            &[
                "leadership",
                "led",
                "influence",
                "influenced",
                "aligned",
                "coordinated",
            ],
        ),
        (
            &["pressure", "deadline", "urgent"],
            &[
                "pressure",
                "deadline",
                "urgent",
                "time sensitive",
                "time critical",
            ],
        ),
        (
            &["customer", "client"],
            &["customer", "customers", "client", "clients"],
        ),
        (
            &["decision", "tradeoff", "tradeoffs", "trade off"],
            &[
                "decision",
                "decided",
                "tradeoff",
                "tradeoffs",
                "trade off",
                "chose",
            ],
        ),
        (
            &["rag", "retrieval", "machine learning", "ml", "ai"],
            &[
                "rag",
                "retrieval",
                "embedding",
                "embeddings",
                "vector database",
                "machine learning",
                "ml",
                "ai",
                "model",
                "models",
            ],
        ),
        (
            &["payment", "billing", "charge", "refund"],
            &[
                "payment", "payments", "billing", "charge", "charged", "refund", "refunded",
            ],
        ),
        (
            &["pipeline", "etl", "spark", "kafka", "data"],
            &[
                "pipeline",
                "pipelines",
                "etl",
                "spark",
                "kafka",
                "data",
                "dataset",
            ],
        ),
        (
            &["challenge", "adversity", "obstacle"],
            &[
                "challenge",
                "challenging",
                "adversity",
                "obstacle",
                "blocked",
                "constraint",
            ],
        ),
        (
            &[
                "persuade",
                "persuaded",
                "persuasion",
                "convince",
                "convinced",
            ],
            &[
                "persuade",
                "persuaded",
                "persuasion",
                "convince",
                "convinced",
                "influence",
                "influenced",
                "alignment",
                "aligned",
            ],
        ),
        (
            &["innovate", "innovated", "innovation", "creative"],
            &[
                "innovate",
                "innovated",
                "innovation",
                "creative",
                "invented",
                "prototype",
                "prototyped",
            ],
        ),
        (
            &["adapt", "adapted", "adaptation", "transition"],
            &["adapt", "adapted", "adaptation", "adjusted", "transition"],
        ),
        (
            &["quality", "defect", "defects"],
            &["quality", "defect", "defects", "validation"],
        ),
        (
            &["risk", "risky"],
            &[
                "risk",
                "risky",
                "mitigated",
                "mitigation",
                "experiment",
                "rollback",
            ],
        ),
        (
            &["cloud", "aws", "azure", "gcp"],
            &["cloud", "aws", "azure", "gcp"],
        ),
        (
            &["cost", "costs", "spend", "budget"],
            &["cost", "costs", "spend", "budget", "saved", "savings"],
        ),
    ];

    let mut matched_question_category = false;
    for (question_terms, story_terms) in categories {
        if contains_terms(&question, question_terms) {
            matched_question_category = true;
            if !contains_terms(&story, story_terms) {
                return false;
            }
        }
    }
    if matched_question_category {
        return true;
    }

    let normalized_question = question.trim();
    if matches!(
        normalized_question,
        "tell me about a time"
            | "tell us about a time"
            | "describe a time"
            | "share a time"
            | "give me a time"
    ) {
        return true;
    }

    const STORY_PROMPT_STOPWORDS: &[&str] = &[
        "about",
        "action",
        "achieved",
        "answer",
        "built",
        "candidate",
        "changed",
        "created",
        "delivered",
        "demonstrated",
        "describe",
        "designed",
        "developed",
        "example",
        "give",
        "have",
        "handled",
        "implemented",
        "improved",
        "interview",
        "managed",
        "owned",
        "process",
        "project",
        "reduced",
        "result",
        "share",
        "situation",
        "solved",
        "someone",
        "story",
        "task",
        "tell",
        "that",
        "this",
        "time",
        "when",
        "where",
        "which",
        "with",
        "worked",
        "your",
    ];
    const CONTROLLED_TOPIC_TERMS: &[&str] = &[
        "api",
        "billing",
        "cache",
        "caching",
        "customer",
        "database",
        "deployment",
        "incident",
        "kafka",
        "latency",
        "migration",
        "outage",
        "payment",
        "performance",
        "pipeline",
        "postgres",
        "privacy",
        "release",
        "reliability",
        "security",
        "spark",
        "sql",
        "stakeholder",
    ];
    if CONTROLLED_TOPIC_TERMS
        .iter()
        .any(|term| contains_term(&question, term) && contains_term(&story, term))
    {
        return true;
    }

    normalized_question
        .split_whitespace()
        .filter(|term| term.len() >= 5)
        .filter(|term| !STORY_PROMPT_STOPWORDS.contains(term))
        .filter(|term| contains_term(&story, term))
        .take(2)
        .count()
        >= 2
}

fn latest_legacy_interviewer_question(planning_context: &str) -> Option<String> {
    let mut current = String::new();
    let mut latest = String::new();
    let mut response_started = false;
    for line in planning_context.lines() {
        let trimmed = line.trim();
        let normalized = trimmed.to_ascii_lowercase();
        if let Some(prefix) = ["interviewer:", "system:"]
            .iter()
            .find_map(|prefix| normalized.starts_with(prefix).then_some(*prefix))
        {
            let fragment = trimmed[prefix.len()..].trim();
            if response_started {
                latest = std::mem::take(&mut current);
                response_started = false;
            }
            if !fragment.is_empty() {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(fragment);
            }
            continue;
        }
        if normalized.starts_with("mic:") || normalized.starts_with("user:") {
            response_started = true;
        }
    }
    if !current.is_empty() {
        latest = current;
    }
    (!latest.trim().is_empty()).then_some(latest)
}

fn request_has_prior_system_design_answer(req: &CompleteRequest) -> bool {
    extract_previous_system_design_answer(&req.user).is_some()
        || req.context.iter().any(|context| {
            context.kind == cue_core::AnswerContextKind::MeetingMemory
                && normalize_guardrail_text(&context.content).contains("previous bluey answer")
                && looks_like_system_design_question(&normalize_guardrail_text(&context.content))
        })
}

fn story_question_from_request(req: &CompleteRequest) -> Option<String> {
    let direct = extract_search_question(&req.user);
    let normalized_direct = normalize_guardrail_text(&direct);
    let direct_story = looks_like_lived_interview_story_request(&normalized_direct);
    let direct_followup = looks_like_lived_interview_story_followup(&normalized_direct);
    let inherited_system_design = request_has_prior_system_design_answer(req)
        && !direct_question_confirms_story_ownership(&direct);
    if direct_story || (direct_followup && !inherited_system_design) {
        return Some(direct.trim().to_string());
    }

    if !is_generic_live_transcript_prompt(&normalize_guardrail_text(&direct)) {
        return None;
    }

    latest_typed_transcript_turn(req)
        .filter(|turn| {
            let normalized = normalize_guardrail_text(&turn.question);
            looks_like_lived_interview_story_request(&normalized)
                || (looks_like_lived_interview_story_followup(&normalized)
                    && !inherited_system_design)
        })
        .map(|turn| turn.question)
        .or_else(|| {
            req.context.is_empty().then(|| {
                latest_legacy_interviewer_question(&extract_planning_context(&req.user)).filter(
                    |question| {
                        looks_like_lived_interview_story_or_followup(&normalize_guardrail_text(
                            question,
                        ))
                    },
                )
            })?
        })
}

fn split_labeled_context_blocks(context: &str) -> Vec<(String, String)> {
    let mut blocks = Vec::new();
    let mut label: Option<String> = None;
    let mut body = String::new();

    for line in context.lines() {
        let trimmed = line.trim();
        let bracket_label = trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.find(']').map(|end| rest[..end].trim().to_string()));
        if let Some(next_label) = bracket_label {
            let current_is_opaque = label
                .as_deref()
                .is_some_and(context_label_is_opaque_document);
            let next_is_outer_envelope = label.as_deref().is_some_and(|current| {
                context_label_is_ordered_outer_envelope(current, &next_label)
            });
            if current_is_opaque && !next_is_outer_envelope {
                if !body.is_empty() {
                    body.push('\n');
                }
                body.push_str(line);
                continue;
            }
            if let Some(previous_label) = label.take() {
                blocks.push((previous_label, body.trim().to_string()));
            }
            label = Some(next_label);
            body.clear();
        } else if label.is_some() {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(line);
        }
    }
    if let Some(previous_label) = label {
        blocks.push((previous_label, body.trim().to_string()));
    }
    blocks
}

fn context_label_is_opaque_document(label: &str) -> bool {
    context_label_source_rank(label).is_some()
}

fn context_label_source_rank(label: &str) -> Option<u8> {
    let normalized = normalize_guardrail_text(label);
    if candidate_history_label(label) {
        return Some(10);
    }
    if contains_any(
        &normalized,
        &["job description", "job posting", "role description"],
    ) {
        return Some(20);
    }
    if contains_any(
        &normalized,
        &[
            "interview preparation",
            "interview guide",
            "guide",
            "worksheet",
            "notes",
            "document",
            "file",
            "attachment",
            "template",
            "sample",
            "example",
            "reference",
            "resume",
            "profile",
            "work history",
            "assistant",
            "bluey answer",
            "rag",
            "memory",
        ],
    ) || normalized.contains(" from ")
        || [".pdf", ".docx", ".doc", ".txt", ".md", ".rtf"]
            .iter()
            .any(|extension| label.to_ascii_lowercase().contains(extension))
    {
        return Some(30);
    }
    if contains_any(&normalized, &["role target", "target role", "competency"]) {
        return Some(40);
    }
    contains_any(
        &normalized,
        &[
            "live transcript",
            "current transcript",
            "microphone",
            "retained conversation",
            "screen",
        ],
    )
    .then_some(50)
}

fn context_label_is_ordered_outer_envelope(current: &str, next: &str) -> bool {
    match (
        context_label_source_rank(current),
        context_label_source_rank(next),
    ) {
        (Some(current), Some(next)) => next > current,
        _ => false,
    }
}

fn authoritative_story_label(label: &str) -> bool {
    let normalized = normalize_guardrail_text(label);
    matches!(
        normalized.as_str(),
        "candidate story"
            | "verified story"
            | "user story"
            | "user provided story"
            | "my story"
            | "candidate draft"
            | "user draft"
    )
}

fn story_source_is_incomplete(text: &str) -> bool {
    let normalized = normalize_guardrail_text(text);
    contains_any(
        &normalized,
        &["compacted for", "truncated", "excerpt", "content omitted"],
    )
}

fn story_slot_has_concrete_evidence(field: &str, _value: &str, normalized: &str) -> bool {
    let padded = format!(" {normalized} ");
    let first_person = contains_any(&padded, &[" i ", " my ", " we ", " our "]);
    let words = normalized
        .split_whitespace()
        .filter(|word| word.chars().any(|ch| ch.is_alphanumeric()))
        .collect::<Vec<_>>();
    let generic_words = [
        "company",
        "project",
        "background",
        "context",
        "responsibility",
        "responsibilities",
        "role",
        "steps",
        "taken",
        "impact",
        "achieved",
        "outcome",
        "metrics",
        "details",
        "example",
    ];
    let only_generic_words = words
        .iter()
        .all(|word| generic_words.contains(&word.trim_matches(|ch: char| !ch.is_alphanumeric())));
    let minimum_words = if field == "result" { 2 } else { 3 };
    words.len() >= minimum_words
        && !only_generic_words
        && (!matches!(field, "task" | "action") || first_person)
}

fn story_slot_has_value(text: &str, field: &str) -> bool {
    let cleaned = text.replace(['*', '_', '#', '`'], "").replace("\r\n", "\n");
    let lower = cleaned.to_lowercase();
    let field = field.to_lowercase();
    let markers = [format!("{field}:"), format!("{field} -")];

    markers.iter().any(|marker| {
        lower.match_indices(marker).any(|(index, marker)| {
            let after = &cleaned[index + marker.len()..];
            let end = BEHAVIORAL_STORY_FIELDS
                .iter()
                .filter(|candidate| !candidate.eq_ignore_ascii_case(&field))
                .filter_map(|candidate| {
                    let candidate = candidate.to_lowercase();
                    [format!("{candidate}:"), format!("{candidate} -")]
                        .iter()
                        .filter_map(|next| after.to_lowercase().find(next))
                        .min()
                })
                .min()
                .unwrap_or(after.len());
            let value = after[..end].trim().trim_matches(|ch: char| {
                ch.is_whitespace() || matches!(ch, ',' | ';' | '-' | '\u{2022}')
            });
            let normalized_value = normalize_guardrail_text(value);
            let is_format_instruction = [
                "background",
                "background and context",
                "context",
                "your responsibility",
                "responsibility",
                "what you did",
                "actions taken",
                "action you took",
                "outcome",
                "outcome and metrics",
                "result and metrics",
                "metrics",
                "example",
                "details",
                "company project",
                "responsibilities for the role",
                "steps taken",
                "impact achieved",
            ]
            .contains(&normalized_value.as_str())
                || [
                    "describe ",
                    "explain ",
                    "summarize ",
                    "write ",
                    "provide ",
                    "insert ",
                    "state ",
                    "add ",
                    "include ",
                ]
                .iter()
                .any(|prefix| normalized_value.starts_with(prefix))
                || contains_any(
                    &normalized_value,
                    &[
                        "briefly describe",
                        "fill this",
                        "insert your",
                        "add your",
                        "describe the background",
                        "describe my responsibility",
                        "describe the steps",
                        "describe the outcome",
                    ],
                );
            value.chars().filter(|ch| ch.is_alphanumeric()).count() >= 4
                && !value.starts_with('[')
                && !value.starts_with('<')
                && !value.contains(['{', '}', '[', ']', '<', '>'])
                && !is_format_instruction
                && story_slot_has_concrete_evidence(&field, value, &normalized_value)
                && !contains_any(
                    &normalized_value,
                    &["not provided", "unknown", "n a", "to fill", "placeholder"],
                )
        })
    })
}

fn story_slots_in_authoritative_block(text: &str) -> [bool; 4] {
    std::array::from_fn(|index| story_slot_has_value(text, BEHAVIORAL_STORY_FIELDS[index]))
}

fn candidate_history_label(label: &str) -> bool {
    let normalized = normalize_guardrail_text(label);
    if contains_any(
        &normalized,
        &[
            "sample",
            "template",
            "example",
            "reference",
            "mock",
            "fictional",
            "not my",
        ],
    ) {
        return false;
    }
    normalized == "resume"
        || normalized.starts_with("resume from ")
        || normalized.starts_with("candidate resume")
        || normalized.starts_with("user resume")
        || normalized.starts_with("my resume")
        || normalized == "candidate profile"
        || normalized.starts_with("candidate profile from ")
        || normalized == "candidate background"
        || normalized.starts_with("candidate background from ")
        || normalized == "work history"
        || normalized.starts_with("candidate work history")
        || normalized.starts_with("my work history")
        || normalized == "professional history"
        || normalized.starts_with("candidate professional history")
        || normalized == "linkedin profile"
        || normalized.starts_with("candidate linkedin profile")
}

fn body_has_nested_story_heading(body: &str) -> bool {
    let normalized = normalize_guardrail_text(body);
    contains_any(
        &normalized,
        &[
            "candidate story",
            "my story",
            "user story",
            "sample answer",
            "example story",
            "star worksheet",
            "interview guide",
            "model answer",
        ],
    ) && body.lines().any(|line| line.trim_start().starts_with('['))
}

fn looks_like_untrusted_story_narrative(body: &str) -> bool {
    let normalized = format!(" {} ", normalize_guardrail_text(body));
    let words = normalized.split_whitespace().count();
    words >= 8 && contains_any(&normalized, &[" i ", " my "])
}

fn story_context_has_hazardous_unowned_story(context: &str) -> bool {
    split_labeled_context_blocks(context)
        .into_iter()
        .filter(|(label, _)| !authoritative_story_label(label))
        .any(|(label, body)| {
            if candidate_history_label(&label) {
                return body_has_nested_story_heading(&body);
            }
            let label = normalize_guardrail_text(&label);
            let style_only = contains_any(
                &label,
                &[
                    "job description",
                    "role target",
                    "target role",
                    "competency",
                ],
            );
            if style_only {
                return body_has_nested_story_heading(&body)
                    || story_slots_in_authoritative_block(&body)
                        .iter()
                        .any(|present| *present)
                    || looks_like_untrusted_story_narrative(&body);
            }
            story_source_is_incomplete(&body)
                || body_has_nested_story_heading(&body)
                || story_slots_in_authoritative_block(&body)
                    .iter()
                    .any(|present| *present)
                || looks_like_untrusted_story_narrative(&body)
        })
}

fn structured_story_style_context(req: &CompleteRequest) -> String {
    const COMPETENCIES: [(&str, &[&str]); 18] = [
        ("ownership", &["ownership", "accountability"]),
        ("leadership", &["leadership", "lead a team", "team lead"]),
        ("collaboration", &["collaboration", "cross functional"]),
        ("communication", &["communication", "communicate"]),
        ("customer focus", &["customer focus", "customer obsession"]),
        ("problem solving", &["problem solving", "analytical"]),
        ("reliability", &["reliability", "resilience"]),
        ("scalability", &["scalability", "scale"]),
        ("security", &["security", "secure"]),
        ("data engineering", &["data engineer", "data engineering"]),
        (
            "software engineering",
            &["software engineer", "software engineering"],
        ),
        (
            "backend engineering",
            &["backend engineer", "backend engineering"],
        ),
        (
            "frontend engineering",
            &["frontend engineer", "frontend engineering"],
        ),
        (
            "machine learning",
            &["machine learning", "machine learning engineer"],
        ),
        ("distributed systems", &["distributed system"]),
        ("cloud", &["cloud", "aws", "azure", "gcp"]),
        (
            "observability",
            &["observability", "monitoring", "telemetry"],
        ),
        ("data quality", &["data quality", "validation"]),
    ];

    let mut targets = Vec::new();
    for context in req
        .context
        .iter()
        .filter(|context| context.role == cue_core::AnswerContextRole::JobDescription)
    {
        let normalized = format!(" {} ", normalize_guardrail_text(&context.content));
        for (label, terms) in COMPETENCIES {
            if terms
                .iter()
                .any(|term| normalized.contains(&format!(" {term} ")))
                && !targets.contains(&label)
            {
                targets.push(label);
            }
        }
    }
    if targets.is_empty() {
        String::new()
    } else {
        format!(
            "[Job competency targets; fixed vocabulary only]\n{}",
            targets
                .into_iter()
                .map(|target| format!("- {target}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

fn structured_candidate_history_context(req: &CompleteRequest) -> String {
    req.context
        .iter()
        .filter(|context| context.role == cue_core::AnswerContextRole::CandidateResume)
        .filter(|context| !context.content.trim().is_empty())
        .filter(|context| !body_has_nested_story_heading(&context.content))
        .map(|context| {
            format!(
                "[Candidate-provided resume evidence]\n{}",
                truncate_chars(context.content.trim(), 8_000)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn structured_context_has_hazardous_unowned_story(req: &CompleteRequest) -> bool {
    req.context.iter().any(|context| {
        if context.role == cue_core::AnswerContextRole::UserConfirmedStory {
            return false;
        }
        if context.role == cue_core::AnswerContextRole::CandidateResume {
            return body_has_nested_story_heading(&context.content);
        }
        let story_shaped = story_slots_in_authoritative_block(&context.content)
            .iter()
            .any(|present| *present)
            || looks_like_untrusted_story_narrative(&context.content)
            || body_has_nested_story_heading(&context.content);
        match context.role {
            cue_core::AnswerContextRole::JobDescription => story_shaped,
            cue_core::AnswerContextRole::InterviewPreparation => {
                story_shaped || story_source_is_incomplete(&context.content)
            }
            cue_core::AnswerContextRole::Other => {
                context.kind != cue_core::AnswerContextKind::Transcript && story_shaped
            }
            cue_core::AnswerContextRole::CandidateResume
            | cue_core::AnswerContextRole::UserConfirmedStory => false,
        }
    })
}

fn behavioral_story_grounding(
    req: &CompleteRequest,
    plan: &AnswerPlan,
) -> BehavioralStoryGrounding {
    if !uses_typed_answer_context_v1(req) {
        return BehavioralStoryGrounding::NotRequired;
    }
    if !matches!(
        plan.intent,
        AnswerIntent::Behavioral | AnswerIntent::FollowUp | AnswerIntent::General
    ) {
        return BehavioralStoryGrounding::NotRequired;
    }
    let Some(question) = story_question_from_request(req) else {
        return BehavioralStoryGrounding::NotRequired;
    };

    let planning_context = extract_planning_context(&req.user);
    let has_structured_context = !req.context.is_empty();
    let style_context = if has_structured_context {
        structured_story_style_context(req)
    } else {
        String::new()
    };
    let mut sources = Vec::new();
    let direct_question = extract_search_question(&req.user);
    if looks_like_lived_interview_story_or_followup(&normalize_guardrail_text(&direct_question))
        && direct_question_confirms_story_ownership(&direct_question)
    {
        sources.push((direct_question, true));
    }
    if has_structured_context {
        sources.extend(
            req.context
                .iter()
                .filter(|context| context.role == cue_core::AnswerContextRole::UserConfirmedStory)
                .filter(|context| confirmed_story_matches_question(&question, &context.content))
                .map(|context| (context.content.clone(), false)),
        );
        if let Some(turn) = latest_typed_transcript_turn(req) {
            if turn.question.trim() == question.trim() && !turn.user_response.trim().is_empty() {
                sources.push((turn.user_response, false));
            }
        }
    }

    let mut best_slots = [false; 4];
    for (source, source_is_question) in sources {
        if story_source_is_incomplete(&source) {
            continue;
        }
        let slots = story_slots_in_authoritative_block(&source);
        if slots.iter().all(|present| *present) {
            let mut provider_user = if source_is_question {
                format!("Question:\n{}", question.trim())
            } else {
                format!(
                    "Question:\n{}\n\nSession context:\n[Verified user-provided story]\n{}",
                    question.trim(),
                    source.trim()
                )
            };
            if !style_context.is_empty() {
                provider_user.push_str("\n\n");
                provider_user.push_str(&style_context);
            }
            return BehavioralStoryGrounding::Complete { provider_user };
        }
        if slots.iter().filter(|present| **present).count()
            > best_slots.iter().filter(|present| **present).count()
        {
            best_slots = slots;
        }
    }

    let has_partial_confirmed_story = best_slots.iter().any(|present| *present);
    let hazardous_unowned_story = if has_structured_context {
        structured_context_has_hazardous_unowned_story(req)
    } else {
        story_context_has_hazardous_unowned_story(&planning_context)
    };
    if has_partial_confirmed_story {
        return BehavioralStoryGrounding::Missing {
            fields: BEHAVIORAL_STORY_FIELDS
                .iter()
                .enumerate()
                .filter_map(|(index, field)| (!best_slots[index]).then_some(*field))
                .collect(),
        };
    }

    if hazardous_unowned_story {
        return BehavioralStoryGrounding::Missing {
            fields: BEHAVIORAL_STORY_FIELDS.to_vec(),
        };
    }

    BehavioralStoryGrounding::Missing {
        fields: BEHAVIORAL_STORY_FIELDS.to_vec(),
    }
}

fn behavioral_provider_user(
    req: &CompleteRequest,
    plan: &AnswerPlan,
    grounding: &BehavioralStoryGrounding,
) -> Option<String> {
    if !uses_typed_answer_context_v1(req) {
        return None;
    }
    match grounding {
        BehavioralStoryGrounding::Complete { provider_user } => {
            return Some(provider_user.clone());
        }
        BehavioralStoryGrounding::Missing { .. } => return None,
        BehavioralStoryGrounding::NotRequired => {}
    }
    if plan.intent != AnswerIntent::Behavioral {
        return None;
    }
    if req.context.is_empty() {
        return None;
    }

    let direct_question = extract_search_question(&req.user);
    let normalized_direct = normalize_guardrail_text(&direct_question);
    let transcript_turn = is_generic_live_transcript_prompt(&normalized_direct)
        .then(|| latest_typed_transcript_turn(req))
        .flatten();
    let question = transcript_turn
        .as_ref()
        .map(|turn| turn.question.trim())
        .filter(|question| !question.is_empty())
        .unwrap_or_else(|| direct_question.trim());
    let mut provider_user = format!("Question:\n{question}");

    if let Some(turn) = transcript_turn {
        if !turn.user_response.trim().is_empty() {
            provider_user.push_str("\n\nCurrent live transcript response:\n");
            provider_user.push_str(turn.user_response.trim());
        }
    }

    let candidate_context = structured_candidate_history_context(req);
    if !candidate_context.is_empty() {
        provider_user.push_str("\n\n");
        provider_user.push_str(&candidate_context);
    }
    for story in req
        .context
        .iter()
        .filter(|context| context.role == cue_core::AnswerContextRole::UserConfirmedStory)
        .filter(|context| !context.content.trim().is_empty())
    {
        provider_user.push_str("\n\n[User-confirmed background]\n");
        provider_user.push_str(&truncate_chars(story.content.trim(), 8_000));
    }
    let style_context = structured_story_style_context(req);
    if !style_context.is_empty() {
        provider_user.push_str("\n\n");
        provider_user.push_str(&style_context);
    }

    Some(provider_user)
}

fn behavioral_story_truth_gap_text(missing_fields: &[&str]) -> String {
    let missing = if missing_fields.is_empty() {
        "one complete user-confirmed story".to_string()
    } else {
        missing_fields.join(", ")
    };
    format!(
        "I don’t have one complete, user-confirmed story I can safely put in your voice yet. I’m missing these facts from one story: {missing}. Send explicit Situation, Task, Action, and Result fields; a qualitative result is fine.\n\nLive bridge: I want to choose a real example and keep the details accurate, so I’d like a moment to structure it.\n\nFill-in template (not a factual answer):\nSituation: [company/project], [situation]\nTask: [task]\nAction: [actions]\nResult: [verified outcome]"
    )
}

fn complete_grounding_guard_response(
    pool: &crate::db::DbPool,
    account: &Account,
    request_id: &str,
    missing_fields: &[&str],
) -> Result<CompleteResponse, Box<(StatusCode, Json<ApiError>)>> {
    let live_account = Account::fetch_by_id(pool, &account.id)
        .map_err(|error| {
            let _ = idempotency::release(pool, &account.id, request_id);
            tracing::error!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                request_id,
                error = %error,
                "behavioral grounding response could not refresh account state"
            );
            Box::new((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Bluey could not verify the account for this response.".into(),
                    reason: Some("grounding_account_refresh_failed".into()),
                    ..Default::default()
                }),
            ))
        })?
        .ok_or_else(|| {
            let _ = idempotency::release(pool, &account.id, request_id);
            Box::new((
                StatusCode::UNAUTHORIZED,
                Json(ApiError {
                    error: "Account is no longer available.".into(),
                    reason: Some("account_not_found".into()),
                    ..Default::default()
                }),
            ))
        })?;
    let response = CompleteResponse {
        text: behavioral_story_truth_gap_text(missing_fields),
        provider: "bluey".into(),
        model: "grounding-guard-v1".into(),
        input_tokens: 0,
        output_tokens: 0,
        cost_cents: 0,
        balance_cents_after: live_account.balance_cents,
        trial_seconds_remaining: live_account.trial_seconds_remaining,
        artifact_type: Some("needs_story_facts".into()),
        artifact_body: Some(
            serde_json::json!({
                "state": "needs_story_facts",
                "required_fields": missing_fields,
                "all_fields": BEHAVIORAL_STORY_FIELDS,
            })
            .to_string(),
        ),
        cost_label: Some(router_cost_label(0, live_account.balance_cents)),
        confidence: None,
        sources: Vec::new(),
    };
    let json = serde_json::to_string(&response).map_err(|error| {
        let _ = idempotency::release(pool, &account.id, request_id);
        tracing::error!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            request_id,
            error = %error,
            "behavioral grounding response could not be serialized"
        );
        Box::new((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Bluey could not persist the grounded response.".into(),
                reason: Some("grounding_response_persistence_failed".into()),
                ..Default::default()
            }),
        ))
    })?;
    if let Err(error) = idempotency::mark_complete(pool, &account.id, request_id, &json) {
        let _ = idempotency::release(pool, &account.id, request_id);
        tracing::error!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            request_id,
            error = %error,
            "behavioral grounding response could not be cached"
        );
        return Err(Box::new((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Bluey could not persist the grounded response.".into(),
                reason: Some("grounding_response_persistence_failed".into()),
                ..Default::default()
            }),
        )));
    }
    Ok(response)
}

fn looks_like_resume_intro_request(normalized: &str) -> bool {
    let intro_signal = contains_any(
        normalized,
        &[
            "introduction",
            "intro",
            "introduce",
            "self introduction",
            "about myself",
            "about yourself",
            "about you",
        ],
    );
    let resume_signal = contains_any(
        normalized,
        &[
            "resume",
            "résumé",
            "background",
            "profile",
            "experience",
            "attached document",
            "attached file",
            "based on the document",
            "based on this document",
        ],
    );

    intro_signal && resume_signal
}

fn looks_like_interview_answer_context(normalized: &str, normalized_context: &str) -> bool {
    looks_like_interview_coaching_question(normalized)
        || contains_any(
            normalized,
            &[
                "interview",
                "interviewer",
                "interviewing",
                "candidate",
                "tell me about yourself",
                "resume",
                "résumé",
                "job description",
                " jd",
                "goldman",
                "amazon",
                "caterpillar",
                "may mobility",
                "onsite",
                "phone screen",
                "hiring manager",
                "behavioral",
                "star answer",
            ],
        )
        || contains_any(
            normalized_context,
            &[
                "resume",
                "résumé",
                "job description",
                " jd",
                "interview",
                "interviewer",
                "candidate",
                "role requirements",
                "preferred qualifications",
            ],
        )
}

fn looks_like_interview_coaching_question(normalized: &str) -> bool {
    let coaching_frame = contains_any(
        normalized,
        &[
            "what should i say",
            "how should i answer",
            "how do i answer",
            "answer this like",
            "if they ask",
            "if interviewer",
            "interviewer asks",
            "interviewer asked",
            "interviewer pushes",
            "interviewer push",
            "they ask me",
            "asked in interview",
        ],
    );
    let interview_frame = coaching_frame
        || contains_any(
            normalized,
            &[
                "interview",
                "interviewer",
                "amazon",
                "caterpillar",
                "may mobility",
                "leadership principle",
                "dive deep",
                "star answer",
            ],
        );
    let role_domain = contains_any(
        normalized,
        &[
            "software engineer",
            "sde",
            "developer",
            "backend",
            "frontend",
            "full stack",
            "full-stack",
            "api",
            "microservice",
            "distributed system",
            "system design",
            "data engineer",
            "data engineering",
            "business intelligence",
            "bie",
            "data analyst",
            "data scientist",
            "machine learning",
            "ai/ml",
            "ai engineer",
            "autonomy",
            "perception",
            "robot",
            "robotics",
            "object detection",
            "semantic segmentation",
            "instance segmentation",
            "localization",
            "sensor calibration",
            "llm",
            "rag",
            "retrieval",
            "embedding",
            "vector db",
            "vector database",
            "agent",
            "multi-agent",
            "mcp",
            "bedrock",
            "langsmith",
            "chunking",
            "hallucination",
            "grounding",
            "evaluation framework",
            "ml engineer",
            "devops",
            "platform",
            "cloud",
            "security",
            "cybersecurity",
            "product manager",
            "program manager",
            "engineering manager",
            "software engineering manager",
            "people manager",
            "technical manager",
            "team lead",
            "tech lead",
            "project manager",
            "director",
            "senior manager",
            "dashboard",
            "tableau",
            "power bi",
            "sql",
            "redshift",
            "snowflake",
            "spark",
            "airflow",
            "kafka",
            "dbt",
            "python",
            "java",
            "react",
            "node",
            "aws",
            "azure",
            "etl",
            "pipeline",
            "metric",
            "kpi",
            "data quality",
            "data availability",
            "reconciliation",
            "row count",
            "upstream",
            "reporting",
        ],
    );
    let story_prompt = looks_like_interview_story_question(normalized)
        || contains_any(
            normalized,
            &[
                "can you talk about a project",
                "talk about a project",
                "project that you built",
                "technical project",
                "most challenging project",
                "complex project",
                "production issue",
                "debug a production",
                "debugged a production",
                "incident",
                "outage",
                "stakeholder",
                "prioritize",
                "can you talk about a dashboard",
                "talk about a dashboard",
                "dashboard that you built",
                "can you talk about a pipeline",
                "pipeline that you built",
                "built from scratch",
                "what was the business problem",
                "what metrics",
                "what visual",
                "favorite sql function",
                "favorite programming language",
                "favorite design pattern",
                "how did you evaluate",
                "how you evaluate",
                "evaluation metric",
                "handle authentication",
                "handle authorization",
                "chunking strategy",
                "embedding model",
                "rag pipeline",
                "mcp server",
                "multi-agent",
                "agent orchestration",
                "solve a problem that required in-depth thought",
                "focusing on the right problem",
                "how did you know that you were focusing",
                "tableau filters",
                "backend lag",
                "backend query",
                "backend refresh",
                "refresh lag",
                "dashboard refresh",
                "source table",
                "source tables",
            ],
        );
    let direct_code_or_design = (looks_like_coding_question(normalized)
        || looks_like_system_design_question(normalized))
        && !coaching_frame
        && !story_prompt;

    !direct_code_or_design && ((interview_frame && role_domain) || story_prompt)
}
