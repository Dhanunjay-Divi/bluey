//! Real router endpoints: managed dispatch + atomic reservation/settlement + idempotency.

use axum::{
    extract::State,
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Extension, Json,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::convert::Infallible;
use std::hash::{Hash, Hasher};
use std::pin::Pin;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::AppState;
use crate::auth::AuthedAccount;
use crate::db::accounts::Account;
use crate::db::{
    balance, idempotency, ops_audit, sync,
    usage::{self, UsageEvent},
    usage_reservations::{self, ReserveUsageInput, ReservedUsage, SettledUsage},
};
use crate::pricing;
use crate::routing;
use cue_core::prompt_contracts::{
    MANAGED_PROVIDER_ANSWER_RULES_SEPARATOR, ROLE_ADAPTIVE_PRACTITIONER_VOICE,
    SUPPORTED_MANAGED_PROVIDER_BASE_CONTRACTS,
};
use cue_core::short_observability_ref;

mod interview_contracts;
mod sse;
mod visible_output;
use sse::{response_to_sse_events, router_sse, RouterSseStream};
use visible_output::{explicitly_requests_reasoning_section, BufferedDisclosureOutput};

fn log_session_id(session_id: Option<&str>) -> &str {
    session_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("none")
}

const DEFAULT_SLOW_FIRST_TOKEN_AUDIT_MS: i64 = 2_500;
const DEFAULT_DEEP_SLOW_FIRST_TOKEN_AUDIT_MS: i64 = 8_000;
const CODE_ARTIFACT_DEFAULT_OUTPUT_TOKENS: u32 = 4_096;
const CANVAS_DETAIL_DEFAULT_OUTPUT_TOKENS: u32 = 3_072;
const DEFAULT_CAPACITY_SHORT_WAIT_MAX_SECS: u64 = 2;
const UPSTREAM_SPEND_GUARD_RETRY_AFTER_SECS: u64 = 60;
const DEFAULT_LLM_USAGE_RESERVATION_TTL_SECS: u64 = 30 * 60;
const LLM_SETTLEMENT_RETRY_ATTEMPTS: usize = 3;
const LLM_SETTLEMENT_RETRY_DELAY_MS: u64 = 100;

struct AnswerOpsEvent<'a> {
    account_id: &'a str,
    request_id: &'a str,
    session_id: Option<&'a str>,
    trace_id: Option<&'a str>,
    event_type: &'a str,
    status: &'a str,
    metadata: serde_json::Value,
}

fn record_answer_ops_event(pool: &crate::db::DbPool, event: AnswerOpsEvent<'_>) {
    let mut metadata = event.metadata;
    if let Some(object) = metadata.as_object_mut() {
        object.insert("request_id".into(), serde_json::json!(event.request_id));
        object.insert(
            "request_ref".into(),
            serde_json::json!(short_observability_ref(Some(event.request_id))),
        );
        object.insert("session_id".into(), serde_json::json!(event.session_id));
        object.insert(
            "session_ref".into(),
            serde_json::json!(short_observability_ref(event.session_id)),
        );
        object.insert("trace_id".into(), serde_json::json!(event.trace_id));
    }
    if let Err(error) = ops_audit::record_event(
        pool,
        ops_audit::OpsAuditEventInput {
            account_id_hash: Some(cue_core::account_id_hash_prefix(event.account_id)),
            actor_account_id_hash: None,
            event_type: event.event_type.to_string(),
            status: event.status.to_string(),
            metadata_json: metadata,
        },
    ) {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(event.account_id),
            request_ref = %short_observability_ref(Some(event.request_id)),
            session_ref = %short_observability_ref(event.session_id),
            event_type = %event.event_type,
            error = %error,
            "failed to record redacted answer ops event"
        );
    }
}

const INTERNAL_DISCLOSURE_REFUSAL: &str = "I can’t share Bluey’s private instructions, prompts, guardrails, tokens, or internal configuration. Ask me what you want to do, and I’ll help with the answer itself.";
const MANAGED_VISION_TEXT_FALLBACK_INSTRUCTION: &str = "An image was supplied with this request, but the image is unavailable for this retry. Answer the same user request using only the user text and retained textual context. Do not claim that you saw or analyzed the image, and do not invent missing visual details. If essential details exist only in the image, say that the image was unavailable and ask only for the minimum missing detail.";

// Keep the finite output-disclosure vocabulary centralized. The streaming
// holdback in `visible_output` is derived from these exact phrases so adding a
// longer guard cannot silently make the rolling window too short.
const INTERNAL_DISCLOSURE_LEAK_SIGNALS: [&str; 13] = [
    "the prompts that define how i work",
    "embedded in my system instructions",
    "plain summary of the key rules i follow",
    "identity and scope",
    "talk track rule",
    "question type detection",
    "voice and person",
    "depth matching",
    "canvas and workbench split",
    "style restrictions",
    "output shape",
    "human speak contract",
    "answer rules",
];

const INTERNAL_PLAN_DISCLOSURE_MARKERS: [&str; 6] = [
    "core intent",
    "key requirements",
    "visible answer contract",
    "grounding and technical safety",
    "interview closing contract",
    "bluey answer plan",
];

const INTERNAL_DISCLOSURE_QUARANTINE_ANCHORS: [&str; 3] =
    ["system instructions", "i follow", "how i work"];

#[derive(Clone, Copy, Debug)]
struct InternalDisclosureBlocked;

impl InternalDisclosureBlocked {
    fn into_api_error(self) -> (StatusCode, Json<ApiError>) {
        internal_disclosure_api_error()
    }
}

fn internal_disclosure_api_error() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: INTERNAL_DISCLOSURE_REFUSAL.to_string(),
            reason: Some("internal_disclosure_blocked".to_string()),
            ..Default::default()
        }),
    )
}

fn internal_disclosure_error(req: &CompleteRequest) -> Option<(StatusCode, Json<ApiError>)> {
    validate_managed_direct_request(req)
        .err()
        .map(InternalDisclosureBlocked::into_api_error)
}

