# ROUND-445 Transcript Fallback Compact Paragraph

## Trigger

User shared a screenshot where the overlay history fallback showed a transcript as many repeated lines:

```text
user: Hey. How's it
user: going?
user: So
user: today, let's discuss about
...
```

That looked confusing because the user expected one readable sentence or paragraph, not a debug-style list of transcript fragments.

## Findings

- The screen was showing the overlay history fallback card titled `Transcript`.
- That fallback used `MeetingRecord::last_transcript_text_bounded`, which intentionally keeps speaker labels per transcript segment.
- This is useful for model context and audit/debugging, but it is not the right visual treatment for a user-facing history card.

## Change

Updated the overlay history fallback card to use a display-only transcript formatter:

- Consecutive transcript segments from the same speaker are merged into one readable paragraph.
- Segment whitespace is normalized so short STT chunks read like a sentence.
- Speaker labels are hidden when there is only one speaker.
- Speaker labels are preserved only when multiple speakers are present.
- Long transcript fallback cards still use a tail cap so the overlay does not become overloaded.

This does not change the underlying transcript storage, sync, RAG, or audit data. The structured segment data remains available for debugging and quality review.

## Verification

- Added `overlay_history_transcript_fallback_merges_spoken_segments`.
- Ran `cargo test -p cue-daemon overlay_history_transcript_fallback_merges_spoken_segments --quiet`.
- Ran `cargo test -p cue-daemon overlay_history_cards --quiet`.
- Ran `git diff --check`.

## Notes

- This is source-only for now. No signed release deploy was run in this round.
- The installed `~/.bluey/bin` app will not show this fix until we build/install locally or do the next signed deploy.
