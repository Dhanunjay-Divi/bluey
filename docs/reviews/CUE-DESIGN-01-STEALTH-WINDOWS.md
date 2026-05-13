# bluey Design — Stealth + Windows + Screen Capture

**Synthesized from**: 7 deep-analysis docs (6 reference repos, 5,814 lines of analysis)
**Scope**: macOS + Windows + Linux stealth, content protection, overlay windows, screenshot capture

## Executive Summary

Bluey's stealth architecture must layer three independent protection mechanisms: (1) OS-level content protection that excludes windows from all capture APIs (Tauri's `.content_protected(true)` on macOS, raw `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` on Windows), (2) non-activating overlay via NSPanel on macOS that never steals focus from the user's active app, and (3) process-level masquerading that hides the app from dock/taskbar/Activity Monitor. The reference repos converge on this layered approach but diverge on implementation quality — natively-cluely has the most battle-tested masquerading (7-step process with re-assertion timers), pluely has the cleanest Tauri 2 + NSPanel integration, Aura has the most comprehensive Windows Win32 stealth (the "trifecta" of `WDA_EXCLUDEFROMCAPTURE` + `SW_SHOWNOACTIVATE` + `WS_EX_TRANSPARENT`), and Vysper adds window binding as a unique UX pattern. Bluey should combine pluely's Tauri-native APIs with Aura's Win32 depth and natively-cluely's masquerading discipline.

Key tradeoffs: (a) Tauri abstracts opacity/click-through/taskbar-hide but has NO abstraction for `SetWindowDisplayAffinity` or `SW_SHOWNOACTIVATE` — raw Win32 via `windows` crate is required; (b) NSPanel requires `macos-private-api` feature flag which disables App Store distribution; (c) `app.setName()` on macOS causes dock re-registration flicker — must be skipped during undetectable mode.

## Architecture Diagram (ASCII)

```
┌─────────────────────────────────────────────────────────────────────┐
│                        STEALTH LAYER STACK                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Layer 3: PROCESS MASQUERADE                                        │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ process.title ← "Terminal"                                   │   │
│  │ CFBundleName ← "Terminal"  (macOS)                          │   │
│  │ AppUserModelID ← "com.apple.Terminal" (Windows)             │   │
│  │ Dock/Taskbar icon ← fake terminal.png                       │   │
│  │ Window titles ← "Terminal — bash"                           │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  Layer 2: WINDOW STEALTH                                            │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ macOS: NSPanel(NonActivating) + FloatWindowLevel            │   │
│  │ Windows: SW_SHOWNOACTIVATE + WS_EX_TOOLWINDOW              │   │
│  │ Both: set_skip_taskbar(true) + set_always_on_top(true)      │   │
│  │ Dock: ActivationPolicy::Accessory (macOS)                   │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  Layer 1: CONTENT PROTECTION (capture exclusion)                    │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ macOS: .content_protected(true) → NSWindow.sharingType=none │   │
│  │ Windows: SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)   │   │
│  │ Linux: Best-effort (compositor-dependent)                   │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  Layer 0: INTERACTION MODES                                         │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ Normal: full interaction                                     │   │
│  │ Ghost: click-through (WS_EX_TRANSPARENT / ignore_cursor)    │   │
│  │ Opacity: 40% / 70% / 100% presets                           │   │
│  │ Hidden: setOpacity(0) → hide() (no fade flash)              │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
├─────────────────────────────────────────────────────────────────────┤
│  SCREEN CAPTURE (outbound — bluey capturing user's screen)         │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ Full: xcap::Monitor::all() → hide self → capture → restore  │   │
│  │ Selective: per-monitor overlay windows → canvas draw → crop  │   │
│  └─────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

## Feature Matrix: What Each Repo Does

| Feature | natively-cluely | pluely | solveWatchAi | Aura | Vysper | OpenCluely | **Bluey (proposed)** |
|---|---|---|---|---|---|---|---|
| Content protection | `setContentProtection(true)` on all 5 windows | `.content_protected(true)` builder | `setContentProtection(true)` + `alwaysOnTop:'screen-saver'` | `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` | `setContentProtection(true)` + `setVisibleOnAllWorkspaces` | Same as Vysper | **Tauri `.content_protected(true)` + raw Win32 `SetWindowDisplayAffinity` for Windows edge cases** |
| NSPanel overlay | N/A (Electron) | `tauri-nspanel` with `panel_delegate!` | N/A | N/A (Windows-only) | N/A | N/A | **`tauri-nspanel` pinned commit** |
| Click-through | `setIgnoreMouseEvents(true, {forward:true})` | `set_ignore_cursor_events(true)` | N/A | `WS_EX_TRANSPARENT` toggle | `setIgnoreMouseEvents(true, {forward:true})` | Same as Vysper | **Tauri `set_ignore_cursor_events` + raw Win32 fallback** |
| Dock/taskbar hide | `app.dock.hide()` + debounce 150ms | `ActivationPolicy::Accessory` + `set_skip_taskbar` | N/A | `WS_EX_TOOLWINDOW` + remove `WS_EX_APPWINDOW` | `setSkipTaskbar(true)` | Same as Vysper | **`ActivationPolicy::Accessory` (macOS) + `set_skip_taskbar` (Win/Linux)** |
| Process masquerade | 7-step with re-assertion timers | N/A | N/A | N/A | `process.title` + `app.setName` + dock icon | Same as Vysper | **7-step from natively-cluely, skip `setName` in undetectable** |
| Focus preservation | Capture focus state before dock.hide, restore with `window.focus()` | NSPanel inherently non-activating | N/A | `SW_SHOWNOACTIVATE` + `SWP_NOACTIVATE` | N/A | N/A | **NSPanel (macOS) + `SW_SHOWNOACTIVATE` (Windows)** |
| Opacity presets | Slider 0.35–1.0 with power curves | Same slider system | 0-100 via IPC | 40%/70%/100% via Alt+1/2/3 | N/A | N/A | **3 presets (40/70/100) + continuous slider** |
| Fade prevention | `setOpacity(0)` before `hide()` | N/A | N/A | N/A | N/A | N/A | **Port from natively-cluely** |
| Screenshot (full) | Hide all windows → 80ms wait → capture → restore | `xcap::Monitor::all()` in Rust | `desktopCapturer` | N/A | N/A | `desktopCapturer` → Gemini Vision | **xcap in Rust with window-ready events (no sleep)** |
| Screenshot (selective) | `CropperWindowHelper` fullscreen overlay | Per-monitor overlay windows + canvas | N/A | N/A | N/A | N/A | **pluely's multi-monitor overlay pattern** |
| Window movement | N/A | Hold-to-move 60fps (12px/16ms) | Drag via IPC | 20px increments via hotkey | 20px bound-window movement | Same as Vysper | **Hold-to-move 60fps with DPI-aware steps + bounds clamping** |
| Dynamic resize | `setOverlayDimensionsCentered()` | 54px↔600px via invoke | N/A | N/A | Content-driven: `lineCount*25+100` | Same as Vysper | **Content-aware + centered expansion** |
| Window binding | N/A | N/A | N/A | N/A | Vertical column (main+LLM, 10px gap) | Same as Vysper | **Vertical binding with configurable gap** |
| Screen-share detection | N/A | N/A | N/A | `EnumWindows` + 80+ title heuristics + class matching | `desktopCapturer.getSources` polling 5s | Same as Vysper | **`EnumWindows` heuristics (Win) + CGDisplayStream (macOS)** |
| Always-on-top enforcement | Standard | `NSFloatWindowLevel` (4) | `'screen-saver'` level | `HWND_TOPMOST` via `SetWindowPos` | Cascading levels + 3s re-enforcement | Same as Vysper | **NSFloatWindowLevel (macOS) + HWND_TOPMOST (Win) + periodic re-assert** |


---

## Design Section 1: macOS NSPanel Overlay

### What the Reference Repos Do

- **pluely**: Converts main window to NSPanel via `tauri-nspanel` crate (git, v2 branch). Sets `NSWindowStyleMaskNonActivatingPanel` (1<<7), `NSFloatWindowLevel` (4), collection behavior `FullScreenAuxiliary | CanJoinAllSpaces`. Uses `panel_delegate!` macro for key/resign callbacks. [Source: CUE-REF-02-PLUELY.md lines 167-173]
- **natively-cluely**: Electron — no NSPanel equivalent. Uses `alwaysOnTop: true` + `focusable: false` on overlay window, but this is inferior (still appears in Mission Control, can steal focus in edge cases). [Source: CUE-REF-01A-NATIVELY-BACKEND.md lines 83-86]
- **Aura**: Windows-only — uses `HWND_TOPMOST` + `SWP_NOACTIVATE` for equivalent behavior. [Source: CUE-REF-04-AURA.md lines 323-362]

### Tradeoffs

- NSPanel requires `tauri = { features = ["macos-private-api"] }` — disables Mac App Store distribution
- `tauri-nspanel` is from a git branch (not stable release) — pin to specific commit
- Panel doesn't appear in Mission Control or Cmd+Tab — users need a hotkey to find it
- `NSFloatWindowLevel` (4) is above normal windows but below screen-saver level — some fullscreen apps may cover it
- Deprecated `cocoa` APIs used internally — works but generates warnings

### Bluey Recommendation

```rust
// Cargo.toml
// [dependencies]
// tauri-nspanel = { git = "https://github.com/nicepkg/tauri-nspanel", branch = "v2" }