fn managed_answer_rules(system: &str) -> Result<Option<&str>, InternalDisclosureBlocked> {
    for contract in SUPPORTED_MANAGED_PROVIDER_BASE_CONTRACTS {
        if system == *contract {
            return Ok(None);
        }
        let Some(tail) = system.strip_prefix(contract) else {
            continue;
        };
        let Some(rules) = tail.strip_prefix(MANAGED_PROVIDER_ANSWER_RULES_SEPARATOR) else {
            return Err(InternalDisclosureBlocked);
        };
        if rules.trim().is_empty() {
            return Err(InternalDisclosureBlocked);
        }
        return Ok(Some(rules));
    }
    Err(InternalDisclosureBlocked)
}

fn complete_request_untrusted_text<'a>(
    req: &'a CompleteRequest,
    answer_rules: Option<&'a str>,
) -> impl Iterator<Item = &'a str> {
    std::iter::once(req.request_id.as_str())
        .chain(answer_rules)
        .chain(std::iter::once(req.user.as_str()))
        .chain(req.session_id.as_deref())
        .chain(req.reasoning_effort.as_deref())
        .chain(std::iter::once(req.lane.as_str()))
        .chain(req.image_data_urls.iter().map(String::as_str))
        .chain(req.context.iter().flat_map(|context| {
            [
                Some(context.content.as_str()),
                context.title.as_deref(),
                context.source.as_deref(),
            ]
            .into_iter()
            .flatten()
        }))
}

fn validate_untrusted_direct_fields(
    req: &CompleteRequest,
    answer_rules: Option<&str>,
) -> Result<(), InternalDisclosureBlocked> {
    if complete_request_untrusted_text(req, answer_rules).any(is_internal_disclosure_request) {
        return Err(InternalDisclosureBlocked);
    }
    Ok(())
}

fn validate_managed_direct_request(
    req: &CompleteRequest,
) -> Result<Option<&str>, InternalDisclosureBlocked> {
    let answer_rules = managed_answer_rules(&req.system)?;
    validate_untrusted_direct_fields(req, answer_rules)?;
    Ok(answer_rules)
}

fn is_internal_disclosure_request(text: &str) -> bool {
    // `user` is an untrusted API field even when it resembles Bluey's internal
    // Question/Context envelope. Scan the entire value so a caller cannot hide
    // a disclosure request in a later paragraph or forged context section.
    let normalized = normalize_guardrail_text(text.trim());
    if normalized.is_empty() {
        return false;
    }

    let bypass_signal = [
        "ignore previous",
        "ignore your instructions",
        "ignore the instructions",
        "forget your instructions",
        "bypass guardrails",
        "bypass your guardrails",
        "jailbreak",
        "developer mode",
        "act as system",
        "act as developer",
    ]
    .iter()
    .any(|signal| normalized.contains(signal));
    if bypass_signal {
        return true;
    }

    let internal_target = [
        "system prompt",
        "system instruction",
        "developer instruction",
        "developer message",
        "hidden instruction",
        "hidden prompt",
        "private instruction",
        "internal prompt",
        "internal instruction",
        "guardrail",
        "behind the scenes",
        "bluey prompt",
        "bluey prompts",
        "bluey instruction",
        "bluey instructions",
        "prompt used in bluey",
        "prompts used in bluey",
        "your prompt",
        "your instructions",
        "instructions you follow",
        "rules you follow",
        "prompt you use",
        "prompt you were given",
    ]
    .iter()
    .any(|signal| normalized.contains(signal));

    if !internal_target {
        return false;
    }

    [
        "show", "give", "reveal", "print", "list", "dump", "share", "tell", "explain", "what is",
        "what are", "display", "output", "send",
    ]
    .iter()
    .any(|verb| normalized.contains(verb))
}

fn looks_like_internal_disclosure_leak(text: &str) -> bool {
    let normalized = normalize_guardrail_text(text);
    internal_disclosure_leak_normalized_start(&normalized).is_some()
}

/// Returns the source byte where the first confirmed disclosure signature
/// begins. The streaming guard uses this only after the fast Boolean detector
/// fires, so ordinary deltas do not pay for the source-offset map.
fn internal_disclosure_leak_start(text: &str) -> Option<usize> {
    let (normalized, source_offsets) = normalize_guardrail_text_with_source_offsets(text);
    let normalized_start = internal_disclosure_leak_normalized_start(&normalized)?;
    source_offsets.get(normalized_start).copied()
}

fn internal_disclosure_leak_normalized_start(normalized: &str) -> Option<usize> {
    if normalized.is_empty() {
        return None;
    }

    let mut earliest = INTERNAL_DISCLOSURE_LEAK_SIGNALS
        .iter()
        .filter_map(|signal| normalized.find(signal))
        .min();

    let mut internal_plan_markers = 0usize;
    let mut earliest_internal_plan_marker = None;
    for marker in INTERNAL_PLAN_DISCLOSURE_MARKERS {
        if let Some(start) = normalized.find(marker) {
            internal_plan_markers += 1;
            earliest_internal_plan_marker = Some(
                earliest_internal_plan_marker.map_or(start, |current: usize| current.min(start)),
            );
        }
    }
    if normalized.contains("bluey answer plan") || internal_plan_markers >= 2 {
        earliest = earliest_option(earliest, earliest_internal_plan_marker);
    }

    if let Some(system_start) = normalized.find("system instructions") {
        let companion_start = ["i follow", "how i work", "bluey"]
            .iter()
            .filter_map(|companion| normalized.find(companion))
            .min();
        if let Some(companion_start) = companion_start {
            earliest = earliest_option(earliest, Some(system_start.min(companion_start)));
        }
    }

    earliest
}

fn earliest_option(left: Option<usize>, right: Option<usize>) -> Option<usize> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn contains_internal_plan_disclosure_anchor(normalized: &str) -> bool {
    INTERNAL_PLAN_DISCLOSURE_MARKERS
        .iter()
        .any(|marker| normalized.contains(marker))
}

fn contains_internal_disclosure_quarantine_anchor(normalized: &str) -> bool {
    INTERNAL_DISCLOSURE_QUARANTINE_ANCHORS
        .iter()
        .any(|anchor| normalized.contains(anchor))
        || contains_internal_plan_disclosure_anchor(normalized)
}

fn internal_disclosure_quarantine_anchor_start(text: &str) -> Option<usize> {
    let (normalized, source_offsets) = normalize_guardrail_text_with_source_offsets(text);
    let normalized_start = INTERNAL_DISCLOSURE_QUARANTINE_ANCHORS
        .iter()
        .chain(INTERNAL_PLAN_DISCLOSURE_MARKERS.iter())
        .filter_map(|anchor| normalized.find(anchor))
        .min()?;
    source_offsets.get(normalized_start).copied()
}

