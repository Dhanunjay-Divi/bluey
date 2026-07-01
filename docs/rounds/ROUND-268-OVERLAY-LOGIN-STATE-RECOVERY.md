# Round 268 - Overlay Login State Recovery

## Trigger

After browser/CLI login, the installed CLI could fetch the current balance, but the overlay could still show signed-out chrome such as `Login`, `Sign in`, or the stale `Sign in to use Listen` prompt. The attached photo showed the exact mixed state: Ready status plus a valid account in CLI, but signed-out copy in the overlay.

## Root Cause

The daemon only sent balance text and generic boot cards to the native overlay. A running overlay could receive a signed-out warning card from the Listen auth gate, then later have a valid account/balance without an explicit "account is signed in now" command to clear the signed-out chrome and prompt.

Standalone `bluey login` also saved tokens locally without nudging an already-running daemon to refresh cloud state immediately.

## Fix

- Added shared overlay IPC command `set_account_state`.
- The daemon now sends `set_account_state { signed_in: true }` when:
  - balance refresh succeeds
  - the balance watcher publishes a snapshot
  - Listen auth verification succeeds
- The daemon now sends `set_account_state { signed_in: false }` during logout.
- `bluey login` now best-effort pings daemon `CloudStatus` after saving cloud tokens, so an already-running overlay refreshes immediately.
- The macOS overlay now:
  - parses `set_account_state`
  - clears signed-out chrome without wiping document/context state
  - changes the signed-out balance affordance from `Login` to `Sign in`
  - hides stale sign-in toasts once signed-in state is confirmed
  - treats real balance labels as signed-in evidence

## Verification

Passed:

```bash
cargo fmt
cargo test -p cue-core set_account_state_serializes_as_overlay_command -- --nocapture
cargo check -p cue-daemon -p cue-cli --quiet
swift build -c debug --package-path native/macos/cue-overlay
git diff --check
```

Local install/smoke:

```bash
cargo build -p cue-cli --bin bluey
cargo build -p cue-daemon --bin bluey-daemon
install -m 0755 target/debug/bluey ~/.bluey/bin/bluey
install -m 0755 target/debug/bluey-daemon ~/.bluey/bin/bluey-daemon
install -m 0755 native/macos/cue-overlay/.build/debug/cue-overlay ~/.bluey/bin/bluey-overlay-macos
install -m 0755 native/macos/cue-overlay/.build/debug/cue-overlay ~/.bluey/bin/cue-overlay-macos
codesign --force --sign - ~/.bluey/bin/bluey ~/.bluey/bin/bluey-daemon ~/.bluey/bin/bluey-overlay-macos ~/.bluey/bin/cue-overlay-macos
~/.bluey/bin/bluey off
~/.bluey/bin/bluey on
~/.bluey/bin/bluey status
~/.bluey/bin/bluey credits
~/.bluey/bin/bluey cloud status
```

Observed after restart:

- daemon started successfully
- overlay visible
- cloud status showed `auth: TokenConfigured`
- `bluey credits` returned `Balance: $14.50`

## Current State

Local macOS binaries in `~/.bluey/bin` are hot-installed with this fix and Bluey is running. The shared command schema gives Windows parity at the Rust protocol layer; Windows native overlay still needs a host build/smoke if its UI receiver is enabled in the Windows artifact path.

## Remaining QA

- Manual visual confirmation from the live overlay that the top-right balance area shows the dollar balance and no stale `Login`/`Sign in` text after browser login.
- If a user logs in from a fresh browser while Bluey is already running, verify that `bluey login` returns and the overlay updates without requiring `bluey off && bluey on`.
