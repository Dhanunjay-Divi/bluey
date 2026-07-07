//! Decisions ledger — pure types + anti-hallucination verify harness.
//!
//! Every N transcript turns the daemon asks a cheap LLM lane to extract the
//! meeting's decisions / constraints / owners as JSON (see [`EXTRACTION_PROMPT`]).
//! Whatever the model returns is run through [`parse_and_verify`], which is the
//! actual guarantee: an item survives only if its `quote` is a verbatim substring
//! of the transcript window it was extracted from. The prompt is a request; this
//! module is the enforcement.
//!
//! This module does no I/O and knows nothing about providers — it is fully
//! unit-testable without an LLM.

use serde::{Deserialize, Serialize};

/// The kind of a ledger item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerKind {
    Decision,
    Constraint,
    Owner,
}

impl LedgerKind {
    /// Human label ("Decision" / "Constraint" / "Owner") — used by the render
    /// block and by the cross-meeting facts memory when indexing items.
    pub fn label(self) -> &'static str {
        match self {
            LedgerKind::Decision => "Decision",
            LedgerKind::Constraint => "Constraint",
            LedgerKind::Owner => "Owner",
        }
    }
}

/// A single verified ledger entry. `quote` is guaranteed to be a verbatim
/// substring of the transcript window it came from (enforced by the harness).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerItem {
    pub kind: LedgerKind,
    /// Normalized statement (e.g. "Phased rollout for Q3").
    pub text: String,
    /// Verbatim proof sentence copied from the transcript.
    pub quote: String,
    /// Speaker label that said it, if it maps to someone in the window.
    #[serde(default)]
    pub speaker: Option<String>,
}

impl LedgerItem {
    /// Key used for dedup: kind + normalized text.
    fn dedup_key(&self) -> String {
        format!("{}|{}", self.kind.label(), normalize(&self.text))
    }
}

/// The accumulated ledger for a meeting. Capacity-bounded (oldest evicted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerState {
    items: Vec<LedgerItem>,
    cap: usize,
}

impl Default for LedgerState {
    fn default() -> Self {
        Self::new(DEFAULT_LEDGER_CAP)
    }
}

/// Default maximum number of items retained in a ledger.
pub const DEFAULT_LEDGER_CAP: usize = 40;