fn normalize_guardrail_text(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut last_was_space = false;
    for original in text.chars() {
        if guardrail_format_char(original) {
            continue;
        }
        let folded = fold_guardrail_compatibility_char(original);
        for ch in folded.to_lowercase() {
            if guardrail_format_char(ch) {
                continue;
            }
            let ch = fold_guardrail_confusable(ch);
            if ch.is_ascii_alphanumeric() {
                normalized.push(ch);
                last_was_space = false;
            } else if !last_was_space {
                normalized.push(' ');
                last_was_space = true;
            }
        }
    }
    normalized.trim().to_string()
}

fn normalize_guardrail_text_with_source_offsets(text: &str) -> (String, Vec<usize>) {
    let mut normalized = String::with_capacity(text.len());
    let mut source_offsets = Vec::with_capacity(text.len());
    let mut last_was_space = false;
    for (source_byte, original) in text.char_indices() {
        if guardrail_format_char(original) {
            continue;
        }
        let folded = fold_guardrail_compatibility_char(original);
        for ch in folded.to_lowercase() {
            if guardrail_format_char(ch) {
                continue;
            }
            let ch = fold_guardrail_confusable(ch);
            if ch.is_ascii_alphanumeric() {
                normalized.push(ch);
                source_offsets.push(source_byte);
                last_was_space = false;
            } else if !last_was_space {
                normalized.push(' ');
                source_offsets.push(source_byte);
                last_was_space = true;
            }
        }
    }

    let trimmed_start = normalized
        .as_bytes()
        .iter()
        .position(|byte| *byte != b' ')
        .unwrap_or(normalized.len());
    let trimmed_end = normalized
        .as_bytes()
        .iter()
        .rposition(|byte| *byte != b' ')
        .map_or(trimmed_start, |index| index + 1);
    (
        normalized[trimmed_start..trimmed_end].to_string(),
        source_offsets[trimmed_start..trimmed_end].to_vec(),
    )
}

fn guardrail_format_char(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{00ad}'
                | '\u{034f}'
                | '\u{061c}'
                | '\u{180b}'..='\u{180f}'
                | '\u{200b}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{fe00}'..='\u{fe0f}'
                | '\u{feff}'
        )
        || matches!(ch, '\u{0300}'..='\u{036f}')
}

fn fold_guardrail_compatibility_char(ch: char) -> char {
    let value = ch as u32;
    let ascii = match value {
        0xff01..=0xff5e => Some(value - 0xfee0),
        0x24b6..=0x24cf => Some(value - 0x24b6 + u32::from(b'A')),
        0x24d0..=0x24e9 => Some(value - 0x24d0 + u32::from(b'a')),
        0x1d400..=0x1d419 => Some(value - 0x1d400 + u32::from(b'A')),
        0x1d41a..=0x1d433 => Some(value - 0x1d41a + u32::from(b'a')),
        0x1d44e..=0x1d454 => Some(value - 0x1d44e + u32::from(b'a')),
        0x1d456..=0x1d467 => Some(value - 0x1d456 + u32::from(b'i')),
        0x1d468..=0x1d481 => Some(value - 0x1d468 + u32::from(b'A')),
        0x1d482..=0x1d49b => Some(value - 0x1d482 + u32::from(b'a')),
        0x1d4d0..=0x1d4e9 => Some(value - 0x1d4d0 + u32::from(b'A')),
        0x1d4ea..=0x1d503 => Some(value - 0x1d4ea + u32::from(b'a')),
        0x1d51e..=0x1d537 => Some(value - 0x1d51e + u32::from(b'a')),
        0x1d552..=0x1d56b => Some(value - 0x1d552 + u32::from(b'a')),
        0x1d56c..=0x1d585 => Some(value - 0x1d56c + u32::from(b'A')),
        0x1d586..=0x1d59f => Some(value - 0x1d586 + u32::from(b'a')),
        0x1d5a0..=0x1d5b9 => Some(value - 0x1d5a0 + u32::from(b'A')),
        0x1d5ba..=0x1d5d3 => Some(value - 0x1d5ba + u32::from(b'a')),
        0x1d5d4..=0x1d5ed => Some(value - 0x1d5d4 + u32::from(b'A')),
        0x1d5ee..=0x1d607 => Some(value - 0x1d5ee + u32::from(b'a')),
        0x1d608..=0x1d621 => Some(value - 0x1d608 + u32::from(b'A')),
        0x1d622..=0x1d63b => Some(value - 0x1d622 + u32::from(b'a')),
        0x1d63c..=0x1d655 => Some(value - 0x1d63c + u32::from(b'A')),
        0x1d656..=0x1d66f => Some(value - 0x1d656 + u32::from(b'a')),
        0x1d670..=0x1d689 => Some(value - 0x1d670 + u32::from(b'A')),
        0x1d68a..=0x1d6a3 => Some(value - 0x1d68a + u32::from(b'a')),
        0x1d7ce..=0x1d7d7 => Some(value - 0x1d7ce + u32::from(b'0')),
        0x1d7d8..=0x1d7e1 => Some(value - 0x1d7d8 + u32::from(b'0')),
        0x1d7e2..=0x1d7eb => Some(value - 0x1d7e2 + u32::from(b'0')),
        0x1d7ec..=0x1d7f5 => Some(value - 0x1d7ec + u32::from(b'0')),
        0x1d7f6..=0x1d7ff => Some(value - 0x1d7f6 + u32::from(b'0')),
        _ => None,
    };
    ascii.and_then(char::from_u32).unwrap_or(ch)
}

fn fold_guardrail_confusable(ch: char) -> char {
    match ch {
        '\u{210e}' => 'h',
        'а' | 'ɑ' | 'α' => 'a',
        'в' | 'β' => 'b',
        'с' | 'ϲ' => 'c',
        'ԁ' => 'd',
        'е' | 'ε' => 'e',
        'ғ' => 'f',
        'ɡ' => 'g',
        'һ' | 'հ' => 'h',
        'і' | 'ι' | 'ı' => 'i',
        'ј' => 'j',
        'к' | 'κ' => 'k',
        'ӏ' | 'ⅼ' => 'l',
        'м' | 'μ' => 'm',
        'ո' => 'n',
        'о' | 'ο' | 'օ' => 'o',
        'р' | 'ρ' => 'p',
        'ԛ' => 'q',
        'г' => 'r',
        'ѕ' => 's',
        'т' | 'τ' => 't',
        'υ' | 'ս' => 'u',
        'ν' | 'ѵ' => 'v',
        'ԝ' | 'ω' => 'w',
        'х' | 'χ' => 'x',
        'у' | 'γ' => 'y',
        'ᴢ' | 'ζ' => 'z',
        _ => ch,
    }
}

