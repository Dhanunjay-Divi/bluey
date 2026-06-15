//! Fix-button prompts + proposal parser (PLAN-FIX-BUTTON §4, §5, slice F2).
//!
//! The **Fix** button asks an attached agent to *propose* a fix — diagnosis,
//! reasoning, and the exact commands/diff — and then **stop**. Nothing is
//! applied or pushed until the user approves; only then is the approved plan
//! sent back to the agent to apply. This module owns the two load-bearing
//! pieces of that lane:
//!
//! 1. **Prompt templates** ([`fix_proposal_prompt`], [`fix_apply_prompt`]) —
//!    pure `String` builders. The proposal prompt is the *portable* guardrail
//!    (PLAN §2.2): it forces propose-and-wait and a strict, machine-parseable
//!    output contract even on agents whose native propose-only flag is
//!    unreliable. The apply prompt (post-approval only) constrains the agent to
//!    apply *exactly* the approved fix and to never push or open a PR.
//! 2. **Proposal parser** ([`parse_fix_proposal`]) — pulls the three contract
//!    sections back out of the agent's streamed text/JSON. It is deliberately
//!    fail-soft: malformed or missing output yields a [`FixParseError`] naming
//!    what was missing, so the daemon can warn "the agent didn't follow the
//!    format" rather than applying garbage. It never panics.
//!
//! Pure logic only: no I/O, no subprocess, no network. The marker strings are
//! `pub const` so the prompt, the parser, and the tests can never drift apart.

/// Section marker for the diagnosis block in the proposal contract.
pub const MARKER_DIAGNOSIS: &str = "===DIAGNOSIS===";
/// Section marker for the reasoning block in the proposal contract.
pub const MARKER_REASONING: &str = "===REASONING===";
/// Section marker for the fix block (commands and/or a fenced `diff` block).
pub const MARKER_FIX: &str = "===FIX===";

/// The fenced-code language tag that wraps a unified diff inside the FIX
/// section, so the UI can render it as a diff (see [`extract_diff`]).
pub const DIFF_FENCE_LANG: &str = "diff";

/// A fix proposed by an agent: the three contract sections plus the full raw
/// output the parser saw (kept verbatim for audit/debug and id-matching).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixProposal {
    /// What is wrong (the agent's diagnosis of the problem).
    pub diagnosis: String,
    /// Why this fix is correct (the agent's reasoning).
    pub reasoning: String,
    /// The exact fix: shell commands and/or a unified diff. Raw FIX section.
    pub fix: String,
    /// The full original agent output the proposal was parsed from.
    pub raw: String,
}

/// Why parsing an agent's proposed fix failed. Returned (never panicked) so the
/// daemon can surface a clear "agent didn't follow the format" message instead
/// of applying something it could not understand.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FixParseError {
    /// A required contract section marker was absent. The `&'static str` names
    /// the section: `"diagnosis"`, `"reasoning"`, or `"fix"`.
    #[error("agent output is missing the {0:?} section")]
    MissingSection(&'static str),
    /// The output was empty or whitespace-only — nothing to parse.
    #[error("agent output was empty")]
    Empty,
}

/// Build the **propose** prompt: wrap the user's problem into an instruction
/// that forces propose-and-wait with a strict, parseable output contract.
///
/// The wording is intentionally firm and repetitive — this is the portable
/// guardrail (PLAN §2.2) that holds even on agents whose native propose-only
/// flag is unreliable. The agent is told to diagnose, reason, give the exact
/// fix, emit the three [`MARKER_DIAGNOSIS`]/[`MARKER_REASONING`]/[`MARKER_FIX`]
/// markers, and apply nothing.
pub fn fix_proposal_prompt(user_request: &str) -> String {
    format!(
        "You are proposing a fix for the problem below. This is PROPOSE-ONLY: \
you must apply nothing.\n\
\n\
PROBLEM:\n\
{user_request}\n\
\n\
Diagnose the root cause, explain your reasoning, and give the EXACT fix \
(shell commands and/or a unified diff).\n\
\n\
Output your answer as EXACTLY three sections, each marker alone on its own \
line, with the section content between the markers:\n\
\n\
{MARKER_DIAGNOSIS}\n\
<what is wrong and why it happens>\n\
{MARKER_REASONING}\n\
<why this fix is correct and what it changes>\n\
{MARKER_FIX}\n\
<the exact shell commands to run and/or a unified diff>\n\
\n\
If the fix is a code change, put the unified diff inside a fenced ```{DIFF_FENCE_LANG} \
block so it can be rendered as a diff. If it is commands, list them verbatim.\n\
\n\
HARD CONSTRAINTS — you MUST obey every one:\n\
- Do NOT edit, create, or delete any files.\n\
- Do NOT run any commands.\n\
- Do NOT commit. Do NOT push. Do NOT open a pull request.\n\
- Apply NOTHING. Stop after writing the three sections above.\n\
\n\
Your entire job is to PROPOSE the fix and wait. Applying it is a separate, \
later step that happens only after a human approves your proposal.",
    )
}