// src-tauri/src/panel.rs
use tauri_nspanel::cocoa::appkit::NSWindowCollectionBehavior;
use tauri_nspanel::{panel_delegate, WebviewWindowExt as PanelExt};

panel_delegate!(BlueyPanelDelegate {
    window_did_become_key,
    window_did_resign_key
});

pub fn init_panel(app: &tauri::AppHandle) -> tauri::Result<()> {
    let window = app.get_webview_window("main").unwrap();
    let panel = window.to_panel()?;

    let delegate = BlueyPanelDelegate::new();
    delegate.set_listener(Box::new(|event: String| {
        // Log panel focus events for debugging
        tracing::debug!("panel event: {event}");
    }));
    panel.set_delegate(delegate);

    // Non-activating + float level + all spaces
    panel.set_level(4); // NSFloatWindowLevel
    panel.set_collection_behaviour(
        NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces,
    );
    // NSWindowStyleMaskNonActivatingPanel = 1 << 7
    panel.set_style_mask(panel.style_mask() | (1 << 7));

    Ok(())
}
```

```json
// tauri.conf.json (partial)
{
  "tauri": {
    "macOSPrivateApi": true
  }
}
```

### Codex Task

- **B1.1** [S] NSPanel init wiring via `tauri-nspanel` + `panel_delegate!` setup in `src-tauri/src/panel.rs`

---

## Design Section 2: Windows Stealth Trifecta

### What the Reference Repos Do

- **Aura** (definitive source): Three Win32 APIs combined for full stealth [Source: CUE-REF-04-AURA.md lines 40-71]:
  1. `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE=0x11)` — window renders as black rect in ALL capture (OBS, Teams, Zoom, screenshots, BitBlt, PrintWindow)
  2. `ShowWindow(hwnd, SW_SHOWNOACTIVATE=4)` — show without stealing focus (proctoring software detects focus changes)
  3. `SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_TRANSPARENT=0x20)` — click-through ghost mode
  Plus: `WS_EX_TOOLWINDOW=0x80` hides from taskbar/Alt-Tab, `WS_EX_LAYERED=0x80000` enables per-pixel alpha

- **natively-cluely/pluely/solveWatchAi/Vysper**: All use Electron/Tauri's `setContentProtection(true)` which maps to `SetWindowDisplayAffinity` internally on Windows. But none use `SW_SHOWNOACTIVATE` or raw `WS_EX_TRANSPARENT`. [Source: CUE-REF-01A-NATIVELY-BACKEND.md line 317, CUE-REF-02-PLUELY.md line 276]

### Tradeoffs

- Tauri's `show()` always activates the window — no `SW_SHOWNOACTIVATE` flag exposed [Source: CUE-REF-04-AURA.md line 618]
- Tauri's `set_ignore_cursor_events(true)` maps to `WS_EX_TRANSPARENT` on Windows — use Tauri API when possible
- `SetWindowDisplayAffinity` has NO Tauri abstraction — must call via `windows` crate directly [Source: CUE-REF-04-AURA.md line 616]
- `WDA_EXCLUDEFROMCAPTURE` (0x11) is more comprehensive than older `WDA_MONITOR` (0x01) — use 0x11

### Bluey Recommendation

```rust
// Cargo.toml
// [target.'cfg(windows)'.dependencies]
// windows = { version = "0.58", features = [
//     "Win32_UI_WindowsAndMessaging",
//     "Win32_Foundation",
// ] }

// src-tauri/src/stealth_win.rs
#[cfg(target_os = "windows")]
mod win_stealth {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::*;

    const WDA_EXCLUDEFROMCAPTURE: u32 = 0x00000011;

    /// Apply capture protection — window appears as black rect in recordings
    pub unsafe fn apply_capture_protection(hwnd: HWND) -> windows::core::Result<()> {
        SetWindowDisplayAffinity(hwnd, WINDOW_DISPLAY_AFFINITY(WDA_EXCLUDEFROMCAPTURE))
    }

    /// Show window without stealing focus
    pub unsafe fn show_no_activate(hwnd: HWND) {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }

