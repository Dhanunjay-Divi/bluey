import AppKit
import Foundation

// MARK: - Protocol types matching crates/cue-core/src/overlay_ipc.rs

/// Inbound messages from daemon (tagged union via "type" field, snake_case).
private enum OverlayMessage {
    case sessionSwitched(sessionId: String?, title: String?)
    case listeningStateChanged(state: String)
    case transcriptPartial(source: String, text: String)
    case transcriptFinal(source: String, text: String)
    case ping
    case unknown
}

/// Outbound commands to daemon.
private struct IpcCommand: Encodable {
    var token: String?
    let type: String
    var payload: String?
}

// MARK: - JSON helpers

private func parseMessage(_ line: String) -> OverlayMessage {
    guard let data = line.data(using: .utf8),
          let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
          let type = obj["type"] as? String else {
        return .unknown
    }
    switch type {
    case "session_switched":
        return .sessionSwitched(
            sessionId: obj["session_id"] as? String,
            title: obj["title"] as? String
        )
    case "listening_state_changed":
        return .listeningStateChanged(state: obj["state"] as? String ?? "idle")
    case "transcript_partial":
        return .transcriptPartial(
            source: obj["source"] as? String ?? "",
            text: obj["text"] as? String ?? ""
        )
    case "transcript_final":
        return .transcriptFinal(
            source: obj["source"] as? String ?? "",
            text: obj["text"] as? String ?? ""
        )
    case "ping":
        return .ping
    default:
        return .unknown
    }
}

private let sessionToken: String? = ProcessInfo.processInfo.environment["BLUEY_OVERLAY_SESSION_TOKEN"]

private func sendCommand(_ cmd: IpcCommand) {
    var cmdWithToken = cmd
    cmdWithToken.token = sessionToken
    guard let data = try? JSONEncoder().encode(cmdWithToken),
          var json = String(data: data, encoding: .utf8) else { return }
    json += "\n"
    FileHandle.standardOutput.write(json.data(using: .utf8)!)
}

// MARK: - Overlay state

private final class OverlayState {
    var partialText: String = ""
    var finalText: String = ""
    var bannerText: String? = nil
    var bannerExpiry: Date? = nil
}

private let state = OverlayState()

// MARK: - Overlay view

private final class TranscriptView: NSView {
    override func draw(_ dirtyRect: NSRect) {
        NSColor.clear.setFill()
        dirtyRect.fill()

        let bg = NSColor(white: 0.0, alpha: 0.72)
        let path = NSBezierPath(roundedRect: bounds, xRadius: 10, yRadius: 10)
        bg.setFill()
        path.fill()

        let inset = bounds.insetBy(dx: 12, dy: 8)

        // Session banner (3s)
        if let banner = state.bannerText,
           let expiry = state.bannerExpiry, Date() < expiry {
            let attrs: [NSAttributedString.Key: Any] = [
                .font: NSFont.boldSystemFont(ofSize: 13),
                .foregroundColor: NSColor(red: 0.4, green: 0.9, blue: 1.0, alpha: 1.0),
            ]
            (banner as NSString).draw(in: inset, withAttributes: attrs)
            return
        }

        var y = inset.maxY

        // Final text (normal weight, white)
        if !state.finalText.isEmpty {
            let attrs: [NSAttributedString.Key: Any] = [
                .font: NSFont.systemFont(ofSize: 13, weight: .medium),
                .foregroundColor: NSColor.white,
            ]
            let size = (state.finalText as NSString).boundingRect(
                with: NSSize(width: inset.width, height: 40),
                options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine],
                attributes: attrs
            )
            y -= size.height
            let rect = NSRect(x: inset.minX, y: y, width: inset.width, height: size.height)
            (state.finalText as NSString).draw(in: rect, withAttributes: attrs)
            y -= 2
        }

