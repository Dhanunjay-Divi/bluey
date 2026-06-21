# Bluey On/Off Sign-In Flow

Date: 2026-05-23
Owner: Codex

## Decision

Customer-facing terminal flow is only:

```bash
bluey on
bluey off
```

Customer-facing launch is `bluey on` / `bluey off`. `bluey on` starts the
daemon/session and keeps Bluey usable locally even when no cloud token exists.
If no cloud token is present, it opens `https://bluey.sh/login` automatically so
the user can sign in or create an account without learning a separate command.
Smoke tests can set `BLUEY_SKIP_SIGNIN_OPEN=1` to suppress browser launch.

`bluey login` remains as a visible/supportable account-linking command for now
because it is the safest terminal-only way to complete the server device flow:
it opens `https://bluey.sh/login?user_code=...`, polls the server, and stores
tokens in Bluey's private local account profile by default. Dashboard onboarding
can also open `/login` and receive tokens through the `bluey://link?code=...`
deep-link handoff. `/link` remains a compatibility alias for older links.

Support/dev commands still exist for diagnostics, automation, and non-browser
testing, but they are hidden from normal CLI help.

## Implementation

- `crates/cue-cli/src/app.rs`
  - `on` / `off` are the only visible commands in `bluey --help`.
  - `bluey on` checks the local account profile. If missing, it starts locally,
    opens `/login`, and prints the same fallback URL if the browser opener
    fails.
  - `bluey login` is the explicit browser/device-code path. It uses an
    in-memory token store while polling, then persists returned tokens to the
    account profile.
  - Boot-card copy reports managed-ready or sign-in-available states.
- Smoke/install/docs now point users to first `bluey on` sign-in.

## Verification

```bash
cargo fmt --all --check
cargo test -p cue-cli bluey_on_boot_lines
cargo check -p cue-cli -p cue-cloud-client -p cue-llm
cargo clippy -p cue-cli -p cue-cloud-client -p cue-llm -- -D warnings
bash -n scripts/smoke-test.sh ops/install/install.sh
git diff --check
./target/debug/bluey --help
```

Expected help output shows only `on`, `off`, and `help`.

## Kiro Review Notes

- Treat `bluey login` as the explicit terminal account-linking flow until the
  dashboard deep-link flow has a live production smoke.
- The server/device-code terminology can remain internal, but customer docs
  should always say first `bluey on` sign-in.
- Smoke tests must keep `BLUEY_SKIP_SIGNIN_OPEN=1` unless they explicitly test
  the browser/deep-link path.
