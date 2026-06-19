# Disguise Icons

Placeholder icons used when Bluey is running in disguise mode. Each mode
changes the app-facing identity to a familiar system utility.

**Status:** macOS and Windows tray icons are embedded in the dashboard binary
and swapped at runtime by `set_disguise`. The dashboard window title is updated
on both platforms. macOS also gets best-effort process title changes. Windows
gets AppUserModelID/taskbar identity changes, but the executable image name in
Task Manager remains the shipped binary name; Windows does not support safely
renaming that at runtime.

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

### macOS
1. Open Bluey -> Settings -> Disguise section.
2. Select "Activity Monitor" from the dropdown.
3. Verify the tray/menu-bar icon changes to the activity placeholder.
4. Check Activity Monitor label appears in the app menu bar.
5. Switch to "Terminal" and verify tray/menu-bar icon and menu bar update.
6. Switch to "None" and verify original Bluey title restores.

### Windows
1. Open Bluey -> Settings -> Disguise section.
2. Select "Task Manager" from the dropdown.
3. Verify the tray/taskbar icon changes to the activity placeholder.
4. Check the dashboard window title shows "Task Manager".
5. Switch to "Command Prompt" and verify tray/taskbar icon and title update.
6. Switch to "None" and verify original Bluey title restores.
