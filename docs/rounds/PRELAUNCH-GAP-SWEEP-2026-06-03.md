# Prelaunch Gap Sweep — 2026-06-03

## Scope

Review the current `feat/phase-3-round-12` live state after the Resend mail
fix and identify what can be truthfully closed before provider-key and inbox
operator checks.

## Verified Live

- `https://bluey.sh/health` returns 200 with server commit `94366e4`.
- `https://bluey.sh/admin/health` returns 200 over HTTPS.
- `https://bluey.sh/pricing/tiers` returns the canonical pricing tiers.
- `bluey.sh` A and AAAA records point to the DigitalOcean droplet.
- `www.bluey.sh` is a CNAME to `bluey.sh`.
- CAA restricts issuance to Let's Encrypt.
- Resend DKIM, SPF TXT, and DMARC TXT records are publicly visible.
- Caddy and `bluey-api` are active on the droplet.
- Public listeners are 22, 80, and 443.
- `install.sh`, the macOS arm64 tarball, and `SHA256SUMS.txt` return 200.
- The release tarball contains `bin/bluey`, `bin/bluey-daemon`, and native
  helper binaries.
- Observability analyzer passes with zero transitional/PII findings.
- Observability acceptance smoke passes all assertions.
- `bluey doctor --json` reports actual local probe/log state.
- `bluey logs export` produces a redacted support zip.

## Completed Fixes In This Sweep

- Updated prelaunch checklist from stale "observability in progress" wording to
  the current completed state.
- Split server env readiness into:
  - core/Square/SMTP env present,
  - managed provider keys still pending.
- Corrected env-file permission guidance from `0600 root:bluey` to
  `0640 root:bluey`; the service runs as group `bluey`, so group-read is
  required.
- Updated server README and deployment runbook from older SMTP/no-reply wording
  to Resend + `hello@bluey.sh`.
- Updated the deploy pass doc to reflect the currently deployed server commit.
- Tightened the log-export leak check: search for concrete secret shapes rather
  than broad words like "key", which can appear in harmless diagnostics such as
  "no OpenAI API key configured".

## Still Pending

These are not code blockers in this repo, but they block an honest public-alpha
claim:

- Managed provider keys are not installed on the droplet:
  - `OPENAI_API_KEYS`
  - `ANTHROPIC_API_KEYS`
  - `DEEPGRAM_API_KEYS`
- Live inbox confirmation is pending for verification and password reset email.
  The server returned 202 and logged delivery handoff to Resend from
  `hello@bluey.sh`; the operator still needs to confirm receipt/link open.
- Square sandbox checkout and production checkout still need real hosted
  checkout completion tests.
- Square webhook registration and signature key should be confirmed in the
  Square dashboard before accepting real money.
- Backup cron/off-host destination/restore verification are still pending.
- DNSSEC is not enabled.
- Clean-Mac installer smoke is still pending.
- Real managed Mac smoke steps 4-10 are pending until provider keys and a
  logged-in test account are available.

## Next Operator Step

Check Gmail/Resend delivery for the latest `hello@bluey.sh` smoke. If email
arrives, click both links and close the SMTP inbox gates. If it does not arrive,
check the Resend Emails dashboard delivery status for the message id and mailbox
provider response.
