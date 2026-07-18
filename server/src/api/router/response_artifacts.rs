use super::{
    looks_like_internal_disclosure_leak, truncate_chars, web_search_usage_label, AnswerIntent,
    AnswerOutput, AnswerPlan, WebSearchOutcome,
};

pub(super) struct ResponseArtifact {
    pub(super) artifact_type: &'static str,
    pub(super) body: String,
    pub(super) confidence: f32,
}

pub(super) fn response_artifact_for_plan(
    text: &str,
    plan: &AnswerPlan,
) -> Option<ResponseArtifact> {
    if plan.intent == AnswerIntent::SystemDesign && plan.output == AnswerOutput::CanvasDetail {
        let body = text.trim();
        if body.is_empty() || looks_like_internal_disclosure_leak(body) {
            return None;
        }
        let lower = body.to_lowercase();
        if looks_like_diagram_artifact(body, &lower) {
            return Some(ResponseArtifact {
                artifact_type: "diagram",
                body: format_structured_artifact(body, "Diagram"),
                confidence: 0.90,
            });
        }
        return Some(ResponseArtifact {
            artifact_type: "system_design",
            body: format_structured_artifact(body, "System Design"),
            confidence: 0.92,
        });
    }
    response_artifact_for_output(text, plan.output)
}

pub(super) fn response_artifact_for_output(
    text: &str,
    output: AnswerOutput,
) -> Option<ResponseArtifact> {
    match output {
        AnswerOutput::CodeArtifact => response_artifact(text),
        AnswerOutput::CanvasDetail => response_canvas_detail_artifact(text),
        AnswerOutput::Compact | AnswerOutput::SourceAnswer | AnswerOutput::InterviewAnswer => None,
    }
}

pub(super) fn visible_response_text_for_artifact(
    text: &str,
    artifact: Option<&ResponseArtifact>,
) -> String {
    let clean = text.trim();
    let Some(artifact) = artifact else {
        return clean.to_string();
    };
    if artifact.artifact_type == "system_design" {
        return system_design_spoken_answer(clean);
    }
    if artifact.artifact_type != "code" {
        return clean.to_string();
    }

    let visible = strip_fenced_code(clean);
    let visible = strip_canvas_pointer_lines(&visible);
    let visible = visible.trim();
    if visible.is_empty() || code_answer_is_pointer_only(visible) {
        return "I found the implementation shape and prepared the complete code artifact."
            .to_string();
    }
    visible.to_string()
}

pub(super) fn visible_response_text_for_plan(
    text: &str,
    artifact: Option<&ResponseArtifact>,
    plan: &AnswerPlan,
) -> String {
    // Streaming code answers are shown in full as deltas. Persist the same
    // canonical text in the terminal/billing event so retries, audit logs, and
    // non-streaming clients never receive a different prose-only answer shape.
    if plan.output == AnswerOutput::CodeArtifact {
        return text.trim().to_string();
    }
    if plan.output == AnswerOutput::CanvasDetail {
        // Canvas streaming always exposes only the spoken overlay. Apply the
        // same projection to terminal/refusal responses even when the safety
        // guard intentionally suppresses artifact creation.
        return canvas_overlay_text(text.trim());
    }
    visible_response_text_for_artifact(text, artifact)
}

pub(super) fn is_spoken_answer_heading(line: &str) -> bool {
    line.trim()
        .trim_start_matches('#')
        .trim()
        .trim_end_matches([':', '-', '\u{2013}', '\u{2014}'])
        .trim()
        .eq_ignore_ascii_case("spoken answer")
}

fn canvas_detail_heading_name(line: &str) -> String {
    line.trim()
        .trim_start_matches('#')
        .trim()
        .trim_matches(['*', '_', '`'])
        .trim()
        .trim_end_matches([':', '-', '\u{2013}', '\u{2014}'])
        .trim()
        .to_ascii_lowercase()
}

pub(super) fn is_canvas_detail_heading(line: &str) -> bool {
    if line.trim_start().starts_with('#') && !is_spoken_answer_heading(line) {
        return true;
    }
    matches!(
        canvas_detail_heading_name(line).as_str(),
        "canvas"
            | "canvas detail"
            | "diagram"
            | "architecture"
            | "components"
            | "data flow"
            | "requirements"
            | "storage"
            | "scaling"
            | "tradeoffs"
            | "failure modes"
    )
}

pub(super) fn could_be_canvas_detail_heading_prefix(fragment: &str) -> bool {
    let fragment = fragment
        .trim_start()
        .trim_start_matches('#')
        .trim_start()
        .trim_start_matches(['*', '_', '`'])
        .to_ascii_lowercase();
    if fragment.is_empty() {
        return true;
    }
    [
        "canvas",
        "canvas detail",
        "diagram",
        "architecture",
        "components",
        "data flow",
        "requirements",
        "storage",
        "scaling",
        "tradeoffs",
        "failure modes",
    ]
    .iter()
    .any(|heading| heading.starts_with(fragment.trim_end_matches([':', '-', ' '])))
}

