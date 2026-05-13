# Skill: macOS Native APIs

## NSPanel (Overlay Window)

Cue uses NSPanel for the floating overlay — non-activating, always-on-top.

```rust
use cocoa::appkit::{NSPanel, NSWindowStyleMask, NSFloatingWindowLevel};
use cocoa::base::nil;

// Create non-activating panel
let panel: id = NSPanel::alloc(nil).initWithContentRect_styleMask_backing_defer_(
    rect,
    NSWindowStyleMask::NSBorderlessWindowMask
        | NSWindowStyleMask::NSNonactivatingPanelMask,
    NSBackingStoreBuffered,
    NO,
);
panel.setLevel_(NSFloatingWindowLevel as i64 + 1);
panel.setCollectionBehavior_(
    NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::FullScreenAuxiliary,
);
```

## CoreAudio (System Audio Capture)

```rust
// Audio tap for capturing system audio (macOS 14.4+)
// Uses ScreenCaptureKit for audio-only capture
use screencapturekit::{
    sc_stream::SCStream,
    sc_stream_configuration::SCStreamConfiguration,
};

let config = SCStreamConfiguration::new()
    .set_captures_audio(true)
    .set_excludes_current_process_audio(true)
    .set_sample_rate(16000)
    .set_channel_count(1);
```

## ScreenCaptureKit (Audio Stream)

```rust
// Request permission
SCShareableContent::get_with_completion_handler(|content, error| {
    // Filter to exclude own app
    let filter = SCContentFilter::new_with_display_excluding_apps_and_windows(
        display, &[own_app], &[],
    );
    let stream = SCStream::new(filter, config, delegate);
    stream.start_capture();
});
```

## Content Protection

```rust
// Prevent screen recording of overlay content
use cocoa::appkit::NSWindow;
window.setSharingType_(NSWindowSharingNone); // macOS 10.0+
```

## Accessibility Permissions

```rust
// Check accessibility permission (needed for global hotkeys)
use core_foundation::boolean::CFBoolean;
use accessibility_sys::AXIsProcessTrustedWithOptions;

let trusted = unsafe {
    let options = /* kAXTrustedCheckOptionPrompt: true */;
    AXIsProcessTrustedWithOptions(options)
};
```

## Global Hotkeys

```rust
// Register global hotkey via Carbon Events or CGEvent tap
// Preferred: use tauri-plugin-global-shortcut
use tauri_plugin_global_shortcut::GlobalShortcutExt;

app.global_shortcut().register("CmdOrCtrl+Shift+Space")?;
```
