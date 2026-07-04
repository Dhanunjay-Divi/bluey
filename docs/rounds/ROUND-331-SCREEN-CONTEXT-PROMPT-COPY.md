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
- Bumped desktop workspace version from `0.1.68` to `0.1.70`.

Mac/Windows parity:

- The macOS native overlay now distinguishes screen-only, file-only, and mixed fallback questions.
- The Windows native overlay source now uses the same fallback copy rule for pending context chips.

## Verification

Passed locally:

```bash
bash native/macos/cue-overlay/build.sh
cargo fmt --check
cargo check -p cue-daemon
x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c -ld2d1 -ldwrite -lole32 -lshell32 -lshlwapi -lcomdlg32
```

## Deployment

Deployed as desktop release `0.1.70`.

- Code commit for the original macOS fix: `e63555dde634fcf606ccfeda413cdd8f0e0316c7`.
- Follow-up release includes Windows fallback source parity and the `0.1.70` version bump.
- Darwin arm64 release artifact:
  `https://bluey.sh/releases/v0.1.70/bluey-0.1.70-darwin-arm64.tar.gz`
- Darwin arm64 SHA256:
  `0ade2e66e79e0a23b249d21de754cbdc5e83fd041d4d1267103220727dc25a6f`
- Live `latest.json` reports version `0.1.70`.
- Deploy verifier passed:
  - release artifact dev-flag/secret scan
  - `latest.json` signature verification
  - `install.sh` content-type `application/x-shellscript`
  - `install.ps1` content-type `application/x-powershell`
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` version checks for `0.1.70`
- Local machine installed from public `install.sh`; `/Users/uno/.bluey/bin/bluey` and `/Users/uno/.bluey/bin/bluey-daemon` both report `0.1.70`.
- Local daemon restarted into fresh session `76fc0635-11ed-4481-bbe4-9dbd79bbd847`.