fn explicit_system_design_spoken_answer(text: &str) -> Option<String> {
    let lines = text.lines().collect::<Vec<_>>();
    if let Some(start) = lines.iter().position(|line| is_spoken_answer_heading(line)) {
        let spoken = lines[start + 1..]
            .iter()
            .take_while(|line| !is_canvas_detail_heading(line))
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        let spoken = spoken.trim();
        if !spoken.is_empty() {
            return Some(spoken.to_string());
        }
    }

    None
}

pub(super) fn canvas_overlay_text(text: &str) -> String {
    if let Some(spoken) = explicit_system_design_spoken_answer(text) {
        return spoken;
    }

    let mut prose = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if is_spoken_answer_heading(trimmed) {
            continue;
        }
        if is_canvas_detail_heading(trimmed) || trimmed.starts_with("```") {
            break;
        }
        if trimmed.is_empty() {
            if !prose.is_empty() {
                break;
            }
            continue;
        }
        prose.push(trimmed);
    }
    let prose = truncate_chars(&prose.join(" "), 700);
    if prose.trim().is_empty() {
        "I prepared the complete system design in the workbench.".to_string()
    } else {
        prose
    }
}

fn system_design_spoken_answer(text: &str) -> String {
    canvas_overlay_text(text)
}

fn strip_canvas_pointer_lines(text: &str) -> String {
    text.lines()
        .filter(|line| {
            let lower = line.trim().to_ascii_lowercase();
            !(lower.contains("is in the canvas")
                || lower.contains("in the canvas")
                || lower.contains("code panel")
                || lower.contains("right panel")
                || lower.contains("workbench"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn code_answer_is_pointer_only(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    let references_missing_context = lower.contains("already")
        || lower.contains("above")
        || lower.contains("earlier")
        || lower.contains("same code")
        || lower.contains("shown")
        || lower.contains("prepared")
        || lower.contains("complete code");
    references_missing_context
        && lower.chars().count() < 220
        && lower.contains("code")
        && !lower.contains("approach")
        && !lower.contains("complexity")
        && !lower.contains("def ")
        && !lower.contains("class ")
        && !lower.contains("return ")
        && !lower.contains("for ")
        && !lower.contains("while ")
}

fn response_canvas_detail_artifact(text: &str) -> Option<ResponseArtifact> {
    let body = text.trim();
    if body.is_empty() || looks_like_internal_disclosure_leak(body) {
        return None;
    }

    let lower = body.to_lowercase();
    let fenced_code = extract_fenced_code(body);
    if looks_like_diagram_artifact(body, &lower) {
        return Some(ResponseArtifact {
            artifact_type: "diagram",
            body: format_structured_artifact(body, "Diagram"),
            confidence: 0.88,
        });
    }
    // Canvas-detail answers are primarily design/screen artifacts. SQL,
    // schema, JSON, pseudocode, and fenced text are supporting material, not
    // evidence that the whole design should become a code artifact.
    if looks_like_system_design_artifact(body, &lower) {
        return Some(ResponseArtifact {
            artifact_type: "system_design",
            body: format_structured_artifact(body, "System Design"),
            confidence: 0.88,
        });
    }
    if !fenced_code.blocks.is_empty() {
        let artifact_body = format_code_artifact(body, &fenced_code);
        if code_artifact_has_complete_code(&artifact_body) {
            return Some(ResponseArtifact {
                artifact_type: "code",
                body: artifact_body,
                confidence: 0.94,
            });
        }
    }
    if lower.contains("screenshot")
        || lower.contains("screen context")
        || lower.contains("analyse screen")
        || lower.contains("analyze screen")
        || lower.contains("image shows")
    {
        return Some(ResponseArtifact {
            artifact_type: "screen",
            body: format_structured_artifact(body, "Screen Context"),
            confidence: 0.86,
        });
    }
    if lower.contains("attached document")
        || lower.contains("pdf")
        || lower.contains("resume")
        || lower.contains("document context")
    {
        return Some(ResponseArtifact {
            artifact_type: "document",
            body: format_structured_artifact(body, "Document Context"),
            confidence: 0.78,
        });
    }
    if body.chars().count() > 700 && has_structured_shape(body) {
        return Some(ResponseArtifact {
            artifact_type: "structured",
            body: format_structured_artifact(body, "Details"),
            confidence: 0.70,
        });
    }

    None
}

pub(super) fn response_artifact(text: &str) -> Option<ResponseArtifact> {
    let body = text.trim();
    if body.is_empty() {
        return None;
    }
    if looks_like_internal_disclosure_leak(body) {
        return None;
    }

    let lower = body.to_lowercase();
    let fenced_code = extract_fenced_code(body);
    if looks_like_diagram_artifact(body, &lower) {
        return Some(ResponseArtifact {
            artifact_type: "diagram",
            body: format_structured_artifact(body, "Diagram"),
            confidence: 0.88,
        });
    }
    if !fenced_code.blocks.is_empty() {
        let artifact_body = format_code_artifact(body, &fenced_code);
        if !code_artifact_has_complete_code(&artifact_body) {
            return None;
        }
        return Some(ResponseArtifact {
            artifact_type: "code",
            body: artifact_body,
            confidence: 0.95,
        });
    }
    if looks_like_system_design_artifact(body, &lower) {
        return Some(ResponseArtifact {
            artifact_type: "system_design",
            body: format_structured_artifact(body, "System Design"),
            confidence: 0.88,
        });
    }
    if lower.contains("screenshot")
        || lower.contains("screen context")
        || lower.contains("analyse screen")
        || lower.contains("analyze screen")
        || lower.contains("image shows")
    {
        return Some(ResponseArtifact {
            artifact_type: "screen",
            body: format_structured_artifact(body, "Screen Context"),
            confidence: 0.86,
        });
    }
    if lower.contains("attached document")
        || lower.contains("pdf")
        || lower.contains("resume")
        || lower.contains("document context")
    {
        return Some(ResponseArtifact {
            artifact_type: "document",
            body: format_structured_artifact(body, "Document Context"),
            confidence: 0.78,
        });
    }
    if body.chars().count() > 950 && has_structured_shape(body) {
        return Some(ResponseArtifact {
            artifact_type: "structured",
            body: format_structured_artifact(body, "Details"),
            confidence: 0.70,
        });
    }

    None
}

fn looks_like_diagram_artifact(body: &str, lower: &str) -> bool {
    lower.contains("```mermaid")
        || lower.contains("flowchart td")
        || lower.contains("flowchart lr")
        || lower.contains("graph td")
        || lower.contains("graph lr")
        || lower.contains("sequencediagram")
        || ((lower.contains("diagram")
            || lower.contains("flowchart")
            || lower.contains("pictorial representation")
            || lower.contains("visual representation"))
            && (body.contains("-->")
                || body.contains("->")
                || body.contains("+---")
                || body.contains("|--")
                || body.contains("[")
                || body.contains("]")))
}

pub(super) fn router_cost_label(cost_cents: i64, balance_cents_after: i64) -> String {
    format!(
        "${:.2} · balance ${:.2}",
        cost_cents as f64 / 100.0,
        balance_cents_after as f64 / 100.0
    )
}

pub(super) fn router_cost_label_with_web_search(
    cost_cents: i64,
    balance_cents_after: i64,
    web_search: &WebSearchOutcome,
) -> String {
    let base = router_cost_label(cost_cents, balance_cents_after);
    if web_search.searches_used <= 0 {
        return base;
    }
    format!(
        "{base} · {}",
        web_search_usage_label(web_search.searches_used, web_search.sources.len())
    )
}

fn keyword_count(text: &str, keywords: &[&str]) -> usize {
    keywords
        .iter()
        .filter(|keyword| text.contains(**keyword))
        .count()
}

fn looks_like_system_design_artifact(body: &str, lower: &str) -> bool {
    if looks_like_interview_profile_answer(lower) {
        return false;
    }

    let signal_count = keyword_count(
        lower,
        &[
            "system design",
            "architecture",
            "api",
            "database",
            "cache",
            "queue",
            "scale",
            "latency",
            "throughput",
            "tradeoff",
            "shard",
            "load balancer",
            "microservice",
            "event-driven",
        ],
    );
    if signal_count < 3 {
        return false;
    }

    lower.contains("system design")
        || lower.contains("design a ")
        || lower.contains("design an ")
        || lower.contains("architect a ")
        || lower.contains("high-level architecture")
        || has_structured_shape(body)
}

fn looks_like_interview_profile_answer(lower: &str) -> bool {
    if lower.contains("tell me about yourself") || lower.contains("tell me about myself") {
        return true;
    }

    let profile_signals = keyword_count(
        lower,
        &[
            "i'm ",
            "i am ",
            "i've ",
            "i’ve ",
            "i was at ",
            "before that i",
            "where i worked",
            "what drew me",
            "this role",
            "my background",
            "my experience",
            "senior software engineer",
            "master's",
            "masters",
        ],
    );
    let behavioral_signals = keyword_count(
        lower,
        &[
            "tell me about a time",
            "describe a time",
            "give me an example",
            "situation",
            "task",
            "action",
            "result",
            "stakeholder",
            "conflict",
        ],
    );

    profile_signals >= 3 || behavioral_signals >= 4
}

pub(super) fn has_code_shape(lower: &str) -> bool {
    keyword_count(
        lower,
        &[
            "class solution",
            "def ",
            "function ",
            "const ",
            "let ",
            "public ",
            "private ",
            "time complexity",
            "space complexity",
            "test case",
            "edge case",
            "sql",
        ],
    ) >= 2
}

#[derive(Default)]
struct FencedCode {
    blocks: Vec<String>,
    recovered_notes: Vec<String>,
}

fn extract_fenced_code(text: &str) -> FencedCode {
    let mut extracted = FencedCode::default();
    let mut current = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        if !in_fence {
            let Some((_before, after_fence)) = line.split_once("```") else {
                continue;
            };
            if let Some(inline_code) = inline_code_after_fence_tail(after_fence) {
                let (inline_code, closes_inline) =
                    inline_code.split_once("```").unwrap_or((inline_code, ""));
                if !inline_code.trim().is_empty() {
                    current.push(inline_code.to_string());
                }
                if !closes_inline.is_empty() || after_fence.matches("```").count() > 0 {
                    push_fenced_code_block(&mut extracted, &current);
                    current.clear();
                    in_fence = false;
                    continue;
                }
            }
            in_fence = true;
            continue;
        }

        if let Some((before, _after)) = line.split_once("```") {
            if !before.trim().is_empty() {
                current.push(before.to_string());
            }
            push_fenced_code_block(&mut extracted, &current);
            current.clear();
            in_fence = false;
        } else {
            current.push(line.to_string());
        }
    }
    extracted
}

fn push_fenced_code_block(extracted: &mut FencedCode, lines: &[String]) {
    let block = lines.join("\n");
    let block = block.trim();
    if block.is_empty() {
        return;
    }

    let (code, recovered_notes) = split_misplaced_fenced_notes(block);
    if !code.is_empty() {
        extracted.blocks.push(repair_code_block_layout(&code));
    }
    if let Some(recovered_notes) = recovered_notes {
        extracted.recovered_notes.push(recovered_notes);
    }
}

fn split_misplaced_fenced_notes(block: &str) -> (String, Option<String>) {
    let lines = block.lines().collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate().skip(1) {
        if fenced_presentation_heading(line).is_none() {
            continue;
        }

        let code = lines[..index].join("\n").trim().to_string();
        let suffix = &lines[index..];
        let first_heading = fenced_presentation_heading(line).expect("heading checked above");
        let has_distinct_later_heading = suffix[1..]
            .iter()
            .filter_map(|line| fenced_presentation_heading(line))
            .any(|heading| heading != first_heading);
        if !has_distinct_later_heading
            || !looks_like_real_code(&code)
            || suffix_has_executable_code(suffix)
        {
            continue;
        }

        let recovered_notes = suffix
            .iter()
            .map(|line| strip_recovered_comment_prefix(line))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        if !recovered_notes.is_empty() {
            return (code, Some(recovered_notes));
        }
    }

    (block.trim().to_string(), None)
}

fn fenced_presentation_heading(line: &str) -> Option<&'static str> {
    let heading = strip_recovered_comment_prefix(line)
        .trim()
        .trim_start_matches('#')
        .trim()
        .trim_matches(['*', '_', '`'])
        .trim()
        .trim_end_matches([':', '-', '\u{2013}', '\u{2014}'])
        .trim()
        .to_ascii_lowercase();

    match heading.as_str() {
        "line notes" | "line-by-line notes" | "line by line notes" | "line annotations"
        | "visual line notes" => Some("line_notes"),
        "explanation" | "approach" | "walkthrough" | "why this works" => Some("explanation"),
        "complexity" | "time and space complexity" => Some("complexity"),
        "edge case" | "edge cases" => Some("edge_cases"),
        "notes" => Some("notes"),
        _ => None,
    }
}

fn strip_recovered_comment_prefix(line: &str) -> &str {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("//") {
        return rest.trim_start();
    }
    if let Some(rest) = trimmed.strip_prefix('#') {
        return rest.trim_start();
    }
    trimmed
}

fn suffix_has_executable_code(lines: &[&str]) -> bool {
    lines.iter().any(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || fenced_presentation_heading(trimmed).is_some()
            || looks_like_recovered_comment_line(trimmed)
            || looks_like_markdown_prose_bullet(trimmed)
            || looks_like_recovered_line_note(trimmed)
            || looks_like_presentation_sentence(trimmed)
            || is_complexity_line(trimmed)
            || is_section_separator_line(trimmed)
        {
            return false;
        }

        // Ambiguous content stays in CODE. Recovery is intentionally limited
        // to unmistakable headings, comments, prose sentences, bullets,
        // line-note labels, and complexity lines after a presentation heading
        // so an unrecognized branch or expression cannot be silently moved.
        true
    })
}

fn looks_like_recovered_comment_line(line: &str) -> bool {
    if line.starts_with("//") {
        return true;
    }
    let Some(rest) = line.strip_prefix('#') else {
        return false;
    };
    let rest = rest.trim_start();
    looks_like_recovered_line_note(rest) || looks_like_presentation_sentence(rest)
}

fn looks_like_markdown_prose_bullet(line: &str) -> bool {
    ["- ", "* ", "+ "]
        .iter()
        .find_map(|prefix| line.strip_prefix(prefix))
        .is_some_and(looks_like_presentation_sentence)
}

fn looks_like_presentation_sentence(line: &str) -> bool {
    line.chars()
        .find(|ch| ch.is_alphabetic())
        .is_some_and(char::is_uppercase)
        && line.trim_end().ends_with(['.', '?', '!'])
}

fn looks_like_recovered_line_note(line: &str) -> bool {
    let Some((label, note)) = line.split_once(':') else {
        return false;
    };
    !label.trim().is_empty()
        && !note.trim().is_empty()
        && label
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '-' | '\u{2013}' | '\u{2014}' | ',' | ' '))
}