        // Partial text (italic, dim)
        if !state.partialText.isEmpty {
            let font = NSFontManager.shared.convert(
                NSFont.systemFont(ofSize: 12), toHaveTrait: .italicFontMask
            )
            let attrs: [NSAttributedString.Key: Any] = [
                .font: font,
                .foregroundColor: NSColor(white: 1.0, alpha: 0.6),
            ]
            let size = (state.partialText as NSString).boundingRect(
                with: NSSize(width: inset.width, height: 30),
                options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine],
                attributes: attrs
            )
            y -= size.height
            let rect = NSRect(x: inset.minX, y: y, width: inset.width, height: size.height)
            (state.partialText as NSString).draw(in: rect, withAttributes: attrs)
        }
    }
}

// MARK: - App setup

private let app = NSApplication.shared

// MARK: - Stealth

// Hide from Dock and Cmd+Tab — .accessory activation policy means no Dock
// icon, no main menu bar, and no entry in the application switcher. The
// overlay is invisible in the macOS UI chrome.
app.setActivationPolicy(.accessory)

private let window: NSWindow = {
    let screen = NSScreen.main ?? NSScreen.screens[0]
    let size = NSSize(width: 400, height: 80)
    let origin = NSPoint(
        x: screen.visibleFrame.maxX - size.width - 16,
        y: screen.visibleFrame.minY + 16
    )
    let w = NSWindow(
        contentRect: NSRect(origin: origin, size: size),
        styleMask: .borderless,
        backing: .buffered,
        defer: false
    )
    w.level = .floating
    w.isOpaque = false
    w.backgroundColor = .clear
    w.hasShadow = false
    w.ignoresMouseEvents = true  // Click-through
    w.collectionBehavior = [.canJoinAllSpaces, .stationary]

    // Stealth: hide from screen recording, screenshots, and screen-share.
    // NSWindowSharingType.none excludes this window from all capture APIs
    // (ScreenCaptureKit, CGWindowListCreateImage, OBS, Zoom/Teams share).
    // The window is fully invisible in any recorded or shared output.
    w.sharingType = .none

    w.contentView = TranscriptView(frame: NSRect(origin: .zero, size: size))
    return w
}()

// MARK: - Stdin reader

private func startStdinReader() {
    let thread = Thread {
        let handle = FileHandle.standardInput
        var buffer = ""
        while true {
            let data = handle.availableData; guard !data.isEmpty else {
                // stdin EOF — daemon closed pipe, exit cleanly
                DispatchQueue.main.async { app.terminate(nil) }
                return
            }
            guard let chunk = String(data: data, encoding: .utf8) else { continue }
            buffer += chunk
            while let newline = buffer.firstIndex(of: "\n") {
                let line = String(buffer[buffer.startIndex..<newline])
                buffer = String(buffer[buffer.index(after: newline)...])
                let msg = parseMessage(line)
                DispatchQueue.main.async { handleMessage(msg, raw: line) }
            }
        }
    }
    thread.name = "stdin-reader"
    thread.start()
}

private func handleMessage(_ msg: OverlayMessage, raw: String) {
    switch msg {
    case .ping:
        sendCommand(IpcCommand(type: "pong"))
    case .transcriptPartial(_, let text):
        state.partialText = text
        window.contentView?.needsDisplay = true
    case .transcriptFinal(_, let text):
        state.finalText = text
        state.partialText = ""
        window.contentView?.needsDisplay = true
    case .sessionSwitched(_, let title):
        state.bannerText = "Session: \(title ?? "new session")"
        state.bannerExpiry = Date().addingTimeInterval(3.0)
        state.partialText = ""
        state.finalText = ""
        window.contentView?.needsDisplay = true
        // Clear banner after 3s
        DispatchQueue.main.asyncAfter(deadline: .now() + 3.1) {
            window.contentView?.needsDisplay = true
        }
    case .listeningStateChanged:
        break  // Future use
    case .unknown:
        break  // Gracefully ignore unknown variants
    }
}

// MARK: - Key monitor (ESC → RequestSync)

private func installKeyMonitor() {
    NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
        if event.keyCode == 53 {  // ESC
            sendCommand(IpcCommand(type: "request_sync"))
        }
        return event
    }
}

// MARK: - Entry point

window.orderFront(nil)
startStdinReader()
installKeyMonitor()
app.run()
