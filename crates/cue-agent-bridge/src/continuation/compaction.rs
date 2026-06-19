//! Replay-tier compaction: bound a long prior transcript before replaying it as
//! continuation context.
//!
//! The strategy mirrors the hierarchical-memory pattern the coding agents use:
//! keep the most recent turns (the "hot layer") verbatim and summarize the older
//! ones into a single turn. Bluey runs no AI of its own, so the summary is
//! produced by driving the user's OWN agent — injected as a closure
//! ([`maybe_compact`]) so this module stays decoupled from the daemon's drive
//! machinery. Everything else here is pure and unit-tested.

use std::future::Future;

use crate::{Role, Transcript, Turn};

/// Char budget over which a Replay-tier continuation summarizes older turns
/// instead of replaying them whole. Below the drive layer's safety-net budget so
/// the summary path engages BEFORE the net has to trim. ~4 chars/token.
pub const CONTINUATION_SUMMARY_BUDGET: usize = 360_000;

/// How many most-recent turns are always kept VERBATIM when compacting a long
/// transcript for Replay-tier continuation (the "hot layer"); older turns are
/// summarized. Mirrors the hierarchical-memory pattern the coding agents use.
pub const CONTINUATION_HOT_TURNS: usize = 12;

/// Split a transcript for Replay-tier compaction: returns `(older, recent)`
/// where `recent` is the last [`CONTINUATION_HOT_TURNS`] turns and `older` is
/// everything before. Pure — the caller summarizes `older` (via a drive through
/// the user's own agent) and replays `[summary] + recent`. Returns `(&[], all)`
/// when the transcript already fits within [`CONTINUATION_HOT_TURNS`].
#[must_use]
pub fn split_for_compaction(turns: &[Turn]) -> (&[Turn], &[Turn]) {
    if turns.len() <= CONTINUATION_HOT_TURNS {
        return (&[], turns);
    }
    let cut = turns.len() - CONTINUATION_HOT_TURNS;
    (&turns[..cut], &turns[cut..])
}

/// Total character size of a transcript's turn text — the cheap proxy for
/// "is this too big to replay whole" (~4 chars/token).
#[must_use]
pub fn transcript_chars(turns: &[Turn]) -> usize {
    turns.iter().map(|t| t.text.len()).sum()
}

/// The prompt asked of the user's OWN agent to compress the older part of a long
/// conversation (Bluey runs no AI of its own — the user's agent summarizes the
/// user's conversation). Bounded output so the summary itself stays small.
#[must_use]
pub fn summarize_older_prompt(older_text: &str) -> String {
    format!(
        "Summarize the earlier part of our conversation below in at most 400 words. \
Preserve key decisions, file names, code identifiers, and any open questions or \
next steps. Output ONLY the summary, no preamble.\n\n----- earlier conversation -----\n{older_text}"
    )
}

/// If a Replay transcript is too big to replay whole, summarize its older turns
/// and return `[summary] + recent`; otherwise return it unchanged.
///
/// `summarize` is an async closure that drives the user's own agent with the
/// given prompt and returns its answer (`None`/empty → summary failed). It is
/// injected so this policy lives in the spine while the drive *mechanism* stays
/// in the daemon. Fail-soft: a failed/empty summary returns the raw transcript
/// (the drive layer's char budget then trims it as the safety net).
pub async fn maybe_compact<F, Fut>(transcript: Transcript, summarize: F) -> Transcript
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Option<String>>,
{
    if transcript_chars(&transcript.turns) <= CONTINUATION_SUMMARY_BUDGET {
        return transcript; // fits — replay whole, no extra drive
    }
    let (older, recent) = split_for_compaction(&transcript.turns);
    if older.is_empty() {
        return transcript;
    }

    // Flatten the older turns and ask the user's own agent to summarize them.
    let older_text: String = older
        .iter()
        .map(|t| format!("{:?}: {}", t.role, t.text))
        .collect::<Vec<_>>()
        .join("\n");

    match summarize(summarize_older_prompt(&older_text)).await {
        Some(summary) if !summary.trim().is_empty() => {
            let mut turns = Vec::with_capacity(recent.len() + 1);
            turns.push(Turn {
                role: Role::System,
                text: format!("Summary of the earlier conversation:\n{}", summary.trim()),
            });
            turns.extend(recent.iter().cloned());
            Transcript { turns }
        }
        // Summary failed/empty → raw transcript; the drive budget trims it.
        _ => transcript,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turns(n: usize) -> Vec<Turn> {
        (0..n)
            .map(|i| Turn {
                role: Role::User,
                text: format!("turn {i}"),
            })
            .collect()
    }

    #[test]
    fn split_keeps_recent_hot_turns() {
        let few = turns(5);
        let (older, recent) = split_for_compaction(&few);
        assert!(older.is_empty(), "under hot-turn count → nothing older");
        assert_eq!(recent.len(), 5);

        let many = turns(CONTINUATION_HOT_TURNS + 8);
        let (older, recent) = split_for_compaction(&many);
        assert_eq!(older.len(), 8);
        assert_eq!(recent.len(), CONTINUATION_HOT_TURNS);
    }

    #[test]
    fn transcript_chars_sums_turn_text() {
        let t = vec![
            Turn {
                role: Role::User,
                text: "abc".to_string(),
            },
            Turn {
                role: Role::Assistant,
                text: "de".to_string(),
            },
        ];
        assert_eq!(transcript_chars(&t), 5);
    }

    #[test]
    fn summarize_prompt_embeds_the_older_text() {
        let p = summarize_older_prompt("User: earlier stuff");
        assert!(p.contains("earlier stuff"));
        assert!(p.contains("at most 400 words"));
    }

    #[tokio::test]
    async fn maybe_compact_replays_whole_when_under_budget() {
        // Small transcript: summarizer must NOT be called.
        let t = Transcript { turns: turns(3) };
        let out = maybe_compact(t, |_p| async {
            panic!("summarizer should not run under budget");
            #[allow(unreachable_code)]
            None
        })
        .await;
        assert_eq!(out.turns.len(), 3);
    }

    #[tokio::test]
    async fn maybe_compact_summarizes_older_and_keeps_recent() {
        // Build an over-budget transcript: one huge older turn + hot layer.
        let mut all = vec![Turn {
            role: Role::User,
            text: "x".repeat(CONTINUATION_SUMMARY_BUDGET + 10),
        }];
        all.extend(turns(CONTINUATION_HOT_TURNS));
        let t = Transcript { turns: all };

        let out = maybe_compact(t, |_p| async { Some("SHORT SUMMARY".to_string()) }).await;
        // 1 summary turn + the hot layer.
        assert_eq!(out.turns.len(), CONTINUATION_HOT_TURNS + 1);
        assert_eq!(out.turns[0].role, Role::System);
        assert!(out.turns[0].text.contains("SHORT SUMMARY"));
    }

    #[tokio::test]
    async fn maybe_compact_falls_back_to_raw_on_empty_summary() {
        let mut all = vec![Turn {
            role: Role::User,
            text: "x".repeat(CONTINUATION_SUMMARY_BUDGET + 10),
        }];
        all.extend(turns(CONTINUATION_HOT_TURNS));
        let original_len = all.len();
        let t = Transcript { turns: all };

        // Empty summary → raw transcript returned unchanged.
        let out = maybe_compact(t, |_p| async { Some("   ".to_string()) }).await;
        assert_eq!(out.turns.len(), original_len);
    }
}