#[derive(Default)]
struct CanvasSpokenStream {
    pending_line: String,
    seen_spoken_heading: bool,
    stopped_at_canvas: bool,
    content_line: bool,
    delivered_chars: usize,
}

/// Holds the implementation portion of a strict first-principles LRU answer
/// until the completed provider response passes the pre-billing contract
/// checks. Text before the opening fence remains streaming, so this does not
/// change normal chat latency or any non-strict code request.
struct StrictLruCodeStreamGate {
    enabled: bool,
    code_fence_seen: bool,
    pending_fence_prefix: String,
    held_code: String,
    delivered_meaningful_content: bool,
}

impl StrictLruCodeStreamGate {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            code_fence_seen: false,
            pending_fence_prefix: String::new(),
            held_code: String::new(),
            delivered_meaningful_content: false,
        }
    }

    /// Releases ordinary text immediately, retaining up to two trailing
    /// backticks so a fence split across provider deltas is still recognized.
    /// From the first opening fence onward, content remains private until the
    /// complete response is validated.
    fn push(&mut self, delta: &str) -> Option<String> {
        if !self.enabled {
            self.delivered_meaningful_content |= delta.chars().any(|ch| !ch.is_whitespace());
            return (!delta.is_empty()).then(|| delta.to_string());
        }
        if self.code_fence_seen {
            self.held_code.push_str(delta);
            return None;
        }

        self.pending_fence_prefix.push_str(delta);
        if let Some(fence_start) = self.pending_fence_prefix.find("```") {
            let held = self.pending_fence_prefix.split_off(fence_start);
            self.held_code.push_str(&held);
            self.code_fence_seen = true;
            return self.release_pending_prefix();
        }

        let trailing_backticks = self
            .pending_fence_prefix
            .as_bytes()
            .iter()
            .rev()
            .take_while(|byte| **byte == b'`')
            .count()
            .min(2);
        let release_bytes = self
            .pending_fence_prefix
            .len()
            .saturating_sub(trailing_backticks);
        if release_bytes == 0 {
            return None;
        }
        let released: String = self.pending_fence_prefix.drain(..release_bytes).collect();
        self.record_delivery(&released, false)
    }

    /// Releases the held code only after the final response has passed all
    /// contract checks. An unterminated candidate fence is ordinary text only
    /// when no opening fence was ever recognized.
    fn release_after_quality_pass(&mut self) -> Option<String> {
        let mut released = std::mem::take(&mut self.pending_fence_prefix);
        released.push_str(&std::mem::take(&mut self.held_code));
        self.record_delivery(&released, true)
    }

    /// Error paths must not strand an ordinary prefix or one/two backticks
    /// while a fence is still ambiguous. Once an opening fence was recognized,
    /// however, the buffered implementation is deliberately discarded.
    fn release_after_failure(&mut self) -> Option<String> {
        if self.code_fence_seen {
            self.pending_fence_prefix.clear();
            self.held_code.clear();
            return None;
        }
        let released = std::mem::take(&mut self.pending_fence_prefix);
        self.record_delivery(&released, false)
    }

    fn has_delivered(&self) -> bool {
        self.delivered_meaningful_content
    }

    fn release_pending_prefix(&mut self) -> Option<String> {
        let released = std::mem::take(&mut self.pending_fence_prefix);
        self.record_delivery(&released, false)
    }

    fn record_delivery(&mut self, released: &str, terminal_success: bool) -> Option<String> {
        let released = if self.enabled && !self.delivered_meaningful_content {
            released.trim_start()
        } else {
            released
        };
        let released = if self.enabled && terminal_success {
            released.trim_end()
        } else {
            released
        };
        if released.is_empty() {
            None
        } else {
            self.delivered_meaningful_content |= released.chars().any(|ch| !ch.is_whitespace());
            Some(released.to_string())
        }
    }
}

fn append_visible_delta(existing: &mut Option<String>, suffix: Option<String>) {
    let Some(suffix) = suffix else {
        return;
    };
    if let Some(existing) = existing {
        existing.push_str(&suffix);
    } else {
        *existing = Some(suffix);
    }
}

impl CanvasSpokenStream {
    /// Streams only complete lines inside `### Spoken answer`. The next
    /// markdown heading stays buffered and is never exposed to the overlay,
    /// even when its bytes are split across provider deltas.
    fn push(&mut self, delta: &str) -> Option<String> {
        if self.stopped_at_canvas {
            return None;
        }
        self.pending_line.push_str(delta);
        let mut released = String::new();

        loop {
            if self.content_line {
                if let Some(newline) = self.pending_line.find('\n') {
                    let line: String = self.pending_line.drain(..=newline).collect();
                    released.push_str(&line);
                    self.content_line = false;
                    continue;
                }
                released.push_str(&std::mem::take(&mut self.pending_line));
                break;
            }

            let Some(newline) = self.pending_line.find('\n') else {
                if self.seen_spoken_heading
                    && self.pending_line.chars().any(|ch| !ch.is_whitespace())
                    && !self.pending_line.trim_start().starts_with('#')
                    && !could_be_canvas_detail_heading_prefix(&self.pending_line)
                {
                    self.content_line = true;
                    released.push_str(&std::mem::take(&mut self.pending_line));
                }
                break;
            };

            let line: String = self.pending_line.drain(..=newline).collect();
            let content = line.trim_end_matches(['\r', '\n']);
            if !self.seen_spoken_heading {
                if is_spoken_answer_heading(content) {
                    self.seen_spoken_heading = true;
                }
                continue;
            }
            if is_canvas_detail_heading(content) {
                self.stopped_at_canvas = true;
                self.pending_line.clear();
                break;
            }
            if self.delivered_chars == 0 && content.trim().is_empty() {
                continue;
            }
            released.push_str(&line);
        }

        if released.trim().is_empty() {
            return None;
        }
        self.delivered_chars = self
            .delivered_chars
            .saturating_add(released.chars().count());
        Some(released)
    }