    /// Toggle click-through (ghost mode)
    pub unsafe fn set_ghost_mode(hwnd: HWND, enabled: bool) {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let new_style = if enabled {
            style | WS_EX_TRANSPARENT.0 as isize
        } else {
            style & !(WS_EX_TRANSPARENT.0 as isize)
        };
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style);
    }

    /// Hide from taskbar and Alt-Tab
    pub unsafe fn hide_from_taskbar(hwnd: HWND) {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let new_style = (style | WS_EX_TOOLWINDOW.0 as isize)
            & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style);
    }

    /// Move window without changing focus or z-order
    pub unsafe fn move_stealth(hwnd: HWND, dx: i32, dy: i32) -> windows::core::Result<()> {
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect)?;
        SetWindowPos(
            hwnd,
            HWND::default(),
            rect.left + dx,
            rect.top + dy,
            0, 0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        )
    }
}
```

### Codex Task

- **B1.3** [M] Windows stealth module: `SetWindowDisplayAffinity` + `ShowWindow(SW_SHOWNOACTIVATE)` + `WS_EX_TRANSPARENT` toggle via `windows` crate

---

## Design Section 3: Process Masquerading

### What the Reference Repos Do

- **natively-cluely** (most complete, 7-step): [Source: CUE-REFERENCE-ANALYSIS.md lines 12-26]
  1. `process.title = "Terminal "` — Activity Monitor display
  2. `app.setName("Terminal ")` — macOS Menu + Dock text (SKIP if undetectable mode)
  3. `process.env.CFBundleName = "Terminal"` — macOS bundle identity
  4. `app.setAppUserModelId("com.natively.assistant.terminal")` — Windows taskbar grouping (unique per disguise!)
  5. `nativeImage.createFromPath(iconPath)` + `app.dock.setIcon(image)` — fake dock icon
  6. `window.setIcon(image)` — Windows/Linux per-window icon
  7. `window.setTitle("Terminal")` — window title text
  - Re-assertion at 200ms/1s/5s (process.title drifts on some systems)
  - NEVER repeat `app.setName()` in re-assertion (causes second dock tile)
  - Pre-built icons in `assets/fakeicon/{mac,win}/{terminal,settings,activity}.png`

- **Vysper**: Simpler version — `process.title` + `app.setName` + dock icon swap. Multiple refresh attempts at 50/100/200/500ms. [Source: CUE-REF-05-VYSPER.md lines 257-259]

- **Aura**: No process masquerading (Windows-only, relies on `WS_EX_TOOLWINDOW` to hide from taskbar entirely)

### Tradeoffs

- `app.setName()` on macOS causes system re-registration → brief second dock tile [Source: CUE-REFERENCE-ANALYSIS.md line 23]
- `process.title` in Rust/Tauri requires platform-specific approach (no direct equivalent to Node's `process.title`)
- macOS can still expose real bundle identifier in some system UIs regardless of runtime changes [Source: CUE-REF-01A-NATIVELY-BACKEND.md line 353]
- Proper solution: build with different bundle ID for stealth mode (compile-time, not runtime)

### Bluey Recommendation

```rust
// src-tauri/src/masquerade.rs
use std::ffi::CString;

#[derive(Clone, serde::Deserialize)]
pub enum DisguiseMode {
    Terminal,
    SystemSettings,
    ActivityMonitor,
    None,
}

impl DisguiseMode {
    pub fn window_title(&self) -> &str {
        match self {
            Self::Terminal => "Terminal — bash",
            Self::SystemSettings => "System Settings",
            Self::ActivityMonitor => "Activity Monitor",
            Self::None => "bluey",
        }
    }

    pub fn icon_filename(&self) -> Option<&str> {
        match self {
            Self::Terminal => Some("terminal.png"),
            Self::SystemSettings => Some("settings.png"),
            Self::ActivityMonitor => Some("activity.png"),
            Self::None => None,
        }
    }

    #[cfg(target_os = "windows")]
    pub fn app_user_model_id(&self) -> &str {
        match self {
            Self::Terminal => "Microsoft.WindowsTerminal",
            Self::SystemSettings => "windows.immersivecontrolpanel",
            Self::ActivityMonitor => "Microsoft.Taskmgr",
            Self::None => "com.bluey.app",
        }
    }
}

#[tauri::command]
pub fn apply_disguise(
    app: tauri::AppHandle,
    mode: DisguiseMode,
) -> Result<(), String> {
    // Set window titles on all windows
    for (_, window) in app.webview_windows() {
        let _ = window.set_title(mode.window_title());
    }

    // Platform-specific process title
    #[cfg(unix)]
    set_process_title(mode.window_title());

    Ok(())
}

#[cfg(unix)]
fn set_process_title(title: &str) {
    // prctl on Linux, no reliable runtime equivalent on macOS
    #[cfg(target_os = "linux")]
    unsafe {
        let c_title = CString::new(title).unwrap();
        libc::prctl(libc::PR_SET_NAME, c_title.as_ptr(), 0, 0, 0);
    }
}
```

### Codex Task

- **B1.5** [M] Process masquerading module with 3 disguise presets + icon assets + re-assertion timer


---

## Design Section 4: Dock Hiding + Focus Preservation

### What the Reference Repos Do

- **natively-cluely** (most nuanced): [Source: CUE-REFERENCE-ANALYSIS.md lines 24-26]
  - Dock toggle debounced 150ms to prevent race with `dock.show()` + `NSApp.activate()`
  - **Critical**: Capture `nativelyWasFocused` BEFORE `dock.hide()` — dock.hide triggers macOS app-deactivation which leaks focus to next app (e.g., Chrome)
  - If was focused, restore with `window.focus()` NOT `app.focus()` (latter calls `[NSApp activateIgnoringOtherApps:YES]` which has side-effects)
  - `setIgnoreBlur(true)` on modal windows (Settings, ModelSelector) during stealth transitions — prevents self-hide, restore after 500ms

- **pluely**: `ActivationPolicy::Accessory` (macOS) or `set_skip_taskbar` (Windows/Linux). Clean but no focus preservation logic. [Source: CUE-REF-02-PLUELY.md lines 268-270]

- **Aura**: `WS_EX_TOOLWINDOW` + remove `WS_EX_APPWINDOW` — hides from taskbar AND Alt-Tab. [Source: CUE-REF-04-AURA.md lines 288-317]

### Tradeoffs

- macOS `ActivationPolicy::Accessory` removes menu bar entirely — can't access app via menu
- Dock hide on macOS triggers app-deactivation event → must save/restore focus state
- 150ms debounce needed because rapid dock toggle races with system animations
- On Windows, `set_skip_taskbar(true)` is sufficient (no focus side-effects)

### Bluey Recommendation

```rust
// src-tauri/src/dock.rs
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Manager;

static DOCK_VISIBLE: AtomicBool = AtomicBool::new(true);