fn strip_fenced_code(text: &str) -> String {
    let mut lines = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        if !in_fence {
            if let Some((before, after_fence)) = line.split_once("```") {
                if !before.trim().is_empty() {
                    lines.push(before.trim_end());
                }
                if let Some((_inside, after_close)) = after_fence.split_once("```") {
                    if !after_close.trim().is_empty() {
                        lines.push(after_close.trim_start());
                    }
                    continue;
                }
                in_fence = true;
                continue;
            }
            lines.push(line);
        } else if let Some((_before, after)) = line.split_once("```") {
            in_fence = false;
            if !after.trim().is_empty() {
                lines.push(after.trim_start());
            }
        }
    }
    remove_empty_code_headings(&lines.join("\n"))
}

fn inline_code_after_fence_tail(tail: &str) -> Option<&str> {
    let tail = tail.trim_start();
    if tail.is_empty() || tail.starts_with('`') {
        return None;
    }

    const LANGS: &[&str] = &[
        "typescript",
        "javascript",
        "python",
        "kotlin",
        "csharp",
        "swift",
        "ruby",
        "bash",
        "shell",
        "java",
        "rust",
        "json",
        "yaml",
        "html",
        "css",
        "cpp",
        "php",
        "sql",
        "tsx",
        "jsx",
        "py",
        "rs",
        "kt",
        "cs",
        "go",
        "sh",
        "ts",
        "js",
        "c",
    ];

    for language in LANGS {
        if let Some(rest) = tail.strip_prefix(language) {
            let rest = rest.trim_start();
            if looks_like_inline_code_after_fence(rest) {
                return Some(rest);
            }
        }
    }

    None
}