    fn has_delivered(&self) -> bool {
        self.delivered_chars > 0
    }

    /// Flushes a final spoken line. If the provider ignored the spoken-section
    /// contract, use the already-sanitized terminal visible answer once.
    fn finish(&mut self, fallback_visible: &str) -> String {
        if !self.stopped_at_canvas && self.seen_spoken_heading {
            let content = self.pending_line.trim_end_matches(['\r', '\n']);
            if !is_canvas_detail_heading(content) && !content.trim().is_empty() {
                let released = std::mem::take(&mut self.pending_line);
                self.delivered_chars = self
                    .delivered_chars
                    .saturating_add(released.chars().count());
                return released;
            }
        }
        if !self.has_delivered() {
            return fallback_visible.to_string();
        }
        String::new()
    }
}

fn completion_delta_event(text: &str) -> Event {
    Event::default().data(
        serde_json::json!({
            "choices": [
                { "delta": { "content": text } }
            ]
        })
        .to_string(),
    )
}

fn managed_usage_now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn llm_usage_reservation_ttl() -> Duration {
    let seconds = std::env::var("BLUEY_LLM_USAGE_RESERVATION_TTL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(DEFAULT_LLM_USAGE_RESERVATION_TTL_SECS);
    Duration::from_secs(seconds)
}

fn reconcile_expired_llm_usage(
    pool: &crate::db::DbPool,
    account_id: &str,
) -> Result<(), Box<(StatusCode, Json<ApiError>)>> {
    match usage_reservations::reconcile_expired_for_account(
        pool,
        account_id,
        managed_usage_now_ms(),
    ) {
        Ok(0) => Ok(()),
        Ok(released) => {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                released,
                "reconciled expired managed usage reservations"
            );
            Ok(())
        }
        Err(error) => {
            tracing::error!(
                account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                error = %error,
                "failed to reconcile expired managed usage reservations"
            );
            Err(Box::new((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "managed usage reconciliation failed".into(),
                    reason: Some("usage_reconciliation_failed".into()),
                    ..Default::default()
                }),
            )))
        }
    }
}

fn reserve_llm_usage(
    state: &AppState,
    account: &Account,
    request_id: &str,
    estimated_customer_cents: i64,
    estimated_upstream_cents: i64,
    reason: &'static str,
) -> Result<ReservedUsage, Box<(StatusCode, Json<ApiError>)>> {
    let created_at_ms = managed_usage_now_ms();
    let ttl_ms = llm_usage_reservation_ttl()
        .as_millis()
        .min(i64::MAX as u128) as i64;
    let expires_at_ms = created_at_ms.saturating_add(ttl_ms);
    match usage_reservations::reserve(
        &state.pool,
        ReserveUsageInput {
            account_id: &account.id,
            request_id,
            kind: "llm",
            reason,
            estimated_customer_cents,
            estimated_upstream_cents,
            upstream_spend_guard: state.config.upstream_spend_guard,
            created_at_ms,
            expires_at_ms,
        },
    ) {
        Ok(reservation) => {
            spawn_usage_expiry_reconciler(
                state.pool.clone(),
                account.id.clone(),
                request_id.to_string(),
                reservation.attempt,
                reservation.expires_at_ms,
            );
            Ok(reservation)
        }
        Err(usage_reservations::UsageReservationError::InsufficientBalance) => {
            let _ = idempotency::mark_failed(&state.pool, &account.id, request_id);
            let balance_cents = balance::current_balance(&state.pool, &account.id).unwrap_or(0);
            Err(Box::new((
                StatusCode::PAYMENT_REQUIRED,
                Json(ApiError {
                    error: "insufficient balance".into(),
                    balance_cents: Some(balance_cents),
                    estimated_cost_cents: Some(estimated_customer_cents),
                    reason: Some("insufficient_balance".into()),
                    reload_url: Some(format!("{}/reload", state.config.public_url)),
                    ..Default::default()
                }),
            )))
        }
        Err(usage_reservations::UsageReservationError::InProgress) => Err(Box::new((
            StatusCode::CONFLICT,
            Json(ApiError {
                error: "request already has an active usage reservation".into(),
                reason: Some("request_in_progress".into()),
                ..Default::default()
            }),
        ))),
        Err(usage_reservations::UsageReservationError::AlreadySettled) => Err(Box::new((
            StatusCode::CONFLICT,
            Json(ApiError {
                error: "request usage is settled but its response needs reconciliation".into(),
                reason: Some("request_settled_reconciliation_pending".into()),
                ..Default::default()
            }),
        ))),
        Err(usage_reservations::UsageReservationError::AccountUnavailable) => {
            let _ = idempotency::mark_failed(&state.pool, &account.id, request_id);
            Err(Box::new((
                StatusCode::FORBIDDEN,
                Json(ApiError {
                    error: "Account usage is unavailable.".into(),
                    reason: Some("account_unavailable".into()),
                    ..Default::default()
                }),
            )))
        }
        Err(usage_reservations::UsageReservationError::UpstreamSpendLimit) => {
            let _ = idempotency::release(&state.pool, &account.id, request_id);
            Err(Box::new((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiError {
                    error: "Managed AI is temporarily at its upstream spend limit.".into(),
                    reason: Some("upstream_spend_limit".into()),
                    ..Default::default()
                }),
            )))
        }
        Err(error) => {
            let _ = idempotency::release(&state.pool, &account.id, request_id);
            tracing::error!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                request_id,
                error = %error,
                "managed usage reservation failed"
            );
            Err(Box::new((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "managed usage reservation failed".into(),
                    reason: Some("usage_reservation_failed".into()),
                    ..Default::default()
                }),
            )))
        }
    }
}