#[tauri::command]
pub fn set_dock_visibility(app: tauri::AppHandle, visible: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy;
        if visible {
            app.set_activation_policy(ActivationPolicy::Regular);
            DOCK_VISIBLE.store(true, Ordering::SeqCst);
        } else {
            // Focus preservation: NSPanel doesn't need this since it's non-activating
            // But if we have modal windows open, protect them
            app.set_activation_policy(ActivationPolicy::Accessory);
            DOCK_VISIBLE.store(false, Ordering::SeqCst);
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Windows/Linux: just skip taskbar
        for (_, window) in app.webview_windows() {
            let _ = window.set_skip_taskbar(!visible);
        }
    }

    Ok(())
}
```

**Key insight from natively-cluely**: On macOS, the NSPanel approach (Section 1) inherently solves the focus problem — panels never become the active app, so hiding the dock icon doesn't trigger focus loss. This is why pluely's simpler approach works: NSPanel + Accessory policy = no focus issues.

### Codex Task

- **B1.6** [S] Dock/taskbar visibility toggle with `ActivationPolicy::Accessory` (macOS) + `set_skip_taskbar` (Win/Linux)

---

## Design Section 5: Opacity Management (Per-Window + Presets)

### What the Reference Repos Do

- **natively-cluely** (frontend-driven): Parametric appearance system in `overlayAppearance.ts` — opacity slider (0.35–1.0) drives backdrop-blur, surface alpha, border alpha via power curves. Single source of truth for all overlay surface styles. [Source: CUE-REF-01B-NATIVELY-FRONTEND.md lines 74-78]
  - **Fade prevention**: `setOpacity(0)` before `hide()` on macOS/Linux eliminates fade-animation flash. `setOpacity(1)` restore before every `show()` — windows were coming back invisible. [Source: CUE-REFERENCE-ANALYSIS.md lines 84-85]

- **Aura** (Windows-native): Three presets via `Alt+1/2/3` — 40% (transparent), 70% (semi), 100% (opaque). Uses `WS_EX_LAYERED` + `SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA)`. [Source: CUE-REF-04-AURA.md lines 554-562]

- **solveWatchAi**: Simple 0-100 opacity via IPC `hud-set-opacity`. [Source: CUE-REF-03-SOLVEWATCHAI.md line 181]

- **pluely**: Same slider system as natively-cluely (shared codebase heritage). [Source: CUE-REF-02-PLUELY.md line 42]

### Tradeoffs

- Tauri's `window.set_opacity()` works on Windows (wraps `SetLayeredWindowAttributes`) but behavior varies on macOS/Linux
- CSS-driven opacity (backdrop-filter + alpha) gives finer control than window-level opacity
- Window-level opacity affects the ENTIRE window including chrome — CSS opacity is content-only
- Fade prevention (`setOpacity(0)` pre-hide) is essential on macOS where `hide()` has a system animation

### Bluey Recommendation

```rust
// src-tauri/src/opacity.rs
#[derive(Clone, Copy, serde::Deserialize)]
pub enum OpacityPreset {
    Transparent, // 0.4
    Semi,        // 0.7
    Opaque,      // 1.0
}

impl OpacityPreset {
    fn value(self) -> f64 {
        match self {
            Self::Transparent => 0.4,
            Self::Semi => 0.7,
            Self::Opaque => 1.0,
        }
    }
}

