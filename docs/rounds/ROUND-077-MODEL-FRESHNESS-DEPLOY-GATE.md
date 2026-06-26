# Round 077 - Model Freshness Deploy Gate

Date: 2026-06-20
Branch: `codex/bluey-ai-site`

## Why

Bluey routes paid work through managed provider models. Provider model catalogs,
prices, limits, context windows, and deprecation status change often enough that
model selection cannot be a one-time setup. Every deploy that can reach paying
users needs an explicit model freshness check.

## What Changed

- Added a required "Model Freshness Release Gate" to `docs/MODEL-ROUTING.md`.
- Added the same gate to `docs/RELEASE-RUNBOOK.md` pre-flight.
- Added the same launch-readiness item to `docs/PRELAUNCH-CHECKLIST.md`.

## Required Per-Deploy Evidence

For each release, the operator or agent must record:

- Date checked.
- OpenAI, Anthropic, Gemini, Deepgram, and embedding model IDs.
- Pricing snapshot date.
- Any route/fallback/capacity changes.
- Live-smoke trace IDs or an explicit waiver.

## Guardrail

Do not silently swap production model IDs. A route update must include pricing,
fallback order, capacity implications, tests, and live-smoke evidence.

## Verification

Docs-only change. Verification required:

```bash
git diff --check
```
