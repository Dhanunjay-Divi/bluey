# Bluey On Login Auto-Open - 2026-06-04

## Goal

Make the first-run customer path one command:

```bash
bluey on
```

If the desktop is already linked, Bluey starts quietly. If no account token is
available, Bluey starts the local overlay/session and opens
`https://bluey.sh/login` automatically so the user can sign in or create an
account.

## Changes

- `bluey on` now opens the sign-in page only when account state is unlinked.
- `BLUEY_SIGNIN_URL` still overrides the browser target for staging.
- `BLUEY_SKIP_SIGNIN_OPEN=1` suppresses browser launch for smoke tests and CI.
- `bluey login` remains as the explicit support/device-code path, but now opens
  `/login?user_code=...`.
- Server device-flow `verification_uri` now returns `/login`.
- Dashboard sign-in URL now defaults to `/login`.
- `web/index.html` treats `/login` as the primary account/link page and keeps
  `/link` as a compatibility alias.
- The static account page was simplified into a centered Pinky-style sign-in
  card with the same existing DOM IDs and API contracts.

## Verification

- Visually checked `http://127.0.0.1:8915/login`.
- Visually checked `http://127.0.0.1:8915/login?user_code=ABCD-EFGH`.
- `cargo fmt --all --check`
- `cargo test -p cue-cli`
- `cargo test -p cue-cli device_login_url`
- `cd server && cargo test`
- `cd server && cargo clippy --all-targets -- -D warnings`
- `cargo clippy -p cue-cli --all-targets -- -D warnings`
- `cargo clippy -p cue-dashboard --all-targets -- -D warnings`
- `git diff --check`

## Notes For Next Agent

Do not remove `/link`; it is still useful as a compatibility alias and the
desktop deep-link scheme remains `bluey://link?code=...`. The customer-facing
browser URL is `/login`.