#[tauri::command]
pub fn set_opacity_preset(window: tauri::WebviewWindow, preset: OpacityPreset) -> Result<(), String> {
    window.set_opacity(preset.value()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_opacity(window: tauri::WebviewWindow, value: f64) -> Result<(), String> {
    let clamped = value.clamp(0.35, 1.0);
    window.set_opacity(clamped).map_err(|e| e.to_string())
}

/// Hide window without fade flash (macOS/Linux)
#[tauri::command]
pub fn stealth_hide(window: tauri::WebviewWindow) -> Result<(), String> {
    // Set opacity to 0 first to prevent fade animation
    let _ = window.set_opacity(0.0);
    // Small delay for compositor to process
    std::thread::sleep(std::time::Duration::from_millis(16));
    window.hide().map_err(|e| e.to_string())
}

/// Show window with opacity restore
#[tauri::command]
pub fn stealth_show(window: tauri::WebviewWindow, opacity: f64) -> Result<(), String> {
    // Restore opacity BEFORE showing to prevent invisible window
    let _ = window.set_opacity(opacity);
    window.show().map_err(|e| e.to_string())
}
```

### Codex Task

- **B1.4** [S] `setOpacity(0)` fade-prevention wrapper + opacity preset system (40/70/100)

---

## Design Section 6: Click-Through / Mouse Passthrough

### What the Reference Repos Do

- **natively-cluely**: `setIgnoreMouseEvents(true, { forward: true })` — Electron API. The `forward: true` option means mouse events are forwarded to the window beneath (not just dropped). Toggle via global shortcut. [Source: CUE-REF-01A-NATIVELY-BACKEND.md line 83, CUE-REFERENCE-ANALYSIS.md line 82]
  - **Bug**: OS can silently drop Carbon/IOKit hotkey registrations when window focusability changes → must call `revalidateShortcuts()` after passthrough toggle [Source: CUE-REF-01A-NATIVELY-BACKEND.md line 290]

- **pluely**: Tauri's `set_ignore_cursor_events(true)` — native Tauri 2 API. [Source: CUE-REF-02-PLUELY.md line 268]

- **Aura**: Raw `WS_EX_TRANSPARENT` style flag toggle. Combined with `WS_EX_LAYERED` for transparency. [Source: CUE-REF-04-AURA.md lines 251-284]

- **Vysper**: `setIgnoreMouseEvents(true, { forward: true })` + dual-mode arrow keys (interactive vs non-interactive). Alt+A toggles. [Source: CUE-REF-05-VYSPER.md lines 247-252]

### Tradeoffs

- Tauri's `set_ignore_cursor_events(true)` is the cross-platform abstraction — use it
- On macOS, changing window focusability can drop global shortcut registrations (must re-register)
- Click-through + opacity < 50% = effectively invisible window that's hard to recover
- Need a guaranteed "escape hatch" shortcut that works regardless of passthrough state

### Bluey Recommendation

```rust
// src-tauri/src/interaction.rs
use std::sync::atomic::{AtomicBool, Ordering};

static GHOST_MODE: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn set_ghost_mode(window: tauri::WebviewWindow, enabled: bool) -> Result<(), String> {
    window
        .set_ignore_cursor_events(enabled)
        .map_err(|e| e.to_string())?;

    GHOST_MODE.store(enabled, Ordering::SeqCst);

    // Re-register shortcuts after focusability change (macOS bug)
    // This is handled by the shortcut module listening to this state
    window.emit("ghost-mode-changed", enabled).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn toggle_ghost_mode(window: tauri::WebviewWindow) -> Result<bool, String> {
    let new_state = !GHOST_MODE.load(Ordering::SeqCst);
    set_ghost_mode(window, new_state)?;
    Ok(new_state)
}
```

### Codex Task

- **B1.7** [S] Click-through toggle via `set_ignore_cursor_events` + shortcut re-registration on state change


---

## Design Section 7: Full-Screen Screenshot

### What the Reference Repos Do

- **natively-cluely**: `withScreenshotCaptureSession` pattern — records window visibility state, hides ALL app windows (main, settings, model selector), waits 80ms for compositor flush, captures screen, restores all windows to previous state. Mutex prevents concurrent captures. [Source: CUE-REF-01A-NATIVELY-BACKEND.md lines 214-219]
  - 80ms delay is macOS-tuned magic number; Windows might need different timing

- **pluely**: `xcap::Monitor::all()` captures all monitors in Rust. No hide/restore needed because content-protected windows are already excluded from capture. Uses `spawn_blocking` since xcap is synchronous. [Source: CUE-REF-02-PLUELY.md lines 228-231]

- **solveWatchAi**: Electron `desktopCapturer` API. [Source: CUE-REF-03-SOLVEWATCHAI.md line 40]

- **OpenCluely**: `desktopCapturer` → raw PNG buffer → Gemini Vision `inlineData` (no OCR). [Source: CUE-REF-06-OPENCLUELY.md line 40]

### Tradeoffs

- Content-protected windows are ALREADY excluded from `xcap` capture — no need to hide/restore
- `xcap` is synchronous (blocks thread) — must use `spawn_blocking` or dedicated thread
- Multi-monitor: must capture each monitor separately and stitch or let user choose
- macOS requires Screen Recording permission (TCC) — zero-filled buffer if denied

### Bluey Recommendation

```rust
// src-tauri/src/capture.rs
use xcap::Monitor;
use image::DynamicImage;
use base64::{engine::general_purpose::STANDARD, Engine};

#[tauri::command]
pub async fn capture_full_screen(monitor_index: Option<usize>) -> Result<String, String> {
    // Content-protected windows are automatically excluded from capture
    let image = tauri::async_runtime::spawn_blocking(move || {
        let monitors = Monitor::all().map_err(|e| e.to_string())?;
        let monitor = match monitor_index {
            Some(idx) => monitors.into_iter().nth(idx).ok_or("Invalid monitor index")?,
            None => monitors.into_iter().next().ok_or("No monitors found")?,
        };
        monitor.capture_image().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    // Encode to PNG base64
    let mut buf = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;

    Ok(STANDARD.encode(&buf))
}
```

### Codex Task

- **B1.8** [S] Full-screen capture via `xcap` in `spawn_blocking` with base64 PNG output

---

## Design Section 8: Selective Screenshot with Multi-Monitor Overlay

### What the Reference Repos Do

- **pluely** (best implementation): [Source: CUE-REF-02-PLUELY.md lines 228-231, CUE-REFERENCE-ANALYSIS.md lines 167]
  1. `start_screen_capture` captures all monitors via `xcap::Monitor::all()`
  2. Creates transparent overlay window per monitor: `capture-overlay-{idx}` — transparent, always-on-top, no decorations, no taskbar, positioned at monitor coords
  3. Physical pixels from xcap → logical units via `scale_factor` for window placement
  4. Primary monitor gets `set_focus()` + `request_user_attention(Critical)`
  5. `accept_first_mouse(true)` — first click works without needing focus
  6. User draws selection on canvas overlay → `capture_selected_area` crops and returns base64 PNG
  7. Stale overlay cleanup: iterate `app.webview_windows()`, destroy labels starting with `capture-overlay-`

- **natively-cluely**: `CropperWindowHelper` — single fullscreen transparent overlay, user draws rectangle, returns bounds. Main process captures only that region. [Source: CUE-REF-01A-NATIVELY-BACKEND.md lines 472-475]

### Tradeoffs

- pluely's multi-monitor approach is superior but uses same `index.html` entry point (wasteful)
- 100ms `thread::sleep` for window settle is a race condition workaround — should use window-ready events
- No crosshair cursor in pluely's implementation
- Overlay windows must be cleaned up on cancel/complete (stale overlay bug)

### Bluey Recommendation

```rust
// src-tauri/src/capture.rs (continued)
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{Manager, WebviewWindowBuilder, WebviewUrl};

pub struct CaptureState {
    pub monitors: Arc<Mutex<HashMap<usize, MonitorCapture>>>,
    pub active: Arc<std::sync::atomic::AtomicBool>,
}

struct MonitorCapture {
    image: image::RgbaImage,
    scale_factor: f64,
}

#[tauri::command]
pub async fn start_screen_capture(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<CaptureState>();

    // Cleanup stale overlays
    let windows: Vec<_> = app.webview_windows().keys()
        .filter(|k| k.starts_with("capture-overlay-"))
        .cloned()
        .collect();
    for label in windows {
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.destroy();
        }
    }

    // Capture all monitors
    let monitors = tauri::async_runtime::spawn_blocking(|| {
        Monitor::all().map_err(|e| e.to_string())
    }).await.map_err(|e| e.to_string())??;

    let mut captures = HashMap::new();

    for (idx, monitor) in monitors.iter().enumerate() {
        let image = tauri::async_runtime::spawn_blocking({
            let monitor = monitor.clone();
            move || monitor.capture_image().map_err(|e| e.to_string())
        }).await.map_err(|e| e.to_string())??;

        let scale = monitor.scale_factor();
        captures.insert(idx, MonitorCapture {
            image: image.into_rgba8(),
            scale_factor: scale,
        });

        // Create overlay window for this monitor
        let x = monitor.x() as f64;
        let y = monitor.y() as f64;
        let w = monitor.width() as f64 / scale;
        let h = monitor.height() as f64 / scale;

        let label = format!("capture-overlay-{idx}");
        let overlay = WebviewWindowBuilder::new(
            &app,
            &label,
            WebviewUrl::App("/capture-overlay".into()),
        )
        .title("")
        .position(x, y)
        .inner_size(w, h)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .accept_first_mouse(true)
        .build()
        .map_err(|e| e.to_string())?;

        if idx == 0 {
            let _ = overlay.set_focus();
        }
    }

    *state.monitors.lock().unwrap() = captures;
    state.active.store(true, std::sync::atomic::Ordering::SeqCst);

    Ok(())
}

#[tauri::command]
pub fn capture_selected_area(
    app: tauri::AppHandle,
    monitor_index: usize,
    x: u32, y: u32, width: u32, height: u32,
) -> Result<String, String> {
    let state = app.state::<CaptureState>();
    let monitors = state.monitors.lock().unwrap();
    let capture = monitors.get(&monitor_index).ok_or("Monitor not captured")?;

    // Scale selection coords to physical pixels
    let scale = capture.scale_factor;
    let px = (x as f64 * scale) as u32;
    let py = (y as f64 * scale) as u32;
    let pw = (width as f64 * scale) as u32;
    let ph = (height as f64 * scale) as u32;

    let cropped = image::imageops::crop_imm(&capture.image, px, py, pw, ph).to_image();

    let mut buf = Vec::new();
    image::DynamicImage::ImageRgba8(cropped)
        .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;

    // Cleanup overlays
    drop(monitors);
    cleanup_capture_overlays(&app);

    Ok(STANDARD.encode(&buf))
}

fn cleanup_capture_overlays(app: &tauri::AppHandle) {
    let windows: Vec<_> = app.webview_windows().keys()
        .filter(|k| k.starts_with("capture-overlay-"))
        .cloned()
        .collect();
    for label in windows {
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.destroy();
        }
    }
}
```

### Codex Task

- **B1.9** [L] Multi-monitor selective screenshot: per-monitor overlay windows + canvas selection + crop + cleanup

---

## Design Section 9: Keyboard-Driven Window Movement (Hold-to-Move)

### What the Reference Repos Do

- **pluely** (best — 60fps smooth): On `ShortcutState::Pressed` for `move_window_*`, spawns a tokio task that moves window 12px every 16ms (60fps). On `ShortcutState::Released`, sets `Arc<AtomicBool>` stop flag. Uses `MoveWindowState` with `HashMap<String, Arc<AtomicBool>>`. Multiple directions can be active simultaneously. [Source: CUE-REF-02-PLUELY.md lines 218-226]
  - **Weakness**: 12px step is hardcoded (not DPI-aware), no acceleration curve, no bounds checking

- **Aura**: 20px increment per keypress (not held). Uses `SetWindowPos` with `SWP_NOACTIVATE | SWP_NOZORDER`. [Source: CUE-REF-04-AURA.md lines 323-362, CUE-REFERENCE-ANALYSIS.md line 474]

- **Vysper**: 20px bound-window movement via Cmd+arrows in non-interactive mode. Bounds-checked against screen edges. [Source: CUE-REF-05-VYSPER.md lines 280, 307-335]

### Tradeoffs

- pluely's 60fps approach is smoothest but wastes CPU when window is at screen edge
- Aura's per-press approach is simpler but jerky
- Bounds checking is essential — window moving off-screen is unrecoverable without shortcut
- DPI-aware step: 12px on 1x = 12px on 2x Retina, which is only 6 logical pixels — too small

### Bluey Recommendation

```rust
// src-tauri/src/movement.rs
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;

pub struct MoveWindowState {
    pub active_moves: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

const MOVE_INTERVAL_MS: u64 = 16; // ~60fps
const BASE_STEP_PX: f64 = 16.0;   // logical pixels per tick

#[tauri::command]
pub fn start_window_move(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    direction: String, // "up" | "down" | "left" | "right"
) -> Result<(), String> {
    let state = app.state::<MoveWindowState>();
    let stop_flag = Arc::new(AtomicBool::new(false));

    // Store stop flag for this direction
    state.active_moves.lock().unwrap().insert(direction.clone(), stop_flag.clone());

    let scale = window.scale_factor().unwrap_or(1.0);
    let step = BASE_STEP_PX; // Already in logical pixels

    tauri::async_runtime::spawn(async move {
        loop {
            if stop_flag.load(Ordering::SeqCst) {
                break;
            }

            if let Ok(pos) = window.outer_position() {
                let (dx, dy): (f64, f64) = match direction.as_str() {
                    "up" => (0.0, -step),
                    "down" => (0.0, step),
                    "left" => (-step, 0.0),
                    "right" => (step, 0.0),
                    _ => break,
                };

                let new_x = pos.x as f64 + dx;
                let new_y = pos.y as f64 + dy;

                // Bounds check against current monitor
                if let Some(monitor) = window.current_monitor().ok().flatten() {
                    let mp = monitor.position();
                    let ms = monitor.size();
                    let clamped_x = new_x.clamp(
                        mp.x as f64 - 100.0, // allow partial off-screen
                        mp.x as f64 + ms.width as f64 / scale - 50.0,
                    );
                    let clamped_y = new_y.clamp(
                        mp.y as f64,
                        mp.y as f64 + ms.height as f64 / scale - 50.0,
                    );
                    let _ = window.set_position(tauri::LogicalPosition::new(clamped_x, clamped_y));
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(MOVE_INTERVAL_MS)).await;
        }
    });

    Ok(())
}

#[tauri::command]
pub fn stop_window_move(app: tauri::AppHandle, direction: String) -> Result<(), String> {
    let state = app.state::<MoveWindowState>();
    if let Some(flag) = state.active_moves.lock().unwrap().remove(&direction) {
        flag.store(true, Ordering::SeqCst);
    }
    Ok(())
}
```

### Codex Task

- **B1.10** [M] Hold-to-move window at 60fps with DPI-aware steps + screen bounds clamping


---

## Design Section 10: Dynamic Content-Aware Window Resize

### What the Reference Repos Do

- **pluely**: Main window starts at 54px (just input bar). Resizes to 600px via `invoke("set_window_height", { height })` when AI response opens. Frontend uses MutationObserver on Radix popover state to auto-collapse. [Source: CUE-REF-02-PLUELY.md lines 175-177]

- **natively-cluely**: `setOverlayDimensionsCentered()` — computes X offset to keep content's horizontal center fixed during width changes. Formula: `desiredX = currentBounds.x - Math.floor(widthDelta / 2)`. Prevents visual "jumping" during code-expansion animations (600↔780px). [Source: CUE-REF-01A-NATIVELY-BACKEND.md lines 840-841]

- **Vysper**: Content-driven resize — renderer measures `{ lineCount, avgLineLength }`, sends metrics via IPC. Main process calculates: `width = clamp(avgLineLength*8, 500, screenWidth*0.8)`, `height = clamp(lineCount*25+100, 300, screenHeight*0.8)`. Default 840×480. [Source: CUE-REF-05-VYSPER.md lines 339-358]

### Tradeoffs

- Fixed max height (pluely's 600px) wastes space on large monitors
- Content-aware (Vysper) is more adaptive but requires renderer→backend measurement IPC
- Centered expansion (natively-cluely) prevents visual jumping — essential for overlay UX
- MutationObserver on body is expensive — prefer explicit resize triggers

### Bluey Recommendation

```rust
// src-tauri/src/window.rs
#[derive(serde::Deserialize)]
pub struct ContentMetrics {
    line_count: u32,
    avg_line_length: u32,
    has_code_block: bool,
}

#[tauri::command]
pub fn resize_for_content(
    window: tauri::WebviewWindow,
    metrics: ContentMetrics,
) -> Result<(), String> {
    let scale = window.scale_factor().unwrap_or(1.0);
    let monitor = window.current_monitor().ok().flatten()
        .ok_or("No monitor")?;
    let screen_w = monitor.size().width as f64 / scale;
    let screen_h = monitor.size().height as f64 / scale;

    // Calculate optimal size
    let char_width = if metrics.has_code_block { 8.5 } else { 8.0 };
    let target_w = (metrics.avg_line_length as f64 * char_width).clamp(400.0, screen_w * 0.8);
    let target_h = (metrics.line_count as f64 * 24.0 + 80.0).clamp(54.0, screen_h * 0.8);

    // Centered expansion: adjust X to keep center fixed
    if let Ok(pos) = window.outer_position() {
        if let Ok(size) = window.outer_size() {
            let current_w = size.width as f64 / scale;
            let width_delta = target_w - current_w;
            let new_x = pos.x as f64 - (width_delta / 2.0);
            let _ = window.set_position(tauri::LogicalPosition::new(new_x, pos.y as f64));
        }
    }

    window
        .set_size(tauri::LogicalSize::new(target_w, target_h))
        .map_err(|e| e.to_string())
}

/// Collapse to minimal input-bar height
#[tauri::command]
pub fn collapse_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window
        .set_size(tauri::LogicalSize::new(600.0, 54.0))
        .map_err(|e| e.to_string())
}
```

### Codex Task

- **B1.11** [M] Content-aware window resize with centered expansion + collapse to 54px

---

## Design Section 11: Window Binding (Multi-Window Group)

### What the Reference Repos Do

- **Vysper** (unique feature): Main toolbar (520×35) + LLM response window (840×480) move as a unit. Vertical column layout with configurable gap (default 10px). [Source: CUE-REF-05-VYSPER.md lines 242-244, 307-335]
  - `positionBoundWindows()`: Get display workArea → calculate maxWidth → center horizontally → main at top with 20px margin → LLM below with gap
  - `moveBoundWindows(dx, dy)`: Move both by same delta, enforce bounds (min Y = displayY+20, max Y = displayY+screenHeight-totalHeight)
  - Triggers: binding enabled, LLM response shown, screen change, arrow keys in non-interactive mode
  - State: `bindWindows` (bool), `windowGap` (number), `boundWindowsPosition` ({x, y})

- **OpenCluely**: Same implementation (fork of Vysper). [Source: CUE-REF-06-OPENCLUELY.md lines 191-193]

### Tradeoffs

- Window binding adds complexity but is essential for multi-window overlay UX
- Must re-position on screen change (monitor connect/disconnect)
- Bound movement must respect screen bounds for BOTH windows
- Gap should be configurable (some users want tight, some want spaced)

### Bluey Recommendation

```rust
// src-tauri/src/binding.rs
use tauri::Manager;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct BindingState {
    pub enabled: Arc<Mutex<bool>>,
    pub gap: Arc<Mutex<f64>>,
    pub bound_labels: Arc<Mutex<Vec<String>>>, // ordered top-to-bottom
}

#[tauri::command]
pub fn set_window_binding(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let state = app.state::<BindingState>();
    *state.enabled.lock().unwrap() = enabled;
    if enabled {
        reposition_bound_windows(&app)?;
    }
    Ok(())
}

#[tauri::command]
pub fn move_bound_windows(app: tauri::AppHandle, dx: f64, dy: f64) -> Result<(), String> {
    let state = app.state::<BindingState>();
    if !*state.enabled.lock().unwrap() {
        return Ok(());
    }

    let labels = state.bound_labels.lock().unwrap().clone();
    let scale = app.get_webview_window(labels.first().unwrap())
        .and_then(|w| w.scale_factor().ok())
        .unwrap_or(1.0);

    for label in &labels {
        if let Some(window) = app.get_webview_window(label) {
            if let Ok(pos) = window.outer_position() {
                let new_x = pos.x as f64 + dx;
                let new_y = pos.y as f64 + dy;
                let _ = window.set_position(tauri::LogicalPosition::new(new_x, new_y));
            }
        }
    }
    Ok(())
}

fn reposition_bound_windows(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<BindingState>();
    let gap = *state.gap.lock().unwrap();
    let labels = state.bound_labels.lock().unwrap().clone();

    if labels.is_empty() { return Ok(()); }

    let first = app.get_webview_window(&labels[0]).ok_or("Window not found")?;
    let monitor = first.current_monitor().ok().flatten().ok_or("No monitor")?;
    let scale = first.scale_factor().unwrap_or(1.0);
    let screen_w = monitor.size().width as f64 / scale;
    let screen_y = monitor.position().y as f64;

    // Find max width across all bound windows
    let mut max_w: f64 = 0.0;
    let mut total_h: f64 = 0.0;
    let mut heights = Vec::new();
    for label in &labels {
        if let Some(w) = app.get_webview_window(label) {
            let size = w.outer_size().map_err(|e| e.to_string())?;
            let w_logical = size.width as f64 / scale;
            let h_logical = size.height as f64 / scale;
            max_w = max_w.max(w_logical);
            heights.push(h_logical);
            total_h += h_logical;
        }
    }
    total_h += gap * (labels.len() as f64 - 1.0);

    // Center horizontally, top with 20px margin
    let x = (screen_w - max_w) / 2.0;
    let mut y = screen_y + 20.0;

    for (i, label) in labels.iter().enumerate() {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.set_position(tauri::LogicalPosition::new(x, y));
            y += heights[i] + gap;
        }
    }

    Ok(())
}
```

### Codex Task

- **B1.12** [M] Window binding system: vertical column layout with coordinated movement + bounds checking

---

## Design Section 12: Screen-Share Detection (Auto-Adjust Behavior)

### What the Reference Repos Do

- **Aura** (most comprehensive, Windows): `find_screen_share_indicators()` — `EnumWindows` callback scans all top-level windows. Two-pass detection: [Source: CUE-REF-04-AURA.md lines 366-436, 568-588]
  1. Title matching: case-insensitive substring against 80+ indicator strings (Zoom, Teams, OBS, Chrome, Firefox, TeamViewer, AnyDesk, GameBar)
  2. Class name matching with secondary verification: `ZPContentViewWndClass` (Zoom), `TeamsWebView`, `Qt5QWindowIcon` (OBS), `Chrome_WidgetWin_1`, `GameBarDisplayCaptureIndicator`
  3. Visibility gate: only `IsWindowVisible()` = true
  - `hide_screen_share_indicator(hwnd)` — hides the indicator itself (disables red "you're sharing" border)
  - `start_screen_share_monitor()` — background thread polls every 1s, hides indicators reactively
  - State: `screen_share_monitor_active: bool`, `hidden_screen_share_windows: set`

- **Vysper**: Polls `desktopCapturer.getSources` every 5s. When sharing detected, hides all windows and moves to (-10000, -10000). Restores on sharing end. [Source: CUE-REF-05-VYSPER.md lines 273-280]

### Tradeoffs

- Aura's approach is Windows-only (EnumWindows)
- macOS has no direct equivalent — could use `CGDisplayStreamCreate` or `SCStream` callbacks
- Polling every 1s is acceptable for detection latency
- Hiding the share indicator itself is aggressive — may confuse users who WANT to know they're sharing
- Better approach for bluey: detect sharing → auto-enable content protection + show user notification

### Bluey Recommendation

```rust
// src-tauri/src/screen_share_detect.rs
#[cfg(target_os = "windows")]
mod win_detect {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::*;
    use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

    const INDICATORS: &[&str] = &[
        "screen sharing indicator",
        "recording in progress",
        "you're sharing your screen",
        "zoom is sharing",
        "microsoft teams is sharing",
        "obs studio",
        "is sharing your screen",
        "chrome is sharing",
        "firefox is sharing",
    ];

    pub struct ScreenShareMonitor {
        running: Arc<AtomicBool>,
    }

    impl ScreenShareMonitor {
        pub fn new() -> Self {
            Self { running: Arc::new(AtomicBool::new(false)) }
        }

        pub fn start(&self, on_detected: impl Fn(bool) + Send + 'static) {
            let running = self.running.clone();
            running.store(true, Ordering::SeqCst);

            std::thread::spawn(move || {
                let mut was_sharing = false;
                while running.load(Ordering::SeqCst) {
                    let is_sharing = detect_screen_share();
                    if is_sharing != was_sharing {
                        on_detected(is_sharing);
                        was_sharing = is_sharing;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            });
        }

        pub fn stop(&self) {
            self.running.store(false, Ordering::SeqCst);
        }
    }

    fn detect_screen_share() -> bool {
        let found = Arc::new(AtomicBool::new(false));
        let found_clone = found.clone();

        unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let found = &*(lparam.0 as *const AtomicBool);

            if !IsWindowVisible(hwnd).as_bool() {
                return BOOL(1);
            }

            let mut title_buf = [0u16; 512];
            let len = GetWindowTextW(hwnd, &mut title_buf);
            if len == 0 { return BOOL(1); }

            let title = String::from_utf16_lossy(&title_buf[..len as usize]).to_lowercase();

            if INDICATORS.iter().any(|ind| title.contains(ind)) {
                found.store(true, Ordering::SeqCst);
                return BOOL(0); // stop enumeration
            }
            BOOL(1)
        }

        unsafe {
            let _ = EnumWindows(
                Some(callback),
                LPARAM(Arc::as_ptr(&found_clone) as isize),
            );
        }

        found.load(Ordering::SeqCst)
    }
}

#[tauri::command]
pub fn start_screen_share_monitor(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let app_clone = app.clone();
        let monitor = win_detect::ScreenShareMonitor::new();
        monitor.start(move |is_sharing| {
            let _ = app_clone.emit("screen-share-detected", is_sharing);
        });
        // Store monitor in app state for cleanup
    }
    Ok(())
}
```

### Codex Task

- **B1.13** [M] Screen-share detection via `EnumWindows` heuristics (Windows) + event emission for auto-behavior

---

## Design Section 2b: Content Protection (Tauri Native)

### What the Reference Repos Do

All 6 repos implement content protection. The cleanest is pluely's Tauri-native approach:

- **pluely**: `.content_protected(true)` in `WebviewWindowBuilder` — zero platform code. [Source: CUE-REF-02-PLUELY.md lines 274-279, CUE-REFERENCE-ANALYSIS.md line 359]
- **natively-cluely**: `win.setContentProtection(true)` on ALL 5 windows (launcher, overlay, settings, modelSelector, cropper). [Source: CUE-REFERENCE-ANALYSIS.md line 12]
- **solveWatchAi**: `setContentProtection(true)` + `alwaysOnTop: 'screen-saver'` — confirmed working on Zoom, Meet, Teams, Loom, OBS. [Source: CUE-REF-03-SOLVEWATCHAI.md lines 571-576, 813]
- **Aura**: Raw `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` for Windows. [Source: CUE-REF-04-AURA.md lines 129-163]

### Bluey Recommendation

```json
// tauri.conf.json — apply to ALL windows at build time
{
  "app": {
    "windows": [
      {
        "label": "main",
        "contentProtected": true,
        "alwaysOnTop": true
      },
      {
        "label": "dashboard",
        "contentProtected": true
      }
    ]
  }
}
```

```rust
// For dynamically created windows:
let window = WebviewWindowBuilder::new(app, "settings", url)
    .content_protected(true)
    .build()?;

// For Windows edge cases where Tauri's abstraction isn't sufficient:
#[cfg(target_os = "windows")]
{
    let hwnd = window.hwnd().unwrap();
    unsafe { win_stealth::apply_capture_protection(hwnd)?; }
}
```

**Note**: Content protection doesn't work in dev mode on macOS (Electron binary isn't the app bundle). Tauri has the same limitation — test in release builds. [Source: CUE-REF-01A-NATIVELY-BACKEND.md line 318]

### Codex Task

- **B1.2** [S] Content protection via `.content_protected(true)` on all windows + Win32 fallback

---

## Summary: Codex Task List for This Domain

| ID | Size | Task | Source Pattern |
|---|---|---|---|
| **B1.1** | S | NSPanel macOS overlay via `tauri-nspanel` + `panel_delegate!` | pluely lib.rs:L165-210 |
| **B1.2** | S | Content protection via `.content_protected(true)` on all windows | pluely tauri.conf.json, all repos |
| **B1.3** | M | Windows stealth: `SetWindowDisplayAffinity` + `SW_SHOWNOACTIVATE` + `WS_EX_TRANSPARENT` | Aura window_manager.py |
| **B1.4** | S | `setOpacity(0)` fade-prevention + opacity preset system (40/70/100) | natively-cluely FIXES.md #89, Aura |
| **B1.5** | M | Process masquerading: 3 disguise presets + icon assets + re-assertion timer | natively-cluely main.ts:3390-3540 |
| **B1.6** | S | Dock/taskbar visibility: `ActivationPolicy::Accessory` + `set_skip_taskbar` | pluely shortcuts.rs:L470-510 |
| **B1.7** | S | Click-through toggle via `set_ignore_cursor_events` + shortcut re-registration | pluely, natively-cluely |
| **B1.8** | S | Full-screen capture via `xcap` in `spawn_blocking` | pluely capture.rs |
| **B1.9** | L | Multi-monitor selective screenshot: overlay windows + canvas + crop | pluely capture.rs:L45-160 |
| **B1.10** | M | Hold-to-move window at 60fps with bounds clamping | pluely shortcuts.rs:L130-175 |
| **B1.11** | M | Content-aware window resize with centered expansion | natively-cluely + Vysper |
| **B1.12** | M | Window binding: vertical column with coordinated movement | Vysper window.manager.js |
| **B1.13** | M | Screen-share detection via `EnumWindows` heuristics (Windows) | Aura window_manager.py |
| **B1.14** | S | Custom cursor hiding for stealth (CSS `cursor: none`) | pluely app.context.tsx |
| **B1.15** | S | Always-on-top enforcement with periodic re-assertion (3s) | Vysper window.manager.js |

**Size key**: S = < 4 hours, M = 4-12 hours, L = 12-24 hours

---

## Anti-Patterns / Don't Port

1. **Process title manipulation as primary stealth** — macOS exposes real bundle identifier regardless. Use proper bundle ID at build time for true stealth. [Source: CUE-REF-01A-NATIVELY-BACKEND.md line 353]
2. **80ms magic sleep for compositor flush** — use window-ready events or `requestAnimationFrame` callbacks instead. [Source: CUE-REF-01A-NATIVELY-BACKEND.md line 219]
3. **100ms `thread::sleep` for overlay window settle** — same issue as above, race condition workaround. [Source: CUE-REF-02-PLUELY.md line 230]
4. **Repeated `app.setName()` calls** — causes dock re-registration flicker on macOS. Set once, never re-assert. [Source: CUE-REFERENCE-ANALYSIS.md line 23]
5. **Hiding screen-share indicators** (Aura's `hide_screen_share_indicator`) — too aggressive, may confuse users. Detect and notify instead. [Source: CUE-REF-04-AURA.md line 114]
6. **`desktopCapturer.getSources` polling** (Vysper) — expensive, Electron-specific. Use platform APIs. [Source: CUE-REF-05-VYSPER.md line 273]
7. **Single `index.html` for capture overlays** — wasteful, loads full React app. Use dedicated lightweight HTML. [Source: CUE-REF-02-PLUELY.md line 230]

---

## Open Questions for the User

1. **Bundle ID strategy**: Should bluey ship with a single bundle ID or support compile-time alternate IDs for true stealth? (Runtime masquerading is always detectable by sophisticated tools.)

2. **Screen-share detection scope**: Should bluey only detect sharing (and notify/auto-protect), or should it also attempt to hide share indicators like Aura does?

3. **Linux stealth**: Linux compositor support for content protection is inconsistent (Wayland vs X11). Should we invest in Linux stealth or mark it as best-effort?

4. **NSPanel vs standard window**: NSPanel requires `macos-private-api` which blocks Mac App Store. Is App Store distribution a goal? If so, we need a fallback non-NSPanel mode.

5. **Opacity presets**: Aura uses Alt+1/2/3 for 40/70/100%. Should bluey use the same keys or integrate with the existing shortcut system? Should there be a continuous slider in addition to presets?

6. **Window binding default**: Should the main overlay + response panel be bound by default (Vysper's approach) or independent by default (pluely's approach)?

7. **Stealth activation UX**: Aura has a single `Alt+Shift+S` that enables ALL stealth features at once. Should bluey have a similar "panic button" or require individual toggles?
