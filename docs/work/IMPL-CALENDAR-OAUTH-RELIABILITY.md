# IMPL: CALENDAR-OAUTH-RELIABILITY — End-to-End Calendar Connection

## Scope

**Does:**

- Makes Google/Microsoft native PKCE onboarding actionable and fail-fast.
- Keeps refresh credentials in the OS credential vault with legacy migration.
- Starts/stops connected providers without restarting the daemon and serializes
  each provider's connect/disconnect lifecycle.
- Correctly merges provider baseline, pagination, delta, and deletion results.
- Preserves stable provider occurrence IDs, structured conferencing meeting IDs,
  organizer details, and attendee email/RSVP data through approved meeting prep.
- Requests Microsoft Graph immutable IDs so event identity survives mailbox
  moves, while keeping conferencing IDs distinct from provider event IDs.
- Retries provider initialization after transient credential-vault failures.
- Reports configured, healthy, disconnected, and reconnect-required states.
- Restores and manages connection state both during onboarding and later in
  persistent account settings.
- Includes calendar support in every runnable distribution path.
- Validates the existing public webhook doorbell endpoints.
- Re-reads refreshed provider snapshots within 30 seconds so a newly connected
  account or changed meeting does not remain hidden behind a five-minute sleep.

**Does NOT:**

- Create provider OAuth applications or invent public client IDs.
- Move OAuth code exchange, bearer tokens, or calendar content through Bluey's
  server.
- Implement webhook subscription lifecycle or server-to-device push delivery.
- Provide multi-tenant webhook routing. The current environment configuration
  represents one Google channel/token and one Microsoft client state.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-calendar-cloud/src/provider.rs` | Modified | Runtime/build public-client config and validation |
| `crates/cue-calendar-cloud/src/oauth.rs` | Modified | PKCE exchange, refresh, timeouts, safe errors |
| `crates/cue-calendar-cloud/src/loopback.rs` | Modified | CSRF/error callback handling and bounded reads |
| `crates/cue-calendar-cloud/src/tokens.rs` | Reworked | Atomic keychain bundle, migration, cache, redaction |
| `crates/cue-calendar-cloud/src/google.rs` | Modified | Paginated sync-token merge and rolling baseline |
| `crates/cue-calendar-cloud/src/microsoft.rs` | Modified | Graph calendarView delta pagination, immutable IDs, and merge |
| `crates/cue-core/src/calendar.rs` | Modified | Connection health, provider occurrence identity, conferencing ID, and attendee schema |
| `crates/cue-daemon/src/calendar.rs` | Modified | Retryable dynamic multi-provider source registry and 30-second snapshot observation |
| `crates/cue-daemon/src/app.rs` | Modified | Serialized provider lifecycle, safe browser launch, and bounded calendar pre-context |
| `crates/cue-core/src/overlay.rs` and overlay clients | Modified | Scope meeting-prep responses to the exact occurrence |
| `crates/cue-meeting-overlay/ui/src/screens/Onboarding.tsx` | Modified | Reusable account controls, explicit errors, disconnect |
| `crates/cue-meeting-overlay/ui/src/screens/AgentsScreen.tsx` | Modified | Persistent post-onboarding calendar account management |
| `server/src/api/calendar.rs` | Reworked | Authenticated Google/MS webhook ingress |
| Build/release scripts and workflow | Modified | Ship the feature and require release client IDs |
| `docs/deploy/CALENDAR-OAUTH.md` | Added | Provider registration, runtime/build configuration, and live-test checklist |
| `ops/bluey-api.env.example` | Modified | Document optional webhook ingress credentials |

## Final validation

Final branch-tip verification completed on 2026-07-26:

- [x] `cargo fmt --all -- --check` — passed for the full workspace.
- [x] `cargo test -p cue-calendar-cloud` — 68 passed, 0 failed.
- [x] `cargo test -p cue-daemon --features parakeet-stt,local-memory,cloud-calendar` —
      423 passed, 0 failed, 18 intentionally ignored; all 8 calendar tests
      passed within this suite.
- [x] `cargo clippy -p cue-calendar-cloud --all-targets -- -D warnings` —
      passed.
- [x] `cargo clippy -p cue-daemon --all-targets --features parakeet-stt,local-memory,cloud-calendar -- -D warnings` —
      passed.
- [x] `cargo test -p cue-meeting-overlay` — 4 passed, 0 failed, including both
      pending-banner tests.
- [x] `npx --yes prettier@3.6.2 --check <18 changed UI files>` — all 18 files
      passed.
- [x] `(cd crates/cue-meeting-overlay/ui && npm run build)` — passed with 78
      modules transformed; the three mixed static/dynamic import advisories are
      pre-existing and non-fatal.
- [x] `(cd server && cargo test calendar)` — 8 passed, 0 failed, 130 filtered.
- [x] `(cd server && cargo clippy --all-targets -- -D warnings)` — passed.
- [x] `git diff --check` — passed for staged and unstaged changes.

An isolated live daemon IPC smoke verified the feature-on binary returns
actionable Google and Microsoft missing-client-ID errors immediately. Real
provider consent remains gated on external app registration and interactive
test accounts; this is not a code blocker.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Live consent not executed | No provider client IDs or interactive test accounts are available in this checkout |
| Polling remains authoritative | The repository has webhook receivers but no subscription/renewal/device-relay implementation |

## Known Follow-ups

- Register the Google Desktop and Microsoft mobile/desktop public clients, set
  the GitHub repository variables, and run one interactive consent/refresh test
  per provider.
- Add webhook subscription renewal and a content-free authenticated device nudge
  if latency below the polling path's roughly 75-second worst case is required
  (45-second provider refresh plus 30-second daemon observation).

## Review Checklist (for reviewer)

- [ ] Files match the scope described above
- [ ] No unrelated changes included
- [ ] Tests cover acceptance criteria from plan
- [ ] Code style matches AGENTS.md rules
- [ ] No TODOs without linked task IDs
