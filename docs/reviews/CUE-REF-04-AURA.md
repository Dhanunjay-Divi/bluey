# DEEP ANALYSIS — Aura-AI (Python AI assistant, Win32 window management reference)

**Scope**: ~17K LOC Python desktop AI assistant with Win32 window management via ctypes
**Repository**: `Aura-AI-master/` — pywebview + FastAPI + Win32 stealth overlay
**Why this matters**: Best concrete reference for Python ctypes → Rust `windows` crate port of content-protection APIs

---

## File Manifest

| File | Size | Role |
|------|------|------|
| `window_manager.py` | ~55KB, 1300 lines | Win32 API surface: capture protection, transparency, hotkeys, window enumeration |
| `main.py` | ~22KB | Entry point: FastAPI server, pywebview window, asyncio thread orchestration |
| `api/websocket.py` | ~2KB | WebSocket endpoint for real-time client↔server messaging |
| `api/config_api.py` | ~4KB | REST endpoints for config, transparency, provider management |
| `api/session_manager.py` | ~4KB | Interview session lifecycle, STT/LLM manager initialization |
| `api/utils.py` | ~0.5KB | WebSocket JSON send helper (orjson) |
| `core/config.py` | ~2KB | Pydantic settings from `.env` |
| `core/env_utils.py` | ~2KB | `.env` file read/write utility |
| `core/prompts.py` | ~15KB | AI system prompts for interview coaching |
| `services/llm_service.py` | ~12KB | Multi-provider LLM with key rotation + failover |
| `services/stt_service.py` | ~8KB | Deepgram real-time speech-to-text |
| `services/vision_service.py` | ~10KB | Screenshot analysis with vision models |
| `services/context_manager.py` | ~4KB | Persistent candidate context + conversation history |
| `web/` | 24 files | Vanilla HTML/CSS/JS frontend with WebSocket streaming |
| `ai_providers.example.json` | ~3KB | Multi-provider config schema (Groq, Cerebras, Gemini, OpenRouter) |
| `run.bat` | ~2KB | Windows launcher (venv + deps + launch) |
| `silent_run.vbs` | 2 lines | VBScript silent launcher (hides console window) |
| `requirements.txt` | 12 deps | pywebview, pywin32, fastapi, uvicorn, openai, deepgram-sdk, pynput, etc. |

---

## Win32 API Surface + Constants (Reference Tables)

### Table 1: Win32 Functions Used (Python → Rust Mapping)

