# Round 512: Jobs Freshness And Application Evidence

Date: 2026-07-10

## Goal

Keep Bluey focused on recent, still-open jobs and make every submitted application auditable from one clean timeline. A user must be able to see the exact resume used, the submission confirmation, relevant inbox updates, and interview calendar events without inspecting internal records.

## Freshness Policy

- The default maximum posting age is 14 days.
- Users can select 7, 14, 21, or 30 days in Jobs Settings. The server accepts a bounded 1-60 day policy.
- Public ATS discovery parses ISO timestamps, epoch timestamps, and relative labels such as `Posted Today`, `Yesterday`, and `3 days ago`.
- Old and undated public-feed listings are removed before matching.
- The server independently rejects packet preparation for listings outside the account policy or listings marked expired/unknown.
- Queueing or starting an application requires a still-open verification from the last 24 hours. UI filtering cannot bypass this rule.
- Pasted job links are treated as first seen when saved. Their application runner must still confirm that the form remains open before submission.

## Application Evidence

Added an encrypted, tenant-scoped, append-only evidence ledger keyed to one application:

- exact resume artifact and resume version
- cover letter and other attachments
- submission confirmation
- Gmail or Outlook status messages
- Google or Outlook interview calendar events

Every record has a stable provider/document identity. Replaying a browser receipt, webhook, inbox sync, or calendar sync does not create duplicates.

An application cannot enter `submitted` until both conditions are true:

1. The exact resume artifact matches the resume version frozen on that application.
2. A submission confirmation is present.

This prevents a workflow from claiming success while losing the document that was actually attached.

## Automation Contract

`@bluey/jobs-automation` now projects its deterministic receipt bundle into the server evidence shape. Resume documents preserve storage key, file name, media type, SHA-256 digest, and resume version. Submitted results produce a confirmation record.

Authorized mailbox and calendar workers have one typed helper that requires:

- Bluey application ID
- provider name
- provider event/message ID
- occurrence time
- user-facing label and selected metadata

This is the ingestion contract for authorized Gmail, Outlook, Google Calendar, and Outlook Calendar connections. Provider authorization and event polling remain controlled by the Jobs beta integration flow; the UI does not claim a disconnected provider is syncing.

## Portal Changes

- Matches now show only active jobs inside the selected freshness window.
- Match rows say `Posted today`, `Posted yesterday`, or `Posted N days ago`.
- Match detail shows the posting age and last verification time.
- Settings explains that Bluey skips older listings and checks that a job remains open.
- Submission receipts no longer dump internal JSON.
- Receipts show submission status, exact resume file, application email, and a compact evidence timeline for submission, email, and interview events.
- Missing evidence is shown as an explicit incomplete state.

## Privacy And Terms

Privacy and Terms now explain that:

- submitted application receipts retain the exact resume/files and linked status/interview events;
- posting dates and availability come from employers/providers and may be incomplete or delayed;
- authorized email and calendar signals join the matching application timeline.

## Verification

- Rust Jobs DB tests cover old-posting rejection, live re-verification, immutable/idempotent evidence, and the exact-resume-plus-confirmation submission gate.
- Automation tests cover recent/old/undated ATS listings, relative date parsing, receipt projection, and provider event linking.
- Portal type checks and tests cover the updated workspace shape.
- Dark and light desktop views and the mobile receipt layout were checked in the local Jobs preview.
- No Bluey host overlay, audio, meeting runtime, or native session code was changed.
- No deployment was performed in this round.
