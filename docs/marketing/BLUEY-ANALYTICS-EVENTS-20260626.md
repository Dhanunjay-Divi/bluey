# Bluey Analytics Events - 2026-06-26

Analytics should explain the product funnel without storing sensitive content. Do not send raw transcript, screenshots, document text, prompts, answers, access tokens, provider keys, or payment card data.

## Web Events

| Event | When | Suggested properties |
| --- | --- | --- |
| `page_view` | Page loaded | `path`, `referrer_host`, `utm_source`, `utm_medium`, `utm_campaign` |
| `download_click` | Download CTA clicked | `platform`, `path`, `utm_source`, `auth_state` |
| `login_start` | Login form opened | `path`, `utm_source` |
| `signup_start` | Signup begins | `path`, `utm_source` |
| `reload_click` | Reload CTA clicked | `amount_bucket`, `path`, `auth_state` |
| `faq_open` | FAQ row opened | `question_id`, `path` |

## Product Events

| Event | When | Suggested properties |
| --- | --- | --- |
| `overlay_started` | Bluey overlay starts | `platform`, `version`, `auth_state` |
| `answer_requested` | User submits a question | `route`, `has_screen`, `has_files`, `has_transcript`, `has_memory` |
| `answer_stream_started` | First answer delta/status arrives | `route`, `model_family`, `source_count_bucket` |
| `answer_completed` | Answer finishes | `route`, `duration_bucket`, `token_bucket`, `cost_bucket` |
| `screen_context_attached` | Screen context attached | `platform`, `capture_mode`, `result` |
| `file_attached` | File added | `file_type`, `size_bucket`, `parse_result` |
| `credit_balance_low` | User is close to zero | `balance_bucket` |
| `reload_completed` | Credits credited | `amount_usd`, `provider`, `result` |

## Privacy Rules

- Use buckets for balances, token counts, cost, latency, and file sizes where possible.
- Keep account identifiers hashed or server-side only.
- Never send raw content from user work into web analytics.
- Keep billing reconciliation in server logs and database records, not third-party analytics payloads.
- Add data deletion and export coverage before using analytics for customer-specific reports.

## Funnel To Watch

1. Landing page view.
2. Download click.
3. Install success.
4. `bluey on` start.
5. Account link or guest start.
6. First answer.
7. First attached context.
8. First reload.
9. Repeat session within 7 days.
