# Round 331 - Screen Context Prompt Copy

Date: 2026-07-04
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-overlay-spacing-20260626

## Trigger

The owner showed a screen-context ask where the visible question bubble said:

```text
Answer using the attached screen capture, documents, and current session context.
```

Only a screen context chip was attached. The generic wording made it look like documents/session context were attached or required.

## Root Cause

The macOS overlay used `pendingContextItemIds.isEmpty` to decide whether attachments existed. A screen-context chip itself is a pending context item, so the `screen + pending attachments` branch fired even when the only pending item was the screen capture.

## Fix

- Added a helper that filters pending context items down to non-screen items.
- Screen-only asks now show:
  `Answer using the attached screen context.`
- Screen plus actual file/document asks now show:
  `Answer using the attached screen context and files.`
- File-only asks now show:
  `Answer using the attached files.`
- The backend context payload is unchanged; this only fixes the user-facing question bubble copy.
- Bumped desktop workspace version from `0.1.68` to `0.1.69`.

Mac/Windows parity:

- This round changes the macOS native overlay. The same copy rule should be mirrored in the Windows overlay prompt builder if/when that native UI has a matching hardcoded fallback string.

## Verification

Passed locally:

```bash
bash native/macos/cue-overlay/build.sh
cargo fmt --check
cargo check -p cue-daemon
```

## Deployment

Pending at initial doc write.
