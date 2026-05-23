# Server Stage 8 — Codex Review Asks

> **Commit:** `92467dc feat(cli): bluey usage + bluey credits commands (Stage 8)`

Daemon-side customer-facing CLI commands. Both talk to bluey-server
via `cue-cloud-client` (Stage 3b), using the auth token in keyring.

## What I verified locally

- `cargo build` clean.
- `cargo test` workspace 400 unchanged (CLI commands are wrappers
  around CloudClient; integration tests are gated on a running
  server).

## What I want you to review

### 1. `bluey usage` (`crates/cue-cli/src/bluey_cmds.rs::show_usage`)

Format mirrors `docs/PRICING-MODEL.md` Section 4.3. Calls
`/account/me` + `/account/usage`. Prints:
- balance + auto-top-up status
- last 7 days cues + dollars spent
- tier label
- projected days remaining
- free trial minutes (if applicable)
- static three-tier comparison table
- per-bucket breakdown sorted by cost

**Ask:**
- Does the printed format match what you want customers to see in
  v0.2? The static three-tier table is hard-coded numbers from
  PRICING-MODEL.md; if those drift, the CLI drift too. Worth
  fetching them from a `/pricing/tiers` endpoint, or accepting the
  duplication for v0.2?

### 2. `bluey credits` stub

Just prints the balance + the 1-year-validity reminder. Per-batch
expiration listing is a TODO until we add a server endpoint
(`/account/credits` or similar) that returns per-batch rows.

**Ask:**
- Is the stub OK for v0.2 launch, or do you want the per-batch
  endpoint built before launch?

### 3. CLI integration without breaking the hidden legacy login path

The existing hidden login subcommand uses a separate (env var +
AccountConfig) flow that pre-dates cue-cloud-client. I deliberately
did NOT refactor it to use CloudClient because:
- It would require migrating AccountConfig storage to keyring.
- Existing dev users may have BLUEY_CLOUD_TOKEN env vars they rely on.
- The new commands (`bluey usage`, `bluey credits`) only need the
  CloudClient path to work.

If a customer has the legacy AccountConfig but no keyring token,
the usage command correctly tells them to run first `bluey on` because
`CloudClient::current_tokens()` returns None.

**Ask:**
- Should I bridge the existing hidden login subcommand to also save to keyring
  for cue-cloud-client compat? That would let one login command
  serve both flows. Or keep them separate until we deprecate the
  legacy flow at v0.2? My take: keep separate — legacy is for dev
  BYOK and is documented as such.

### 4. Not yet in Stage 8

- **Live balance display in overlay top strip.** Needs daemon to
  poll `/account/me` periodically AND push updates over IPC to the
  overlay. ~50 lines but lives in cue-daemon, separate from this
  CLI commit.
- **Per-card cost label in dashboard.** The infra is there (RouterMeta
  carries cost_cents from BlueyManagedProvider once it dispatches
  through the server), but the inline UI component isn't built yet.
- **Onboarding screen** (web UI on bluey.dev): not in this repo.
- **ProviderRegistry rewire** to use BlueyManagedProvider: queued
  for the daemon-integration commit; defining a separate stage to
  not bloat this one.

## Suggested verdict

Stage 8 is small and standalone. Risk is mostly in whether the
output format actually helps customers (UX critique welcome).
