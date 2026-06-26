# Round 056 - Bluey On Permission Preflight - 2026-06-18

## Summary

`bluey on` now checks required macOS permissions before opening the desktop pill.

If Accessibility, Microphone, or Screen Recording is missing, the CLI:

1. Prints the missing permission names and status in the terminal.
2. Opens the matching macOS System Settings privacy panes.
3. Waits for up to 120 seconds, polling every two seconds.
4. Continues automatically once every required permission is granted.
5. Exits with a clear rerun message if approval is still missing at timeout.

If all permissions are already granted, `bluey on` behaves as before and proceeds directly to daemon/pill startup.

## Product Contract

Fresh Mac flow:

```text
bluey on
Bluey needs a few macOS permissions before the pill opens.
Approve the missing items in System Settings; Bluey will continue automatically.
Press Ctrl-C to stop waiting.

Missing macOS permission(s):
  - Microphone: not determined
  - Screen Recording: denied
```

Bluey should not show the pill first and then fail silently. The permission concierge runs first so the user knows exactly what macOS access is blocking real Listen, screen analysis, or hotkey behavior.

## Implementation

Changed file:

- `crates/cue-cli/src/app.rs`

New behavior is wired at the beginning of `cue_on`, immediately after the signed-update check:

```rust
crate::update::maybe_update_before_on(args.title.as_deref()).await?;
ensure_bluey_on_permissions_ready().await?;
```

The preflight is macOS-only. Non-macOS builds return `Ok(())`.

Permission statuses treated as ready:

- `Granted`
- `NotApplicable`
- `Unknown`

`Unknown` intentionally does not block startup. A probe failure should not brick launch; `bluey doctor` remains the support surface for investigating why a probe could not classify a permission.

Permission statuses treated as actionable blockers:

- `NotDetermined`
- `Denied`
- `Restricted`

Dev/headless escape hatch:

```bash
BLUEY_SKIP_PERMISSION_PREFLIGHT=1 bluey on
```

This is for development and automation only. It should not be used by normal install instructions.

## Verification

Commands run:

```bash
cargo fmt --all --check
cargo test -p cue-cli bluey_on_permission
cargo test -p cue-cli macos_permission_settings_uri
cargo clippy -p cue-cli --all-targets -- -D warnings
./target/debug/bluey doctor
```

Results:

- Formatting clean.
- Permission gate unit test passed.
- macOS Settings URI unit test passed.
- `cue-cli` clippy clean with `-D warnings`.
- `bluey doctor` on uno reports `permissions: 3/3 granted`.

## Areas Most Likely Wrong

- The System Settings URI fragments are private Apple URL routes. They work on current macOS, but Apple can rename panes. If opening the pane fails, the CLI still prints the exact missing permissions.
- The 120-second wait is a product choice. It gives users time to grant access without leaving a process hanging forever.
- Screen Recording approval may require app restart on some macOS releases. If that happens, the timeout message asks the user to rerun `bluey on`.

