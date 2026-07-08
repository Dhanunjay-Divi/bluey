# ROUND-432 Audio Retention Terms

Date: 2026-07-08
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-web-ui-parallel-20260704

## Goal

Make the Session Audit Bundle handoff and public Terms/Privacy copy explicit for STT quality review.

## Changes

- Updated `ROUND-430-SESSION-AUDIT-BUNDLE-REVIEW-HANDOFF.md` so the runtime agent has a clear instruction: persist audio chunks locally and upload audio chunks through the same training/QA retention path as other session data.
- Removed future/public-launch consent language from the handoff so the implementation target is not ambiguous.
- Updated `/privacy` to include voice/audio recordings or chunks in processed context, training/improvement, AI/speech processing, retention, and deletion language.
- Updated `/terms` to include voice/audio recordings or chunks in user-controlled context, training/improvement, and cloud sync retention language.

## Product Position

Bluey may use synced session data, including voice/audio recordings or chunks, to review transcription quality, improve answers, train or tune Bluey systems, improve routing, debug behavior, and build safeguards.

The retention period stays aligned with synced session content: up to 90 days from last sync or use unless deleted earlier.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/index.html docs/rounds/ROUND-430-SESSION-AUDIT-BUNDLE-REVIEW-HANDOFF.md docs/rounds/ROUND-432-AUDIO-RETENTION-TERMS.md`
