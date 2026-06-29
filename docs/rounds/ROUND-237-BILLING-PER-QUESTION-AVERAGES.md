# Round 237 - Billing Per Question Averages

Date: 2026-06-29 19:24 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner asked how much Bluey is charging on average for each question and
wanted a clearer breakdown after the Round 235 billing margin audit.

## Data Source

- Command checked: `bluey usage`
- Temporary export checked: `bluey-export-20260629-231921.json`
- Export timestamp: `2026-06-29T23:19:21Z`
- Temporary export file was removed after aggregation and was not committed.

The export includes `usage_events` rows. It does not directly join every STT,
embedding, and LLM row back into one user-facing "question", so this round
reports two views:

- average per billable usage row
- average per actual LLM answer row

## Last 7 Days

Current `bluey usage`:

- `474` billable rows/cues
- `$7.52` customer spend
- estimated provider actual: about `$1.99`

Average across every billable row:

- customer charge: `1.59c`
- provider estimate: `0.42c`

Average for actual AI answer rows only:

- `58` LLM answer rows
- customer charge: `$4.25`, average `7.33c` per answer
- provider estimate: `$1.49`, average `2.57c` per answer

## Last 7 Days By Type

Normal balanced text answers:

- `42` Anthropic Sonnet balanced rows
- customer charge: `$1.71`
- average customer charge: `4.07c`
- provider estimate: `$0.50`
- average provider estimate: `1.20c`

Screen/vision answers:

- `16` OpenAI `gpt-5.5` vision rows
- customer charge: `$2.54`
- average customer charge: `15.88c`
- provider estimate: `$0.98`
- average provider estimate: `6.16c`

Listen/STT:

- `74` Deepgram STT rows, split evenly:
  - `37` system rows
  - `37` microphone rows
- customer charge: `$1.45`
- average customer charge: `1.96c` per STT lane row
- provider estimate: `$0.50`
- average provider estimate: `0.67c` per STT lane row
- with both mic and system active, the average Listen window is roughly:
  - customer charge: `3.92c`
  - provider estimate: `1.35c`

Embeddings / indexing:

- `342` OpenAI embedding rows
- customer charge: `$1.82`
- average customer charge: `0.53c` per embedding row
- provider estimate: about `$0.0046` total
- average provider estimate: about `0.001c` per embedding row

## Practical User-Facing Examples

Approximate current averages:

- Typed normal text answer without screen/docs/listen: about `4c`
- Screen or image-context answer: about `16c`
- Spoken Listen ask with mic + system plus normal answer: about `8c`
- Spoken Listen ask with mic + system plus screen/vision answer: about `20c`

What `$30` buys at these averages:

- normal balanced answers: about `735` answers
- screen/vision answers: about `189` answers
- spoken Listen + normal answer: about `375` answers
- mixed current usage pattern from the last 7 days: about `1,890` billable
  rows/cues, or about `409` LLM answer rows before accounting for extra STT
  and embedding rows

## Interpretation

The biggest reason "average cue" and "average question" are different is that
one user-facing question can trigger more than one billing row:

- live audio can create one STT row per audio lane
- document/saved-memory work can create embedding rows
- the answer itself creates an LLM row
- screen/image questions use the more expensive vision route

For product copy, the clearest explanation is:

- simple typed answers are usually only a few cents
- screen/image answers cost more
- Listen costs separately while it transcribes
- docs/memory indexing can add small rows
- the UI should show a per-answer cost detail like:
  `Answer 4c · Listen 4c · Docs <1c`

## Product Follow-Ups

- Add an admin-only usage-cost report so owner margin does not require manual
  exports.
- Add a user-facing per-answer cost breakdown in overlay/dashboard:
  - LLM answer
  - Listen/STT
  - docs/memory embeddings
  - web search when used
- Reduce customer-visible confusion from tiny embedding rows by batching or
  grouping them under the parent answer/session where possible.
