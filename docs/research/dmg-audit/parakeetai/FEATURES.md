# ParakeetAI feature map

## Reading this map

“Observed” means the feature has a client implementation or call site in this exact DMG. “Partial” means only part of the requested workflow is present. “Not observed” means static app resources did not expose it; a remote dashboard or backend may still provide it. Marketing copy was not used as evidence. Bundle member hashes and reproducible offsets are in [bundle-static.txt](evidence/bundle-static.txt).

The constrained unauthenticated run stopped at the initial packaged-file route. Its visible text was `No Credits`, `ParakeetAI`, `Login to your ParakeetAI account to start your interview.`, and `Login`; the only textual links targeted buy-credits and desktop-auth web routes. No click or route transition was made. The renderer attempted unauthenticated `user.get` and Mixpanel-token configuration, but the network sandbox produced zero-byte/status-zero entries. [runtime-unauthenticated.txt](evidence/runtime-unauthenticated.txt)

## Product workflows

| Area | Static finding | Classification and provenance |
| --- | --- | --- |
| Onboarding | Desktop opens browser authentication, accepts a `parakeetai:` return link, then loads account/subscription state. A manual token path is also present. | **Observed.** `app.asar::dist/main/main.js` bytes 397000–402000 and 433000–435000; `dist/renderer/renderer.js` around bytes 1029000–1032000. |
| Authentication | NextAuth-named cookies are set for the configured API root; authenticated requests include credentials. | **Observed.** Main bytes 433851–434140; renderer `/api/trpc` at byte 1233944 and `/api/chat` at 1214837. Server enforcement is unknown. |
| Meeting-session setup | Regular and interview modes; interview form accepts company, title, description/job URL, resume, documents, language, model, extra instructions, automatic-answer and transcript-save options. | **Observed.** Renderer bytes 1006000–1053000; procedure names `callDocument.getMany`, `resume.getMany`, `scrapeJobPost.scrape`, and `callSession.create`. |
| Job discovery and ranking | The only job-related client flow statically observed is scraping a supplied job post to seed interview context. There is no discovery feed, ranking engine, eligibility gate, or tracked-job workflow. | **Not observed.** Full ASAR inventory plus the procedure/call-site index in [bundle-static.txt](evidence/bundle-static.txt). A remote service could have unrelated functionality not reachable from this client. |
| Resume import/tailoring/export | The session form selects an existing backend resume and documents. No desktop resume importer, evidence-preserving tailoring/diff, or DOCX/PDF export was found. | **Partial.** `resume.getMany` at renderer bytes 1043138 and 1051154; `callDocument.getMany` at 1043090/1046297. |
| Live transcription | Captures microphone and loopback/system audio, frames 16-kHz audio, streams to Speechmatics, displays partial/final transcript, and persists final batches to the backend. | **Observed.** Speechmatics URL at renderer byte 149538; `getDisplayMedia` at 1224969; `callSession.transcription.*` call sites listed in [bundle-static.txt](evidence/bundle-static.txt). |
| Live answers | Manual help, auto-help after speech, direct text, and analyze-screen triggers feed `/api/chat`; responses stream into the overlay. | **Observed.** Renderer around bytes 975000–1005000, 1097000–1126000, and `/api/chat` byte 1214837. |
| Answer memory | Prior AI messages and transcripts can rehydrate a call, and pending transcript context accompanies a chat request. No normalized reusable job-form answer memory was observed. | **Partial.** `callSession.aiMessages.get`, `transcription.get/createMany`, and chat transport in [bundle-static.txt](evidence/bundle-static.txt). |
| Screenshot assistance | Main process captures a display through `desktopCapturer`; renderer attaches screenshots for analyze-screen/chat. UI limits static call sites to 10 screenshots and 4 MiB encoded total. | **Observed.** Main bytes 418778 and 419584; renderer bytes 1097752–1098035. No application-submission receipt semantics were found. |
| Meeting auto-detection | A utility process and native module detect active microphone use by a configured list of conferencing/browser apps and can show a start/stop toast. | **Observed.** Main byte 409435, `mic-monitor-worker.js`, and unpacked Rust source described in [ARCHITECTURE.md](ARCHITECTURE.md). |
| Multi-device conflict | Live plan sessions use ping and expose a takeover action that warns the other device will be interrupted. | **Observed.** `callSession.ping` and `callSession.takeOver`; renderer byte 982007. Server lease semantics are unknown. |

