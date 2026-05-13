# Skill: Windows Native APIs

## WASAPI (Audio Capture)

```rust
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;

// Initialize COM
unsafe { CoInitializeEx(None, COINIT_MULTITHREADED)? };

// Get default audio endpoint (loopback for system audio)
let enumerator: IMMDeviceEnumerator =
    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;

// Activate audio client
let audio_client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;

// Initialize in loopback mode for system audio capture
audio_client.Initialize(
    AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_LOOPBACK,
    buffer_duration,
    0,
    &wave_format,
    None,
)?;

let capture_client: IAudioCaptureClient = audio_client.GetService()?;
audio_client.Start()?;
```

## Win32 Overlay Window

```rust
use windows::Win32::UI::WindowsAndMessaging::*;

// Create layered, topmost, transparent-to-input window
let hwnd = CreateWindowExW(
    WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW,
    class_name,
    window_name,
    WS_POPUP,
    x, y, width, height,
    None, None, hinstance, None,
)?;

// Set transparency
SetLayeredWindowAttributes(hwnd, COLORREF(0), 200, LWA_ALPHA)?;
ShowWindow(hwnd, SW_SHOWNOACTIVATE);
```

## Content Protection (DRM/Anti-Capture)

```rust
use windows::Win32::Graphics::Dwm::*;

// Exclude window from screen capture (Windows 10 2004+)
let affinity = WDA_EXCLUDEFROMCAPTURE;
SetWindowDisplayAffinity(hwnd, affinity)?;
```

## Global Hotkeys

```rust
use windows::Win32::UI::Input::KeyboardAndMouse::*;

// Register system-wide hotkey
RegisterHotKey(hwnd, HOTKEY_ID, MOD_CONTROL | MOD_SHIFT, VK_SPACE.0 as u32)?;

// Handle in message loop
if msg.message == WM_HOTKEY && msg.wParam.0 == HOTKEY_ID as usize {
    toggle_overlay();
}
```

## Named Pipes (IPC Alternative)

```rust
use tokio::net::windows::named_pipe::{ServerOptions, ClientOptions};

// Server
let server = ServerOptions::new()
    .first_pipe_instance(true)
    .create(r"\\.\pipe\cue-daemon")?;
server.connect().await?;
```