fn spawn_usage_expiry_reconciler(
    pool: crate::db::DbPool,
    account_id: String,
    request_id: String,
    attempt: i64,
    expires_at_ms: i64,
) {
    tokio::spawn(async move {
        let delay_ms = match usage_reservations::reservation_expiry_delay_ms(
            &pool,
            &account_id,
            &request_id,
            attempt,
            expires_at_ms,
        ) {
            Ok(Some(delay_ms)) => delay_ms,
            Ok(None) => return,
            Err(error) => {
                tracing::error!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
                    request_id,
                    error = %error,
                    "failed to read DB-clock managed usage expiry"
                );
                return;
            }
        };
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms as u64)).await;
        }
        match usage_reservations::release_expired_attempt(
            &pool,
            &account_id,
            &request_id,
            attempt,
            expires_at_ms,
        ) {
            Ok(true) => {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
                    request_id,
                    attempt,
                    "released expired managed usage reservation"
                );
            }
            Ok(false) => {}
            Err(error) => {
                tracing::error!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
                    request_id,
                    error = %error,
                    "failed delayed managed usage reconciliation"
                );
            }
        }
    });
}

fn release_llm_usage(
    pool: &crate::db::DbPool,
    account_id: &str,
    request_id: &str,
    reason: &'static str,
) {
    match usage_reservations::release(pool, account_id, request_id, reason, managed_usage_now_ms())
    {
        Ok(released) => tracing::info!(
            account_id_hash = %cue_core::account_id_hash_prefix(account_id),
            request_id,
            refunded_cents = released.refunded_cents,
            refunded_trial_seconds = released.refunded_trial_seconds,
            reason,
            "managed usage reservation released"
        ),
        Err(usage_reservations::UsageReservationError::AlreadyReleased)
        | Err(usage_reservations::UsageReservationError::NotFound) => {}
        Err(error) => tracing::error!(
            account_id_hash = %cue_core::account_id_hash_prefix(account_id),
            request_id,
            reason,
            error = %error,
            "failed to release managed usage reservation"
        ),
    }
}

fn fail_stream_llm_usage(
    pool: &crate::db::DbPool,
    account_id: &str,
    request_id: &str,
    delivered_delta: bool,
    reason: &'static str,
) {
    if delivered_delta {
        let _ = idempotency::mark_failed(pool, account_id, request_id);
    }
    release_llm_usage(pool, account_id, request_id, reason);
}

async fn settle_llm_usage_with_retry(
    pool: &crate::db::DbPool,
    account_id: &str,
    request_id: &str,
    actual_customer_cents: i64,
    elapsed_ms: i64,
    reason: &'static str,
    events: &[usage_reservations::SettlementUsageEvent],
) -> Result<SettledUsage, usage_reservations::UsageReservationError> {
    let mut last_error = None;
    for attempt in 1..=LLM_SETTLEMENT_RETRY_ATTEMPTS {
        match usage_reservations::settle_with_events(
            pool,
            account_id,
            request_id,
            actual_customer_cents,
            elapsed_ms,
            reason,
            managed_usage_now_ms(),
            events,
        ) {
            Ok(settled) => return Ok(settled),
            Err(error @ usage_reservations::UsageReservationError::Db(_))
                if attempt < LLM_SETTLEMENT_RETRY_ATTEMPTS =>
            {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                    request_id,
                    attempt,
                    error = %error,
                    "managed usage settlement attempt failed; retrying"
                );
                last_error = Some(error);
                tokio::time::sleep(Duration::from_millis(LLM_SETTLEMENT_RETRY_DELAY_MS)).await;
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.expect("settlement retry loop must retain its database error"))
}

fn detach_router_stream(mut source: RouterSseStream) -> RouterSseStream {
    // The source must keep running after a client stops polling so billing and
    // idempotency settle. Use a lossless queue: the former bounded try_send
    // path silently dropped ordinary deltas and only appeared correct while
    // the router emitted one full-answer delta.
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(event) = source.next().await {
            // A closed receiver means the client disconnected. Continue
            // draining the source even though there is nowhere left to send.
            let _ = sender.send(event);
        }
    });
    Box::pin(async_stream::stream! {
        while let Some(event) = receiver.recv().await {
            yield event;
        }
    })
}

fn billing_restricted_error(account: &Account) -> Option<(StatusCode, Json<ApiError>)> {
    if !account.billing_restricted {
        return None;
    }
    Some((
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "Account usage is paused while billing is under review.".into(),
            reason: Some(
                account
                    .billing_restriction_reason
                    .clone()
                    .unwrap_or_else(|| "billing_restricted".into()),
            ),
            ..Default::default()
        }),
    ))
}

enum LiveAccountState {
    Active,
    BillingRestricted,
    Missing,
    CheckFailed(String),
}

fn live_account_state(pool: &crate::db::DbPool, account_id: &str) -> LiveAccountState {
    match Account::fetch_by_id(pool, account_id) {
        Ok(Some(account)) if account.billing_restricted => LiveAccountState::BillingRestricted,
        Ok(Some(_)) => LiveAccountState::Active,
        Ok(None) => LiveAccountState::Missing,
        Err(e) => LiveAccountState::CheckFailed(e.to_string()),
    }
}

fn live_account_error_payload(state: LiveAccountState) -> Option<serde_json::Value> {
    match state {
        LiveAccountState::Active => None,
        LiveAccountState::BillingRestricted => Some(serde_json::json!({
            "error": "Account usage is paused while billing is under review.",
            "reason": "billing_restricted",
        })),
        LiveAccountState::Missing => Some(serde_json::json!({
            "error": "This Bluey account was deleted. The answer was stopped and was not billed.",
            "reason": "account_deleted",
        })),
        LiveAccountState::CheckFailed(_error) => Some(serde_json::json!({
            "error": "Bluey could not verify this account before billing, so the answer was stopped.",
            "reason": "account_check_failed",
        })),
    }
}

fn account_not_active_error(
    pool: &crate::db::DbPool,
    account_id: &str,
) -> Option<(StatusCode, Json<ApiError>)> {
    match live_account_state(pool, account_id) {
        LiveAccountState::Active => None,
        LiveAccountState::BillingRestricted => Some((
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "Account usage is paused while billing is under review.".into(),
                reason: Some("billing_restricted".into()),
                ..Default::default()
            }),
        )),
        LiveAccountState::Missing => Some((
            StatusCode::GONE,
            Json(ApiError {
                error: "This Bluey account was deleted.".into(),
                reason: Some("account_deleted".into()),
                ..Default::default()
            }),
        )),
        LiveAccountState::CheckFailed(_error) => Some((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError {
                error: "Bluey could not verify this account before billing.".into(),
                reason: Some("account_check_failed".into()),
                retry_after_secs: Some(5),
                ..Default::default()
            }),
        )),
    }
}