fn remove_empty_code_headings(text: &str) -> String {
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = trim_markdown_heading(line).trim_end_matches(':');
        if matches!(
            trimmed.to_ascii_lowercase().as_str(),
            "code" | "implementation" | "solution code"
        ) {
            continue;
        }
        out.push(line);
    }
    out.join("\n").trim().to_string()
}

fn repair_code_block_layout(code: &str) -> String {
    let code = code.trim();
    let non_empty_lines = code.lines().filter(|line| !line.trim().is_empty()).count();
    if non_empty_lines > 3 || !(code.contains('{') || code.contains(';')) {
        return code.to_string();
    }

    let mut out = String::with_capacity(code.len() + 32);
    let mut paren_depth = 0usize;
    for ch in code.chars() {
        match ch {
            '(' | '[' => {
                paren_depth = paren_depth.saturating_add(1);
                out.push(ch);
            }
            ')' | ']' => {
                paren_depth = paren_depth.saturating_sub(1);
                out.push(ch);
            }
            '{' => {
                trim_trailing_spaces(&mut out);
                if !out.ends_with(' ') && !out.ends_with('\n') {
                    out.push(' ');
                }
                out.push('{');
                out.push('\n');
            }
            '}' => {
                trim_trailing_spaces(&mut out);
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push('}');
                out.push('\n');
            }
            ';' if paren_depth == 0 => {
                trim_trailing_spaces(&mut out);
                out.push(';');
                out.push('\n');
            }
            _ => out.push(ch),
        }
    }

    out.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn trim_trailing_spaces(out: &mut String) {
    while out.ends_with(' ') || out.ends_with('\t') {
        out.pop();
    }
}

fn looks_like_inline_code_after_fence(rest: &str) -> bool {
    if rest.is_empty() {
        return false;
    }

    [
        "from ",
        "import ",
        "class ",
        "def ",
        "for ",
        "while ",
        "if ",
        "return ",
        "let ",
        "const ",
        "function ",
        "public ",
        "private ",
        "package ",
        "SELECT ",
        "select ",
        "{",
        "[",
    ]
    .iter()
    .any(|prefix| rest.starts_with(prefix))
}

fn format_code_artifact(body: &str, fenced_code: &FencedCode) -> String {
    let mut note_parts = Vec::new();
    let visible_notes = strip_fenced_code(body).trim().to_string();
    if !visible_notes.is_empty() {
        note_parts.push(visible_notes);
    }
    note_parts.extend(fenced_code.recovered_notes.iter().cloned());
    let notes = note_parts.join("\n\n");
    let (line_notes, remaining_notes) = split_line_notes(&notes);
    let mut sections = Vec::new();
    if !fenced_code.blocks.is_empty() {
        sections.push(format!(
            "CODE\n----\n{}",
            fenced_code.blocks.join("\n\n// ---\n\n")
        ));
    }
    if let Some(line_notes) = line_notes {
        sections.push(format!("LINE NOTES\n----------\n{line_notes}"));
    }
    let complexity = extract_complexity_lines(&remaining_notes);
    if !complexity.is_empty() {
        sections.push(format!("COMPLEXITY\n----------\n{complexity}"));
    }
    let remaining_notes = strip_complexity_lines(&remaining_notes);
    if !remaining_notes.is_empty() {
        sections.push(format!("NOTES\n-----\n{remaining_notes}"));
    }
    if sections.is_empty() {
        body.to_string()
    } else {
        sections.join("\n\n")
    }
}

fn extract_complexity_lines(text: &str) -> String {
    let mut captured = Vec::new();
    let mut fallback = Vec::new();
    let mut in_complexity = false;

    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        if !in_complexity {
            if let Some(rest) = complexity_heading_remainder(line) {
                in_complexity = true;
                if !rest.trim().is_empty() {
                    captured.push(rest.trim().to_string());
                }
                continue;
            }
            if is_complexity_line(line.trim()) {
                fallback.push(line.trim().to_string());
            }
            continue;
        }

        if looks_like_post_complexity_heading(line) {
            break;
        }
        if is_section_separator_line(line) {
            continue;
        }
        captured.push(line.to_string());
    }

    let captured = trim_joined_lines(captured);
    if !captured.is_empty() {
        captured
    } else {
        trim_joined_lines(fallback)
    }
}

