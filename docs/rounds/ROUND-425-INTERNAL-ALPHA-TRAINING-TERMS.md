# Round 425 - Internal Alpha Training Terms

## Goal

Make Bluey's public Terms and Privacy copy reflect the current internal alpha posture: Bluey responses and submitted context may be used to improve and train Bluey, and synced session content should be described with a 90-day retention period.

## Changes

- Updated `/privacy` with a new internal alpha training and improvement section.
- Updated `/terms` with explicit permission to process and use submitted context and generated responses for service operation, quality evaluation, routing improvement, training/tuning, and safeguards during the internal alpha.
- Changed cloud-synced content retention language from 12 months to 90 days for session records, transcripts, synced bytes, extracted text, summaries, generated responses, and search indexes.
- Left purchased balance/accounting language at 12 months so credit expiration does not get confused with session-content retention.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