## Job-application automation

| Requested capability | Static finding |
| --- | --- |
| Local browser execution | **Not observed.** Electron renders the product UI; no Playwright/Puppeteer application runner or job browser workflow was found. |
| Cloud browser execution | **Not observed.** No cloud-browser allocation/run API was found. |
| Browser profile and multi-account isolation | **Not observed for job automation.** Electron has its own Chromium session, but no account/identity-scoped application profile system was found. |
| ATS adapters and generic fallback | **Not observed.** No Greenhouse, Lever, Ashby, Workday, SmartRecruiters, or semantic form-adapter implementation was found. |
| Form intelligence and interventions | **Not observed.** There is no application-form field planner, sensitive-field policy, or reusable answer-memory workflow. |
| CAPTCHA, 2FA, assessments, browser takeover | **Not observed as application controls.** `callSession.takeOver` is a live-session device lock, not human takeover of an application browser. |
| Queueing, retries, crash recovery, duplicate prevention | **Not observed for applications.** Live audio has a bounded recovery path, but no durable application queue/lease/idempotency design was found. |
| Submission receipts and proof | **Not observed.** Screenshots are chat inputs, not tied to a submitted application, document hashes, confirmation URL, or evidence record. |
| Email/calendar integrations and outcome tracking | **Not observed.** The app links to web/dashboard pages but static client calls expose no Gmail, Outlook, Google Calendar, or Outlook Calendar connector workflow. |

The “not observed” results above derive from the verified complete ASAR file inventory and unpacked resource inventory, not marketing text. They remain bounded to the supplied desktop artifact.

## Reliability and user intervention

Observed live-session behavior serializes start/restart/stop/recovery actions, watches audio liveness, attempts a bounded recovery, and restores backend transcript/AI history. Pending transcript entries are only durable after the backend mutation succeeds; there is no local write-ahead journal in the static client. Consequently, the crash window for an unpersisted transcript tail is a reasoned risk, while exact loss behavior remains runtime-dependent. `app.asar::dist/renderer/renderer.js` session orchestration around bytes 1218000–1230000; procedures in [bundle-static.txt](evidence/bundle-static.txt).

The UI exposes manual controls for ending/extending/restarting a limited session, selecting capture inputs, toggling automatic generation, entering direct prompts, copying/rating responses, and taking over a conflicting session. It does not implement a general durable intervention queue comparable to a browser-automation runner.

## Plans, limits, settings, privacy, and updates

Static client state distinguishes subscription/lifetime access, credits, limited/free sessions, and a buy-credits dashboard route. Client constants include a limited trial/cooldown model and call-duration/extension bounds; those values are presentation/control hints, not authoritative billing evidence. The backend `subscription.getState` response is the apparent authority. `app.asar::dist/renderer/renderer.js` bytes 1029146, 1050625, 1064191–1067955, and 1170687–1170990.

Observed desktop settings include private/content-protection mode, auto meeting detection, selected screen, overlay opacity, font size/zoom, language/transcript behavior, model/auto-answer choices, launch at login, API environment/development controls, logs/devtools, and logout. The static desktop UI does not expose account deletion, a privacy export, retention controls, or telemetry opt-out. Those features may exist on the linked web dashboard and require runtime/server validation.

Mixpanel initialization uses EU ingestion and a backend-supplied token. Static event call sites include call/session identifiers, selected model/trigger, screenshot count, timing/region, recovery, copy/rating, and post-call feedback. The bundled Mixpanel library contains session-recording support as library code, but this audit found no app call enabling session recording; it must not be reported as enabled. Renderer byte 1151457 and nearby app-owned instrumentation call sites.

Updates use Electron Updater/Squirrel against `parakeetai/parakeetai-desktop-releases`, with periodic checks, user-facing severity, forced/server-gated state, and installation on quit. [identity.txt](evidence/identity.txt), `app.asar::dist/main/main.js` bytes 413000–417000.

## Model and language surface

The static model picker contains OpenAI GPT-4.1/GPT-5 variants, Claude Sonnet/Haiku 4.5, Cerebras GPT-OSS 120B, and Gemini 3.x variants; the displayed default is GPT-5 Mini. This is client configuration at renderer bytes 1006515–1009969, not proof of the exact provider endpoint, server model mapping, availability, or data-processing contract. Languages come from `configuration.getLanguages`, so the shipped bundle does not establish the production list.
