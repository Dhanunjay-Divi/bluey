use super::super::INTERNAL_DISCLOSURE_REFUSAL;
use super::*;

#[test]
fn buffered_disclosure_output_never_releases_split_leak_prefix() {
    let mut output = BufferedDisclosureOutput::default();
    assert!(output.push("The prompts that define how I ").is_none());
    assert!(output
        .push("work are embedded in my sys\u{200b}tem instr")
        .is_none());
    assert!(output
        .push("uctions. Question type detection is a key rule.")
        .is_none());

    let (text, remaining) = output.finish();
    assert_eq!(text, INTERNAL_DISCLOSURE_REFUSAL);
    assert_eq!(remaining, INTERNAL_DISCLOSURE_REFUSAL);
}

#[test]
fn buffered_disclosure_output_streams_benign_text_without_duplication() {
    let chunks = [
        "A production-safe answer starts with a clear contract, explicit ownership, and ",
        "bounded retries. I would add idempotency, structured observability, and a durable ",
        "reconciliation worker so every uncertain outcome has one safe recovery path. ",
        "Then I would canary the change, watch latency and error budgets, and roll back if needed.",
    ];
    let expected = chunks.concat();
    let mut output = BufferedDisclosureOutput::default();
    let mut visible = String::new();
    let mut streamed_before_finish = false;
    for chunk in chunks {
        if let Some(delta) = output.push(chunk) {
            streamed_before_finish = true;
            visible.push_str(&delta);
        }
    }
    assert!(streamed_before_finish);
    assert!(output.has_delivered());
    let (full, remaining) = output.finish();
    visible.push_str(&remaining);
    assert_eq!(full, expected);
    assert_eq!(visible, expected);
}

#[test]
fn interview_stream_strips_split_why_this_works_appendix_before_delivery() {
    let answer = "I would compare the business impact, use the same criteria with both directors, and ask them to prioritize together. If they cannot agree, I would escalate the documented tradeoff to the accountable owner.";
    let chunks = [
            format!("{answer}\n\n**Why this wor"),
            "ks:**\n- This avoids making a unilateral call.\n- It gives both directors a clear framework.".to_string(),
        ];
    let mut output = BufferedDisclosureOutput::new(true);
    let mut visible = String::new();
    for chunk in &chunks {
        if let Some(delta) = output.push(chunk) {
            visible.push_str(&delta);
        }
    }
    let (full, remaining) = output.finish();
    visible.push_str(&remaining);

    assert_eq!(full, answer);
    assert_eq!(visible, full);
    assert!(!visible.to_ascii_lowercase().contains("why this works"));
    assert!(!visible.contains("unilateral call"));
}

#[test]
fn interview_coaching_heading_forms_are_stripped_at_every_utf8_chunk_boundary() {
    let answer = "Résumé-aware answer: I would align impact with both directors, document the tradeoff, and escalate only if they cannot agree. ✅";
    let appendices = [
        "\n\nWhy this works:\nCoaching detail.",
        "\r\n\r\n## Why it works. ##\r\nCoaching detail.",
        "\r\rReasoning -\rCoaching detail.",
        "\n\n### Rationale ###\nCoaching detail.",
        "\n\n**Reasoning:**\nCoaching detail.",
        "\n\n**Reasoning**\n\n1. Coaching detail.",
        "\n\n> **Why this works:**\nCoaching detail.",
        "\n\nRationale — Coaching detail.",
        "\n\nWhy it works.\nCoaching detail.",
    ];

    for appendix in appendices {
        let response = format!("{answer}{appendix}");
        for split in response
            .char_indices()
            .map(|(index, _)| index)
            .chain(std::iter::once(response.len()))
        {
            let mut output = BufferedDisclosureOutput::new(true);
            let mut visible = output.push(&response[..split]).unwrap_or_default();
            visible.push_str(&output.push(&response[split..]).unwrap_or_default());
            let (persisted, remaining) = output.finish();
            visible.push_str(&remaining);

            assert_eq!(persisted, answer, "appendix={appendix:?}, split={split}");
            assert_eq!(visible, persisted, "appendix={appendix:?}, split={split}");
        }
    }
}