impl LedgerState {
    pub fn new(cap: usize) -> Self {
        Self {
            items: Vec::new(),
            cap: cap.max(1),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn items(&self) -> &[LedgerItem] {
        &self.items
    }

    /// Merge freshly verified items in, skipping duplicates (by kind + normalized
    /// text). Returns how many were newly added. Oldest items are evicted past cap.
    pub fn merge(&mut self, incoming: impl IntoIterator<Item = LedgerItem>) -> usize {
        let mut seen: std::collections::HashSet<String> =
            self.items.iter().map(LedgerItem::dedup_key).collect();
        let mut added = 0;
        for item in incoming {
            let key = item.dedup_key();
            if seen.insert(key) {
                self.items.push(item);
                added += 1;
            }
        }
        // Evict oldest to respect cap.
        if self.items.len() > self.cap {
            let overflow = self.items.len() - self.cap;
            self.items.drain(0..overflow);
        }
        added
    }

    /// Render the ledger as a pinned context block for the answer prompt.
    /// Returns `None` when empty (nothing to pin).
    pub fn render(&self) -> Option<String> {
        if self.items.is_empty() {
            return None;
        }
        let mut out = String::from("Meeting ledger (decided so far):\n");
        for section in [
            LedgerKind::Decision,
            LedgerKind::Constraint,
            LedgerKind::Owner,
        ] {
            let rows: Vec<&LedgerItem> = self.items.iter().filter(|i| i.kind == section).collect();
            if rows.is_empty() {
                continue;
            }
            out.push_str(&format!("\n{}s:\n", section.label()));
            for item in rows {
                match &item.speaker {
                    Some(sp) if !sp.is_empty() => {
                        out.push_str(&format!("- {} ({})\n", item.text.trim(), sp));
                    }
                    _ => out.push_str(&format!("- {}\n", item.text.trim())),
                }
            }
        }
        Some(out)
    }
}

/// System prompt for the extraction pass. The verbatim-`quote` contract is what
/// makes the harness able to reject fabrications.
pub const EXTRACTION_PROMPT: &str = "\
You extract a meeting ledger. Use ONLY the transcript provided. Never infer or invent. \
If nothing qualifies, output empty arrays.

For each item, FIRST copy the exact transcript sentence that proves it (the \"quote\"), \
THEN the normalized fields.
Rules:
- \"quote\" MUST be copied verbatim from the transcript.
- \"speaker\" MUST be a speaker label that actually said it.
- Do not merge two statements into one.
- Extract ONLY what is explicitly stated. If unsure, leave it out.

Output ONLY JSON in this exact shape:
{
  \"decisions\":   [{\"quote\":\"...\", \"text\":\"...\", \"speaker\":\"Speaker N\"}],
  \"constraints\": [{\"quote\":\"...\", \"text\":\"...\", \"speaker\":\"Speaker N\"}],
  \"owners\":      [{\"quote\":\"...\", \"owner\":\"...\", \"task\":\"...\"}]
}";

// ---- raw model output shapes (lenient) ------------------------------------

#[derive(Debug, Deserialize)]
struct RawExtraction {
    #[serde(default)]
    decisions: Vec<RawStatement>,
    #[serde(default)]
    constraints: Vec<RawStatement>,
    #[serde(default)]
    owners: Vec<RawOwner>,
}

#[derive(Debug, Deserialize)]
struct RawStatement {
    #[serde(default)]
    quote: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    speaker: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawOwner {
    #[serde(default)]
    quote: String,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    task: String,
}

/// Parse the model's raw text and verify every item against the transcript
/// `window`. This is the anti-hallucination gate:
///
/// 1. Extract the JSON object (tolerant of ```json fences / surrounding prose).
/// 2. Drop any item whose `quote` is not a verbatim substring of the window.
/// 3. Null any `speaker` that does not appear in the window.
///
/// Returns only the items that survive. Returns an empty vec (never errors) so a
/// bad pass simply contributes nothing.
pub fn parse_and_verify(raw: &str, window: &str) -> Vec<LedgerItem> {
    let Some(json) = extract_json_object(raw) else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_str::<RawExtraction>(&json) else {
        return Vec::new();
    };

    let window_norm = normalize(window);
    let mut out = Vec::new();

    let mut push_statement = |kind: LedgerKind, s: RawStatement| {
        let quote = s.quote.trim();
        let text = s.text.trim();
        if quote.is_empty() || text.is_empty() {
            return;
        }
        // The guarantee: quote must literally appear in the transcript.
        if !window_norm.contains(&normalize(quote)) {
            return;
        }
        let speaker = verify_speaker(s.speaker, window);
        out.push(LedgerItem {
            kind,
            text: text.to_string(),
            quote: quote.to_string(),
            speaker,
        });
    };

    for s in parsed.decisions {
        push_statement(LedgerKind::Decision, s);
    }
    for s in parsed.constraints {
        push_statement(LedgerKind::Constraint, s);
    }
    for o in parsed.owners {
        let quote = o.quote.trim();
        let owner = o.owner.trim();
        let task = o.task.trim();
        if quote.is_empty() || owner.is_empty() {
            continue;
        }
        if !window_norm.contains(&normalize(quote)) {
            continue;
        }
        let text = if task.is_empty() {
            owner.to_string()
        } else {
            format!("{owner} — {task}")
        };
        out.push(LedgerItem {
            kind: LedgerKind::Owner,
            text,
            quote: quote.to_string(),
            speaker: None,
        });
    }

    out
}

/// Keep a speaker label only if it actually occurs in the window.
fn verify_speaker(speaker: Option<String>, window: &str) -> Option<String> {
    let sp = speaker?;
    let sp = sp.trim();
    if sp.is_empty() {
        return None;
    }
    if window
        .to_ascii_lowercase()
        .contains(&sp.to_ascii_lowercase())
    {
        Some(sp.to_string())
    } else {
        None
    }
}

/// Pull the first balanced `{...}` JSON object out of arbitrary model text,
/// tolerating code fences and surrounding prose.
fn extract_json_object(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let start = raw.find('{')?;
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        let c = b as char;
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(raw[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Normalize whitespace + case for substring matching and dedup. Collapses
/// runs of whitespace to a single space and lowercases, so trivial spacing /
/// casing differences don't defeat the substring check.
fn normalize(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: &str = "\
Speaker 1: Okay so for the Q3 launch, I think we should go with the phased rollout instead of the big-bang release.
Speaker 2: Agreed. Let's do phased.
Speaker 3: One constraint: the payments migration has to be done before we touch the checkout flow.
Speaker 2: We have a hard cap of forty thousand for the infra spend this quarter.";

    #[test]
    fn verified_quote_is_kept() {
        let raw = r#"{
          "decisions": [{"quote":"we should go with the phased rollout instead of the big-bang release","text":"Phased rollout for Q3","speaker":"Speaker 1"}],
          "constraints": [],
          "owners": []
        }"#;
        let items = parse_and_verify(raw, WINDOW);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, LedgerKind::Decision);
        assert_eq!(items[0].text, "Phased rollout for Q3");
        assert_eq!(items[0].speaker.as_deref(), Some("Speaker 1"));
    }

    #[test]
    fn fabricated_quote_is_dropped() {
        // This quote never appears in the transcript — the model invented it.
        let raw = r#"{
          "decisions": [{"quote":"we will hire five new engineers next month","text":"Hire 5 engineers","speaker":"Speaker 1"}],
          "constraints": [], "owners": []
        }"#;
        let items = parse_and_verify(raw, WINDOW);
        assert!(items.is_empty(), "fabricated item must be dropped");
    }

    #[test]
    fn absent_speaker_is_nulled_but_item_kept() {
        let raw = r#"{
          "decisions": [{"quote":"Let's do phased","text":"Phased rollout","speaker":"Speaker 9"}],
          "constraints": [], "owners": []
        }"#;
        let items = parse_and_verify(raw, WINDOW);
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].speaker, None,
            "speaker not in window must be nulled"
        );
    }

    #[test]
    fn constraint_and_owner_extracted() {
        let raw = r#"{
          "decisions": [],
          "constraints": [{"quote":"the payments migration has to be done before we touch the checkout flow","text":"Payments migration before checkout","speaker":"Speaker 3"}],
          "owners": [{"quote":"We have a hard cap of forty thousand for the infra spend this quarter","owner":"Speaker 2","task":"keep infra under 40k"}]
        }"#;
        let items = parse_and_verify(raw, WINDOW);
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|i| i.kind == LedgerKind::Constraint));
        let owner = items.iter().find(|i| i.kind == LedgerKind::Owner).unwrap();
        assert_eq!(owner.text, "Speaker 2 — keep infra under 40k");
    }

    #[test]
    fn json_wrapped_in_fences_and_prose_is_parsed() {
        let raw = "Sure! Here is the ledger:\n```json\n{\"decisions\":[{\"quote\":\"Let's do phased\",\"text\":\"Phased\",\"speaker\":\"Speaker 2\"}],\"constraints\":[],\"owners\":[]}\n```\nHope that helps.";
        let items = parse_and_verify(raw, WINDOW);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "Phased");
    }

    #[test]
    fn garbage_output_yields_nothing() {
        assert!(parse_and_verify("I could not find anything.", WINDOW).is_empty());
        assert!(parse_and_verify("", WINDOW).is_empty());
        assert!(parse_and_verify("{not valid json", WINDOW).is_empty());
    }

    #[test]
    fn merge_dedups_and_reports_added() {
        let mut state = LedgerState::new(40);
        let a = LedgerItem {
            kind: LedgerKind::Decision,
            text: "Phased rollout for Q3".into(),
            quote: "q".into(),
            speaker: None,
        };
        assert_eq!(state.merge(vec![a.clone()]), 1);
        // Same normalized text + kind → duplicate, not added.
        let a_dup = LedgerItem {
            text: "phased   rollout for q3".into(),
            ..a.clone()
        };
        assert_eq!(state.merge(vec![a_dup]), 0);
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn merge_evicts_oldest_past_cap() {
        let mut state = LedgerState::new(2);
        for i in 0..5 {
            state.merge(vec![LedgerItem {
                kind: LedgerKind::Decision,
                text: format!("decision {i}"),
                quote: "q".into(),
                speaker: None,
            }]);
        }
        assert_eq!(state.len(), 2);
        // Oldest ("decision 0/1/2") evicted; newest retained.
        assert!(state.items().iter().any(|i| i.text == "decision 4"));
        assert!(!state.items().iter().any(|i| i.text == "decision 0"));
    }

    #[test]
    fn render_none_when_empty_and_grouped_when_full() {
        let state = LedgerState::new(40);
        assert!(state.render().is_none());

        let mut state = LedgerState::new(40);
        state.merge(vec![
            LedgerItem {
                kind: LedgerKind::Decision,
                text: "Phased rollout".into(),
                quote: "q".into(),
                speaker: Some("Speaker 1".into()),
            },
            LedgerItem {
                kind: LedgerKind::Constraint,
                text: "Payments before checkout".into(),
                quote: "q".into(),
                speaker: None,
            },
        ]);
        let rendered = state.render().unwrap();
        assert!(rendered.contains("Decisions:"));
        assert!(rendered.contains("Phased rollout (Speaker 1)"));
        assert!(rendered.contains("Constraints:"));
        assert!(rendered.contains("- Payments before checkout"));
    }
}
