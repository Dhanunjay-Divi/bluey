# Phase 626 — exact text-preprod receipt and continuation

Date: 2026-10-09. Preflight: `bluey-ops`, `pinky-ops`,
`pinky-bluey-integration-ops`; repository AGENTS and shared local-machine queue.
This is a bounded text integration, **not whole-product completion or promotion**.

## Active immutable artifacts

| Role | Live source | Artifact/verification anchor |
| --- | --- | --- |
| Pinky API/web | `76482a171693f67d833a839a10504895be25a449` | hashes.txt SHA256 `49a9366be24cda50a74593e2800b779a47270e4a14212db51f1e9e40878996e6` |
| Bluey AI | `556525a5be58f4c68ece5a994fc3c44822a31645` | manifest SHA256 `49fe4f2098a8b741a358a4590f36a1585ca40403c3a6a7f7a13b715bcc78141a` |

Pinky binary SHA256 `629f6a9e5b9cb65f1ba1ee065d4cdfeafd007568d2615a8a9c7e1b0f0633e7d7`;
web archive `8d88c5b7af05cbf9bac50a110b46ee067f76c960277596291fd415fb894b926b`.
Bluey binary `f2037cd45671ee896ad1b0d6a11ecff60607bfcd38b394c1060e6529ea13acc7`;
x86_64 Linux/glibc2.39. Private exact artifacts are retained under
`~/.local/state/bluey-pinky-integration/{pinky-linux-builds,linux-builds}/<source>`.
Later documentation commits are not the binary source pin.

Host: dedicated `bluey-pinky-assist-preprod` /162.243.248.189, approved $7/month.
Live origins: assist-preprod.bluey.sh and api-assist-preprod.bluey.sh.
Relay origin intentionally503; media unavailable. Reviewed `cutover-assist-preprod.sh`
verified artifact hashes, root-owned immutable resources, health and `/proc` binary
digest. Bluey rollback source `d203d0d4bcfa68adc254b1daf69f7157f955ca6e` is retained;
no DB/schema/seed/env/keys/credit changed by this same-schema cutover.
Both units active, NRestarts=0. Host24G/2.4G used/21G available; no resize.

## Local and live results, individually

- Bluey repair: PostgreSQL1/1, focused35/35, all-target932/932,
  strict all-target Clippy `-D warnings` passed. Harness acquired mac-heavy;
  `/tmp/bluey-pinky-postgres-tests.KuTzVN` removed with no owned process.
- Exact Linux build locally on Mac:5m06s; owned build root
  `bluey-pinky-linux-build.bhqlJO` removed. No hosted runner or VM compilation.
- Pinky retained targeted runtime Go race +Node32/Python26 passed; latest
  availability renderer raised Node aggregate to35/35. Not a full Pinky suite.
- Fresh public negative/lifecycle harness passed: exact-origin/CSRF, immutable
  ownership, two AI accounts and one not-added, all media unavailable,
  Stop-before-Ask zero dispatch, foreign-run denial and durable cancelled recovery.
- Real synthetic three-case answer harness passed streaming, settlement,
  authoritative status, Short length/plain format and STAR labels. No real user
  facts, transcripts or payments were used. No request retry or reseed.

| Mode | Words | First nonempty text | Total client time | Actual provider model |
| --- | --- | --- | --- | --- |
| Short | 88 | 913ms | 1548ms | claude-haiku-5-5 |
| Default | 257 | 784ms | 3318ms | claude-haiku-5-5 |
| STAR | 108 | 1433ms | 1901ms | gpt-5.4-mini |

Content-free completion metadata respectively: input/output1613/153,
1384/475,3068/125; server latency1467/3241/1798ms; each synthetic customer debit1c.
These are individual observations, **not first-token percentiles, provider
invoices or an always-fastest comparison**. GPT6Sol remains server-benchmark-only
default-off. Selected approved existing OpenAI/Anthropic keys stay root-only;
upstream cap100c/24h and Jobs model/local/cloud distribution flags0 preserved.

**Strict STAR factual quality FAILED manual review.** The API answer added
query-plan improvement/access-pattern details not provided. A second visible
STAR answer generalized a one-time completion into resolved timeouts. Both
transport and labels worked; neither is a factual PASS. Do not silently discard
this evidence, relax the requirement, or call the grounding bug closed.
Visible UI recovered Ask enabled/Stop removed after completion. Private synthetic
screenshot: `~/.local/state/bluey-pinky-integration/evidence/assist-star-556525a5-20261009.png`.
Screenshot SHA256 `540574c7ed8b5a70819d0a6f0d6ed2929494ab8dcc67e9f2bdc00f0997560adb`.
Synthetic browser account logged out; private credential binding cleared and
temporary viewport override reset. Only explicit private artifact/rollback
and failed-evidence files remain, not temporary database/build folders.
No transcript/credential/content upload is part of routine diagnostics.

## Historical failures retained

65ec baseline Short excessive length/literal Markdown;62f9 Short140words;
d203 Short85words passed but STAR lacked labels and invented details.
556525 fixed labels but did not pass strict factual review. Pinky de57 staging
rejected Mac AppleDouble metadata before activation;37b7 packaging validation
and76482 checking-state UX repaired that slice. Failed builds are not live pins.

## Concrete continuation, not blanket deploy

1. Close STAR grounding with an evidence-aware contract and adversarial cases;
   do not repeatedly rely on prompt-only truth claims. Keep style separate from
   factual authority; missing facts remain missing. Check source excerpts/claim
   IDs once memory exists, and expose review status in personal interview drafts.
2. Implement explicit private interview import only after scoped design review:
   owner speaker attribution, full bounded chunking, confirmed/proposed stories,
   exact collection filtering, citations, text/object/vector deletion and
   anti-resurrection tests. Owner asked for2–3 exports; none imported/read.
   Read `PHASE-626-INTERVIEW-KNOWLEDGE-SEAMS.md`; broad account RAG is not this feature.
3. Run physical Mac/Windows compact overlay/real-stream/Stop/reconnect acceptance
   against accepted current Pinky base. Native compile is not physical testing.
4. Source-specific revocable audio/screen/mic consent is a separate off-by-default
   slice. No text-only result enables media or claims transcription tested.
5. Decide/verify PAYG and active-hour economics, timer boundaries, reload/holds/
   refund/idempotency. $15 PAYG/$9 hour proposals are not accepted real billing.
6. Measure sustained latency percentiles, deadline/timeout/provider fallback,
   crash/reconnect/replay and resource behavior. Tiny samples are insufficient.
7. Coordinate with active Pinky release task (R1014 caption/upload work): preserve
   its new accepted dashboard/native changes; never import its dirty tree wholesale.
   Integration's base dashboard template was unchanged; mount fragment conditionally.
   Feature branches remain separate; no direct main push, tag, hosted runner or
   production promotion until joint gates and owner approval.

Pinky branch `codex/bluey-integration-runtime-20261009`; Bluey branch
`feat/phase-626-pinky-integration`. Production Bluey/Pinky, existing Pinky preprod
and Jobs were not restarted or deployed. The separate Bluey production full-disk
backup incident remains open; do not delete retained backups to make integration space.