#[derive(Deserialize)]
pub struct CompleteRequest {
    /// Client-supplied idempotency key. REQUIRED. Codex S4.1: a retry
    /// after a network timeout/lost response must not be charged twice.
    /// The server rejects duplicate (account_id, request_id) pairs by
    /// returning the cached response (200 if completed) or 409 (if the
    /// original is still in flight).
    pub request_id: String,
    pub system: String,
    pub user: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    /// Optional provider-neutral reasoning effort. Supported values:
    /// off/low/medium/high/auto. Server policy still decides whether the
    /// selected provider/model can safely apply it.
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// Optional internal thinking token budget for provider families that
    /// expose it. Clamped server-side.
    #[serde(default)]
    pub thinking_budget_tokens: Option<u32>,
    pub lane: String,
    #[serde(default)]
    pub estimated_input_tokens: Option<i64>,
    /// User-approved screenshot/screen-analysis images as provider-compatible
    /// data URLs. Presence of any image forces the managed lane to `vision`.
    #[serde(default)]
    pub image_data_urls: Vec<String>,
    /// Explicit capability/version gate for provenance-bearing answer context.
    /// Legacy clients omit this field and retain their original prompt path.
    #[serde(default)]
    pub context_schema_version: Option<u16>,
    /// Provenance-bearing evidence supplied by trusted Bluey clients. Content
    /// remains untrusted, but its source kind cannot be changed by text inside
    /// an attached document.
    #[serde(default)]
    pub context: Vec<cue_core::AnswerContext>,
}

/// Provider-facing prompt fields after every directly supplied request field
/// has passed the disclosure guard. Server-created context is appended only
/// after crossing this typed boundary.
#[derive(Clone, Copy)]
struct TrustedInternalEnvelope<'a> {
    system: &'a str,
    user: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManagedSystemAuthority {
    ExternalClientContract,
    TrustedServerSystem,
}

impl<'a> TrustedInternalEnvelope<'a> {
    fn validate_direct_request(
        req: &'a CompleteRequest,
        system_authority: ManagedSystemAuthority,
    ) -> Result<Self, InternalDisclosureBlocked> {
        match system_authority {
            ManagedSystemAuthority::ExternalClientContract => {
                validate_managed_direct_request(req)?;
            }
            ManagedSystemAuthority::TrustedServerSystem => {
                validate_untrusted_direct_fields(req, None)?;
            }
        }
        Ok(Self {
            system: &req.system,
            user: &req.user,
        })
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CompleteResponse {
    pub text: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_cents: i64,
    pub balance_cents_after: i64,
    pub trial_seconds_remaining: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<CompleteSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompleteSource {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
}

#[derive(Serialize, Default)]
pub struct ApiError {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reload_url: Option<String>,
}

#[derive(Clone, Copy)]
struct PricedRoute {
    provider: &'static str,
    model: &'static str,
    pricing: pricing::ModelPricing,
    estimated_cost_cents: i64,
    estimated_bluey_cost_cents: i64,
}

struct PricedTranscribeRoute {
    provider: &'static str,
    model: String,
    pricing: &'static pricing::ModelPricing,
    estimated_cost_cents: i64,
    estimated_bluey_cost_cents: i64,
}

fn capacity_error(reason: &str, retry_after_secs: u64) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(ApiError {
            error: "Bluey is handling a burst right now; retry shortly".into(),
            reason: Some(reason.to_string()),
            retry_after_secs: Some(retry_after_secs),
            ..Default::default()
        }),
    )
}

fn upstream_spend_guard_error() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiError {
            error: "Bluey's upstream spend safety boundary is temporarily active".into(),
            reason: Some("upstream_spend_guard".into()),
            retry_after_secs: Some(UPSTREAM_SPEND_GUARD_RETRY_AFTER_SECS),
            ..Default::default()
        }),
    )
}

fn release_and_upstream_spend_guard_error(
    pool: &crate::db::DbPool,
    account_id: &str,
    request_id: &str,
) -> (StatusCode, Json<ApiError>) {
    let _ = idempotency::release(pool, account_id, request_id);
    upstream_spend_guard_error()
}

fn internal_capacity_retry_delay(denied: &crate::rate_limit::CapacityDenied) -> Option<Duration> {
    let retry_after_secs = denied.retry_after_secs.clamp(1, 2);
    let is_account_guard = denied.reason.starts_with("account_");
    let is_provider_capacity = denied.reason.starts_with("provider_")
        || denied.reason.contains("provider_key")
        || denied.reason.contains("cooling");
    (!is_account_guard && is_provider_capacity && denied.retry_after_secs <= 2)
        .then_some(Duration::from_secs(retry_after_secs))
}

fn release_and_capacity_error(
    pool: &crate::db::DbPool,
    account_id: &str,
    request_id: &str,
    reason: &str,
    retry_after_secs: u64,
) -> (StatusCode, Json<ApiError>) {
    let _ = idempotency::release(pool, account_id, request_id);
    capacity_error(reason, retry_after_secs)
}

fn capacity_short_wait_max_secs() -> u64 {
    std::env::var("BLUEY_CAPACITY_SHORT_WAIT_MAX_SECS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .map(|secs| secs.min(10))
        .unwrap_or(DEFAULT_CAPACITY_SHORT_WAIT_MAX_SECS)
}

fn short_capacity_wait_secs(retry_after_secs: u64) -> Option<u64> {
    let max_secs = capacity_short_wait_max_secs();
    if max_secs == 0 {
        return None;
    }
    let wait_secs = retry_after_secs.max(1);
    (wait_secs <= max_secs).then_some(wait_secs)
}

// Axum handler errors intentionally carry the complete bounded JSON error envelope.
#[allow(clippy::result_large_err)]
async fn check_account_llm_or_short_wait(
    state: &AppState,
    account_id: &str,
    request_id: &str,
    session_ref: &str,
    streaming: bool,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let first_denied = match state.rate_limiters.check_account_llm(account_id).await {
        Ok(()) => return Ok(()),
        Err(denied) => denied,
    };
    if let Some(wait_secs) = short_capacity_wait_secs(first_denied.retry_after_secs) {
        tracing::info!(
            account_id_hash = %cue_core::account_id_hash_prefix(account_id),
            request_id = %request_id,
            session_ref = %session_ref,
            streaming,
            reason = first_denied.reason,
            retry_after_secs = first_denied.retry_after_secs,
            wait_secs,
            "account answer burst guard short-waiting before dispatch"
        );
        tokio::time::sleep(Duration::from_secs(wait_secs)).await;
        match state.rate_limiters.check_account_llm(account_id).await {
            Ok(()) => {
                tracing::info!(
                    account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                    request_id = %request_id,
                    session_ref = %session_ref,
                    streaming,
                    waited_secs = wait_secs,
                    "account answer burst guard passed after short wait"
                );
                return Ok(());
            }
            Err(denied) => {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                    request_id = %request_id,
                    session_ref = %session_ref,
                    streaming,
                    reason = denied.reason,
                    retry_after_secs = denied.retry_after_secs,
                    waited_secs = wait_secs,
                    "account answer burst guard still busy after short wait"
                );
                return Err(release_and_capacity_error(
                    &state.pool,
                    account_id,
                    request_id,
                    denied.reason,
                    denied.retry_after_secs,
                ));
            }
        }
    }
    Err(release_and_capacity_error(
        &state.pool,
        account_id,
        request_id,
        first_denied.reason,
        first_denied.retry_after_secs,
    ))
}

