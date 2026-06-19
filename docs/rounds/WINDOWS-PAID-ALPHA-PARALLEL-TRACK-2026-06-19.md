# Windows Paid Alpha Parallel Track — 2026-06-19

## Why This Round Exists

The first 100 paid user plan is macOS-first, but Windows should move in
parallel without changing the cloud architecture. This round records what is
already present, what is missing, and when Windows becomes a launch blocker.

## Findings

- Bluey already has Windows native helper code for overlay and audio capture.
- Pinky uses the right Windows UI model for this product: Win32 windows,
  Direct2D/DirectWrite rendering, and `SetWindowDisplayAffinity` capture
  exclusion.
- Bluey should follow that native model for Windows rather than Electron,
  WebView, WPF, Wails, Fyne, or another large UI runtime.
- Windows packaging is not production-ready yet. Build scripts exist, but the
  canonical hosted artifact, installer, update path, and clean-machine smoke
  still need work.
- Windows should not block the Mac first-100 paid alpha unless Windows users
  are included in that invite list.

## Decision

Keep the first paid alpha macOS-first by default. Add a separate Windows P0
gate. If product decides to invite Windows users, that gate must pass first.

## Files Updated

- `docs/deploy/WINDOWS-PAID-ALPHA-READINESS.md`
- `docs/deploy/FIRST-100-PAID-USERS.md`
- `docs/PRELAUNCH-CHECKLIST.md`

## Verification

```bash
git diff --check
bash scripts/release-hygiene-scan.sh \
  docs/deploy/WINDOWS-PAID-ALPHA-READINESS.md \
  docs/deploy/FIRST-100-PAID-USERS.md \
  docs/PRELAUNCH-CHECKLIST.md \
  docs/rounds/WINDOWS-PAID-ALPHA-PARALLEL-TRACK-2026-06-19.md \
  docs/reviews/REVIEW-WINDOWS-PAID-ALPHA-PARALLEL-TRACK-BY-CODEX-2026-06-19.md
```

## Operator Dependencies

- Clean Windows 10/11 test machine.
- Visual Studio Build Tools.
- A real credited Bluey test account.
- Snipping Tool and at least one meeting/recording app for capture-exclusion
  verification.
- Decision whether Windows users are part of the first paid alpha.