/// Build the **apply** prompt, sent only after the user approves `approved`.
///
/// It pins the agent to the already-approved diagnosis and FIX content and
/// forbids scope creep: apply exactly this, change only this, and never push or
/// open a PR (PLAN §5, §6.2 — pushing stays a human act).
pub fn fix_apply_prompt(approved: &FixProposal) -> String {
    format!(
        "Apply the approved fix below EXACTLY as written. A human has already \
reviewed and approved it.\n\
\n\
APPROVED DIAGNOSIS:\n\
{diagnosis}\n\
\n\
APPROVED FIX (apply exactly this — the commands and/or diff):\n\
{fix}\n\
\n\
HARD CONSTRAINTS — you MUST obey every one:\n\
- Make ONLY the changes described in the approved fix above.\n\
- Do NOT make any unrelated edits, refactors, or improvements.\n\
- Do NOT push. Do NOT open a pull request. Do NOT create a branch for review.\n\
- Stop once the approved fix is applied locally. Pushing is the human's job.",
        diagnosis = approved.diagnosis,
        fix = approved.fix,
    )
}

/// Parse an agent's streamed output into a [`FixProposal`].
///
/// Tolerant by design (the markers may be buried anywhere in a stream of text
/// or JSON): matching is case-insensitive and ignores surrounding `=`/`#`/`-`
/// and whitespace noise around the marker word. Each section's content runs
/// from its marker line to the next recognized marker (in document order) or to
/// the end of input. Whitespace around each section is trimmed.
///
/// Returns [`FixParseError::Empty`] for blank input and
/// [`FixParseError::MissingSection`] (naming the section) when any of the three
/// required markers is absent. Never panics.
pub fn parse_fix_proposal(output: &str) -> Result<FixProposal, FixParseError> {
    if output.trim().is_empty() {
        return Err(FixParseError::Empty);
    }

    // Locate every marker line in document order. For each we record where its
    // own line starts (`line_start`) and where the section content after it
    // starts (`content_start`). A section runs from its `content_start` up to
    // the *next* marker's `line_start`, so an intervening marker line is never
    // captured into the previous section.
    let mut hits: Vec<MarkerHit> = Vec::new();
    let mut offset = 0usize;
    for line in output.split_inclusive('\n') {
        if let Some(section) = match_marker_line(line) {
            hits.push(MarkerHit {
                section,
                line_start: offset,
                content_start: offset + line.len(),
            });
        }
        offset += line.len();
    }

    let find_section = |section: Section| -> Option<String> {
        let idx = hits.iter().position(|h| h.section == section)?;
        let start = hits[idx].content_start;
        // End at the nearest following marker line, or EOF if this is the last.
        let end = hits
            .iter()
            .filter_map(|h| (h.line_start >= start).then_some(h.line_start))
            .min()
            .unwrap_or(output.len());
        Some(output[start..end].trim().to_string())
    };

    let diagnosis =
        find_section(Section::Diagnosis).ok_or(FixParseError::MissingSection("diagnosis"))?;
    let reasoning =
        find_section(Section::Reasoning).ok_or(FixParseError::MissingSection("reasoning"))?;
    let fix = find_section(Section::Fix).ok_or(FixParseError::MissingSection("fix"))?;

    Ok(FixProposal {
        diagnosis,
        reasoning,
        fix,
        raw: output.to_string(),
    })
}

