# Phase 626 — exact text-preprod receipt and continuation

Date: 2026-10-09. Preflight: `bluey-ops`, `pinky-ops`,
`pinky-bluey-integration-ops`; repository AGENTS and shared local-machine queue.
This is a bounded text integration, **not whole-product completion or promotion**.

## Active immutable artifacts

| Role | Live source | Artifact/verification anchor |
| --- | --- | --- |
| Pinky API/web | `b7de1545ef6e10a13f3c5d37bf30998e68d14560` | hashes.txt SHA256 `5e25c38b8e673304ea1b41b9db69f39681f2323515223d6133dd035f0726175a` |
| Bluey AI | `625b9131db0ef6e8b89cd28e9f7a595a12c0350e` | manifest SHA256 `503039d773017a15b115e5857838fed29b881a3434d3c44ed51cfe53b34cb79f` |

Pinky binary SHA256 `bd1c564bf68e70cbf1b5c2baabbd0bb33fd65f9ca2726a39dc27081913d25091`;
web archive `d98013dfee42aa518a0f3de022de0f466ab01ad0cf02817fde6181e7643e570c`.
Bluey binary `153b22c26020db7a1c594a75566c38405ef90d1e7685fb290fbb7cf27e65568d`;
x86_64 Linux/glibc2.39. Private exact artifacts are retained under
`~/.local/state/bluey-pinky-integration/{pinky-linux-builds,linux-builds}/<source>`.
Later documentation commits are not the binary source pin.

Host: dedicated `bluey-pinky-assist-preprod` /162.243.248.189, approved $7/month.
Live origins: assist-preprod.bluey.sh and api-assist-preprod.bluey.sh.
Relay origin intentionally503; media unavailable. Reviewed `cutover-assist-preprod.sh`
verified artifact hashes, root-owned immutable resources, health and `/proc` binary
digest. Bluey rollback source `556525a5be58f4c68ece5a994fc3c44822a31645` is retained;
Pinky previous source `cfcbdab61a81dfad90ce3ca537c387ecbffdb5df` and earlier
`76482a171693f67d833a839a10504895be25a449` are retained. Exact Caddy extensions
allow only authenticated Pinky account GET/HEAD and signed Bluey account POST;
configuration SHA256 `fa2a819ca3bdb5546448c9ddb1ae12126210975c8573bd13ec24cb35b7fa0fe5`.
no DB/schema/seed/env/keys/credit changed by this same-schema cutover.
Both units active, NRestarts=0. Host24G/2.4G used/21G available; no resize.

## Latest combined overlay and product website — 2026-10-09

Pinky b7 supersedes the b68 website described below. Two product tabs,
Remote access/Pinky AI, use one existing Billing entry. Exact same-schema
single-role cutover verified process/web hashes and health/NRestarts0. b68 is
retained for rollback; no Bluey restart, configuration, seed, schema, credit,
production, other preprod or Jobs change. Local queued runtime-race plus
Node50/Python26 passed. Fresh public lifecycle/tenant/CSRF checks and actual
Chrome product/hash/reload/back/keyboard/light/dark/320px checks passed.
Private final screenshot SHA256
`646420bec3db9abd2a362c79275e7529f5e94ce3d64e2d8d5336c2becc74ad10`:
`evidence/assist-workspace-b7de1545-20261009.jpg` in the private integration root.

Native QA source ae3 keeps the 116×36 pill and 320×320 AI panel and adds
Auto/Stacked/Side by side, separate CC/AI Stop/Hide and guarded local shortcuts.
Queued Mac AppKit/both-arch and Dell native tests/compilation passed. Actual
Mac synthetic-caption/real-text tests checked layout, CC off preserving AI,
Stop/Hide/reopen and toolbar light/dark contrast. QA was stopped/cleaned.
The screenshot-visible synthetic QA helper is a distinct unsigned wrapper;
normal capture protection stays intact. Exact hashes/provenance are in Pinky's
`BLUEY-INTEGRATION-CC-AI-LAYOUT-20261009.md` round. Website details are in its
`BLUEY-INTEGRATION-PRODUCT-SHELL-20261009.md` round.

These gates do not close physical Windows/DPI, signing/installer/media,
strict STAR factual quality, private transcript memory, real AI billing,
latency percentiles or production promotion. No main merge or GitHub publication
is claimed; feature commits are privately bundled pending authorized transport.

## Superseding compact website slice — 2026-10-09

Owner rejected the oversized account UI. The b68 Pinky source replaces it with
a compact heading, underline tabs, three quiet cards, dense metadata history,
real available/held credit and responsive opt-in phone navigation. No native
pill size, backend/auth/accounting/consent change. Bluey625 stays deployed.
Previous Pinky c8 is retained for rollback; private exact artifacts use the
usual `pinky-linux-builds/<source>` root. Cutover verified process hash/health
and NRestarts0 without schema/seed/key/env/credit/Caddy changes.

