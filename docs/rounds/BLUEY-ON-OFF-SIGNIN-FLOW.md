# Bluey On/Off Sign-In Flow

Date: 2026-05-23
Owner: Codex

## Decision

Customer-facing terminal flow is only:

```bash
bluey on
bluey off
```

There is no separate customer login subcommand. First `bluey on` starts the
daemon/session and opens `https://bluey.sh/link` if no local Bluey account
token is available. The browser/deep-link flow stores tokens in the OS keyring,
after which later `bluey on` runs directly.

Support/dev commands still exist for diagnostics, automation, and non-browser
testing, but they are hidden from normal CLI help.

## Implementation

- `crates/cue-cli/src/app.rs`
  - `on` / `off` are the only visible commands in `bluey --help`.
  - `login`, account, usage, credits, settings, support, doctor, logs, and
    other support commands are hidden.
  - `bluey on` checks keyring/legacy account config. If missing, it opens the
    sign-in page.
  - `BLUEY_SKIP_SIGNIN_OPEN=1` and `BLUEY_NO_BROWSER=1` keep tests/dev smoke
    from opening a real browser.
  - Boot-card copy reports managed-ready, browser-sign-in-open, skipped, or
    browser-open-failed states.
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

- Treat the hidden login subcommand as a support/dev escape hatch only, not a product
  flow.
- The server/device-code terminology can remain internal, but customer docs
  should always say first `bluey on` sign-in.
- Smoke tests must keep `BLUEY_SKIP_SIGNIN_OPEN=1` unless they explicitly test
  the browser/deep-link path.