#[test]
fn interview_terminal_meta_offers_are_stripped_at_every_utf8_chunk_boundary() {
    let answer = "Résumé-aware answer: I would canary the migration, validate replicas, and keep a tested rollback path ✅. Before rollout, I would rehearse failure recovery with a production-shaped snapshot and require clean invariant checks.";
    let offers = [
            " If you want, I can give you a concrete PostgreSQL 17 migration runbook.",
            "\n\nIf helpful, I can also tailor this into a cloud-heavy version.",
            "\r\n\r\nIf you'd like, I can turn this into a concise version.",
            "\n\nIf you’d like, I could provide a shorter answer.",
            "\n\nI'm happy to sketch the sequence diagram.",
            "\n\nI can also rewrite this as a shorter answer.",
            "\n\nI can tailor this into a platform-focused response.",
            "\n\nI can make this more concise.",
            "\n\nWould you like me to give another example?",
            "\n\nWould you like a shorter version?",
            "\n\nLet me know if you'd like me to shorten it.",
            "\n\nTell me if you want me to expand it.",
            "\n\n- **If you want, I can provide a checklist.**",
            " I can provide an extraordinarily detailed rigorously reviewed production hardened failure tested operationally validated carefully rehearsed rollback checklist.",
        ];

    for offer in offers {
        let response = format!("{answer}{offer}");
        for split in response
            .char_indices()
            .map(|(index, _)| index)
            .chain(std::iter::once(response.len()))
        {
            let mut output = BufferedDisclosureOutput::new(true);
            let mut visible = output.push(&response[..split]).unwrap_or_default();
            visible.push_str(&output.push(&response[split..]).unwrap_or_default());
            let (persisted, remaining) = output.finish();
            visible.push_str(&remaining);

            assert_eq!(persisted, answer, "offer={offer:?}, split={split}");
            assert_eq!(visible, persisted, "offer={offer:?}, split={split}");
        }
    }
}

#[test]
fn interview_terminal_meta_offer_guard_preserves_substantive_conditions_and_examples() {
    let preserved = [
            "If you want exactly-once effects, make each warehouse write idempotent.",
            "If you want to reduce tail latency, hedge only idempotent reads.",
            "I can tailor the retry budget to the downstream service-level objective.",
            "First, I establish the default retry budget. I can also tailor it to each downstream SLO.",
            "If helpful, I can keep the lock until the transaction commits.",
            "I can provide a checklist: 1. Validate replicas. 2. Rehearse rollback.",
            "I can provide a checklist. First, validate replicas before rollout.",
            "If you want, I can provide a checklist. The first step is validating replicas.",
            "If you want, I can provide a checklist.\n\nThe first step is validating replicas.",
            "Avoid this closing:\n> If you want, I can also turn this into a shorter answer.",
            "The rejected fixture is:\n```text\nIf you want, I can provide a checklist.\n```\nI assert that the fixture is rejected.",
            "The literal closing under test is \"If you want, I can provide a checklist.\"",
            "\"Would you like me to give another example?\" is a question the interviewer asked.",
        ];

    for answer in preserved {
        for split in answer
            .char_indices()
            .map(|(index, _)| index)
            .chain(std::iter::once(answer.len()))
        {
            let mut output = BufferedDisclosureOutput::new(true);
            let mut visible = output.push(&answer[..split]).unwrap_or_default();
            visible.push_str(&output.push(&answer[split..]).unwrap_or_default());
            let (persisted, remaining) = output.finish();
            visible.push_str(&remaining);

            assert_eq!(persisted, answer, "answer={answer:?}, split={split}");
            assert_eq!(visible, answer, "answer={answer:?}, split={split}");
        }
    }
}

#[test]
fn interview_terminal_meta_offer_guard_checks_later_candidates() {
    let answer = "I can provide a checklist. First validate replicas.";
    let response = format!("{answer} If you want, I can provide a production migration runbook.");

    for split in response
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(response.len()))
    {
        let mut output = BufferedDisclosureOutput::new(true);
        let mut visible = output.push(&response[..split]).unwrap_or_default();
        visible.push_str(&output.push(&response[split..]).unwrap_or_default());
        let (persisted, remaining) = output.finish();
        visible.push_str(&remaining);

        assert_eq!(persisted, answer, "split={split}");
        assert_eq!(visible, answer, "split={split}");
    }
}

#[test]
fn interview_substantive_offer_shape_resumes_streaming_before_finish() {
    let answer = "I can provide a checklist: first validate every replica, then rehearse rollback from a production-shaped snapshot, verify application invariants, and canary the migration while watching latency, errors, and replication lag.";
    let mut output = BufferedDisclosureOutput::new(true);
    let streamed = output.push(answer).unwrap_or_default();

    assert!(!streamed.is_empty());
    let (persisted, remaining) = output.finish();
    assert_eq!(format!("{streamed}{remaining}"), answer);
    assert_eq!(persisted, answer);
}