fn strip_complexity_lines(text: &str) -> String {
    let mut out = Vec::new();
    let mut in_complexity = false;

    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        if !in_complexity {
            if complexity_heading_remainder(line).is_some() {
                in_complexity = true;
                continue;
            }
            if is_complexity_line(line.trim()) {
                continue;
            }
            out.push(line.to_string());
            continue;
        }

        if looks_like_post_complexity_heading(line) {
            in_complexity = false;
            out.push(line.to_string());
        }
    }

    trim_joined_lines(out)
}

fn complexity_heading_remainder(line: &str) -> Option<&str> {
    let trimmed = trim_markdown_heading(line);
    let lower = trimmed.to_ascii_lowercase();
    if lower == "complexity" {
        return Some("");
    }
    if let Some(rest) = lower.strip_prefix("complexity:") {
        let offset = trimmed.len().saturating_sub(rest.len());
        return Some(trimmed[offset..].trim_start());
    }
    None
}

fn looks_like_post_complexity_heading(line: &str) -> bool {
    let trimmed = trim_markdown_heading(line);
    if trimmed.is_empty() || is_complexity_line(trimmed) {
        return false;
    }
    let lower = trimmed.trim_end_matches(':').to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "notes"
            | "line notes"
            | "line-by-line notes"
            | "line by line notes"
            | "explanation"
            | "approach"
            | "code"
            | "implementation"
            | "edge cases"
            | "examples"
            | "walkthrough"
            | "why this works"
    )
}

