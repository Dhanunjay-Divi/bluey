// BlueyShot — a tiny compiled helper that captures the main display to a PNG.
//
// WHY A COMPILED BINARY (not a shell script around `screencapture`):
//   macOS TCC attributes Screen Recording to the process that calls the capture
//   API. A shell script bundle launches /bin/bash which spawns `screencapture`
//   as a SEPARATE child — TCC attributes the capture to that child, NOT to the
//   bundle's identity (sh.bluey.shot), so the grant never applies and the app
//   never registers in System Settings. A compiled binary that calls
//   ScreenCaptureKit DIRECTLY makes the .app itself the capturer — the same
//   pattern that makes BlueyAudio.app work. This is the whole reason this exists.
//
// Usage:  BlueyShot --out /path/to/shot.png
// Exit 0 + a non-empty PNG on success; non-zero on failure/denial.

import AppKit
import CoreGraphics
import CoreMedia
import ScreenCaptureKit

func outputPath() -> String? {
    let args = CommandLine.arguments
    for i in args.indices where args[i] == "--out" {
        let n = args.index(after: i)
        if n < args.endIndex { return args[n] }
    }
    return nil
}

func fail(_ msg: String) -> Never {
    FileHandle.standardError.write(Data((msg + "\n").utf8))
    exit(1)
}

guard let out = outputPath() else { fail("missing --out <path>") }

// Capture the main display via ScreenCaptureKit's one-shot API (macOS 14+),
// which is TCC-attributed to THIS process (the bundle). Runs the async work on a
// semaphore so `main.swift` stays a straight-line program that exits when done.
let sema = DispatchSemaphore(value: 0)
var captureError: String?

Task {
    do {
        // Enumerate shareable content (this is the call that triggers the Screen
        // Recording permission prompt / check, attributed to sh.bluey.shot).
        let content = try await SCShareableContent.excludingDesktopWindows(
            false, onScreenWindowsOnly: false)
        guard let display = content.displays.first else {
            captureError = "no display found"
            sema.signal()
            return
        }

        let config = SCStreamConfiguration()
        config.width = display.width
        config.height = display.height

        let filter = SCContentFilter(display: display, excludingWindows: [])
        let image = try await SCScreenshotManager.captureImage(
            contentFilter: filter, configuration: config)

        // Encode the CGImage to PNG and write it.
        let bitmap = NSBitmapImageRep(cgImage: image)
        guard let png = bitmap.representation(using: .png, properties: [:]) else {
            captureError = "PNG encode failed"
            sema.signal()
            return
        }
        try png.write(to: URL(fileURLWithPath: out))
    } catch {
        captureError = "capture failed or was denied: \(error.localizedDescription)"
    }
    sema.signal()
}

sema.wait()
if let err = captureError { fail(err) }
exit(0)