Local runtime Go + Node45/45 + Python26/26 and exact Linux build passed;
owned test root `pinky-bluey-tests.vd476y` was removed. The public lifecycle
harness passed auth/CSRF/Origin, real bounded credit, metadata-only history,
tenant isolation and zero-dispatch reordered Stop. Visible Chrome Overview/
history/Balance, keyboard End/Home, desktop light/dark,320px header/menu QA
passed. DOM viewport/document widths both320; tabs96x44. Viewport reset.
Private final screenshot SHA256
`ba7e617fbf251783030557cfa4b3097d47e49b8a13e7b60059903041c7118691`,
`evidence/assist-account-b68b27fa-20261009.png` under the private integration root.
Broader physical-device/200% zoom/screen-reader matrices are not certified.

Unchanged c8 native source also passed exact Go1.26.5 Windows CLI build on the
approved Dell queue and offline invalid-argument smoke; unsigned CLI SHA256
`b06d533ff0449bf4bc5b95ab625788804289de348383244fddb6d9d5e7ee7eeb`.
No Windows GUI/auth/media request: physical Windows UI and full Go suite remain
open. All earlier STAR/media/memory/billing/percentile/promotion gates remain.
Pinky's `BLUEY-INTEGRATION-WORKSPACE-POLISH-20261009.md` is the detailed receipt.
GitHub publication is pending authorized noninteractive Git transport; local
feature commits are retained in verified private bundles. Do not claim a push
or read the Keychain to work around SSH denial.

## Preceding desktop/account slice — 2026-10-09 (c8 native artifact)

- Bluey actual PostgreSQL1/1, focused38/38, all-target935/935 and strict
  all-target Clippy passed. Owned PG root `bluey-pinky-postgres-tests.Dfjqxv`
  and Linux build root `bluey-pinky-linux-build.zdEUUM` removed. No unrelated
  PostgreSQL process was stopped. Exact Linux build took4m59s on the local Mac.
- Final Pinky runtime-race harness passed targeted Go suites, Node44/44 and
  Python26/26. Owned root `pinky-bluey-tests.QohkzS` removed; local Mac arm64
  CLI and arm64/x86_64 native helpers built. No hosted runner was dispatched.
- Public harness rerun against the exact final pair passed auth, immutable
  tenant identity, CSRF/Origin, real bounded integer credit, metadata-only
  owned history, Stop-before-Ask zero dispatch and foreign-run denial. No model
  request or real payment in that harness.
- Actual Mac `PinkyAssistQA.app` streamed a plain SQL answer with Ask becoming
  available at completion. Repeated Close/Open twice retained that answer only
  after reauthenticating the same owned request. A separate post-cutover request
  showed Stop, transitioned through accounting_pending, then returned to Ready
  with editable prompt/Ask and no Stop. This is bounded independent text QA,
  not media, signed installer, Intel hardware or full device acceptance.
- Mac native final helper SHA256
  `e21728433db50a5d6226ebdd80fbd8e1b1e9c19284036e8c835f37f7800918c5`;
  CLI `77a09a00b196cf12d13241c7122fe65702cf1497ca01db2db39bd6725e305004`.
  The capture-excluded panel's AX controls were observable; screenshots remain
  capture-excluded by design, not evidence of a disabled protection.
- Dell GCC15.2 native fixture tests/helper build passed; helper SHA256
  `8612386bbc2a4d78119271c1121e6c86fe012cb2b5fc693a2536e57c65c5e759`.
  Full Windows Go CLI and physical Windows UI acceptance remain open.
- Final live management website passed Overview/AI history/Balance, keyboard
  tab selection, light/dark and 320px scoped account tabs. No website Ask
  composer exists. History contains request metadata, not saved transcripts.
  The legacy global Pinky header still overflows at320px; not an all-page
  mobile acceptance. Viewport restored after testing.
  Private screenshot `evidence/assist-account-c8ac6511-20261009.png` under the
  integration state root, SHA256
  `9c709a95a9dbc2bcd76ff447483149f94d645c3d1f267160ae37d24f45691ce7`.
  Pinky's handoff includes the exact operator-only Mac relaunch command; no
  production account, token argument, saved-auth file or remote capture needed.

## Earlier text baseline results (historical, individually)

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
3. Expand bounded Mac text QA to full device/theme/keyboard/crash/reconnect
   acceptance; run full Windows suite and physical UI acceptance. Native helper
   compilation is not physical testing. Current Mac text checks do not certify media.
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