fn is_complexity_line(line: &str) -> bool {
    let lower = line
        .trim()
        .trim_start_matches(['-', '*', '•'])
        .trim_start()
        .trim_matches('*')
        .trim()
        .to_ascii_lowercase();
    lower.contains("time complexity")
        || lower.contains("space complexity")
        || lower.starts_with("time:")
        || lower.starts_with("space:")
        || lower.starts_with("time ")
        || lower.starts_with("space ")
}

fn is_section_separator_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && trimmed.chars().all(|ch| ch == '-' || ch == '=')
}

fn code_artifact_has_complete_code(body: &str) -> bool {
    let code = extract_code_section_from_canvas(body);
    let code = code.trim();
    if code.is_empty() {
        return false;
    }
    if looks_like_patch_or_diff(code) {
        return false;
    }
    if looks_like_control_flow_fragment_without_entrypoint(code) {
        return false;
    }
    looks_like_real_code(code)
}

fn extract_code_section_from_canvas(body: &str) -> String {
    let normalized = body.replace("\r\n", "\n");
    let mut lines = Vec::new();
    let mut in_code = false;
    let mut saw_canvas_header = false;

    for line in normalized.lines() {
        let trimmed = line.trim();
        let header = trimmed.to_ascii_uppercase();
        if matches!(
            header.as_str(),
            "CODE" | "PATCH" | "DIFF" | "CHANGED BLOCK" | "CHANGED LINES"
        ) {
            in_code = true;
            saw_canvas_header = true;
            continue;
        }
        if matches!(
            header.as_str(),
            "LINE NOTES" | "COMPLEXITY" | "TIME" | "SPACE" | "NOTES" | "EXPLANATION" | "APPROACH"
        ) {
            if in_code {
                break;
            }
            saw_canvas_header = true;
            continue;
        }
        if trimmed.chars().all(|ch| ch == '-' || ch == '=') {
            continue;
        }
        if in_code {
            lines.push(line);
        }
    }

    if saw_canvas_header {
        lines.join("\n")
    } else {
        normalized
    }
}

fn looks_like_real_code(code: &str) -> bool {
    let lower = code.to_ascii_lowercase();
    let syntax_signals = [
        "def ",
        "fn ",
        "func ",
        "function ",
        "class ",
        "struct ",
        "enum ",
        "return ",
        "select ",
        " from ",
        " where ",
        " group by",
        " order by",
        " join ",
        "insert ",
        "update ",
        "delete ",
        "#include",
        "import ",
        "let ",
        "var ",
        "const ",
        "public ",
        "private ",
        "protected ",
        "static ",
        "=>",
        "->",
        "==",
        "!=",
        "<=",
        ">=",
        "+=",
        "-=",
        ".append(",
        ".sort(",
    ];
    let has_signal = syntax_signals.iter().any(|signal| lower.contains(signal));
    let has_punctuation = code.contains('{')
        || code.contains('}')
        || code.contains(';')
        || code.contains('=')
        || code.contains('(') && code.contains(')')
        || code.contains('[') && code.contains(']');
    let non_empty_lines = code
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .count();

    has_signal || looks_like_code_assignment(code) || (non_empty_lines >= 2 && has_punctuation)
}

