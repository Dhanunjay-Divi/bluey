# ROUND-399-LIVE-ANSWER-LATENCY-EVAL

Date: 2026-07-06
Branch: `main`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Run live production answer tests after the `0.1.88` deploy, covering easy to hard prompts across general explanation, behavioral/interview, system design, coding, screen-context style coding, writing, and research lookup.

## Test Path

- API: `https://bluey.sh`
- Endpoint: `/router/complete/stream`
- Account: `codex-smoke-20260608183100@bluey.sh`
- Token handling: loaded from the local Bluey account profile, never printed.
- Measurement:
  - time to first streamed delta
  - total completion time
  - provider/model
  - customer cost
  - artifact type
  - source count
  - visible status events

Raw local result file for this run:

- `tmp/bluey-live-eval-20260706.json`

## Results

| Case | Coverage | First Token | Total | Provider / Model | Cost | Artifact | Sources | Result |
|---|---|---:|---:|---|---:|---|---:|---|
| `quick_concept` | event loop vs thread pool | 1.7s | 6.8s | `zai / glm-5.2` | 1c | none | 0 | pass, but misplanned |
| `behavioral_intro` | tell-me-about-yourself | 2.2s | 18.9s | `zai / glm-5.2` | 1c | none | 0 | pass, too slow |
| `system_design` | URL shortener | 2.7s | 10.6s | `openai / gpt-5.5` | 7c | none | 0 | pass |
| `simple_code` | Fibonacci Python | 1.6s | 14.2s | `zai / glm-5.2` | 1c | `code` | 0 | pass |
| `hard_algorithm_code` | largest rectangle histogram | 1.9s | 12.4s | `openai / gpt-5.5` | 6c | `code` | 0 | pass |
| `coding_from_context` | Alice/Bob screen-style prompt | 4.2s | 6.7s | `openai / gpt-5.5` | 4c | none | 0 | answer present, artifact missing |
| `writing_rewrite` | manager rewrite | 1.3s | 1.9s | `openai / gpt-5.5` | 1c | none | 0 | pass |
| `research_lookup` | Secret Passage Ranch | 3.0s | 6.1s | `openai / gpt-5.5` | 3c | none | 0 | graceful missing-search answer |

Balance movement:

- Before: `681c`
- After: `657c`
- Total customer charge: `24c`

## What Looked Good

- All live calls completed without HTTP/provider failure.
- Streaming started in 1.3s to 4.2s.
- Explicit coding prompts produced `code` artifacts.
- The hard coding answer included approach, commented code, and complexity.
- Writing rewrite was fast and clean.
- Research lookup did not hallucinate when web search was unavailable.
- Provider mix is active:
  - `glm-5.2` handled quick/general and simple code cheaply.
  - `gpt-5.5` handled system design, harder code, screen-context style prompt, writing, and research fallback.

## Issues Found

1. Quick concept prompt was misclassified as `research`.
   - Server plan: `answer_intent=research`, `answer_output=source_answer`, `needs_web_search=true`.
   - User-visible status wrongly showed:
     - `Searching web...`
     - `Web search is not configured yet.`
   - This should have been a normal quick/general explanation with no web status.

2. System design prompt was misclassified as `behavioral`.
   - Server plan: `answer_intent=behavioral`, `answer_output=interview_answer`.
   - The final answer was still usable, but this can distort routing and style.

3. Screen-context style coding prompt was misclassified as `missing_context`.
   - Server plan: `answer_intent=missing_context`, `answer_output=compact`, `needs_screen=true`.
   - The answer contained Python code in the text, but `artifact_type` was `none`.
   - This matches the user-facing issue where the code pane can stay stale or absent even though the answer contains code.

4. Behavioral answer latency is too high for a live call helper.
   - First token was acceptable at `2.2s`.
   - Total was `18.9s` for a template-style answer.
   - The answer also included bracket placeholders, which is less useful than a polished default spoken answer.

5. Simple code total latency is also high.
   - First token was `1.6s`, but total was `14.2s`.
   - The answer quality was good, but the product will feel slow if the overlay waits for complete output before canvas/UI settlement.

6. Web search is still not configured in production.
   - Research answer handled this honestly.
   - It should not show web-search status for non-research prompts.

7. Server log `request_ref` is not unique enough for prefixed IDs.
   - The live eval request IDs all began with `live-eval-...`.
   - Server logs showed `request_ref=LIVEEVAL` for multiple requests.
   - For user support refs, use a suffix/random short ref instead of the prefix so the visible ref maps directly to logs.

## Next Fix Candidates

- Tighten AnswerPlan rules:
  - concept comparison should be `quick` / `compact`, not `research`
  - `Design ...` should be `system_design`, not `behavioral`
  - screen-context text that includes a complete coding prompt should be `coding` with `code_artifact`, not `missing_context`
- Add eval tests for these exact prompts.
- Make user-visible status conditional:
  - do not show `Searching web...` unless a real search provider is configured and a search will actually run
- Improve behavioral prompt style:
  - no bracket placeholders unless explicitly asked for a template
  - prefer one polished speakable answer plus a short customization note
- Consider faster route policy for live-call behavioral/general answers:
  - use faster first-draft lane or lower token cap unless the user asks for a long answer
- Fix support refs:
  - visible short ID should use the entropy-bearing suffix of `request_id`
  - logs should include that same short ref

## Verification Commands

- Live streaming eval script run via Python against `/router/complete/stream`.
- Production logs checked with `journalctl -u bluey-api.service`.
- No secrets were printed.
