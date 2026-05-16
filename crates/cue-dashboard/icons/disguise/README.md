# Disguise Icons

Placeholder icons used when Cue is running in disguise mode. Each mode
makes the app appear as a different system utility in the dock/taskbar.

**Status:** Icon assets are on disk but NOT yet wired into the running app.
The `apply_disguise` function changes process name and window title only.
Dock/taskbar icon changes (`window.set_icon()`) are deferred until
production-quality icons are available. See IMPL-PHASE-3-ROUND-8.md.

## Structure

```
mac/
  terminal.png   — macOS Terminal disguise
  settings.png   — macOS System Settings disguise
  activity.png   — macOS Activity Monitor disguise
win/
  terminal.png   — Windows Command Prompt disguise
  settings.png   — Windows Settings disguise
  activity.png   — Windows Task Manager disguise
```

All icons are 256×256 RGBA PNGs.

## Generation

These placeholders were generated with Python Pillow using simple text
glyphs on solid backgrounds. The script used Menlo font for the text
symbols (">_", "⚙", "i").

To regenerate or replace with production-quality icons:
1. Design 256×256 PNGs matching the target app icon style.
2. Drop replacements into the appropriate `mac/` or `win/` directory.
3. Keep filenames unchanged — the stealth crate references them by name.

## Manual Smoke Test

> **Note:** Steps 3/5 below (dock/taskbar icon changes) are NOT yet
> functional. Only window title and process name changes are active.
> TODO: Wire `window.set_icon()` in cue-stealth once production icons land.

### macOS
1. Open Cue → Settings → Stealth & Disguise section.
2. Select "Activity Monitor" from the dropdown.
3. ~~Verify the dock icon changes to the activity placeholder.~~ (deferred)
4. Check Activity Monitor label appears in the app menu bar.
5. Switch to "Terminal" — verify ~~dock icon and~~ menu bar update.
6. Switch to "None" — verify original Cue title restores.

### Windows
1. Open Cue → Settings → Stealth & Disguise section.
2. Select "Task Manager" from the dropdown.
3. ~~Verify the taskbar icon changes to the activity placeholder.~~ (deferred)
4. Check window title shows "Task Manager".
5. Switch to "Command Prompt" — verify ~~taskbar icon and~~ title update.
6. Switch to "None" — verify original Cue title restores.