fn looks_like_code_assignment(code: &str) -> bool {
    code.lines().map(str::trim).any(|line| {
        if line.is_empty()
            || line.starts_with("//")
            || line.starts_with('#')
            || line.starts_with("- ")
            || line.contains("==")
            || line.contains("!=")
            || line.contains("<=")
            || line.contains(">=")
        {
            return false;
        }
        line.contains('=')
            && (line.contains(',')
                || line.contains('+')
                || line.contains('-')
                || line.contains('*')
                || line.contains('/')
                || line.contains('.')
                || line.contains('[')
                || line.contains('('))
    })
}

fn looks_like_patch_or_diff(code: &str) -> bool {
    let trimmed = code.trim_start();
    trimmed.starts_with("diff --git")
        || trimmed.starts_with("@@")
        || trimmed.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("+ ")
                || line.starts_with("- ")
                || line.starts_with("+\t")
                || line.starts_with("-\t")
        })
}

fn looks_like_control_flow_fragment_without_entrypoint(code: &str) -> bool {
    let lines = code
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with("//")
                && !line.starts_with('#')
                && !line.starts_with("/*")
                && !line.starts_with('*')
        })
        .collect::<Vec<_>>();
    let Some(first) = lines.first() else {
        return false;
    };
    let lower_code = code.to_ascii_lowercase();
    let lower_first = first.to_ascii_lowercase();
    let has_entrypoint = [
        "def ",
        "class ",
        "function ",
        "fn ",
        "func ",
        "public ",
        "private ",
        "protected ",
        "static ",
        "int main",
        "bool ",
        "boolean ",
        "void ",
        "const ",
        "let ",
        "var ",
        "=>",
    ]
    .iter()
    .any(|signal| lower_code.contains(signal));
    let starts_with_control_flow = [
        "for ", "for(", "while ", "while(", "if ", "if(", "else", "switch ", "switch(", "case ",
    ]
    .iter()
    .any(|signal| lower_first.starts_with(signal));

    starts_with_control_flow && !has_entrypoint
}

fn split_line_notes(notes: &str) -> (Option<String>, String) {
    let clean = notes.trim();
    if clean.is_empty() {
        return (None, String::new());
    }

    let mut before = Vec::new();
    let mut line_notes = Vec::new();
    let mut after = Vec::new();
    let mut in_line_notes = false;
    let mut in_after = false;

    for raw_line in clean.lines() {
        let line = raw_line.trim_end();
        if !in_line_notes && !in_after {
            if let Some(rest) = line_notes_heading_remainder(line) {
                in_line_notes = true;
                if !rest.trim().is_empty() {
                    line_notes.push(rest.trim().to_string());
                }
                continue;
            }
            before.push(line.to_string());
            continue;
        }

        if in_line_notes && !in_after && looks_like_post_line_notes_heading(line) {
            in_after = true;
            after.push(line.to_string());
            continue;
        }

        if in_after {
            after.push(line.to_string());
        } else {
            line_notes.push(line.to_string());
        }
    }

    let line_notes_text = trim_joined_lines(line_notes);
    let mut remaining_parts = Vec::new();
    let before_text = trim_joined_lines(before);
    let after_text = trim_joined_lines(after);
    if !before_text.is_empty() {
        remaining_parts.push(before_text);
    }
    if !after_text.is_empty() {
        remaining_parts.push(after_text);
    }

    (
        (!line_notes_text.is_empty()).then_some(line_notes_text),
        remaining_parts.join("\n\n"),
    )
}

fn trim_joined_lines(lines: Vec<String>) -> String {
    lines.join("\n").trim().to_string()
}

fn line_notes_heading_remainder(line: &str) -> Option<&str> {
    let trimmed = trim_markdown_heading(line);
    let lower = trimmed.to_ascii_lowercase();
    for heading in [
        "line notes",
        "line-by-line notes",
        "line by line notes",
        "line annotations",
        "visual line notes",
    ] {
        if lower == heading {
            return Some("");
        }
        if let Some(rest) = lower.strip_prefix(&format!("{heading}:")) {
            let offset = trimmed.len().saturating_sub(rest.len());
            return Some(trimmed[offset..].trim_start());
        }
    }
    None
}

fn looks_like_post_line_notes_heading(line: &str) -> bool {
    let trimmed = trim_markdown_heading(line);
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.trim_end_matches(':').to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "notes"
            | "explanation"
            | "approach"
            | "complexity"
            | "time complexity"
            | "space complexity"
            | "edge cases"
            | "walkthrough"
            | "why this works"
    )
}

fn trim_markdown_heading(line: &str) -> &str {
    line.trim()
        .trim_start_matches('#')
        .trim()
        .trim_matches('*')
        .trim()
}

fn format_structured_artifact(body: &str, fallback_heading: &str) -> String {
    let clean = body.trim();
    if clean.starts_with('#')
        || clean
            .to_lowercase()
            .starts_with(&fallback_heading.to_lowercase())
    {
        clean.to_string()
    } else {
        format!(
            "{fallback_heading}\n{}\n{clean}",
            "-".repeat(fallback_heading.len())
        )
    }
}

