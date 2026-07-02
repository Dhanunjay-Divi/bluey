# ROUND-302-SHORTCUT-GUIDE-MOVE-HANDLE

Date: 2026-07-02
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Clean up the stale keyboard-shortcut affordance after removing plain single-letter shortcuts, and make the click-through 4-way move handle reliable enough to drag Bluey while blank overlay space still clicks through.

## Changes

- macOS overlay shortcut guide button now uses a help-style `?` icon instead of a keyboard icon.
- macOS shortcut guide title/copy now says `Controls and shortcuts`, matching the current behavior where letters type normally.
- macOS click-through drag state now forces the overlay window to keep receiving mouse events while the move-handle drag is active.
- Windows overlay shortcut guide button now says `Shortcuts` instead of `Keys`.
- Windows help/shortcut copy now avoids stale `keyboard shortcuts` wording.
- Windows click-through move handle visual target grew from 34px to 42px and the hit slop grew from 10px to 18px.

## Why

The old keyboard icon and `Keys` label implied single-letter local commands were still active. They are not. The move handle also needed a more forgiving target and a sticky drag state so click-through mode does not drop mouse ownership mid-drag.

## Verification

- `swift build -c debug --package-path native/macos/cue-overlay`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
- `git diff --check`