/// Pull a fenced `diff` block out of a FIX section, if one is present, so the
/// UI can render it as a diff. Returns the inner diff text (fences stripped,
/// trailing newline trimmed) or `None` for a commands-only fix with no fence.
pub fn extract_diff(fix: &str) -> Option<String> {
    let mut lines = fix.lines();
    // Find the opening fence: a line whose trimmed form is ```diff (the only
    // fence language we treat as a renderable diff).
    let mut found_open = false;
    let mut body = String::new();
    for line in lines.by_ref() {
        let trimmed = line.trim();
        if is_diff_fence_open(trimmed) {
            found_open = true;
            break;
        }
    }
    if !found_open {
        return None;
    }
    for line in lines {
        if line.trim() == "```" {
            return Some(body.trim_end_matches('\n').to_string());
        }
        body.push_str(line);
        body.push('\n');
    }
    // Opening fence with no closing fence: treat the remainder as the diff
    // rather than discarding a partial-but-useful block.
    Some(body.trim_end_matches('\n').to_string())
}

/// Which contract section a marker line introduces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Diagnosis,
    Reasoning,
    Fix,
}

/// A located marker: which section it opens, the byte offset of its own line
/// (the boundary the *previous* section ends at), and where this section's
/// content begins (immediately after the marker line).
struct MarkerHit {
    section: Section,
    line_start: usize,
    content_start: usize,
}

/// Does `trimmed` (an already-`trim`med line) open a fenced `diff` block?
/// Accepts both backtick and tilde fences, case-insensitively, ignoring any
/// trailing info-string after the language tag.
fn is_diff_fence_open(trimmed: &str) -> bool {
    let rest = match trimmed
        .strip_prefix("```")
        .or_else(|| trimmed.strip_prefix("~~~"))
    {
        Some(r) => r,
        None => return false,
    };
    rest.trim().eq_ignore_ascii_case(DIFF_FENCE_LANG)
}

