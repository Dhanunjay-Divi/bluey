# Round 200 - Web Search Customer Copy

## Trigger

Owner clarified that paid-search copy should not tell customers about internal
enforcement concepts, scraping, fraud, abuse, or a specific fixed paid cap.
Customer-facing wording should simply make sense: paid search uses credits, with
spend controls that feel like account protection.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`
Workspace: `/Users/uno/Downloads/cue`
Branch: `codex/bluey-overlay-routing-hardening`
Round completed: 2026-06-26 16:07 EDT

## Changes

- Rewrote Round 198 policy language to say paid web search is credit-metered
  with spend controls, not framed as a fixed daily count.
- Removed customer-copy examples that referenced internal enforcement wording.
- Rewrote the handoff summary so future rounds preserve the cleaner policy.
- Updated the Round 199 doc to avoid fixed-count phrasing.
- Softened the repeated-query status from:
  - `Web search skipped because this exact search just ran.`
  to:
  - `Web search paused briefly for this repeated question.`
- Added a regression test to keep web-search skipped labels from exposing
  internal or alarming wording.

## Customer Copy Direction

Use:

- `Paid: web search uses credits.`
- `You can set a daily search spend limit.`
- `Bluey shows when it searches and cites sources.`
- `If a request repeats too quickly, Bluey may briefly pause web search and use available context.`

Avoid:

- fixed paid daily search-count framing
- internal enforcement language
- alarming explanations about misuse patterns

## Verification

Passed:

- `cargo fmt --manifest-path server/Cargo.toml`
- `git diff --check`
- `cargo test --manifest-path server/Cargo.toml web_search_skipped_labels_stay_customer_friendly -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml router_cost_label_includes_web_search_usage -- --nocapture`
- `rg --glob '!**/target/**' -n "50 searches|50/day|scraping|scrape|fraud|abuse|abusive|automation" docs/rounds/ROUND-198-WEB-SEARCH-PAID-QUOTA-POLICY.md docs/rounds/ROUND-199-WEB-SEARCH-CREDIT-METERING-SOURCES.md server/src/api/router.rs`

The final `rg` hit only the regression test's blocked-word list.

## Current State

- Product/customer wording now emphasizes credits, spend limits, cited sources,
  and brief pauses for repeated questions.
- Internal risk rationale remains an operator concern, not customer copy.
