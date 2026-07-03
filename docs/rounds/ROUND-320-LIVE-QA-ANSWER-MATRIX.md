# Round 320 - Live QA Answer Matrix

## Trigger

The owner asked for broad live testing: send questions across major answer types, inspect the actual answers, fix gaps found during testing, and make sure logs/context are good enough to trace future user reports.

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## What Was Tested

Live QA was run against a local debug daemon connected to the production managed router. The fresh final session id was:

`b488f3a4-d0df-4d46-a6d1-5e0a0a0e50ac`

Final output directory:

`/tmp/bluey-live-qa-round320-final-20260703T102110Z`

Covered cases:

- quick factual answer
- code generation
- behavioral interview answer
- writing rewrite
- system design
- missing document context
- web/research-needed question
- missing screenshot context
- empty transcript prompt
- private prompt guardrail

## Issues Found

1. `bluey ask` defaulted to the old broad provider route, which could silently fall through to local deterministic context fallback. That made coding and transcript prompts answer with irrelevant “no screenshots/docs attached” text in CLI live QA.
2. A managed code response completed and billed successfully on the server, and the server logged a code artifact, but the daemon rejected the visible stream because the prose contained an unclosed Markdown code fence.
3. Code artifact previews could include `LINE NOTES` inside the displayed code fence, making copied code invalid.

## Fixes

- Changed CLI `bluey ask` with no explicit provider/model to use managed `balanced` directly, with no local fallback.
- Added a regression ensuring CLI ask does not silently fall back to local context answers.
- Let the daemon recover artifact-backed managed code answers from an unclosed visible code fence.
- Stripped dangling streamed code-fence tails before building the visible code preview from the managed artifact.
- Taught the code preview parser that `LINE NOTES` is metadata, not runnable code.
- Added regressions for unclosed code-fence recovery and line-notes separation.

## Final Live QA Result

All final fresh-session cases returned successfully with no stderr:

| Case | Exit | Latency | Result |
| --- | ---: | ---: | --- |
| quick | 0 | 758ms | Correct one-sentence answer. |
| coding | 0 | 2.3s | Valid Python code, closed fence, no line notes inside code. |
| behavioral | 0 | 7.7s | Natural generic answer with placeholders because no resume/doc context was attached. |
| writing | 0 | 2.4s | Clear rewrite plus optional stronger variant. |
| system_design | 0 | 12.7s | Useful 8-bullet design, but still slower than ideal for overlay. |
| missing_context | 0 | 1.3s | Correctly said no document was attached. |
| research_needed | 0 | 2.8s | Correctly said web search was unavailable and did not invent facts. |
| screen_needed | 0 | 1.5s | Correctly asked for screenshot/error context. |
| transcript_empty | 0 | 6.2s | Correctly said no live transcript was present. |
| private_guard | 0 | 363ms | Local guardrail refused internal prompt disclosure. |

Focused code rerun after the parser fix:

`/tmp/bluey-live-qa-round320-codefix-20260703T101913Z`

The code and follow-up paths returned valid, closed Python code blocks with no stderr.

## Verification

```bash
cargo fmt
cargo test -p cue-cli cli_ask_defaults_to_managed_balanced_without_local_fallback -- --nocapture
cargo test -p cue-daemon code_artifact -- --nocapture
cargo check -p cue-daemon --quiet
cargo build -p cue-cli -p cue-daemon
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh
```

Live QA commands used `./target/debug/bluey ask --stream --metadata`.

## Deployment

- Desktop release `0.1.59` is live on `bluey.sh`.
- Darwin arm64 release artifact:
  `https://bluey.sh/releases/v0.1.59/bluey-0.1.59-darwin-arm64.tar.gz`
- Release artifact SHA256:
  `dfcc71ffc35d5d5f981d941dee90ef9112c1ce5b1d0e9789924559d68feec3ac`
- Live `latest.json.sig` verified successfully.
- Live installer MIME checks passed for `/install.sh` and `/install.ps1`.
- Live macOS artifact SHA verified.
- Unpacked macOS release reports `bluey 0.1.59` and `bluey-daemon 0.1.59`.

## Remaining Notes

- The normal CLI does not expose an `attach` command, so document upload still needs overlay/drop-path or lower-level IPC smoke testing.
- Web search is answer-planned and logged, but the live provider is still not configured in this environment; research questions correctly report that instead of guessing.
- System design and behavioral answers work, but latency and personalization can still improve with better compact/canvas split and richer attached profile context.