| # | Python ctypes call | Rust `windows` crate equivalent | MS Docs | Purpose in Aura |
|---|---|---|---|---|
| 1 | `_user32.SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)` | `use windows::Win32::UI::WindowsAndMessaging::SetWindowDisplayAffinity;`<br>`unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) }` | [SetWindowDisplayAffinity](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity) | Exclude window from screen capture — renders as black rect in recordings |
| 2 | `_user32.FindWindowW(None, "Aura")` | `use windows::Win32::UI::WindowsAndMessaging::FindWindowW;`<br>`unsafe { FindWindowW(None, w!("Aura")) }` | [FindWindowW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-findwindoww) | Locate window by title as fallback for obtaining HWND |
| 3 | `_user32.ShowWindow(hwnd, SW_HIDE)` | `use windows::Win32::UI::WindowsAndMessaging::ShowWindow;`<br>`unsafe { ShowWindow(hwnd, SW_HIDE) }` | [ShowWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindow) | Hide/show window; SW_SHOWNOACTIVATE shows without stealing focus |
| 4 | `_user32.IsWindowVisible(hwnd)` | `use windows::Win32::UI::WindowsAndMessaging::IsWindowVisible;`<br>`unsafe { IsWindowVisible(hwnd) }` | [IsWindowVisible](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-iswindowvisible) | Check if window is currently visible |
| 5 | `self.GetWindowLongPtrW(hwnd, GWL_EXSTYLE)` | `use windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW;`<br>`unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) }` | [GetWindowLongPtrW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowlongptrw) | Read extended window styles (layered, topmost, transparent, toolwindow) |
| 6 | `self.SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style)` | `use windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW;`<br>`unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style) }` | [SetWindowLongPtrW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowlongptrw) | Modify extended window styles (add/remove layered, transparent, toolwindow) |
| 7 | `self.SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE \| SWP_NOSIZE)` | `use windows::Win32::UI::WindowsAndMessaging::SetWindowPos;`<br>`unsafe { SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE \| SWP_NOSIZE) }` | [SetWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos) | Set always-on-top, move window without focus change |
| 8 | `self.SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA)` | `use windows::Win32::UI::WindowsAndMessaging::SetLayeredWindowAttributes;`<br>`unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA) }` | [SetLayeredWindowAttributes](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setlayeredwindowattributes) | Set per-window opacity (0-255 alpha) |
| 9 | `self.EnumWindows(callback, 0)` | `use windows::Win32::UI::WindowsAndMessaging::EnumWindows;`<br>`unsafe { EnumWindows(Some(callback), LPARAM(0)) }` | [EnumWindows](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumwindows) | Iterate all top-level windows to find screen-share indicators |
| 10 | `self.GetWindowTextW(hwnd, buffer, 512)` | `use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;`<br>`let mut buf = [0u16; 512]; unsafe { GetWindowTextW(hwnd, &mut buf) }` | [GetWindowTextW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowtextw) | Get window title text for heuristic matching |
| 11 | `self.GetClassNameW(hwnd, buffer, 256)` | `use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;`<br>`let mut buf = [0u16; 256]; unsafe { GetClassNameW(hwnd, &mut buf) }` | [GetClassNameW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getclassnamew) | Get window class name for heuristic matching |
| 12 | `self.GetWindowRect(hwnd, ctypes.byref(rect))` | `use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;`<br>`let mut rect = RECT::default(); unsafe { GetWindowRect(hwnd, &mut rect) }` | [GetWindowRect](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowrect) | Get window position for stealth movement |
| 13 | `_user32.IsWindow(hwnd)` | `use windows::Win32::UI::WindowsAndMessaging::IsWindow;`<br>`unsafe { IsWindow(hwnd) }` | [IsWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-iswindow) | Verify window handle is still valid |
| 14 | `ctypes.windll.kernel32.GetLastError()` | `use windows::Win32::Foundation::GetLastError;`<br>`unsafe { GetLastError() }` | [GetLastError](https://learn.microsoft.com/en-us/windows/win32/api/errhandlingapi/nf-errhandlingapi-getlasterror) | Debug failed Win32 calls |


### Table 2: Win32 Constants Inventory

| Constant | Hex Value | API That Accepts It | Semantic |
|----------|-----------|---------------------|----------|
| `WDA_EXCLUDEFROMCAPTURE` | `0x00000011` | `SetWindowDisplayAffinity` | Window content excluded from all capture methods (DWM, BitBlt, PrintWindow) — appears as black rect |
| `WDA_MONITOR` | `0x00000001` | `SetWindowDisplayAffinity` | Window content only visible on monitor (older, less comprehensive than 0x11) |
| `WDA_NONE` | `0x00000000` | `SetWindowDisplayAffinity` | Default — window is capturable normally |
| `SW_HIDE` | `0` | `ShowWindow` | Hides the window completely |
| `SW_SHOW` | `5` | `ShowWindow` | Shows window and activates it (steals focus) |
| `SW_SHOWNOACTIVATE` | `4` | `ShowWindow` | Shows window WITHOUT activating — critical for stealth (no focus change detected) |
| `SW_MINIMIZE` | `6` | `ShowWindow` | Minimizes window (used as fallback for hiding indicators) |
| `WS_EX_TOPMOST` | `0x00000008` | `GetWindowLongPtrW` / `SetWindowLongPtrW` | Window stays above all non-topmost windows |
| `WS_EX_TRANSPARENT` | `0x00000020` | `GetWindowLongPtrW` / `SetWindowLongPtrW` | Click-through: mouse events pass to windows beneath ("ghost mode") |
| `WS_EX_TOOLWINDOW` | `0x00000080` | `GetWindowLongPtrW` / `SetWindowLongPtrW` | Hidden from taskbar and Alt+Tab switcher |
| `WS_EX_APPWINDOW` | `0x00040000` | `GetWindowLongPtrW` / `SetWindowLongPtrW` | Forces taskbar button — removed when hiding from taskbar |
| `WS_EX_LAYERED` | `0x00080000` | `GetWindowLongPtrW` / `SetWindowLongPtrW` | Enables per-pixel alpha / SetLayeredWindowAttributes |
| `LWA_ALPHA` | `0x00000002` | `SetLayeredWindowAttributes` | Use the bAlpha parameter for uniform window transparency |
| `LWA_COLORKEY` | `0x00000001` | `SetLayeredWindowAttributes` | Use crKey for color-key transparency (not used in Aura) |
| `HWND_TOPMOST` | `-1` | `SetWindowPos` (hwndInsertAfter) | Place window above all non-topmost windows permanently |
| `HWND_NOTOPMOST` | `-2` | `SetWindowPos` (hwndInsertAfter) | Remove topmost status, place above normal windows |
| `SWP_NOMOVE` | `0x0002` | `SetWindowPos` (flags) | Retain current position (ignore x, y params) |
| `SWP_NOSIZE` | `0x0001` | `SetWindowPos` (flags) | Retain current size (ignore cx, cy params) |
| `SWP_NOACTIVATE` | `0x0010` | `SetWindowPos` (flags) | Do not activate the window — critical for stealth movement |
| `SWP_NOZORDER` | `0x0004` | `SetWindowPos` (flags) | Retain current Z-order (ignore hwndInsertAfter) |
| `SWP_FRAMECHANGED` | `0x0020` | `SetWindowPos` (flags) | Force WM_NCCALCSIZE — needed after style changes |
| `GWL_EXSTYLE` | `-20` | `GetWindowLongPtrW` / `SetWindowLongPtrW` (nIndex) | Access extended window style bits |

---

## window_manager.py Architecture

### Class: `WindowManager` (singleton instance: `window_manager`)

**State:**
- `hwnd: Optional[int]` — cached window handle
- `is_windows: bool` — platform gate
- `current_transparency: float` — 0.0–1.0
- `is_ghost_mode: bool` — click-through state
- `screen_share_monitor_active: bool` — background monitor running
- `hidden_screen_share_windows: set` — HWNDs already hidden
- `scrolling_up/down: bool` — continuous scroll state
- `hotkey_listener` — pynput GlobalHotKeys instance

**Key Methods:**

| Method | Purpose |
|--------|---------|
| `_setup_win32_api_definitions()` | Defines all ctypes function signatures + constants |
| `set_window_handle(hwnd)` | Cache HWND, enable layered style |
| `_enable_transparency()` | Add `WS_EX_LAYERED` to window style |
| `set_transparency(float)` | Convert 0.0–1.0 → 0–255, call `SetLayeredWindowAttributes` |
| `set_always_on_top(bool)` | `SetWindowPos` with `HWND_TOPMOST` / `HWND_NOTOPMOST` |
| `_set_always_on_top_alternative(bool)` | Fallback: modify `WS_EX_TOPMOST` via `SetWindowLongPtrW` |
| `set_ghost_mode(bool)` | Toggle `WS_EX_TRANSPARENT` for click-through |
| `toggle_visibility()` | `ShowWindow(SW_HIDE)` / `ShowWindow(SW_SHOWNOACTIVATE)` |
| `hide_from_taskbar()` | Add `WS_EX_TOOLWINDOW`, remove `WS_EX_APPWINDOW` |
| `move_window(dx, dy)` | `GetWindowRect` → `SetWindowPos` with `SWP_NOACTIVATE \| SWP_NOZORDER` |
| `find_screen_share_indicators()` | `EnumWindows` + title/class heuristic matching |
| `hide_screen_share_indicator(hwnd)` | `ShowWindow(SW_HIDE)` → fallback: move off-screen → fallback: minimize |
| `start_screen_share_monitor()` | Background thread polling every 1s |
| `enable_proctoring_stealth_mode()` | Combines ghost + taskbar-hide + always-on-top + 70% opacity |
| `start_hotkey_listener()` | Spawns thread with `pynput.keyboard.GlobalHotKeys` |
| `_write_command_file(data)` | IPC via temp JSON file (`%TEMP%/aura_command.json`) |

### Module-Level Functions (convenience wrappers):
- `apply_capture_protection(window)` — the core function called from `main.py`
- `find_aura_window()`, `set_app_always_on_top()`, `set_app_transparency()`, etc.


---

## Portable Patterns (Python → Rust)

### 1. Content Protection (SetWindowDisplayAffinity)

**Source**: `window_manager.py` — `apply_capture_protection()` function (lines ~1200-1280)

**Python impl:**
```python
WDA_EXCLUDEFROMCAPTURE = 0x00000011
_user32 = ctypes.windll.user32
_user32.SetWindowDisplayAffinity.restype = wintypes.BOOL
_user32.SetWindowDisplayAffinity.argtypes = (wintypes.HWND, wintypes.DWORD)

# Get HWND from pywebview private attr or FindWindowW fallback
hwnd = getattr(window, '_hwnd', None)
if not hwnd:
    hwnd = _user32.FindWindowW(None, window.title)

success = _user32.SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)
```

**Rust equivalent (`windows = "0.58"`):**
```rust
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowDisplayAffinity, FindWindowW, WDA_EXCLUDEFROMCAPTURE,
};
use windows::core::w;

// In Tauri, get HWND from the window:
// let hwnd = window.hwnd().unwrap(); // tauri::Window::hwnd()

// Fallback: find by title
let hwnd = unsafe { FindWindowW(None, w!("bluey")) };

// Apply content protection
unsafe {
    SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)?;
}
```

**Bluey use case**: Prevent sensitive AI suggestions (code completions, meeting summaries, private notes) from appearing in shared screens during video calls. Same use case as 1Password hiding vault contents, Netflix DRM, or banking apps hiding account details.

---

### 2. Per-Window Opacity (Layered Window + SetLayeredWindowAttributes)

**Source**: `window_manager.py` — `_enable_transparency()` + `set_transparency()`

**Python impl:**
```python
GWL_EXSTYLE = -20
WS_EX_LAYERED = 0x80000
LWA_ALPHA = 0x2

# Step 1: Enable layered style
ex_style = self.GetWindowLongPtr(hwnd, GWL_EXSTYLE)
if not (ex_style & WS_EX_LAYERED):
    self.SetWindowLongPtr(hwnd, GWL_EXSTYLE, ex_style | WS_EX_LAYERED)

# Step 2: Set alpha (0-255)
alpha = int(transparency * 255)  # transparency is 0.0-1.0
self.SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA)
```

**Rust equivalent:**
```rust
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, SetLayeredWindowAttributes,
    GWL_EXSTYLE, WS_EX_LAYERED, LWA_ALPHA,
};
use windows::Win32::Foundation::COLORREF;

unsafe {
    // Enable layered
    let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    if ex_style & WS_EX_LAYERED.0 as isize == 0 {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_LAYERED.0 as isize);
    }

    // Set opacity (0-255)
    let alpha: u8 = (opacity * 255.0) as u8;
    SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA)?;
}
```

**Tauri shortcut**: Tauri already abstracts this via `window.set_opacity(0.7)` on Windows — it internally does the same WS_EX_LAYERED + SetLayeredWindowAttributes dance. Use Tauri's API unless you need finer control.

**Bluey use case**: Allow users to see content beneath the AI overlay (e.g., reading a document while AI suggestions float semi-transparently on top).

---

### 3. Always-On-Top (SetWindowPos with HWND_TOPMOST)

**Source**: `window_manager.py` — `set_always_on_top()`

**Python impl:**
```python
HWND_TOPMOST = -1
HWND_NOTOPMOST = -2
SWP_NOMOVE = 0x2
SWP_NOSIZE = 0x1

hwnd_insert_after = HWND_TOPMOST if on_top else HWND_NOTOPMOST
self.SetWindowPos(hwnd, hwnd_insert_after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE)
```

**Rust equivalent:**
```rust
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, HWND_TOPMOST, HWND_NOTOPMOST, SWP_NOMOVE, SWP_NOSIZE,
};

unsafe {
    let insert_after = if on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
    SetWindowPos(hwnd, insert_after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE)?;
}
```

**Tauri shortcut**: `window.set_always_on_top(true)` — Tauri wraps this identically.

**Bluey use case**: Keep AI assistant visible above other windows during meetings/work sessions.

---

### 4. Click-Through / Ghost Mode (WS_EX_TRANSPARENT)

**Source**: `window_manager.py` — `set_ghost_mode()`

**Python impl:**
```python
WS_EX_TRANSPARENT = 0x20

current_style = self.GetWindowLongPtr(hwnd, GWL_EXSTYLE)
if enabled:
    new_style = current_style | WS_EX_TRANSPARENT
else:
    new_style = current_style & ~WS_EX_TRANSPARENT
self.SetWindowLongPtr(hwnd, GWL_EXSTYLE, new_style)
```

**Rust equivalent:**
```rust
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_TRANSPARENT,
};

unsafe {
    let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    let new_style = if enabled {
        style | WS_EX_TRANSPARENT.0 as isize
    } else {
        style & !(WS_EX_TRANSPARENT.0 as isize)
    };
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style);
}
```

**Bluey use case**: Allow users to interact with applications beneath the AI overlay without dismissing it — read AI suggestions while clicking on the document/IDE underneath.

---

### 5. Hide from Taskbar (WS_EX_TOOLWINDOW)

**Source**: `window_manager.py` — `hide_from_taskbar()`

**Python impl:**
```python
WS_EX_TOOLWINDOW = 0x80
WS_EX_APPWINDOW = 0x40000

ex_style = self.GetWindowLongPtr(hwnd, GWL_EXSTYLE)
new_style = (ex_style | WS_EX_TOOLWINDOW) & ~0x40000
self.SetWindowLongPtr(hwnd, GWL_EXSTYLE, new_style)
```

**Rust equivalent:**
```rust
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE,
    WS_EX_TOOLWINDOW, WS_EX_APPWINDOW,
};

unsafe {
    let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    let new_style = (style | WS_EX_TOOLWINDOW.0 as isize)
                  & !(WS_EX_APPWINDOW.0 as isize);
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style);
}
```

**Tauri shortcut**: `window.set_skip_taskbar(true)` handles this.

**Bluey use case**: Reduce visual clutter — AI assistant doesn't need a taskbar slot when it's always-on-top and controlled via hotkeys.

---

### 6. Stealth Window Movement (SetWindowPos with SWP_NOACTIVATE)

**Source**: `window_manager.py` — `move_window(dx, dy)`

**Python impl:**
```python
SWP_NOSIZE = 0x1
SWP_NOACTIVATE = 0x10
SWP_NOZORDER = 0x4

rect = self.RECT()
self.GetWindowRect(hwnd, ctypes.byref(rect))
new_x = rect.left + dx
new_y = rect.top + dy
self.SetWindowPos(hwnd, 0, new_x, new_y, 0, 0,
                  SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER)
```

**Rust equivalent:**
```rust
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, GetWindowRect, SWP_NOSIZE, SWP_NOACTIVATE, SWP_NOZORDER,
};
use windows::Win32::Foundation::{HWND, RECT};

unsafe {
    let mut rect = RECT::default();
    GetWindowRect(hwnd, &mut rect)?;
    SetWindowPos(
        hwnd,
        HWND::default(), // ignored due to SWP_NOZORDER
        rect.left + dx,
        rect.top + dy,
        0, 0,
        SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
    )?;
}
```

**Bluey use case**: Reposition AI overlay via keyboard shortcuts without stealing focus from the user's active application.

---

### 7. Window Enumeration for Screen-Share Detection

**Source**: `window_manager.py` — `find_screen_share_indicators()`

**Python impl:**
```python
def enum_windows_callback(hwnd, lparam):
    title_buffer = ctypes.create_unicode_buffer(512)
    self.GetWindowTextW(hwnd, title_buffer, 512)
    title = title_buffer.value

    class_buffer = ctypes.create_unicode_buffer(256)
    self.GetClassNameW(hwnd, class_buffer, 256)
    class_name = class_buffer.value

    # Match against SCREEN_SHARE_INDICATORS list
    for indicator_text in SCREEN_SHARE_INDICATORS:
        if indicator_text.lower() in title.lower():
            found_windows.append({'hwnd': hwnd, 'title': title, 'class': class_name})
            break
    return True  # continue enumeration

callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
callback = callback_type(enum_windows_callback)
self.EnumWindows(callback, 0)
```

**Rust equivalent:**
```rust
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetClassNameW, IsWindowVisible,
};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};

struct FoundWindow {
    hwnd: HWND,
    title: String,
    class_name: String,
}

let mut found: Vec<FoundWindow> = Vec::new();

unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let found = &mut *(lparam.0 as *mut Vec<FoundWindow>);

    let mut title_buf = [0u16; 512];
    let len = GetWindowTextW(hwnd, &mut title_buf);
    let title = String::from_utf16_lossy(&title_buf[..len as usize]);

    let mut class_buf = [0u16; 256];
    let len = GetClassNameW(hwnd, &mut class_buf);
    let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);

    let title_lower = title.to_lowercase();
    if INDICATORS.iter().any(|ind| title_lower.contains(&ind.to_lowercase())) {
        if IsWindowVisible(hwnd).as_bool() {
            found.push(FoundWindow { hwnd, title, class_name });
        }
    }
    BOOL(1) // continue
}

unsafe {
    EnumWindows(
        Some(enum_callback),
        LPARAM(&mut found as *mut _ as isize),
    )?;
}
```

**Bluey use case**: Detect when a screen-sharing session is active (e.g., Zoom, Teams, OBS) so the app can auto-adjust behavior — similar to how macOS shows a recording indicator in the menu bar.


---

## main.py Thread Architecture

### Threading Model

```
┌─────────────────────────────────────────────────────────────┐
│  MAIN THREAD (required by pywebview/WinForms)               │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  webview.start(debug=DEV_MODE)                      │    │
│  │  - Creates native window (WinForms backend)         │    │
│  │  - Fires 'shown' event → apply_capture_protection() │    │
│  │  - Fires 'closing' event → shutdown services        │    │
│  │  - Blocks until window closes                       │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  BACKGROUND THREAD: AsyncioServiceThread (daemon=True)       │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  asyncio.new_event_loop() — dedicated event loop    │    │
│  │                                                     │    │
│  │  Concurrent tasks:                                  │    │
│  │  1. UvicornServer (FastAPI on 127.0.0.1:8002)      │    │
│  │  2. GlobalCommandMonitor (polls temp file @ 200ms)  │    │
│  │  3. SessionManager cleanup task                     │    │
│  │                                                     │    │
│  │  Shutdown: threading.Event → graceful task cancel   │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  HOTKEY THREAD (daemon=True, spawned from on_window_shown)   │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  pynput.keyboard.GlobalHotKeys — blocks on join()   │    │
│  │  Writes commands to %TEMP%/aura_command.json        │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  SCREEN SHARE MONITOR THREAD (daemon=True, optional)         │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Polls EnumWindows every 1s, hides indicators       │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  KEY RELEASE LISTENER THREAD (daemon=True)                   │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  pynput.keyboard.Listener — detects Alt+Up/Down     │    │
│  │  release to stop continuous scrolling               │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

### Inter-Thread Communication

**Hotkey → Backend**: File-based IPC via `%TEMP%/aura_command.json`
- Hotkey thread writes JSON command to temp file
- `GlobalCommandMonitor` (asyncio task) polls file every 200ms
- On new command: reads JSON, dispatches to `webview.windows[0].evaluate_js()`
- Commands forwarded to frontend via JS function calls

**Backend → Frontend**: WebSocket (FastAPI → browser JS)
- Real-time bidirectional via `/ws` endpoint
- Session-based with auto-resume on reconnect

### Key Classes

| Class | Thread | Role |
|-------|--------|------|
| `AsyncioServiceThread` | Background | Owns asyncio loop, runs Uvicorn + monitors |
| `UvicornServer` | Background (asyncio) | FastAPI HTTP/WS server |
| `GlobalCommandMonitor` | Background (asyncio task) | Polls command file, dispatches to webview |
| `WindowManager` | Main + Hotkey | Win32 API calls (most are thread-safe) |

---

## Hotkey Reference Table

All hotkeys use `pynput.keyboard.GlobalHotKeys` — system-wide, work regardless of focused window.

| Hotkey | Action | Implementation | Category |
|--------|--------|----------------|----------|
| `Alt+Z` | Toggle window visibility (show/hide) | `toggle_visibility()` → `ShowWindow(SW_HIDE/SW_SHOWNOACTIVATE)` | Stealth |
| `Alt+X` | Toggle ghost mode (click-through) | `set_ghost_mode()` → toggle `WS_EX_TRANSPARENT` | Stealth |
| `Alt+Shift+S` | Enable full proctoring stealth mode | `enable_proctoring_stealth_mode()` → ghost + taskbar-hide + on-top + 70% | Stealth |
| `Alt+1` | Set 40% opacity (transparent) | `send_transparency_command("transparent")` → `set_transparency(0.4)` | Transparency |
| `Alt+2` | Set 70% opacity (semi-transparent) | `send_transparency_command("semi")` → `set_transparency(0.7)` | Transparency |
| `Alt+3` | Set 100% opacity (opaque) | `send_transparency_command("opaque")` → `set_transparency(1.0)` | Transparency |
| `Alt+I` | Move window up 20px | `move_window(0, -20)` | Movement |
| `Alt+J` | Move window down 20px | `move_window(0, 20)` | Movement |
| `Alt+Left` | Move window left 20px | `move_window(-20, 0)` | Movement |
| `Alt+Right` | Move window right 20px | `move_window(20, 0)` | Movement |
| `Alt+Up` | Continuous scroll up (hold) | `send_scroll_command("up")` in loop | Scrolling |
| `Alt+Down` | Continuous scroll down (hold) | `send_scroll_command("down")` in loop | Scrolling |
| `Alt+V` | Toggle vision mode | `send_vision_command("toggle_vision_mode")` | Vision AI |
| `Alt+S` | Capture screenshot (queue up to 4) | `send_vision_command("capture_screenshot")` | Vision AI |
| `Alt+P` | Process screenshot queue with AI | `send_vision_command("process_screenshots")` | Vision AI |
| `Alt+R` | Reset/clear screenshot queue | `send_vision_command("reset_screenshot_queue")` | Vision AI |
| `Alt+T` | Cycle vision model | `send_vision_switch_command("switch_vision_model")` | Vision AI |
| `Alt+Q` | Switch to primary AI preset | `send_preset_switch_signal("primary")` | AI Model |
| `Alt+W` | Switch to secondary AI preset | `send_preset_switch_signal("secondary")` | AI Model |
| `Alt+E` | Auto-select best available AI | `send_context_aware_command("auto_select_preset")` | AI Model |
| `Alt+M` | Toggle microphone mute | `send_audio_command("toggle_mic_mute")` | Audio |
| `Alt+U` | Toggle universal mute/pause | `send_audio_command("toggle_universal_mute")` | Audio |
| `Alt+O` | Reset interview session | `send_interview_command("reset_interview")` | Session |

**Scroll configuration** (via `.env`):
- `SCROLL_SPEED_PX` — pixels per tick (default: 200)
- `SCROLL_INTERVAL_MS` — ms between ticks while held (default: 50)

---

## Opacity Preset Logic

Three presets mapped to `Alt+1/2/3`:

| Preset | Opacity | Alpha (0-255) | Use Case |
|--------|---------|---------------|----------|
| Transparent | 40% | 102 | Maximum see-through — read content beneath while AI suggestions barely visible |
| Semi-transparent | 70% | 178 | Balanced — AI content readable, underlying app still visible |
| Opaque | 100% | 255 | Full visibility — normal window, used when not overlaying other content |

The transparency command flows: Hotkey → temp file → GlobalCommandMonitor → `evaluate_js("window.setTransparency('level')")` → frontend updates UI state → calls `/api/transparency` → backend calls `window_manager.set_transparency()`.

---

## Window Enumeration: Screen-Share Indicator Detection

### Heuristic Logic (`find_screen_share_indicators()`)

**Two-pass detection:**

1. **Title matching** — case-insensitive substring match against 80+ indicator strings:
   - Generic: "Screen sharing indicator", "Recording in progress", "You're sharing your screen"
   - Platform-specific: "Zoom is sharing", "Microsoft Teams is sharing", "OBS Studio is recording"
   - Browser: "Chrome is sharing your screen", "Firefox is sharing your screen"
   - Remote desktop: "TeamViewer", "AnyDesk", "Chrome Remote Desktop"

2. **Class name matching** (with verification) — matches known window classes:
   - `ZPContentViewWndClass` (Zoom), `TeamsWebView` (Teams), `Qt5QWindowIcon` (OBS)
   - `Chrome_WidgetWin_1`, `MozillaDialogClass`, `GameBarDisplayCaptureIndicator`
   - **Requires secondary verification**: title must also contain keywords like "sharing", "screen", "record", "capture"

3. **Visibility gate** — only considers windows where `IsWindowVisible()` returns true

**Relevance to bluey**: This pattern detects when screen-sharing is active. Bluey could use this to:
- Auto-enable content protection if not already active
- Show a "screen share detected" indicator to the user
- Adjust behavior (e.g., suppress sensitive notifications)

---

## pywebview → Tauri Mapping

| pywebview API | Aura Usage | Tauri Equivalent |
|---------------|-----------|------------------|
| `webview.create_window(title, url, width, height, resizable)` | Create main window loading FastAPI server | `tauri::WindowBuilder::new(app, "main", url).title("bluey").inner_size(1000, 750)` |
| `webview.start(debug=True)` | Start GUI event loop (blocks main thread) | `tauri::Builder::default().run()` (Tauri manages the event loop) |
| `window.events.shown += handler` | Hook window-shown for post-creation setup | `window.on_window_event(\|event\| match event { WindowEvent::Focused(_) => ... })` or Tauri `setup` hook |
| `window.events.closing += handler` | Graceful shutdown on close | `window.on_window_event(\|event\| match event { WindowEvent::CloseRequested { .. } => ... })` |
| `window.evaluate_js(code)` | Execute JS in webview from Python | `window.eval(code)` or Tauri commands via `invoke()` |
| `window._hwnd` (private) | Get native window handle | `window.hwnd().unwrap()` (Tauri provides this directly) |
| `window.title` | Get window title | `window.title()` |

**Key architectural difference**: pywebview requires the GUI on the main thread with a separate asyncio thread for the server. Tauri handles this natively — the Rust backend runs alongside the webview without manual thread management. Tauri's command system (`#[tauri::command]`) replaces the FastAPI+WebSocket layer for most IPC.

**What Tauri gives you for free** (no manual Win32 needed):
- `window.set_always_on_top(true)` — replaces `SetWindowPos(HWND_TOPMOST)`
- `window.set_skip_taskbar(true)` — replaces `WS_EX_TOOLWINDOW` manipulation
- `window.hide()` / `window.show()` — replaces `ShowWindow`
- `window.set_ignore_cursor_events(true)` — replaces `WS_EX_TRANSPARENT` (Tauri 2.x)
- `window.set_opacity(0.7)` — replaces `SetLayeredWindowAttributes` (Tauri 2.x, Windows only via plugin)

**What still requires raw Win32 in Tauri**:
- `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` — **no Tauri abstraction exists**
- `EnumWindows` for screen-share detection — no Tauri equivalent
- `SW_SHOWNOACTIVATE` (show without focus) — Tauri's `show()` activates the window

---

## AI Provider JSON Schema

**File**: `ai_providers.json` (created from `ai_providers.example.json`)

```jsonc
[
  {
    "name": "ProviderName",           // Display name (Groq, Cerebras, Gemini, OpenRouter)
    "baseURL": "https://api.../v1",   // OpenAI-compatible endpoint
    "apiKey": "KEY",                  // Single key (legacy, used if apiKeys empty)
    "apiKeys": ["KEY1", "KEY2"],      // Multiple keys for round-robin rotation
    "models": [                       // Text models — string or object format
      "model-name",                   // Simple: just model ID
      {                               // Complex: with routing params
        "modelName": "org/model",
        "description": "Human label",
        "requestParams": {            // Extra body params (e.g., OpenRouter provider routing)
          "provider": { "only": ["Cerebras"] }
        }
      }
    ],
    "visionModels": ["model-a"],      // Models supporting image input
    "supportsVision": true,           // Whether provider has any vision models
    "defaultPrimary": false,          // Auto-select as primary on first launch
    "defaultSecondary": false,        // Auto-select as secondary
    "defaultVisionPrimary": false,    // Auto-select as primary vision
    "defaultVisionSecondary": false,  // Auto-select as secondary vision
    "defaultModel": "model-name",     // Default text model for this provider
    "defaultVisionModel": "model-name" // Default vision model
  }
]
```

**Provider selection flow**:
1. Frontend reads sanitized provider list via `GET /api/ai-providers` (keys stripped)
2. User selects primary + secondary providers and models during onboarding
3. Selection sent via WebSocket `start_interview` message
4. `MultiLLMManager.load_configuration()` creates `LLMManager` instances with key rotation
5. `Alt+Q/W/E` hotkeys switch active preset at runtime

**Key rotation**: `LLMManager._rotate_key()` cycles through `apiKeys` array on each API error — instant retry with next key, zero delay. All keys exhausted → failover to secondary provider.

---

## Python requirements.txt (Annotated)

| Package | Version | Role | Tauri Equivalent |
|---------|---------|------|------------------|
| `pywebview[winforms]` | latest | Desktop webview shell (WinForms backend on Windows) | Tauri itself (wry/tao) |
| `pywin32` | latest | Win32 COM/API access (not heavily used — ctypes preferred) | `windows` crate |
| `fastapi` | latest | HTTP/WebSocket API server | Tauri commands + events (or keep as sidecar) |
| `uvicorn[standard]` | latest | ASGI server for FastAPI | Not needed if using Tauri commands |
| `pydantic-settings` | latest | Typed config from `.env` | `serde` + `config` crate, or `dotenvy` |
| `openai` | latest | OpenAI-compatible LLM client (async) | `async-openai` crate |
| `deepgram-sdk==3.*` | 3.x | Real-time speech-to-text | Deepgram REST API via `reqwest` |
| `requests` | latest | Sync HTTP (minimal use) | `reqwest` |
| `httpx` | latest | Async HTTP client | `reqwest` |
| `orjson` | latest | Fast JSON serialization | `serde_json` (or `simd-json`) |
| `aiofiles` | latest | Async file I/O | `tokio::fs` |
| `pynput` | latest | Global hotkey listener | `global-hotkey` crate (Tauri plugin) or `rdev` |
| `python-dotenv` | latest | Load `.env` files | `dotenvy` crate |

---

## Summary: Aura's Technical Contributions to Bluey's Architecture

### What to port directly (requires raw Win32):
1. **`SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`** — the core content-protection API. No Tauri abstraction. Must call via `windows` crate.
2. **`EnumWindows` + title/class heuristics** — screen-share detection. Useful for auto-adjusting behavior when user is in a meeting.
3. **`ShowWindow(SW_SHOWNOACTIVATE)`** — show window without stealing focus. Tauri's `show()` doesn't support this flag.

### What Tauri already provides:
- Always-on-top (`set_always_on_top`)
- Skip taskbar (`set_skip_taskbar`)
- Opacity (`set_opacity` — Windows only, Tauri 2.x)
- Click-through (`set_ignore_cursor_events` — Tauri 2.x)
- Hide/show (`hide()`/`show()`)
- Window positioning (`set_position`)

### Architecture lessons from Aura:
- **File-based IPC for hotkeys** is a hack around pywebview's limitations. Tauri's event system (`app.emit()`, `window.emit()`) replaces this cleanly.
- **Separate asyncio thread** is unnecessary in Tauri — Rust's async runtime (tokio) runs natively alongside the webview.
- **Global hotkeys** → use Tauri's `global-shortcut` plugin or the `tauri-plugin-global-shortcut` crate.
- **WebSocket for frontend comms** → Tauri commands (`#[tauri::command]`) + events replace this entirely for local IPC. Keep WebSocket only if you need browser-tab access.

### Legitimate privacy use cases (same as banking/password-manager apps):
1. Hide AI-generated content from screen recordings during meetings (prevent accidental sharing of drafts/suggestions)
2. Overlay AI assistance without it appearing in shared presentations
3. Keep sensitive information (API keys in config UI, private notes) invisible to screen capture
4. Detect active screen-sharing to warn users before displaying sensitive content