/// Recognize a marker line tolerantly: strip surrounding `=`/`#`/`-`/`*` and
/// whitespace, lowercase, and compare to the section keyword. Returns the
/// section if the line is a marker, else `None`.
fn match_marker_line(line: &str) -> Option<Section> {
    let core = line
        .trim()
        .trim_matches(|c: char| {
            c == '=' || c == '#' || c == '-' || c == '*' || c == ' ' || c == '\t'
        })
        .to_ascii_lowercase();
    match core.as_str() {
        "diagnosis" => Some(Section::Diagnosis),
        "reasoning" => Some(Section::Reasoning),
        "fix" => Some(Section::Fix),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WELL_FORMED: &str = "\
Here is my analysis.
===DIAGNOSIS===
The config loads before the env var is set, so PORT is always 0.
===REASONING===
Reading the env var lazily fixes the ordering without a refactor.
===FIX===
Apply this patch:
```diff
--- a/src/config.rs
+++ b/src/config.rs
@@ -1,1 +1,1 @@
-let port = 0;
+let port = std::env::var(\"PORT\").ok();
```
";

    #[test]
    fn parses_well_formed_proposal_into_three_sections() {
        let p = parse_fix_proposal(WELL_FORMED).expect("well-formed input must parse");
        assert_eq!(
            p.diagnosis,
            "The config loads before the env var is set, so PORT is always 0."
        );
        assert_eq!(
            p.reasoning,
            "Reading the env var lazily fixes the ordering without a refactor."
        );
        assert!(p.fix.starts_with("Apply this patch:"));
        assert!(p.fix.contains("```diff"));
        // raw keeps the full original text for audit.
        assert_eq!(p.raw, WELL_FORMED);
    }

    #[test]
    fn extracts_diff_from_fix_section() {
        let p = parse_fix_proposal(WELL_FORMED).expect("parse");
        let diff = extract_diff(&p.fix).expect("a ```diff block is present");
        assert!(diff.starts_with("--- a/src/config.rs"));
        assert!(diff.contains("+let port = std::env::var(\"PORT\").ok();"));
        // Fences themselves are stripped.
        assert!(!diff.contains("```"));
    }

    #[test]
    fn commands_only_fix_has_no_diff() {
        let output = "\
===DIAGNOSIS===
Stale build cache.
===REASONING===
Cleaning forces a rebuild from source.
===FIX===
cargo clean
cargo build --workspace
";
        let p = parse_fix_proposal(output).expect("parse");
        assert_eq!(p.fix, "cargo clean\ncargo build --workspace");
        assert_eq!(extract_diff(&p.fix), None);
    }

    #[test]
    fn missing_fix_section_names_the_section() {
        let output = "\
===DIAGNOSIS===
Something is wrong.
===REASONING===
Because of reasons.
";
        let err = parse_fix_proposal(output).expect_err("missing FIX must error");
        assert_eq!(err, FixParseError::MissingSection("fix"));
    }

    #[test]
    fn missing_diagnosis_section_names_the_section() {
        let output = "\
===REASONING===
Because of reasons.
===FIX===
do the thing
";
        let err = parse_fix_proposal(output).expect_err("missing DIAGNOSIS must error");
        assert_eq!(err, FixParseError::MissingSection("diagnosis"));
    }

    #[test]
    fn tolerates_marker_noise_whitespace_and_case() {
        let output = "\
   ### Diagnosis ===
The cache is stale.
== reasoning ==
A clean rebuild fixes it.
-----FIX-----
cargo clean
";
        let p = parse_fix_proposal(output).expect("noisy markers must still parse");
        assert_eq!(p.diagnosis, "The cache is stale.");
        assert_eq!(p.reasoning, "A clean rebuild fixes it.");
        assert_eq!(p.fix, "cargo clean");
    }

    #[test]
    fn empty_input_is_empty_error() {
        assert_eq!(parse_fix_proposal(""), Err(FixParseError::Empty));
        assert_eq!(parse_fix_proposal("   \n\t  \n"), Err(FixParseError::Empty));
    }

    #[test]
    fn no_markers_errors_without_panic() {
        // Plain prose with none of the markers: first missing section wins.
        let err = parse_fix_proposal("just some text, no structure at all")
            .expect_err("no markers must error");
        assert_eq!(err, FixParseError::MissingSection("diagnosis"));
    }

    #[test]
    fn proposal_prompt_carries_apply_nothing_and_all_markers() {
        let prompt = fix_proposal_prompt("the app crashes on launch");
        // The user's problem is embedded.
        assert!(prompt.contains("the app crashes on launch"));
        // The apply-nothing guardrail is present (several phrasings).
        assert!(prompt.contains("Apply NOTHING"));
        assert!(prompt.contains("Do NOT push"));
        assert!(prompt.contains("Do NOT commit"));
        // The contract and parser stay in sync: every marker constant appears.
        assert!(prompt.contains(MARKER_DIAGNOSIS));
        assert!(prompt.contains(MARKER_REASONING));
        assert!(prompt.contains(MARKER_FIX));
        // And the diff-fence hint, so agents emit a renderable diff.
        assert!(prompt.contains(DIFF_FENCE_LANG));
    }

    #[test]
    fn apply_prompt_references_approved_fix_and_forbids_push() {
        let approved = FixProposal {
            diagnosis: "off-by-one in the loop".to_string(),
            reasoning: "bound should be <= not <".to_string(),
            fix: "change `<` to `<=` on line 12".to_string(),
            raw: String::new(),
        };
        let prompt = fix_apply_prompt(&approved);
        // The approved content is pinned into the apply instruction.
        assert!(prompt.contains("off-by-one in the loop"));
        assert!(prompt.contains("change `<` to `<=` on line 12"));
        // No-push / no-PR / no-unrelated-edits guardrails.
        assert!(prompt.contains("Do NOT push"));
        assert!(prompt.contains("Do NOT open a pull request"));
        assert!(prompt.contains("ONLY the changes"));
    }

    #[test]
    fn roundtrip_parse_then_apply_prompt() {
        // A proposal parsed from output feeds the apply prompt cleanly.
        let p = parse_fix_proposal(WELL_FORMED).expect("parse");
        let prompt = fix_apply_prompt(&p);
        assert!(prompt.contains(&p.diagnosis));
        assert!(prompt.contains("```diff"));
    }
}