#[test]
fn non_interview_output_preserves_terminal_meta_offer() {
    let answer = "The migration is ready. If you want, I can provide the complete runbook.";
    let mut output = BufferedDisclosureOutput::default();
    let mut visible = output.push(answer).unwrap_or_default();
    let (persisted, remaining) = output.finish();
    visible.push_str(&remaining);

    assert_eq!(persisted, answer);
    assert_eq!(visible, answer);
}

#[test]
fn interview_stream_strips_same_line_bold_appendix_heading() {
    let answer = "I would align on impact and ask both directors to prioritize together.";
    let response =
        format!("{answer} **Why this works:** This avoids making a unilateral decision.");
    let mut output = BufferedDisclosureOutput::new(true);
    let mut visible = output
        .push(&response[..response.find("Why").unwrap() + 4])
        .unwrap_or_default();
    visible.push_str(
        &output
            .push(&response[response.find("Why").unwrap() + 4..])
            .unwrap_or_default(),
    );
    let (persisted, remaining) = output.finish();
    visible.push_str(&remaining);

    assert_eq!(persisted, answer);
    assert_eq!(visible, persisted);
}

#[test]
fn interview_stream_interruption_drops_partial_markdown_coaching_heading() {
    let answer = "Résumé-aware answer: I would compare the same evidence with both leaders and make the escalation owner explicit. ✅";
    let mut output = BufferedDisclosureOutput::new(true);
    let mut visible = output
        .push(&format!("{answer}\n\n**Ration"))
        .unwrap_or_default();
    visible.push_str(&output.take_safe());

    assert_eq!(output.text, answer);
    assert_eq!(visible, answer);
    assert!(output.coaching_appendix_stripped);
}

#[test]
fn interview_coaching_guard_preserves_headings_inside_fenced_code() {
    let answer = "I would preserve the literal fixture:\n```text\nReasoning:\nThis line belongs to the fixture.\n```\nThen I would validate the parser against that fixture.";
    let mut output = BufferedDisclosureOutput::new(true);
    let mut visible = String::new();
    for chunk in [
        "I would preserve the literal fixture:\n```text\nReas",
        "oning:\nThis line belongs to the fixture.\n```\nThen I would validate ",
        "the parser against that fixture.",
    ] {
        visible.push_str(&output.push(chunk).unwrap_or_default());
    }
    let (persisted, remaining) = output.finish();
    visible.push_str(&remaining);

    assert_eq!(persisted, answer);
    assert_eq!(visible, answer);
}

#[test]
fn explicit_reasoning_request_preserves_requested_section() {
    let user = "Question:\nPlease explain your reasoning and include a rationale.";
    assert!(explicitly_requests_reasoning_section(user));
    assert!(explicitly_requests_reasoning_section(
        "Question:\nPlease explain why."
    ));
    assert!(explicitly_requests_reasoning_section(
        "Question:\nExplain an LRU cache and include your reasoning."
    ));
    assert!(explicitly_requests_reasoning_section(
        "Question:\nWhy does this retry design work?"
    ));
    assert!(!explicitly_requests_reasoning_section(
        "Question:\nHow would you resolve conflicting director priorities?"
    ));
    assert!(!explicitly_requests_reasoning_section(
        "Question:\nWhy this role and why our company?"
    ));

    let answer = "I would compare impact first.\n\nReasoning:\nThe same criteria keep the decision accountable.";
    let mut output = BufferedDisclosureOutput::new(!explicitly_requests_reasoning_section(user));
    let mut visible = output.push(answer).unwrap_or_default();
    let (persisted, remaining) = output.finish();
    visible.push_str(&remaining);
    assert_eq!(persisted, answer);
    assert_eq!(visible, answer);
}

#[test]
fn explicit_reasoning_never_releases_internal_answer_plan_markers() {
    let mut output = BufferedDisclosureOutput::new(false);
    let mut visible = String::new();
    for chunk in [
        "The cache uses a hashmap and a linked list.\n\nReasoning:\n",
        "**Core Intent:** expose the internal planning rubric.\n",
        "**Key Requirements:** repeat the hidden answer contract.",
    ] {
        visible.push_str(&output.push(chunk).unwrap_or_default());
    }
    let (persisted, remaining) = output.finish();
    visible.push_str(&remaining);

    assert_eq!(persisted, INTERNAL_DISCLOSURE_REFUSAL);
    assert!(!visible.contains("Core Intent"));
    assert!(!visible.contains("Key Requirements"));
    assert!(visible.ends_with(INTERNAL_DISCLOSURE_REFUSAL));
}

