# Cluely 2.0.193 Windows feature map

| Area | Static result | Evidence/boundary |
|---|---|---|
| Onboarding/auth | Observed | Clerk sign-in/deep link and permission/listening/hiding demos in renderer routes and main auth window |
| Mic/system audio | Observed | packaged SoX microphone plus Chromium `audio: "loopback"`; `main.js` offsets 501656-503000 |
| Live copilot | Observed | local VAD, cloud transcription/chat, streaming reconnect, transcript context |
| Screen context | Observed | `desktopCapturer` screenshots; no Windows OCR or semantic accessibility reader |
| Overlay/shortcuts | Observed | content-protected always-on-top windows and global shortcuts |
| Sessions/calendar | Observed | history/search/resume and Google Calendar context |
| Modes/files | Observed | custom prompt modes, templates, uploaded context files |
| Billing/settings/updates | Observed | plan gates, audio/settings, electron-updater |
| Job discovery/ranking | Not observed | no job source, ranking, application, or ATS route/service |
| Browser automation/profile isolation | Not observed | Electron UI Chromium is not a controlled job browser |
| Resume tailoring/diff/export | Not observed | files are context only |
| Answer memory/ATS adapters | Not observed | session/mode context is not application answer memory |
| CAPTCHA/2FA/takeover | Not observed for job automation | generic auth dependency text is not product evidence |
| Durable queues/leases/idempotency | Not observed | chat reconnect is not a durable work queue |
| Submission receipts/evidence | Not observed | screenshots are assistant context, not application receipts |
| Email/outcome tracking | Not observed | calendar exists; application email correlation does not |
| Deletion/privacy controls | Unknown | sign-out/reset present; server deletion/retention unreachable |

Workflow: sign in → configure permissions/mode/calendar → start session → capture
mic and loopback → local VAD → cloud transcription/assistant → persist cloud
session → revisit history. It is an interview/meeting assistant, not an
end-to-end job-application system.
