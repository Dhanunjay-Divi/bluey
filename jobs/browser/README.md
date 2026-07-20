# Bluey Browser

Bluey Browser is the isolated local application runner and controller for reviewed Bluey Jobs applications. Its controller and tray are views; durable server claims, leases, checkpoints, final-submit markers, receipts, and recovery remain authoritative.

Background operation is opt-in. On packaged macOS and Windows builds, enabling **Start at sign-in** persists the preference, synchronizes the operating-system login item, and allows window close to hide the controller. A login launch creates the tray and controller quietly; a manual launch remains visible. Disabling the preference removes the same login item and makes window close quit. Linux supports the explicit close-to-tray preference but Bluey Browser does not claim or manage start-at-sign-in there. None of these settings enables employer-facing automation or claims a job by itself.

The tray exposes the current run status and the actions users need without reopening the full controller: show the controller, open the current application, pause or resume new application work, change background readiness, open Bluey Jobs, or quit. Bluey Browser remains local in this mode. It can continue after its window is hidden, but only while the computer is awake, online, and unlocked.

The local runner only claims work while the machine is online, awake, and unlocked. Electron can report suspend/resume, lock/unlock, and whether a machine is on battery power; it does not provide a trustworthy cross-platform critical-battery percentage. Bluey therefore does not claim critical-battery detection and does not block merely because a laptop is on battery. It never interrupts an active final-submit reconciliation for a power, lock, or connectivity transition.

The renderer is a local sandboxed page with a strict Content Security Policy and an action-only preload bridge. Application packets, emails, resumes, answers, claim tickets, and scoped capabilities are never sent to the controller renderer or operating-system notifications.

Every final employer Submit action crosses one shared authorization fence. A
new local delivery must contain separate result, resume, and submit
capabilities. Immediately before a typed ATS or generic adapter clicks Submit,
Bluey validates the submit capability locally, sends only that capability to
`POST /api/jobs/local-runs/:run_id/authorize-submit` with credentials and cache
disabled and redirects forbidden, validates the strict authorization response,
and rechecks local expiry after the bounded ten-second request. Only then may it
write the durable final-submit marker and checkpoint before clicking. Denial,
timeout, transport error, malformed JSON, capability expiry, entitlement or
identity revocation, a changed run binding, or a missing provider-review
approval all stop before the marker and click. Legacy checkpoints remain
readable for reconciliation but have no submit capability and fail closed.
