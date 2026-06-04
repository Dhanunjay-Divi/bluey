# Provider Live Smoke — 2026-06-04

## Goal

Verify Bluey's upstream provider accounts without storing or printing secrets.

The smoke runner is:

```bash
scripts/provider-live-smoke.py
```

It reads keys from environment variables only, keeps them in Python process
memory, and reports provider/check/latency. It does not write keys to repo
config, temp files, curl command lines, or logs.

## What It Tests

- OpenAI text: `gpt-4o-mini`
- OpenAI vision: `gpt-4o` with a tiny PNG data URL
- OpenAI embeddings: `text-embedding-3-small`
- OpenAI transcription: `gpt-4o-mini-transcribe`
- Anthropic text: `claude-3-5-sonnet-latest`
- Deepgram STT: `nova-3`
- Gemini text: `gemini-2.5-flash` when `GEMINI_API_KEY` or `GOOGLE_API_KEY` is present

## Current Product Wiring

Bluey managed routing currently uses OpenAI, Anthropic, and Deepgram. Gemini is
included in this smoke as an upstream availability check only; it is not yet
wired into `bluey-server` managed routing.

Current managed route shape:

- Instant: OpenAI `gpt-4o-mini`
- Balanced: Anthropic `claude-3-5-sonnet-latest`, fallback OpenAI `gpt-4o-mini`
- Deep: Anthropic `claude-3-7-sonnet-latest`, fallback OpenAI `gpt-4o`
- Vision: OpenAI `gpt-4o`
- Embeddings/RAG: OpenAI `text-embedding-3-small`
- STT: Deepgram `nova-3`, fallback OpenAI `gpt-4o-mini-transcribe`

## How To Run

Export keys into the current terminal session only:

```bash
export OPENAI_API_KEYS='...'
export ANTHROPIC_API_KEYS='...'
export DEEPGRAM_API_KEYS='...'
export GEMINI_API_KEY='...' # optional, direct smoke only today

scripts/provider-live-smoke.py
```

Strict mode fails if OpenAI, Anthropic, or Deepgram keys are missing:

```bash
BLUEY_PROVIDER_SMOKE_STRICT=1 scripts/provider-live-smoke.py
```

Gemini can be skipped:

```bash
scripts/provider-live-smoke.py --skip-gemini
```

## Expected Output

Output intentionally contains no secrets:

```text
PASS openai    text:gpt-4o-mini 612ms
PASS openai    vision:gpt-4o 951ms
PASS openai    embeddings:text-embedding-3-small 184ms
PASS openai    stt:gpt-4o-mini-transcribe 822ms
PASS anthropic text:claude-3-5-sonnet-latest 1030ms
PASS deepgram  stt:nova-3 441ms
SKIP gemini    text GEMINI_API_KEY/GOOGLE_API_KEY missing
```

## Follow-Ups

- Add Gemini to managed routing only after pricing, rate-limit, and model-lane
  decisions are locked.
- Add a server-mediated smoke once staging has a test account, credits, and
  stable `BLUEY_SERVER_URL`.
- Rotate any provider keys that were ever pasted into chat or screenshots
  before production.
