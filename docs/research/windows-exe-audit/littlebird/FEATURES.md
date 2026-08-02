# Littlebird 0.81.10 Windows feature map

| Area | Static result | Evidence/boundary |
|---|---|---|
| Onboarding/auth | Observed | onboarding routes/maps, browser/deep-link auth, token handoff |
| Context collection | Observed | Windows helper window/screen/OCR capture, running/front app, exclusions, manual snapshots |
| Mic/system audio | Observed | helper `WasapiCapture` and `WasapiLoopbackCapture` symbols |
| Meeting assistance | Observed | meeting start/stop/transcript/readiness/device events and meeting UI routes |
| Workspaces/projects | Observed | project assignment, meetings, threads, artifacts, notes, contextual chat routes/maps |
| Calendar/contacts/messages | Partial by platform | packaged APIs/events include calendar and integrations; several EventKit/iMessage paths are macOS-only |
| Local search | Observed | signed `rg.exe`, category seed, FlexSearch/Dexie dependencies |
| Privacy exclusions | Observed | app/domain exclusions, sensitive-category seeds, content filters, pause collection |
| Crash recovery | Observed | child restart, critical state replay, queue/callback timeout |
| Billing/telemetry/update | Observed | plan/feature flags, Sentry/PostHog/Axiom, S3 alpha updater |
| Job discovery/ranking | Not observed | no ATS discovery/application model |
| Job browser/profile isolation | Not observed | captured apps are context, not automated job browsers |
| Resume tailoring/diff/export | Not observed | general artifacts/editor/diff libraries do not prove a resume workflow |
| Application answer memory | Not observed | contextual memory is broader assistant memory, not verified application answers |
| ATS adapters/CAPTCHA/2FA takeover | Not observed | no job submission path |
| Durable application leases/receipts | Not observed | helper replay is not irreversible-job idempotency |
| Email/calendar outcome correlation | Not observed for applications | integrations do not prove application outcome tracking |
| Deletion/privacy | Partial | exclusion/delete-context/clear-auth controls exist; backend hard deletion unknown |

Littlebird contributes the broadest coherent assistant/workspace/context model of
the five products. It still does not overlap Bluey's ATS execution, final-submit
safety, browser identity isolation, or application evidence pipeline.