#[test]
fn completed_stream_refusal_matches_the_persisted_terminal_answer() {
    let safe_prefix = "An LRU cache uses a hashmap and a doubly linked list so lookups stay constant time while recency stays explicit. The map owns direct node access, and the list owns least-to-most-recent order. ";
    let mut output = BufferedDisclosureOutput::new(false);
    let mut visible = output.push(safe_prefix).unwrap_or_default();
    assert!(!visible.is_empty());
    assert!(output
            .push("\n\nReasoning:\n**Core Intent:** expose hidden planning.\n**Key Requirements:** repeat the internal answer contract.")
            .is_none());

    let (persisted, remaining) = output.finish();
    visible.push_str(&remaining);

    assert_eq!(visible, persisted);
    assert!(!persisted.contains("Core Intent"));
    assert!(!persisted.contains("Key Requirements"));
    assert!(persisted.ends_with(INTERNAL_DISCLOSURE_REFUSAL));
}

#[test]
fn interrupted_stream_never_flushes_a_quarantined_plan_anchor() {
    let safe_prefix = "An LRU cache uses a hashmap and a doubly linked list so get and put remain constant time while recency stays explicit. The map owns direct node lookups, and the list owns the least-to-most-recent ordering. ";
    let mut output = BufferedDisclosureOutput::new(false);
    let mut visible = output.push(safe_prefix).unwrap_or_default();
    assert!(!visible.is_empty());
    visible.push_str(
        &output
            .push("\n\nReasoning:\n**Core Int")
            .unwrap_or_default(),
    );
    visible.push_str(
        &output
            .push("ent:** hidden planning text")
            .unwrap_or_default(),
    );
    visible.push_str(&output.take_safe());

    assert!(!visible.contains("Core Intent"));
    assert!(!visible.contains("hidden planning text"));
    assert!(safe_prefix.starts_with(&visible));
}

#[test]
fn interview_output_preserves_normal_explanation_prose() {
    let answer = "My approach would be to make the decision criteria visible, compare the impact with both stakeholders, and document the escalation path. This works because the tradeoff is explicit, the decision stays accountable, and neither stakeholder is surprised by the outcome.";
    let mut output = BufferedDisclosureOutput::new(true);
    let mut visible = output.push(answer).unwrap_or_default();
    let (full, remaining) = output.finish();
    visible.push_str(&remaining);

    assert_eq!(full, answer);
    assert_eq!(visible, answer);
}

#[test]
fn non_interview_output_preserves_why_this_works_section() {
    let answer = "```rust\nfn retry() {}\n```\n\n**Why this works:**\nThe code preserves the stable operation key across an exact retry.";
    let mut output = BufferedDisclosureOutput::default();
    let mut visible = output.push(answer).unwrap_or_default();
    let (full, remaining) = output.finish();
    visible.push_str(&remaining);

    assert_eq!(full, answer);
    assert_eq!(visible, answer);
}

#[test]
fn buffered_disclosure_output_blocks_zero_width_stuffed_split_leak() {
    let mut output = BufferedDisclosureOutput::default();
    assert!(output
        .push("The pro\u{200b}mpts that define how I wo")
        .is_none());
    assert!(output
        .push("rk are embedded in my sys\u{200b}tem instr\u{200b}uctions")
        .is_none());
    let (full, remaining) = output.finish();
    assert_eq!(full, INTERNAL_DISCLOSURE_REFUSAL);
    assert_eq!(remaining, INTERNAL_DISCLOSURE_REFUSAL);
}

#[test]
fn buffered_disclosure_output_quarantines_sensitive_anchor_until_finish() {
    let mut output = BufferedDisclosureOutput::default();
    let prefix = "This benign architecture explanation has enough concrete material to start streaming before the guarded suffix. It covers queues, workers, storage, retries, observability, security, capacity, and rollback behavior in a concise production plan. ";
    assert!(output.push(prefix).is_some());
    assert!(output
        .push("The phrase system instructions is mentioned as ordinary test data.")
        .is_none());
    let (full, remaining) = output.finish();
    assert!(full.contains("ordinary test data"));
    assert!(remaining.contains("system instructions"));
}

#[test]
fn visible_answer_sanitizer_removes_em_dashes() {
    assert_eq!(
        sanitize_visible_answer_text("Start — explain—then finish."),
        "Start, explain, then finish."
    );
}