fn has_structured_shape(text: &str) -> bool {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.starts_with('#')
                || numbered_list_prefix(trimmed)
        })
        .count()
        >= 3
}

fn numbered_list_prefix(line: &str) -> bool {
    let mut chars = line.chars().peekable();
    let mut saw_digit = false;
    while matches!(chars.peek(), Some(ch) if ch.is_ascii_digit()) {
        saw_digit = true;
        chars.next();
    }
    saw_digit
        && matches!(chars.next(), Some('.' | ')'))
        && matches!(chars.next(), Some(ch) if ch.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_presentation_sections_accidentally_left_inside_python_fence() {
        let answer = r#"To implement an LRU cache, combine a hash map with a doubly linked list.

```python
class Node:
    def __init__(self, key=0, value=0):
        self.key = key
        self.value = self.value  # Redundant assignment for clarity
        self.prev = None
        self.next = None

class LRUCache:
    def __init__(self, capacity: int):
        self.capacity = capacity
        self.cache = {}

    def get(self, key: int) -> int:
        node = self.cache.get(key)
        return -1 if node is None else node.value

Line notes:
1: Node stores the key, value, and list links.
2: The dictionary maps each key to its node.

Explanation
The dictionary provides constant-time lookup while the list preserves recency order.
- The dictionary provides constant-time lookup.
- The list preserves recency order.

Complexity
- Time Complexity: O(1) for get and put.
- Space Complexity: O(capacity).

Edge cases
- A missing key returns -1.
- Capacity zero stores nothing.
```"#;

        let artifact = response_artifact(answer).expect("code artifact");
        assert_eq!(artifact.artifact_type, "code");

        let code = extract_code_section_from_canvas(&artifact.body);
        assert!(code.contains("self.value = self.value  # Redundant assignment for clarity"));
        assert!(!code.contains("self.value = value"));
        assert!(code.contains("return -1 if node is None else node.value"));
        assert!(!code.contains("Line notes"));
        assert!(!code.contains("Explanation"));
        assert!(!code.contains("Time Complexity"));
        assert!(!code.contains("Edge cases"));

        assert!(artifact.body.contains("LINE NOTES\n----------"));
        assert!(artifact
            .body
            .contains("1: Node stores the key, value, and list links."));
        assert!(artifact.body.contains("COMPLEXITY\n----------"));
        assert!(artifact
            .body
            .contains("Time Complexity: O(1) for get and put."));
        assert!(artifact.body.contains("NOTES\n-----"));
        assert!(artifact.body.contains("Explanation"));
        assert!(artifact
            .body
            .contains("The dictionary provides constant-time lookup while the list preserves recency order."));
        assert!(artifact.body.contains("Edge cases"));
    }

    #[test]
    fn recovers_slash_comment_presentation_headings() {
        let answer = r#"```javascript
function getValue(items) {
  return items[0];
}
// Explanation:
// - Return the first item.
// Complexity:
// - Time Complexity: O(1).
// Edge cases:
// - Empty input returns undefined.
```"#;

        let artifact = response_artifact(answer).expect("code artifact");
        let code = extract_code_section_from_canvas(&artifact.body);
        assert!(code.contains("return items[0];"));
        assert!(!code.contains("Explanation"));
        assert!(artifact.body.contains("COMPLEXITY\n----------"));
        assert!(artifact.body.contains("Edge cases:"));
    }

    #[test]
    fn preserves_heading_like_comments_when_executable_code_follows() {
        let answer = r#"```python
def describe(items):
    # Explanation
    explanation = "items are counted once"
    # Complexity
    complexity = len(items)
    return explanation, complexity
```"#;

        let artifact = response_artifact(answer).expect("code artifact");
        let code = extract_code_section_from_canvas(&artifact.body);
        assert!(code.contains("# Explanation"));
        assert!(code.contains("explanation = \"items are counted once\""));
        assert!(code.contains("# Complexity"));
        assert!(code.contains("complexity = len(items)"));
        assert!(!artifact.body.contains("COMPLEXITY\n----------"));
    }

    #[test]
    fn preserves_unrecognized_control_flow_after_multiple_heading_like_comments() {
        let answer = r#"```python
def require_value(value):
    if value is not None:
        return value
    # Explanation
    # Complexity
    else:
        raise ValueError("missing")
```"#;

        let artifact = response_artifact(answer).expect("code artifact");
        let code = extract_code_section_from_canvas(&artifact.body);
        assert!(code.contains("# Explanation"));
        assert!(code.contains("# Complexity"));
        assert!(code.contains("else:"));
        assert!(code.contains("raise ValueError"));
        assert!(!artifact.body.contains("COMPLEXITY\n----------"));
    }

    #[test]
    fn preserves_a_single_heading_like_trailing_comment() {
        let answer = r#"```python
def identity(value):
    return value

# Explanation
```"#;

        let artifact = response_artifact(answer).expect("code artifact");
        let code = extract_code_section_from_canvas(&artifact.body);
        assert!(code.contains("# Explanation"));
        assert!(!artifact.body.contains("NOTES\n-----"));
    }
}
