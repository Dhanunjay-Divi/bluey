# Round 240 - Release Secret Artifact Guard

Date: 2026-06-29 20:14 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner asked whether provider keys can be deployed safely and confirmed the
important requirement: server keys must not stay inside user-downloadable Mac,
Windows, or Linux binaries.

## Answer

Yes, the intended Bluey architecture is server-side keys only:

- Provider keys are read by `bluey-server` from runtime environment variables.
- User binaries authenticate to Bluey's managed server and do not need provider
  API keys.
- The deploy path must place keys in server env/secrets, never in repo, docs,
  installer scripts, release manifests, or compiled desktop binaries.

The provider keys pasted into chat should still be treated as exposed before a
public production launch. They can be used temporarily for internal smoke if the
owner accepts that risk, but production keys should be rotated and stored only in
server secrets.

## Fix

Extended `scripts/publish-bluey-release.sh` so release artifact publishing now
scans staged download artifacts for:

- dev-only visible overlay flags
- actual configured provider secret values from the publish environment

The scan covers AI, STT, web search, object storage, and billing secret env vars,
including:

- `OPENAI_API_KEY(S)`
- `ANTHROPIC_API_KEY(S)`
- `GEMINI_API_KEY(S)` / `GOOGLE_API_KEY(S)`
- `DEEPSEEK_API_KEY(S)`
- `ZAI_API_KEY(S)` / `ZHIPU_API_KEY(S)`
- `DEEPGRAM_API_KEY(S)`
- web-search provider keys
- object storage secret keys
- Square/Stripe secret keys

If a configured secret value appears inside a release artifact, publishing fails.
The failure message names only the env var label, not the secret value.

## Verification

Passed:

```bash
bash -n scripts/publish-bluey-release.sh
```

Smoke-tested the publish script with temporary fake artifacts:

- clean fake artifact passed the release scan
- artifact containing a fake `DEEPSEEK_API_KEY` value failed the release scan
- failure output did not print the fake secret value

## Current State

Release publishing now has an explicit guard against accidentally shipping
server/provider secret values in downloadable artifacts.

## Remaining QA And Gates

- Deploy keys only into the server environment or secret manager.
- Do not load production secret env vars into local packaging shells unless
  intentionally validating that the release guard catches leaks.
- Rotate the real pasted keys before public production launch.