fn prior_provider_exposure_error(
    state: &AppState,
    account_id: &str,
    request_id: &str,
    scope_key: &str,
) -> Option<(StatusCode, Json<ApiError>)> {
    match crate::db::jobs_provider_cost_holds::has_generation_exposure(
        &state.pool,
        account_id,
        scope_key,
    ) {
        Ok(false) => None,
        Ok(true) | Err(_) => {
            let _ = idempotency::release(&state.pool, account_id, request_id);
            Some((
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "prior provider exposure prevents a safe redispatch".into(),
                    reason: Some("provider_exposure_ambiguous".into()),
                    ..Default::default()
                }),
            ))
        }
    }
}

fn provider_accounting_pending_error(
    pool: &crate::db::DbPool,
    account_id: &str,
    request_id: &str,
) -> (StatusCode, Json<ApiError>) {
    // Do not release the idempotency or customer reservation here. A provider
    // response exists, while its durable exact settlement has not yet been
    // confirmed. The provider guard's Drop retry preserves conservative spend
    // truth and reconciliation can terminalize the customer side later.
    let _ = idempotency::mark_failed(pool, account_id, request_id);
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiError {
            error: "provider accounting is pending reconciliation; use a new request after operator review"
                .into(),
            reason: Some("provider_accounting_pending".into()),
            retry_after_secs: Some(60),
            ..Default::default()
        }),
    )
}

/// Persist the provider-attempt side of accounting before any caller is
/// allowed to settle its customer-facing root. All managed provider endpoints
/// share this transition so a durable-settlement failure has one fail-closed
/// result and never releases or charges the customer reservation.
#[allow(clippy::result_large_err)]
fn settle_provider_attempt_before_customer(
    pool: &crate::db::DbPool,
    account_id: &str,
    root_request_id: &str,
    guard: &mut provider_cost_guard::ProviderCostGuard,
    event: UsageEvent,
    actual_cost_cents: i64,
    usage_provenance: pricing::UsageProvenance,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    guard
        .settle(event, actual_cost_cents, usage_provenance)
        .map_err(|error| {
            tracing::error!(
                request_id = root_request_id,
                error = %error,
                "provider attempt settlement pending reconciliation"
            );
            provider_accounting_pending_error(pool, account_id, root_request_id)
        })
}

fn take_selected_provider_attempt_guard(
    guard: &mut Option<Box<provider_cost_guard::ProviderCostGuard>>,
) -> anyhow::Result<Box<provider_cost_guard::ProviderCostGuard>> {
    guard
        .take()
        .ok_or_else(|| anyhow::anyhow!("selected provider attempt has no armed cost guard"))
}

fn settle_selected_provider_attempt_conservative(
    guard: &mut Option<Box<provider_cost_guard::ProviderCostGuard>>,
) -> anyhow::Result<()> {
    let mut guard = take_selected_provider_attempt_guard(guard)?;
    guard.settle_conservative()?;
    Ok(())
}

fn returned_route_bluey_cost_or_cap(
    provider: &str,
    model: &str,
    input_units: i64,
    output_units: i64,
) -> i64 {
    pricing::lookup(provider, model)
        .map(|price| pricing::compute_cost(price, input_units, output_units).0)
        .unwrap_or(crate::db::usage::MAX_AUTHORITATIVE_EVENT_COST_CENTS)
}

fn prior_provider_prefix_exposure_error(
    pool: &crate::db::DbPool,
    account_id: &str,
    request_id: &str,
) -> Option<(StatusCode, Json<ApiError>)> {
    let prefix = format!("router:{request_id}:");
    match crate::db::jobs_provider_cost_holds::has_scope_prefix_exposure(pool, account_id, &prefix)
    {
        Ok(false) => None,
        Ok(true) | Err(_) => {
            let _ = idempotency::release(pool, account_id, request_id);
            Some((
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "prior provider exposure prevents a safe request replay".into(),
                    reason: Some("provider_exposure_ambiguous".into()),
                    ..Default::default()
                }),
            ))
        }
    }
}

include!("router/provider_runtime.rs");
include!("router/answer_planning.rs");
include!("router/behavioral_grounding.rs");
include!("router/domain_intents.rs");
include!("router/prompt_contracts.rs");
include!("router/web_search.rs");
include!("router/streaming_completion.rs");
include!("router/completion.rs");

mod response_artifacts;
use response_artifacts::{
    canvas_overlay_text, could_be_canvas_detail_heading_prefix, has_code_shape,
    is_canvas_detail_heading, is_spoken_answer_heading, response_artifact_for_plan,
    router_cost_label, router_cost_label_with_web_search, visible_response_text_for_plan,
    ResponseArtifact,
};
#[cfg(test)]
use response_artifacts::{
    response_artifact, response_artifact_for_output, visible_response_text_for_artifact,
};

mod embeddings;
pub(crate) mod provider_cost_guard;
pub use embeddings::{
    embed, embed_batch, EmbedBatchRequest, EmbedBatchResponse, EmbedRequest, EmbedResponse,
};

mod transcribe;
pub use transcribe::{transcribe, TranscribeQuery, TranscribeResponse};
#[cfg(test)]
#[path = "router/tests.rs"]
mod tests;
