// bluey-overlay-macos / cue-overlay-macos
//
// Native macOS overlay process. Speaks NDJSON IPC over a local Unix socket
// when BLUEY_OVERLAY_SOCKET is set, with stdin/stdout retained for test stubs
// and manual protocol checks. Provides:
//
//   - A small top-center pill (collapsed default state), draggable, click to
//     expand into the full feed.
//   - A full feed/composer panel (expanded state) that renders the daemon's
//     CueCards, accepts Ask / Attach / Instructions / Recap input, and shows
//     streaming response chunks via UpdateCard.
//   - A boot card on startup driven by the daemon's Boot command.
//   - All windows are excluded from screen capture (capture_excluded=true)
//     and float above all spaces.
//
// Protocol is the canonical one defined in crates/cue-core/src/overlay.rs
// (OverlayCommand) and crates/cue-core/src/overlay_ipc.rs (OverlayEvent
// envelope wrapping the inner command). Every emitted line carries the
// per-session token from BLUEY_OVERLAY_SESSION_TOKEN so the daemon's
// production validator (validate_and_decode_overlay_line in app.rs) accepts
// our events.

import AppKit
import Darwin
import Foundation

// MARK: - Visual system

private enum BlueyTheme {
    static let cyan = NSColor(red: 0.35, green: 0.82, blue: 1.0, alpha: 1.0)
    static let cyanSoft = NSColor(red: 0.35, green: 0.82, blue: 1.0, alpha: 0.14)
    static let panel = NSColor(red: 0.025, green: 0.029, blue: 0.036, alpha: 0.95)
    static let panelDeep = NSColor(red: 0.015, green: 0.018, blue: 0.024, alpha: 0.97)
    static let surface = NSColor(red: 0.055, green: 0.065, blue: 0.080, alpha: 0.94)
    static let surfaceRaised = NSColor(red: 0.075, green: 0.090, blue: 0.110, alpha: 0.95)
    static let text = NSColor(red: 0.92, green: 0.98, blue: 1.0, alpha: 1.0)
    static let textDim = NSColor(red: 0.58, green: 0.66, blue: 0.72, alpha: 1.0)
    static let hairline = NSColor.white.withAlphaComponent(0.08)
    static let green = NSColor(red: 0.42, green: 1.0, blue: 0.52, alpha: 1.0)
    static let warning = NSColor(red: 1.0, green: 0.72, blue: 0.28, alpha: 1.0)

    static func accent(for kind: String) -> NSColor {
        switch kind {
        case "answer": return cyan
        case "question": return NSColor(red: 0.58, green: 0.70, blue: 1.0, alpha: 1.0)
        case "transcript": return NSColor(red: 0.48, green: 1.0, blue: 0.72, alpha: 1.0)
        case "context": return NSColor(red: 0.78, green: 0.66, blue: 1.0, alpha: 1.0)
        case "action_item": return green
        case "decision": return NSColor(red: 0.72, green: 0.86, blue: 1.0, alpha: 1.0)
        case "warning": return warning
        default: return cyan
        }
    }
}

private enum ExpandedPanelMetrics {
    static let maxCompactWidth: CGFloat = 820
    static let minCompactWidth: CGFloat = 680
    static let maxCanvasWidth: CGFloat = 960
    static let height: CGFloat = 520
    static let screenInset: CGFloat = 12

    static func fittingWidth(for screen: NSRect, preferred: CGFloat) -> CGFloat {
        let available = max(360, screen.width - screenInset * 2)
        return min(preferred, available)
    }

    static func fittingMinimumWidth(for screen: NSRect, targetWidth: CGFloat) -> CGFloat {
        let available = max(360, screen.width - screenInset * 2)
        return min(minCompactWidth, targetWidth, available)
    }
}

private func symbolImage(_ name: String) -> NSImage? {
    guard let image = NSImage(systemSymbolName: name, accessibilityDescription: nil) else {
        return nil
    }
    return image.withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 13, weight: .semibold)) ?? image
}

private final class ComposerTextView: NSTextView {
    var placeholder = "Ask anything..." {
        didSet { needsDisplay = true }
    }
    var onSubmit: (() -> Void)?
    var onMeasuredHeight: ((CGFloat) -> Void)?

    override init(frame frameRect: NSRect, textContainer container: NSTextContainer?) {
        super.init(frame: frameRect, textContainer: container)
        drawsBackground = false
        isRichText = false
        isAutomaticQuoteSubstitutionEnabled = false
        isAutomaticDashSubstitutionEnabled = false
        isAutomaticTextReplacementEnabled = false
        isContinuousSpellCheckingEnabled = false
        textColor = BlueyTheme.text
        insertionPointColor = BlueyTheme.cyan
        font = NSFont.systemFont(ofSize: 14.5, weight: .medium)
        textContainerInset = NSSize(width: 2, height: 7)
        textContainer?.lineFragmentPadding = 0
        textContainer?.widthTracksTextView = true
        textContainer?.heightTracksTextView = false
        minSize = NSSize(width: 0, height: 0)
        maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        isHorizontallyResizable = false
        isVerticallyResizable = true
        autoresizingMask = [.width]
    }

    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard string.isEmpty else { return }
        let attributes: [NSAttributedString.Key: Any] = [
            .font: font ?? NSFont.systemFont(ofSize: 14.5, weight: .medium),
            .foregroundColor: BlueyTheme.textDim.withAlphaComponent(0.78),
        ]
        let rect = NSRect(x: 0, y: textContainerInset.height + 1, width: bounds.width, height: 22)
        placeholder.draw(in: rect, withAttributes: attributes)
    }

    override func didChangeText() {
        super.didChangeText()
        needsDisplay = true
        notifyMeasuredHeight()
    }

    override func layout() {
        super.layout()
        notifyMeasuredHeight()
    }

    override func keyDown(with event: NSEvent) {
        let isReturn = event.keyCode == 36 || event.keyCode == 76
        let wantsNewline = event.modifierFlags.contains(.shift)
            || event.modifierFlags.contains(.option)
            || event.modifierFlags.contains(.control)
        if isReturn && !wantsNewline {
            onSubmit?()
            return
        }
        super.keyDown(with: event)
    }

    func clearText() {
        string = ""
        selectedRange = NSRange(location: 0, length: 0)
        needsDisplay = true
        notifyMeasuredHeight()
    }

    private func notifyMeasuredHeight() {
        guard bounds.width > 24, let container = textContainer, let manager = layoutManager else {
            onMeasuredHeight?(44)
            return
        }
        container.containerSize = NSSize(width: max(80, bounds.width), height: CGFloat.greatestFiniteMagnitude)
        manager.ensureLayout(for: container)
        let used = manager.usedRect(for: container)
        let measured = ceil(used.height + textContainerInset.height * 2 + 6)
        onMeasuredHeight?(measured)
    }
}

// MARK: - Protocol

private struct CueCard: Decodable {
    let id: String
    let kind: String
    let title: String
    let body: String
    let createdAt: String?
    let source: String?
    let costLabel: String?
    let artifact: OverlayArtifact?

    enum CodingKeys: String, CodingKey {
        case id, kind, title, body
        case createdAt = "created_at"
        case source
        case costLabel = "cost_label"
        case artifact
    }
}

private struct OverlayArtifact: Decodable {
    let artifactType: String
    let title: String
    let body: String
    let confidence: Double?

    enum CodingKeys: String, CodingKey {
        case artifactType = "artifact_type"
        case title, body, confidence
    }
}

private struct OverlayContextItem {
    let id: String
    let title: String
    let kind: String
    let path: String?
}

private struct OverlaySessionItem {
    let id: String
    let title: String
    let subtitle: String
    let isActive: Bool
}

/// Inbound commands from the daemon.
private enum OverlayCommand {
    case ping
    case show
    case hide
    case toggle
    case clear
    case boot(title: String, lines: [String])
    case setOpacity(Double)
    case setPosition(String)
    case setBalance(String)
    case setContextItems([OverlayContextItem])
    case setSessions([OverlaySessionItem])
    case pushCard(CueCard)
    case updateCard(id: String, body: String, done: Bool, costLabel: String?, artifact: OverlayArtifact?)
    case shutdown
    case unknown(String)
}

private func parseCommand(_ line: String) -> OverlayCommand {
    guard let data = line.data(using: .utf8),
          let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
          let type = obj["type"] as? String
    else {
        return .unknown(line)
    }
    switch type {
    case "ping":     return .ping
    case "show":     return .show
    case "hide":     return .hide
    case "toggle":   return .toggle
    case "clear":    return .clear
    case "shutdown": return .shutdown
    case "boot":
        let title = obj["title"] as? String ?? ""
        let lines = obj["lines"] as? [String] ?? []
        return .boot(title: title, lines: lines)
    case "set_opacity":
        let opacity = (obj["opacity"] as? Double) ?? 1.0
        return .setOpacity(opacity)
    case "set_position":
        let position = obj["position"] as? String ?? "top_right"
        return .setPosition(position)
    case "set_balance":
        let label = obj["label"] as? String ?? "Balance --"
        return .setBalance(label)
    case "set_context_items":
        let rawItems = obj["items"] as? [[String: Any]] ?? []
        let items = rawItems.map { item in
            OverlayContextItem(
                id: item["id"] as? String ?? UUID().uuidString,
                title: item["title"] as? String ?? "Attached file",
                kind: item["kind"] as? String ?? "document",
                path: item["path"] as? String
            )
        }
        return .setContextItems(items)
    case "set_sessions":
        let rawSessions = obj["sessions"] as? [[String: Any]] ?? []
        let sessions = rawSessions.map { item in
            OverlaySessionItem(
                id: item["id"] as? String ?? "",
                title: item["title"] as? String ?? "Bluey session",
                subtitle: item["subtitle"] as? String ?? "",
                isActive: item["is_active"] as? Bool ?? false
            )
        }.filter { !$0.id.isEmpty }
        return .setSessions(sessions)
    case "push_card":
        guard let cardObj = obj["card"] as? [String: Any],
              let cardData = try? JSONSerialization.data(withJSONObject: cardObj),
              let card = try? JSONDecoder().decode(CueCard.self, from: cardData)
        else { return .unknown(line) }
        return .pushCard(card)
    case "update_card":
        let id = obj["id"] as? String ?? ""
        let body = obj["body"] as? String ?? ""
        let done = obj["done"] as? Bool ?? false
        let costLabel = obj["cost_label"] as? String
        var artifact: OverlayArtifact?
        if let artifactObj = obj["artifact"] as? [String: Any],
           let artifactData = try? JSONSerialization.data(withJSONObject: artifactObj) {
            artifact = try? JSONDecoder().decode(OverlayArtifact.self, from: artifactData)
        }
        return .updateCard(id: id, body: body, done: done, costLabel: costLabel, artifact: artifact)
    default:
        return .unknown(line)
    }
}

/// Outbound events to the daemon. Every event carries the session token
/// embedded as the top-level "token" field; the daemon's
/// validate_and_decode_overlay_line function rejects events without it.
private func argumentValue(_ name: String) -> String? {
    let args = CommandLine.arguments
    guard let idx = args.firstIndex(of: name),
          args.indices.contains(idx + 1)
    else {
        return nil
    }
    return args[idx + 1]
}

private func argumentFlag(_ name: String) -> Bool {
    CommandLine.arguments.contains(name)
}

private func envFlag(_ name: String) -> Bool {
    let raw = ProcessInfo.processInfo.environment[name]?
        .trimmingCharacters(in: .whitespacesAndNewlines)
        .lowercased()
    return raw == "1" || raw == "true" || raw == "yes" || raw == "on"
}

private let sessionToken: String = ProcessInfo.processInfo
    .environment["BLUEY_OVERLAY_SESSION_TOKEN"]
    ?? argumentValue("--bluey-overlay-session-token")
    ?? ""
private let overlaySocketPath: String? = argumentValue("--bluey-overlay-socket")
    ?? ProcessInfo.processInfo
        .environment["BLUEY_OVERLAY_SOCKET"]

private let ipcLock = NSLock()
private var ipcInputHandle: FileHandle?
private var ipcOutputHandle: FileHandle = FileHandle.standardOutput

private let captureVisibleForDebug: Bool = {
    let devEnabled = argumentFlag("--bluey-dev-overlay") || envFlag("BLUEY_DEV_OVERLAY")
    guard devEnabled else { return false }
    return argumentFlag("--bluey-overlay-capture-visible")
        || envFlag("BLUEY_OVERLAY_CAPTURE_VISIBLE")
        || envFlag("BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE")
}()

private func connectUnixSocket(path: String) -> Int32? {
    let fd = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else { return nil }

    var addr = sockaddr_un()
    addr.sun_family = sa_family_t(AF_UNIX)
    let bytes = Array(path.utf8CString)
    let maxPathBytes = MemoryLayout.size(ofValue: addr.sun_path)
    guard bytes.count <= maxPathBytes else {
        Darwin.close(fd)
        return nil
    }

    withUnsafeMutableBytes(of: &addr.sun_path) { rawBuffer in
        let dest = rawBuffer.bindMemory(to: CChar.self)
        for idx in bytes.indices {
            dest[idx] = bytes[idx]
        }
    }

    let result = withUnsafePointer(to: &addr) { pointer in
        pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
            Darwin.connect(fd, sockaddrPointer, socklen_t(MemoryLayout<sockaddr_un>.size))
        }
    }
    guard result == 0 else {
        Darwin.close(fd)
        return nil
    }

    return fd
}

private func connectIpcIfNeeded() {
    guard let path = overlaySocketPath,
          !path.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    else {
        return
    }

    guard let inputFd = connectUnixSocket(path: path) else {
        fputs("bluey-overlay: failed to connect IPC socket \(path)\n", stderr)
        return
    }
    let outputFd = Darwin.dup(inputFd)
    guard outputFd >= 0 else {
        Darwin.close(inputFd)
        fputs("bluey-overlay: failed to duplicate IPC socket fd\n", stderr)
        return
    }

    ipcInputHandle = FileHandle(fileDescriptor: inputFd, closeOnDealloc: true)
    ipcOutputHandle = FileHandle(fileDescriptor: outputFd, closeOnDealloc: true)
}

private func emitEvent(_ payload: [String: Any]) {
    var withToken = payload
    if !sessionToken.isEmpty {
        withToken["token"] = sessionToken
    }
    guard let data = try? JSONSerialization.data(withJSONObject: withToken),
          let json = String(data: data, encoding: .utf8)
    else { return }
    guard let line = (json + "\n").data(using: .utf8) else { return }
    ipcLock.lock()
    ipcOutputHandle.write(line)
    ipcLock.unlock()
}

private func emitReady() {
    emitEvent([
        "type": "ready",
        "platform": "macos",
        "capture_excluded": !captureVisibleForDebug,
    ])
}

private func emitSimple(_ type: String) {
    emitEvent(["type": type])
}

private func emitLifecycle(_ stage: String, status: String = "ok", detail: String? = nil) {
    var payload: [String: Any] = [
        "type": "lifecycle",
        "stage": stage,
        "status": status,
    ]
    if let detail, !detail.isEmpty {
        payload["detail"] = detail
    }
    emitEvent(payload)
}

private func emitAsk(question: String, provider: String?, model: String?, mode: String?) {
    var p: [String: Any] = ["type": "ask_requested", "question": question]
    if let provider = provider { p["provider"] = provider }
    if let model = model       { p["model"]    = model }
    if let mode = mode         { p["mode"]     = mode }
    emitEvent(p)
}

private func emitAttachFiles(paths: [String]) {
    emitEvent(["type": "attach_files_requested", "paths": paths])
}

private func emitInstructions(text: String) {
    emitEvent(["type": "instructions_updated", "text": text])
}

private func emitSessionOpen(id: String) {
    emitEvent(["type": "session_open_requested", "id": id])
}

private func emitSessionRename(id: String, title: String) {
    emitEvent(["type": "session_rename_requested", "id": id, "title": title])
}

private func emitCardRendered(id: String) {
    emitEvent(["type": "card_rendered", "id": id])
}

// MARK: - Overlay NSWindow

/// Borderless, transparent, always-on-top overlay window.
/// Configured for either the small pill or the expanded feed depending on
/// the size passed at construction time.
private final class OverlayWindow: NSWindow {
    var lockedFrameHeight: CGFloat?
    var minimumFrameWidth: CGFloat?
    var maximumFrameWidth: CGFloat?

    init(contentRect: NSRect, draggable: Bool) {
        super.init(
            contentRect: contentRect,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        self.isOpaque = false
        self.backgroundColor = .clear
        self.hasShadow = true
        self.isReleasedWhenClosed = false
        self.level = .floating
        self.collectionBehavior = [
            .canJoinAllSpaces,
            .stationary,
            .ignoresCycle,
            .fullScreenAuxiliary,
        ]
        self.isMovableByWindowBackground = draggable
        self.hidesOnDeactivate = false
        // Production keeps the overlay out of screen capture. Capture-visible
        // QA is dev-gated and must never be enabled in customer launch paths.
        self.sharingType = captureVisibleForDebug ? .readOnly : .none
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }

    override func setFrame(_ frameRect: NSRect, display displayFlag: Bool) {
        super.setFrame(clampedFrame(frameRect), display: displayFlag)
    }

    override func setFrame(_ frameRect: NSRect, display displayFlag: Bool, animate animateFlag: Bool) {
        super.setFrame(clampedFrame(frameRect), display: displayFlag, animate: animateFlag)
    }

    override func setContentSize(_ size: NSSize) {
        if lockedFrameHeight != nil || minimumFrameWidth != nil {
            super.setFrame(
                clampedFrame(NSRect(origin: frame.origin, size: size)),
                display: true)
        } else {
            super.setContentSize(size)
        }
    }

    private func clampedFrame(_ frame: NSRect) -> NSRect {
        var clamped = frame
        if let minimumFrameWidth {
            clamped.size.width = max(minimumFrameWidth, clamped.size.width)
        }
        if let maximumFrameWidth {
            clamped.size.width = min(maximumFrameWidth, clamped.size.width)
        }
        if let lockedFrameHeight {
            clamped.size.height = lockedFrameHeight
        }

        guard lockedFrameHeight != nil || minimumFrameWidth != nil || maximumFrameWidth != nil else {
            return clamped
        }

        let inset: CGFloat = 12
        let visibleFrame = screen?.visibleFrame
            ?? NSScreen.main?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let screenMaxWidth = max(360, visibleFrame.width - inset * 2)
        clamped.size.width = min(clamped.size.width, screenMaxWidth)
        if let minimumFrameWidth, minimumFrameWidth <= screenMaxWidth {
            clamped.size.width = max(minimumFrameWidth, clamped.size.width)
        }
        clamped.origin.x = min(
            max(visibleFrame.minX + inset, clamped.origin.x),
            visibleFrame.maxX - clamped.size.width - inset)
        clamped.origin.y = min(
            max(visibleFrame.minY + inset, clamped.origin.y),
            visibleFrame.maxY - clamped.size.height - inset)
        return clamped
    }
}

// MARK: - Pill view

private final class PillView: NSView {
    var statusText: String = "Bluey" {
        didSet {
            updateTitleDisplay()
            needsDisplay = true
        }
    }
    /// Codex Stage 19 commit 1 follow-up: live balance text rendered
    /// next to status. Daemon sends "$4.98" (or "$4.98 low" when below
    /// auto-topup threshold). Empty string clears.
    var balanceText: String = "" {
        didSet {
            updateTitleDisplay()
            needsDisplay = true
        }
    }
    private func updateTitleDisplay() {
        let trimmed = balanceText.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            titleField.stringValue = statusText
            titleField.textColor = NSColor(red: 0.92, green: 0.98, blue: 1.0, alpha: 1.0)
        } else {
            // Compose "<status> · <balance>" but use orange tint when
            // the daemon flagged it as low.
            let isLow = trimmed.lowercased().hasSuffix("low")
            let displayBalance = isLow
                ? trimmed.replacingOccurrences(of: " low", with: "")
                    .replacingOccurrences(of: " LOW", with: "")
                : trimmed
            titleField.stringValue = "\(statusText) · \(displayBalance)"
            titleField.textColor = isLow
                ? NSColor(red: 0.98, green: 0.74, blue: 0.34, alpha: 1.0)
                : NSColor(red: 0.92, green: 0.98, blue: 1.0, alpha: 1.0)
        }
    }
    func setBalanceLabel(_ label: String) {
        balanceText = label
    }
    var dotColor: NSColor = NSColor.systemGreen {
        didSet {
            dotView.layer?.backgroundColor = dotColor.cgColor
            needsDisplay = true
        }
    }
    var onClick: (() -> Void)?

    private let logoTile = NSView()
    private let logoGlyph = NSTextField(labelWithString: ">_")
    private let titleField = NSTextField(labelWithString: "Bluey")
    private let dotView = NSView()

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor
        layer?.cornerRadius = frameRect.height / 2
        layer?.borderWidth = 0
        layer?.shadowColor = NSColor(red: 0.10, green: 0.70, blue: 0.96, alpha: 1.0).cgColor
        layer?.shadowOpacity = 0.18
        layer?.shadowRadius = 7
        layer?.shadowOffset = .zero

        logoTile.wantsLayer = true
        logoTile.layer?.backgroundColor = NSColor(red: 0.025, green: 0.140, blue: 0.190, alpha: 1.0).cgColor
        logoTile.layer?.cornerRadius = 6
        logoTile.layer?.borderWidth = 1
        logoTile.layer?.borderColor = NSColor(red: 0.42, green: 0.92, blue: 1.0, alpha: 0.72).cgColor
        logoTile.layer?.shadowColor = NSColor(red: 0.15, green: 0.66, blue: 1.0, alpha: 1.0).cgColor
        logoTile.layer?.shadowOpacity = 0.22
        logoTile.layer?.shadowRadius = 5
        logoTile.layer?.shadowOffset = .zero
        addSubview(logoTile)

        logoGlyph.font = NSFont.monospacedSystemFont(ofSize: 9.5, weight: .bold)
        logoGlyph.textColor = NSColor(red: 0.92, green: 0.98, blue: 1.0, alpha: 1.0)
        logoGlyph.alignment = .center
        logoTile.addSubview(logoGlyph)

        titleField.font = NSFont.systemFont(ofSize: 12.5, weight: .semibold)
        titleField.textColor = NSColor(red: 0.92, green: 0.98, blue: 1.0, alpha: 1.0)
        titleField.alignment = .left
        addSubview(titleField)

        dotView.wantsLayer = true
        dotView.layer?.backgroundColor = dotColor.cgColor
        dotView.layer?.cornerRadius = 3
        dotView.layer?.shadowColor = dotColor.cgColor
        dotView.layer?.shadowOpacity = 0.62
        dotView.layer?.shadowRadius = 6
        dotView.layer?.shadowOffset = .zero
        addSubview(dotView)
    }
    required init?(coder: NSCoder) { fatalError() }

    override func layout() {
        super.layout()
        layer?.cornerRadius = bounds.height / 2

        let logoSide: CGFloat = 20
        logoTile.frame = NSRect(x: 7, y: (bounds.height - logoSide) / 2, width: logoSide, height: logoSide)
        logoTile.layer?.cornerRadius = 6
        logoGlyph.frame = logoTile.bounds.insetBy(dx: 3, dy: 5)

        titleField.frame = NSRect(x: 34, y: (bounds.height - 17) / 2 + 1, width: bounds.width - 50, height: 17)

        let labelWidth = ceil((titleField.stringValue as NSString).size(withAttributes: [
            .font: titleField.font ?? NSFont.systemFont(ofSize: 12.5, weight: .semibold),
        ]).width)
        let dotSize: CGFloat = 6
        let dotX = min(titleField.frame.minX + labelWidth + 4, bounds.width - dotSize - 9)
        dotView.frame = NSRect(x: dotX, y: bounds.midY + 3, width: dotSize, height: dotSize)
        dotView.layer?.cornerRadius = dotSize / 2
    }

    override func draw(_ dirtyRect: NSRect) {
        NSGraphicsContext.saveGraphicsState()

        let outer = bounds.insetBy(dx: 1, dy: 1)
        let radius = outer.height / 2
        let path = NSBezierPath(roundedRect: outer, xRadius: radius, yRadius: radius)
        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(0.38)
        shadow.shadowBlurRadius = 9
        shadow.shadowOffset = .zero
        shadow.set()

        let bg = NSGradient(colors: [
            NSColor(red: 0.012, green: 0.016, blue: 0.022, alpha: 0.97),
            NSColor(red: 0.018, green: 0.038, blue: 0.046, alpha: 0.94),
            NSColor(red: 0.011, green: 0.014, blue: 0.020, alpha: 0.98),
        ])
        bg?.draw(in: path, angle: -12)

        NSGraphicsContext.restoreGraphicsState()

        NSColor(red: 0.30, green: 0.78, blue: 0.96, alpha: 0.40).setStroke()
        path.lineWidth = 1
        path.stroke()

        let inner = outer.insetBy(dx: 2, dy: 2)
        let innerPath = NSBezierPath(roundedRect: inner, xRadius: inner.height / 2, yRadius: inner.height / 2)
        NSColor.white.withAlphaComponent(0.055).setStroke()
        innerPath.lineWidth = 1
        innerPath.stroke()

        let gloss = NSBezierPath(roundedRect: outer.insetBy(dx: 2, dy: 2), xRadius: radius - 2, yRadius: radius - 2)
        NSGradient(colors: [
            NSColor.white.withAlphaComponent(0.08),
            NSColor.white.withAlphaComponent(0.00),
        ])?.draw(in: gloss, angle: 90)
    }

    override func mouseDown(with event: NSEvent) {
        let startLocation = event.locationInWindow
        var didDrag = false
        var keepGoing = true
        while keepGoing {
            guard let next = window?.nextEvent(matching: [.leftMouseDragged, .leftMouseUp])
            else { break }
            switch next.type {
            case .leftMouseDragged:
                let dx = next.locationInWindow.x - startLocation.x
                let dy = next.locationInWindow.y - startLocation.y
                if abs(dx) > 4 || abs(dy) > 4 {
                    didDrag = true
                    window?.performDrag(with: event)
                    keepGoing = false
                }
            case .leftMouseUp:
                if !didDrag { onClick?() }
                keepGoing = false
            default:
                keepGoing = false
            }
        }
    }
}

// MARK: - Card feed view

private struct RenderedCard {
    let id: String
    let kind: String
    let title: String
    var body: String
    var done: Bool
    var costLabel: String?
    var artifact: OverlayArtifact?
}

private enum CanvasKind {
    case code
    case systemDesign
    case screen
    case document
    case structured

    static func fromArtifactType(_ value: String) -> CanvasKind {
        switch value {
        case "code": return .code
        case "system_design": return .systemDesign
        case "screen": return .screen
        case "document": return .document
        default: return .structured
        }
    }

    var title: String {
        switch self {
        case .code: return "Code canvas"
        case .systemDesign: return "System design canvas"
        case .screen: return "Screen analysis"
        case .document: return "Document notes"
        case .structured: return "Workspace"
        }
    }

    var subtitle: String {
        switch self {
        case .code: return "Code, tests, complexity"
        case .systemDesign: return "Architecture, tradeoffs, scale"
        case .screen: return "Screen context and answer"
        case .document: return "Attached context notes"
        case .structured: return "Structured workspace"
        }
    }

    var icon: String {
        switch self {
        case .code: return "curlybraces"
        case .systemDesign: return "square.stack.3d.up"
        case .screen: return "rectangle.and.text.magnifyingglass"
        case .document: return "doc.text"
        case .structured: return "sidebar.right"
        }
    }
}

private struct CanvasArtifact {
    let kind: CanvasKind
    let title: String
    let subtitle: String
    let content: String
    let sourceCardId: String
}

private final class FeedView: NSView {
    private var cards: [RenderedCard] = []
    private let stack = NSStackView()
    private let scroll = NSScrollView()
    private let emptyState = NSView()
    var onTranscript: ((RenderedCard) -> Void)?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.backgroundColor = BlueyTheme.panel.cgColor
        layer?.cornerRadius = 16
        layer?.borderWidth = 1
        layer?.borderColor = BlueyTheme.hairline.cgColor

        stack.orientation = .vertical
        stack.alignment = .centerX
        stack.spacing = 12
        stack.edgeInsets = NSEdgeInsets(top: 16, left: 0, bottom: 16, right: 0)
        stack.translatesAutoresizingMaskIntoConstraints = false

        scroll.hasVerticalScroller = true
        scroll.drawsBackground = false
        scroll.documentView = stack
        scroll.translatesAutoresizingMaskIntoConstraints = false
        addSubview(scroll)
        configureEmptyState()
        NSLayoutConstraint.activate([
            scroll.topAnchor.constraint(equalTo: topAnchor),
            scroll.leadingAnchor.constraint(equalTo: leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: bottomAnchor),
            stack.widthAnchor.constraint(equalTo: scroll.widthAnchor),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }

    func push(_ card: RenderedCard) {
        if card.kind == "transcript" {
            onTranscript?(card)
            emitCardRendered(id: card.id)
            return
        }
        cards.append(card)
        emptyState.isHidden = true
        let view = makeCardView(card)
        stack.addArrangedSubview(view)
        view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        scrollToBottom()
        emitCardRendered(id: card.id)
    }

    @discardableResult
    func update(id: String, body: String, done: Bool, costLabel: String?, artifact: OverlayArtifact?) -> RenderedCard? {
        guard let idx = cards.firstIndex(where: { $0.id == id }) else { return nil }
        cards[idx].body = body
        cards[idx].done = done
        if let costLabel {
            cards[idx].costLabel = costLabel
        }
        if let artifact {
            cards[idx].artifact = artifact
        }
        // Replace the corresponding subview.
        let existing = stack.arrangedSubviews[idx]
        stack.removeArrangedSubview(existing)
        existing.removeFromSuperview()
        let view = makeCardView(cards[idx])
        stack.insertArrangedSubview(view, at: idx)
        view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        scrollToBottom()
        return cards[idx]
    }

    func clear() {
        cards.removeAll()
        for v in stack.arrangedSubviews { v.removeFromSuperview() }
        emptyState.isHidden = false
    }

    private func configureEmptyState() {
        emptyState.translatesAutoresizingMaskIntoConstraints = false
        emptyState.wantsLayer = true
        emptyState.layer?.backgroundColor = NSColor.clear.cgColor
        addSubview(emptyState)

        let badge = NSTextField(labelWithString: "READY")
        badge.translatesAutoresizingMaskIntoConstraints = false
        badge.font = NSFont.monospacedSystemFont(ofSize: 10, weight: .bold)
        badge.textColor = BlueyTheme.cyan

        let title = NSTextField(labelWithString: "New recording")
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = NSFont.systemFont(ofSize: 22, weight: .bold)
        title.textColor = BlueyTheme.text
        title.alignment = .center

        let subtitle = NSTextField(labelWithString: "Audio, files, screen context, and answers stay in this session.")
        subtitle.translatesAutoresizingMaskIntoConstraints = false
        subtitle.font = NSFont.systemFont(ofSize: 12.5, weight: .medium)
        subtitle.textColor = BlueyTheme.textDim
        subtitle.alignment = .center
        subtitle.maximumNumberOfLines = 2
        subtitle.lineBreakMode = .byWordWrapping

        let chips = NSStackView()
        chips.translatesAutoresizingMaskIntoConstraints = false
        chips.orientation = .horizontal
        chips.alignment = .centerY
        chips.spacing = 8
        for label in ["Audio", "Files", "Screen", "Canvas"] {
            chips.addArrangedSubview(emptyChip(label))
        }

        emptyState.addSubview(badge)
        emptyState.addSubview(title)
        emptyState.addSubview(subtitle)
        emptyState.addSubview(chips)

        NSLayoutConstraint.activate([
            emptyState.centerXAnchor.constraint(equalTo: centerXAnchor),
            emptyState.centerYAnchor.constraint(equalTo: centerYAnchor, constant: -12),
            emptyState.widthAnchor.constraint(lessThanOrEqualTo: widthAnchor, multiplier: 0.76),

            badge.topAnchor.constraint(equalTo: emptyState.topAnchor),
            badge.centerXAnchor.constraint(equalTo: emptyState.centerXAnchor),

            title.topAnchor.constraint(equalTo: badge.bottomAnchor, constant: 8),
            title.leadingAnchor.constraint(equalTo: emptyState.leadingAnchor),
            title.trailingAnchor.constraint(equalTo: emptyState.trailingAnchor),

            subtitle.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 8),
            subtitle.leadingAnchor.constraint(equalTo: emptyState.leadingAnchor),
            subtitle.trailingAnchor.constraint(equalTo: emptyState.trailingAnchor),

            chips.topAnchor.constraint(equalTo: subtitle.bottomAnchor, constant: 16),
            chips.centerXAnchor.constraint(equalTo: emptyState.centerXAnchor),
            chips.bottomAnchor.constraint(equalTo: emptyState.bottomAnchor),
        ])
    }

    private func emptyChip(_ text: String) -> NSView {
        let chip = NSTextField(labelWithString: text)
        chip.translatesAutoresizingMaskIntoConstraints = false
        chip.font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
        chip.textColor = BlueyTheme.textDim
        chip.alignment = .center
        chip.wantsLayer = true
        chip.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.035).cgColor
        chip.layer?.cornerRadius = 10
        chip.layer?.borderWidth = 1
        chip.layer?.borderColor = BlueyTheme.hairline.cgColor
        NSLayoutConstraint.activate([
            chip.heightAnchor.constraint(equalToConstant: 24),
            chip.widthAnchor.constraint(greaterThanOrEqualToConstant: 58),
        ])
        return chip
    }

    private func makeCardView(_ card: RenderedCard) -> NSView {
        let accent = BlueyTheme.accent(for: card.kind)
        let rightAligned = isUserSide(card)
        let answerLike = card.kind == "answer"
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false

        let bubble = NSView()
        bubble.wantsLayer = true
        bubble.layer?.backgroundColor = rightAligned
            ? NSColor(red: 0.90, green: 0.93, blue: 0.95, alpha: 0.96).cgColor
            : (answerLike ? NSColor.clear : BlueyTheme.surface).cgColor
        bubble.layer?.cornerRadius = rightAligned ? 16 : 12
        bubble.layer?.borderWidth = answerLike ? 0 : 1
        bubble.layer?.borderColor = rightAligned
            ? NSColor.white.withAlphaComponent(0.20).cgColor
            : accent.withAlphaComponent(card.kind == "answer" ? 0.24 : 0.14).cgColor
        bubble.layer?.shadowColor = NSColor.black.cgColor
        bubble.layer?.shadowOpacity = answerLike ? 0 : 0.14
        bubble.layer?.shadowRadius = 10
        bubble.layer?.shadowOffset = NSSize(width: 0, height: -4)
        bubble.translatesAutoresizingMaskIntoConstraints = false

        let metaLabel = NSTextField(labelWithString: kindLabel(card))
        metaLabel.font = NSFont.systemFont(ofSize: 11, weight: .bold)
        metaLabel.textColor = rightAligned ? NSColor.black.withAlphaComponent(0.58) : accent
        metaLabel.translatesAutoresizingMaskIntoConstraints = false

        let titleText = displayTitle(for: card)
        let titleLabel = NSTextField(labelWithString: titleText)
        titleLabel.font = NSFont.systemFont(ofSize: 12.5, weight: .semibold)
        titleLabel.textColor = rightAligned ? NSColor.black.withAlphaComponent(0.74) : BlueyTheme.text
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.lineBreakMode = .byTruncatingTail

        let rawBody = card.body.isEmpty && !card.done ? "Thinking..." : card.body
        let bodyText = chatBody(for: card, rawBody: rawBody)
        let bodyLabel = NSTextField(wrappingLabelWithString: bodyText)
        bodyLabel.font = bodyFont(for: card)
        bodyLabel.textColor = rightAligned ? NSColor.black : BlueyTheme.text
        bodyLabel.translatesAutoresizingMaskIntoConstraints = false
        bodyLabel.preferredMaxLayoutWidth = rightAligned ? 360 : 480

        let statusLabel = NSTextField(labelWithString: statusText(for: card))
        statusLabel.font = NSFont.monospacedSystemFont(ofSize: 9.5, weight: .semibold)
        statusLabel.textColor = rightAligned ? NSColor.black.withAlphaComponent(0.46) : BlueyTheme.textDim
        statusLabel.translatesAutoresizingMaskIntoConstraints = false

        row.addSubview(bubble)
        bubble.addSubview(metaLabel)
        bubble.addSubview(titleLabel)
        bubble.addSubview(bodyLabel)
        bubble.addSubview(statusLabel)

        let leading = bubble.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 8)
        let trailing = bubble.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -8)
        if rightAligned {
            leading.priority = .defaultLow
            trailing.priority = .required
        } else {
            leading.priority = .required
            trailing.priority = .defaultLow
        }

        NSLayoutConstraint.activate([
            row.heightAnchor.constraint(greaterThanOrEqualTo: bubble.heightAnchor),
            bubble.topAnchor.constraint(equalTo: row.topAnchor),
            bubble.bottomAnchor.constraint(equalTo: row.bottomAnchor),
            leading,
            trailing,
            bubble.widthAnchor.constraint(lessThanOrEqualTo: row.widthAnchor, multiplier: answerLike ? 0.90 : (rightAligned ? 0.70 : 0.78)),
            bubble.widthAnchor.constraint(greaterThanOrEqualToConstant: answerLike ? 240 : 170),

            metaLabel.topAnchor.constraint(equalTo: bubble.topAnchor, constant: 10),
            metaLabel.leadingAnchor.constraint(equalTo: bubble.leadingAnchor, constant: 14),

            titleLabel.centerYAnchor.constraint(equalTo: metaLabel.centerYAnchor),
            titleLabel.leadingAnchor.constraint(equalTo: metaLabel.trailingAnchor, constant: 8),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: statusLabel.leadingAnchor, constant: -10),

            statusLabel.centerYAnchor.constraint(equalTo: metaLabel.centerYAnchor),
            statusLabel.trailingAnchor.constraint(equalTo: bubble.trailingAnchor, constant: -14),

            bodyLabel.topAnchor.constraint(equalTo: metaLabel.bottomAnchor, constant: 8),
            bodyLabel.leadingAnchor.constraint(equalTo: metaLabel.leadingAnchor),
            bodyLabel.trailingAnchor.constraint(equalTo: bubble.trailingAnchor, constant: -14),
            bodyLabel.bottomAnchor.constraint(equalTo: bubble.bottomAnchor, constant: -12),
        ])
        return row
    }

    private func kindLabel(_ card: RenderedCard) -> String {
        switch card.kind {
        case "answer":      return "BLUEY"
        case "question":    return "YOU"
        case "action_item": return "ACTION"
        case "decision":    return "DECISION"
        case "context":     return "CONTEXT"
        case "transcript":  return "TRANSCRIPT"
        case "warning":     return "WARNING"
        case "system":      return "SYSTEM"
        default:            return "BLUEY"
        }
    }

    private func displayTitle(for card: RenderedCard) -> String {
        switch card.kind {
        case "answer", "question", "transcript":
            return ""
        default:
            return card.title.isEmpty ? kindTitle(card.kind) : card.title
        }
    }

    private func isUserSide(_ card: RenderedCard) -> Bool {
        card.kind == "question" || card.kind == "transcript"
    }

    private func bodyFont(for card: RenderedCard) -> NSFont {
        return NSFont.systemFont(ofSize: 13.5, weight: .regular)
    }

    private func chatBody(for card: RenderedCard, rawBody: String) -> String {
        guard card.kind == "answer" else { return rawBody }

        if let artifact = card.artifact {
            if artifact.artifactType == "code" {
                let notes = stripFencedCode(from: rawBody)
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                return notes.isEmpty
                    ? "I opened the code in the canvas."
                    : notes + "\n\nCode opened in the canvas."
            }
            if rawBody.count > 1_100 {
                let prefix = String(rawBody.prefix(720))
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                return prefix + "\n\nFull \(artifact.title.lowercased()) opened in the canvas."
            }
        }

        if rawBody.contains("```") {
            let notes = stripFencedCode(from: rawBody)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            if notes.isEmpty {
                return "I opened the code in the canvas."
            }
            return notes + "\n\nCode opened in the canvas."
        }

        if rawBody.count > 1_100 && hasStructuredShape(rawBody) {
            let prefix = String(rawBody.prefix(720))
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return prefix + "\n\nFull structured version opened in the canvas."
        }

        return rawBody
    }

    private func stripFencedCode(from text: String) -> String {
        var lines: [String] = []
        var inFence = false
        for line in text.components(separatedBy: .newlines) {
            if line.trimmingCharacters(in: .whitespaces).hasPrefix("```") {
                inFence.toggle()
                continue
            }
            if !inFence {
                lines.append(line)
            }
        }
        return lines.joined(separator: "\n")
    }

    private func hasStructuredShape(_ text: String) -> Bool {
        let lines = text.components(separatedBy: .newlines)
        let structured = lines.filter { line in
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            return trimmed.hasPrefix("- ")
                || trimmed.hasPrefix("* ")
                || trimmed.hasPrefix("#")
                || trimmed.range(of: #"^\d+[\.\)]\s"#, options: .regularExpression) != nil
        }
        return structured.count >= 3
    }

    private func statusText(for card: RenderedCard) -> String {
        if !card.done { return "streaming..." }
        if let costLabel = card.costLabel, !costLabel.isEmpty { return costLabel }
        switch card.kind {
        case "answer":   return ""
        case "question": return "sent"
        default:         return ""
        }
    }

    private func kindTitle(_ kind: String) -> String {
        switch kind {
        case "answer":      return "Response"
        case "question":    return "Question"
        case "action_item": return "Action item"
        case "decision":    return "Decision"
        case "context":     return "Context attached"
        case "transcript":  return "Transcript"
        case "warning":     return "Needs attention"
        case "system":      return "Bluey"
        default:            return "Update"
        }
    }

    private func scrollToBottom() {
        DispatchQueue.main.async { [weak self] in
            guard let s = self else { return }
            let bottom = NSPoint(x: 0, y: max(0, s.stack.bounds.height - s.scroll.contentView.bounds.height))
            s.scroll.contentView.scroll(to: bottom)
            s.scroll.reflectScrolledClipView(s.scroll.contentView)
        }
    }
}

// MARK: - Canvas pane

private final class CanvasPaneView: NSView {
    private let header = NSView()
    private let iconView = NSImageView()
    private let titleLabel = NSTextField(labelWithString: "Workspace")
    private let subtitleLabel = NSTextField(labelWithString: "Structured output appears here")
    private let closeButton = NSButton(title: "", target: nil, action: nil)
    private let scroll = NSScrollView()
    private let textView = NSTextView()

    var onCollapse: (() -> Void)?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.backgroundColor = NSColor(red: 0.020, green: 0.024, blue: 0.031, alpha: 0.98).cgColor
        layer?.cornerRadius = 16
        layer?.borderWidth = 1
        layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.22).cgColor

        header.translatesAutoresizingMaskIntoConstraints = false
        iconView.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        subtitleLabel.translatesAutoresizingMaskIntoConstraints = false
        closeButton.translatesAutoresizingMaskIntoConstraints = false
        scroll.translatesAutoresizingMaskIntoConstraints = false

        addSubview(header)
        header.addSubview(iconView)
        header.addSubview(titleLabel)
        header.addSubview(subtitleLabel)
        header.addSubview(closeButton)
        addSubview(scroll)

        iconView.imageScaling = .scaleProportionallyDown
        iconView.contentTintColor = BlueyTheme.cyan

        titleLabel.font = NSFont.systemFont(ofSize: 12.5, weight: .bold)
        titleLabel.textColor = BlueyTheme.text
        subtitleLabel.font = NSFont.systemFont(ofSize: 10.5, weight: .medium)
        subtitleLabel.textColor = BlueyTheme.textDim
        subtitleLabel.lineBreakMode = .byTruncatingTail

        closeButton.isBordered = false
        closeButton.wantsLayer = true
        closeButton.layer?.cornerRadius = 12
        closeButton.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.035).cgColor
        closeButton.layer?.borderWidth = 1
        closeButton.layer?.borderColor = BlueyTheme.hairline.cgColor
        closeButton.contentTintColor = BlueyTheme.textDim
        if let image = symbolImage("chevron.right") {
            image.isTemplate = true
            closeButton.image = image
            closeButton.imagePosition = .imageOnly
        } else {
            closeButton.title = "<"
        }
        closeButton.toolTip = "Collapse canvas"
        closeButton.target = self
        closeButton.action = #selector(collapseClicked)

        textView.isEditable = false
        textView.isSelectable = true
        textView.drawsBackground = false
        textView.textColor = BlueyTheme.text
        textView.font = NSFont.monospacedSystemFont(ofSize: 12.2, weight: .regular)
        textView.textContainerInset = NSSize(width: 12, height: 12)
        textView.isHorizontallyResizable = true
        textView.isVerticallyResizable = true
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = false
        textView.textContainer?.containerSize = NSSize(
            width: CGFloat.greatestFiniteMagnitude,
            height: CGFloat.greatestFiniteMagnitude)

        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = true
        scroll.autohidesScrollers = true
        scroll.borderType = .noBorder
        scroll.documentView = textView
        scroll.scrollerStyle = .overlay

        NSLayoutConstraint.activate([
            header.topAnchor.constraint(equalTo: topAnchor, constant: 12),
            header.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            header.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            header.heightAnchor.constraint(equalToConstant: 48),

            iconView.leadingAnchor.constraint(equalTo: header.leadingAnchor),
            iconView.topAnchor.constraint(equalTo: header.topAnchor, constant: 3),
            iconView.widthAnchor.constraint(equalToConstant: 22),
            iconView.heightAnchor.constraint(equalToConstant: 22),

            closeButton.trailingAnchor.constraint(equalTo: header.trailingAnchor),
            closeButton.topAnchor.constraint(equalTo: header.topAnchor),
            closeButton.widthAnchor.constraint(equalToConstant: 26),
            closeButton.heightAnchor.constraint(equalToConstant: 26),

            titleLabel.leadingAnchor.constraint(equalTo: iconView.trailingAnchor, constant: 8),
            titleLabel.topAnchor.constraint(equalTo: header.topAnchor),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: closeButton.leadingAnchor, constant: -8),

            subtitleLabel.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            subtitleLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 3),
            subtitleLabel.trailingAnchor.constraint(equalTo: header.trailingAnchor),

            scroll.topAnchor.constraint(equalTo: header.bottomAnchor, constant: 8),
            scroll.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            scroll.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            scroll.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -8),
        ])
    }

    required init?(coder: NSCoder) { fatalError() }

    func render(_ artifact: CanvasArtifact) {
        titleLabel.stringValue = artifact.title
        subtitleLabel.stringValue = artifact.subtitle
        if let image = symbolImage(artifact.kind.icon) {
            image.isTemplate = true
            iconView.image = image
        }
        textView.string = artifact.content
        textView.scrollRangeToVisible(NSRange(location: 0, length: 0))
    }

    @objc private func collapseClicked() {
        onCollapse?()
    }
}

// MARK: - Expanded panel (feed + composer)

private final class ExpandedPanelView: NSView {
    let feed: FeedView
    let workspace: NSView
    let canvasPane: CanvasPaneView
    let headerBar: NSView
    let headerStack: NSStackView
    let brandStack: NSStackView
    let headerSpacer: NSView
    let titleLabel: NSTextField
    let statusLabel: NSTextField
    let modelMenu: NSPopUpButton
    let routeBadge: NSTextField
    let knowledgeBadge: NSTextField
    let balanceLabel: NSTextField
    let canvasToggleButton: NSButton
    let navButton: NSButton
    let newSessionButton: NSButton
    let sessionDrawer: NSView
    let drawerTitleLabel: NSTextField
    let drawerSubtitleLabel: NSTextField
    let latestSessionButton: NSButton
    let sessionScroll: NSScrollView
    let sessionStack: NSStackView
    let answerStyleLabel: NSTextField
    let answerStyleBox: NSTextField
    let answerStyleSaveButton: NSButton
    let transcriptStrip: NSView
    let transcriptActivityDot: NSView
    let transcriptStateLabel: NSTextField
    let transcriptScroll: NSScrollView
    let transcriptLabel: NSTextField
    let attachmentStrip: NSScrollView
    let attachmentStack: NSStackView
    let composerBar: NSView
    let composerSurface: NSView
    let composer: ComposerTextView
    let recordingButton: NSButton
    let askButton: NSButton
    let analyzeButton: NSButton
    let attachButton: NSButton
    let instructionsButton: NSButton
    let opacityControl: NSView
    let opacityLabel: NSTextField
    let opacitySlider: NSSlider
    let opacityValueLabel: NSTextField
    let hideButton: NSButton
    let closeButton: NSButton
    let closeConfirmOverlay: NSView
    let closeConfirmPanel: NSView
    let closeConfirmTitle: NSTextField
    let closeConfirmBody: NSTextField
    let closeConfirmCancelButton: NSButton
    let closeConfirmTurnOffButton: NSButton

    var onClose: (() -> Void)?
    var onOpacityChanged: ((Double) -> Void)?
    private var recordingActive = false
    private var transcriptSnippets: [String] = []
    private var sessionItems: [OverlaySessionItem] = []
    private var editingSessionId: String?
    private var renameField: NSTextField?
    private var canvasWidthConstraint: NSLayoutConstraint?
    private var composerBarHeightConstraint: NSLayoutConstraint?
    private var composerTextHeightConstraint: NSLayoutConstraint?
    private var latestCanvas: CanvasArtifact?
    private var canvasOpen = false
    override init(frame frameRect: NSRect) {
        feed = FeedView(frame: .zero)
        workspace = NSView()
        canvasPane = CanvasPaneView(frame: .zero)
        headerBar = NSView()
        headerStack = NSStackView()
        brandStack = NSStackView()
        headerSpacer = NSView()
        titleLabel = NSTextField(labelWithString: "Bluey")
        statusLabel = NSTextField(labelWithString: "New recording")
        modelMenu = NSPopUpButton(frame: .zero, pullsDown: false)
        routeBadge = NSTextField(labelWithString: "Auto · ready")
        knowledgeBadge = NSTextField(labelWithString: "KB empty")
        balanceLabel = NSTextField(labelWithString: "Balance --")
        canvasToggleButton = NSButton(title: "", target: nil, action: nil)
        navButton = NSButton(title: "", target: nil, action: nil)
        newSessionButton = NSButton(title: "", target: nil, action: nil)
        sessionDrawer = NSView()
        drawerTitleLabel = NSTextField(labelWithString: "Recordings")
        drawerSubtitleLabel = NSTextField(labelWithString: "Click to continue. Pencil to rename.")
        latestSessionButton = NSButton(title: "Continue latest", target: nil, action: nil)
        sessionScroll = NSScrollView()
        sessionStack = NSStackView()
        answerStyleLabel = NSTextField(labelWithString: "How Bluey should answer")
        answerStyleBox = NSTextField()
        answerStyleSaveButton = NSButton(title: "Save", target: nil, action: nil)
        transcriptStrip = NSView()
        transcriptActivityDot = NSView()
        transcriptStateLabel = NSTextField(labelWithString: "IDLE")
        transcriptScroll = NSScrollView()
        transcriptLabel = NSTextField(labelWithString: "Live captions preview")
        attachmentStrip = NSScrollView()
        attachmentStack = NSStackView()
        composerBar = NSView()
        composerSurface = NSView()
        composer = ComposerTextView(frame: .zero, textContainer: nil)
        recordingButton = NSButton(title: "Listen", target: nil, action: nil)
        askButton = NSButton(title: "", target: nil, action: nil)
        analyzeButton = NSButton(title: "Screen", target: nil, action: nil)
        attachButton = NSButton(title: "", target: nil, action: nil)
        instructionsButton = NSButton(title: "Style", target: nil, action: nil)
        opacityControl = NSView()
        opacityLabel = NSTextField(labelWithString: "Opacity")
        opacitySlider = NSSlider(value: 0.94, minValue: 0.50, maxValue: 1.0, target: nil, action: nil)
        opacityValueLabel = NSTextField(labelWithString: "94%")
        hideButton = NSButton(title: "", target: nil, action: nil)
        closeButton = NSButton(title: "x", target: nil, action: nil)
        closeConfirmOverlay = NSView()
        closeConfirmPanel = NSView()
        closeConfirmTitle = NSTextField(labelWithString: "Turn Bluey off?")
        closeConfirmBody = NSTextField(wrappingLabelWithString: "This closes Bluey completely. To start again, run: bluey on")
        closeConfirmCancelButton = NSButton(title: "Cancel", target: nil, action: nil)
        closeConfirmTurnOffButton = NSButton(title: "Turn Off", target: nil, action: nil)

        super.init(frame: frameRect)

        wantsLayer = true
        layer?.backgroundColor = BlueyTheme.panelDeep.cgColor
        layer?.cornerRadius = 22
        layer?.borderWidth = 1
        layer?.borderColor = BlueyTheme.hairline.cgColor

        configureHeader()
        configureContextRows()
        configureComposer()
        configureCloseConfirm()
        styleDrawer()
        feed.onTranscript = { [weak self] card in
            self?.appendTranscriptSnippet(card)
        }

        for view in [
            headerBar,
            headerStack,
            brandStack,
            headerSpacer,
            titleLabel,
            statusLabel,
            modelMenu,
            routeBadge,
            knowledgeBadge,
            balanceLabel,
            canvasToggleButton,
            navButton,
            newSessionButton,
            sessionDrawer,
            drawerTitleLabel,
            drawerSubtitleLabel,
            latestSessionButton,
            sessionScroll,
            sessionStack,
            answerStyleLabel,
            answerStyleBox,
            answerStyleSaveButton,
            workspace,
            feed,
            canvasPane,
            transcriptStrip,
            transcriptActivityDot,
            transcriptStateLabel,
            transcriptScroll,
            transcriptLabel,
            attachmentStrip,
            attachmentStack,
            composerBar,
            composerSurface,
            composer,
            recordingButton,
            askButton,
            analyzeButton,
            attachButton,
            instructionsButton,
            opacityControl,
            opacityLabel,
            opacitySlider,
            opacityValueLabel,
            hideButton,
            closeButton,
            closeConfirmOverlay,
            closeConfirmPanel,
            closeConfirmTitle,
            closeConfirmBody,
            closeConfirmCancelButton,
            closeConfirmTurnOffButton,
        ] {
            view.translatesAutoresizingMaskIntoConstraints = false
        }

        headerBar.addSubview(headerStack)
        brandStack.addArrangedSubview(titleLabel)
        brandStack.addArrangedSubview(statusLabel)
        for view in [
            navButton,
            newSessionButton,
            brandStack,
            routeBadge,
            knowledgeBadge,
            headerSpacer,
            canvasToggleButton,
            balanceLabel,
            hideButton,
            closeButton,
        ] {
            headerStack.addArrangedSubview(view)
        }
        addSubview(workspace)
        workspace.addSubview(feed)
        workspace.addSubview(canvasPane)
        addSubview(sessionDrawer)
        sessionDrawer.addSubview(drawerTitleLabel)
        sessionDrawer.addSubview(drawerSubtitleLabel)
        sessionDrawer.addSubview(latestSessionButton)
        sessionDrawer.addSubview(sessionScroll)
        sessionDrawer.addSubview(answerStyleLabel)
        sessionDrawer.addSubview(answerStyleBox)
        sessionDrawer.addSubview(answerStyleSaveButton)
        addSubview(transcriptStrip)
        transcriptStrip.addSubview(transcriptActivityDot)
        transcriptStrip.addSubview(transcriptStateLabel)
        transcriptStrip.addSubview(transcriptScroll)
        transcriptScroll.documentView = transcriptLabel
        transcriptLabel.translatesAutoresizingMaskIntoConstraints = true
        addSubview(attachmentStrip)
        addSubview(composerBar)
        composerBar.addSubview(composerSurface)
        composerSurface.addSubview(composer)
        composerBar.addSubview(attachButton)
        composerBar.addSubview(instructionsButton)
        composerBar.addSubview(recordingButton)
        composerBar.addSubview(opacityControl)
        opacityControl.addSubview(opacityLabel)
        opacityControl.addSubview(opacitySlider)
        opacityControl.addSubview(opacityValueLabel)
        composerBar.addSubview(modelMenu)
        composerBar.addSubview(analyzeButton)
        composerBar.addSubview(askButton)
        addSubview(headerBar, positioned: .above, relativeTo: nil)
        addSubview(closeConfirmOverlay)
        closeConfirmOverlay.addSubview(closeConfirmPanel)
        closeConfirmPanel.addSubview(closeConfirmTitle)
        closeConfirmPanel.addSubview(closeConfirmBody)
        closeConfirmPanel.addSubview(closeConfirmCancelButton)
        closeConfirmPanel.addSubview(closeConfirmTurnOffButton)
        // Keep the fixed chrome rows above transparent scroll/canvas surfaces
        // even when AppKit re-lays out the dense center workspace.
        headerBar.layer?.zPosition = 50
        transcriptStrip.layer?.zPosition = 40
        attachmentStrip.layer?.zPosition = 40
        composerBar.layer?.zPosition = 50

        let canvasWidth = canvasPane.widthAnchor.constraint(equalToConstant: 0)
        canvasWidthConstraint = canvasWidth
        let composerTextHeight = composerSurface.heightAnchor.constraint(equalToConstant: 46)
        let composerBarHeight = composerBar.heightAnchor.constraint(equalToConstant: 108)
        composerTextHeightConstraint = composerTextHeight
        composerBarHeightConstraint = composerBarHeight

        NSLayoutConstraint.activate([
            headerBar.topAnchor.constraint(equalTo: topAnchor, constant: 10),
            headerBar.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            headerBar.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            headerBar.heightAnchor.constraint(equalToConstant: 42),

            headerStack.leadingAnchor.constraint(equalTo: headerBar.leadingAnchor, constant: 9),
            headerStack.trailingAnchor.constraint(equalTo: headerBar.trailingAnchor, constant: -9),
            headerStack.topAnchor.constraint(equalTo: headerBar.topAnchor, constant: 4),
            headerStack.bottomAnchor.constraint(equalTo: headerBar.bottomAnchor, constant: -4),

            navButton.widthAnchor.constraint(equalToConstant: 30),
            navButton.heightAnchor.constraint(equalToConstant: 30),

            newSessionButton.widthAnchor.constraint(equalToConstant: 30),
            newSessionButton.heightAnchor.constraint(equalToConstant: 30),

            brandStack.widthAnchor.constraint(greaterThanOrEqualToConstant: 110),
            brandStack.widthAnchor.constraint(lessThanOrEqualToConstant: 162),

            routeBadge.widthAnchor.constraint(greaterThanOrEqualToConstant: 96),
            routeBadge.widthAnchor.constraint(lessThanOrEqualToConstant: 142),
            routeBadge.heightAnchor.constraint(equalToConstant: 26),

            knowledgeBadge.widthAnchor.constraint(greaterThanOrEqualToConstant: 94),
            knowledgeBadge.widthAnchor.constraint(lessThanOrEqualToConstant: 136),
            knowledgeBadge.heightAnchor.constraint(equalToConstant: 26),

            closeButton.widthAnchor.constraint(equalToConstant: 26),
            closeButton.heightAnchor.constraint(equalToConstant: 26),

            hideButton.widthAnchor.constraint(equalToConstant: 26),
            hideButton.heightAnchor.constraint(equalToConstant: 26),

            balanceLabel.widthAnchor.constraint(greaterThanOrEqualToConstant: 84),
            balanceLabel.widthAnchor.constraint(lessThanOrEqualToConstant: 116),
            balanceLabel.heightAnchor.constraint(equalToConstant: 26),

            canvasToggleButton.widthAnchor.constraint(equalToConstant: 30),
            canvasToggleButton.heightAnchor.constraint(equalToConstant: 30),

            workspace.topAnchor.constraint(equalTo: headerBar.bottomAnchor, constant: 8),
            workspace.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            workspace.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            workspace.bottomAnchor.constraint(equalTo: transcriptStrip.topAnchor, constant: -8),

            feed.topAnchor.constraint(equalTo: workspace.topAnchor),
            feed.leadingAnchor.constraint(equalTo: workspace.leadingAnchor),
            feed.bottomAnchor.constraint(equalTo: workspace.bottomAnchor),
            feed.widthAnchor.constraint(greaterThanOrEqualToConstant: 260),

            canvasPane.topAnchor.constraint(equalTo: workspace.topAnchor),
            canvasPane.leadingAnchor.constraint(equalTo: feed.trailingAnchor, constant: 8),
            canvasPane.trailingAnchor.constraint(equalTo: workspace.trailingAnchor),
            canvasPane.bottomAnchor.constraint(equalTo: workspace.bottomAnchor),
            canvasWidth,

            sessionDrawer.topAnchor.constraint(equalTo: feed.topAnchor, constant: 10),
            sessionDrawer.leadingAnchor.constraint(equalTo: feed.leadingAnchor, constant: 10),
            sessionDrawer.widthAnchor.constraint(equalToConstant: 190),
            sessionDrawer.bottomAnchor.constraint(equalTo: transcriptStrip.topAnchor, constant: -10),

            drawerTitleLabel.topAnchor.constraint(equalTo: sessionDrawer.topAnchor, constant: 14),
            drawerTitleLabel.leadingAnchor.constraint(equalTo: sessionDrawer.leadingAnchor, constant: 14),
            drawerTitleLabel.trailingAnchor.constraint(equalTo: sessionDrawer.trailingAnchor, constant: -14),

            drawerSubtitleLabel.topAnchor.constraint(equalTo: drawerTitleLabel.bottomAnchor, constant: 4),
            drawerSubtitleLabel.leadingAnchor.constraint(equalTo: drawerTitleLabel.leadingAnchor),
            drawerSubtitleLabel.trailingAnchor.constraint(equalTo: drawerTitleLabel.trailingAnchor),

            latestSessionButton.topAnchor.constraint(equalTo: drawerSubtitleLabel.bottomAnchor, constant: 16),
            latestSessionButton.leadingAnchor.constraint(equalTo: sessionDrawer.leadingAnchor, constant: 12),
            latestSessionButton.trailingAnchor.constraint(equalTo: sessionDrawer.trailingAnchor, constant: -12),
            latestSessionButton.heightAnchor.constraint(equalToConstant: 32),

            sessionScroll.topAnchor.constraint(equalTo: latestSessionButton.bottomAnchor, constant: 10),
            sessionScroll.leadingAnchor.constraint(equalTo: sessionDrawer.leadingAnchor, constant: 8),
            sessionScroll.trailingAnchor.constraint(equalTo: sessionDrawer.trailingAnchor, constant: -8),
            sessionScroll.bottomAnchor.constraint(equalTo: answerStyleLabel.topAnchor, constant: -12),

            sessionStack.leadingAnchor.constraint(equalTo: sessionScroll.contentView.leadingAnchor),
            sessionStack.topAnchor.constraint(equalTo: sessionScroll.contentView.topAnchor),
            sessionStack.trailingAnchor.constraint(equalTo: sessionScroll.contentView.trailingAnchor),
            sessionStack.bottomAnchor.constraint(lessThanOrEqualTo: sessionScroll.contentView.bottomAnchor),
            sessionStack.widthAnchor.constraint(equalTo: sessionScroll.widthAnchor),

            answerStyleLabel.leadingAnchor.constraint(equalTo: sessionDrawer.leadingAnchor, constant: 12),
            answerStyleLabel.trailingAnchor.constraint(equalTo: sessionDrawer.trailingAnchor, constant: -12),
            answerStyleLabel.bottomAnchor.constraint(equalTo: answerStyleBox.topAnchor, constant: -6),

            answerStyleBox.leadingAnchor.constraint(equalTo: sessionDrawer.leadingAnchor, constant: 12),
            answerStyleBox.trailingAnchor.constraint(equalTo: sessionDrawer.trailingAnchor, constant: -12),
            answerStyleBox.bottomAnchor.constraint(equalTo: answerStyleSaveButton.topAnchor, constant: -8),
            answerStyleBox.heightAnchor.constraint(equalToConstant: 46),

            answerStyleSaveButton.leadingAnchor.constraint(equalTo: sessionDrawer.leadingAnchor, constant: 12),
            answerStyleSaveButton.trailingAnchor.constraint(equalTo: sessionDrawer.trailingAnchor, constant: -12),
            answerStyleSaveButton.bottomAnchor.constraint(equalTo: sessionDrawer.bottomAnchor, constant: -12),
            answerStyleSaveButton.heightAnchor.constraint(equalToConstant: 30),

            transcriptStrip.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            transcriptStrip.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            transcriptStrip.bottomAnchor.constraint(equalTo: attachmentStrip.topAnchor, constant: -6),
            transcriptStrip.heightAnchor.constraint(equalToConstant: 26),

            transcriptActivityDot.leadingAnchor.constraint(equalTo: transcriptStrip.leadingAnchor, constant: 11),
            transcriptActivityDot.centerYAnchor.constraint(equalTo: transcriptStrip.centerYAnchor),
            transcriptActivityDot.widthAnchor.constraint(equalToConstant: 7),
            transcriptActivityDot.heightAnchor.constraint(equalToConstant: 7),

            transcriptStateLabel.leadingAnchor.constraint(equalTo: transcriptActivityDot.trailingAnchor, constant: 7),
            transcriptStateLabel.centerYAnchor.constraint(equalTo: transcriptStrip.centerYAnchor),
            transcriptStateLabel.widthAnchor.constraint(equalToConstant: 88),

            transcriptScroll.topAnchor.constraint(equalTo: transcriptStrip.topAnchor, constant: 2),
            transcriptScroll.leadingAnchor.constraint(equalTo: transcriptStateLabel.trailingAnchor, constant: 8),
            transcriptScroll.trailingAnchor.constraint(equalTo: transcriptStrip.trailingAnchor, constant: -10),
            transcriptScroll.bottomAnchor.constraint(equalTo: transcriptStrip.bottomAnchor, constant: -2),

            attachmentStrip.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            attachmentStrip.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            attachmentStrip.bottomAnchor.constraint(equalTo: composerBar.topAnchor, constant: -6),
            attachmentStrip.heightAnchor.constraint(equalToConstant: 34),

            attachmentStack.leadingAnchor.constraint(equalTo: attachmentStrip.contentView.leadingAnchor),
            attachmentStack.topAnchor.constraint(equalTo: attachmentStrip.contentView.topAnchor),
            attachmentStack.bottomAnchor.constraint(equalTo: attachmentStrip.contentView.bottomAnchor),
            attachmentStack.heightAnchor.constraint(equalTo: attachmentStrip.heightAnchor),

            composerBar.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            composerBar.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            composerBar.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -10),
            composerBarHeight,

            composerSurface.topAnchor.constraint(equalTo: composerBar.topAnchor, constant: 10),
            composerSurface.leadingAnchor.constraint(equalTo: composerBar.leadingAnchor, constant: 14),
            composerSurface.trailingAnchor.constraint(equalTo: composerBar.trailingAnchor, constant: -14),
            composerTextHeight,

            composer.topAnchor.constraint(equalTo: composerSurface.topAnchor, constant: 3),
            composer.leadingAnchor.constraint(equalTo: composerSurface.leadingAnchor, constant: 14),
            composer.trailingAnchor.constraint(equalTo: composerSurface.trailingAnchor, constant: -14),
            composer.bottomAnchor.constraint(equalTo: composerSurface.bottomAnchor, constant: -3),

            attachButton.leadingAnchor.constraint(equalTo: composerBar.leadingAnchor, constant: 14),
            attachButton.bottomAnchor.constraint(equalTo: composerBar.bottomAnchor, constant: -10),
            attachButton.widthAnchor.constraint(equalToConstant: 36),
            attachButton.heightAnchor.constraint(equalToConstant: 36),

            instructionsButton.leadingAnchor.constraint(equalTo: attachButton.trailingAnchor, constant: 8),
            instructionsButton.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            instructionsButton.widthAnchor.constraint(equalToConstant: 86),
            instructionsButton.heightAnchor.constraint(equalToConstant: 36),

            recordingButton.leadingAnchor.constraint(equalTo: instructionsButton.trailingAnchor, constant: 8),
            recordingButton.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            recordingButton.widthAnchor.constraint(equalToConstant: 94),
            recordingButton.heightAnchor.constraint(equalToConstant: 36),

            opacityControl.leadingAnchor.constraint(equalTo: recordingButton.trailingAnchor, constant: 8),
            opacityControl.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            opacityControl.widthAnchor.constraint(equalToConstant: 96),
            opacityControl.heightAnchor.constraint(equalToConstant: 36),

            opacityLabel.leadingAnchor.constraint(equalTo: opacityControl.leadingAnchor, constant: 10),
            opacityLabel.centerYAnchor.constraint(equalTo: opacityControl.centerYAnchor),
            opacityLabel.widthAnchor.constraint(equalToConstant: 18),

            opacitySlider.leadingAnchor.constraint(equalTo: opacityLabel.trailingAnchor, constant: 5),
            opacitySlider.centerYAnchor.constraint(equalTo: opacityControl.centerYAnchor),
            opacitySlider.trailingAnchor.constraint(equalTo: opacityValueLabel.leadingAnchor, constant: -5),
            opacitySlider.heightAnchor.constraint(equalToConstant: 20),

            opacityValueLabel.trailingAnchor.constraint(equalTo: opacityControl.trailingAnchor, constant: -8),
            opacityValueLabel.centerYAnchor.constraint(equalTo: opacityControl.centerYAnchor),
            opacityValueLabel.widthAnchor.constraint(equalToConstant: 28),

            askButton.trailingAnchor.constraint(equalTo: composerBar.trailingAnchor, constant: -8),
            askButton.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            askButton.widthAnchor.constraint(equalToConstant: 40),
            askButton.heightAnchor.constraint(equalToConstant: 40),

            analyzeButton.trailingAnchor.constraint(equalTo: askButton.leadingAnchor, constant: -7),
            analyzeButton.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            analyzeButton.widthAnchor.constraint(equalToConstant: 88),
            analyzeButton.heightAnchor.constraint(equalToConstant: 36),

            modelMenu.trailingAnchor.constraint(equalTo: analyzeButton.leadingAnchor, constant: -7),
            modelMenu.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            modelMenu.widthAnchor.constraint(greaterThanOrEqualToConstant: 118),
            modelMenu.widthAnchor.constraint(lessThanOrEqualToConstant: 144),
            modelMenu.heightAnchor.constraint(equalToConstant: 34),

            opacityControl.trailingAnchor.constraint(lessThanOrEqualTo: modelMenu.leadingAnchor, constant: -10),

            closeConfirmOverlay.topAnchor.constraint(equalTo: topAnchor),
            closeConfirmOverlay.leadingAnchor.constraint(equalTo: leadingAnchor),
            closeConfirmOverlay.trailingAnchor.constraint(equalTo: trailingAnchor),
            closeConfirmOverlay.bottomAnchor.constraint(equalTo: bottomAnchor),

            closeConfirmPanel.centerXAnchor.constraint(equalTo: closeConfirmOverlay.centerXAnchor),
            closeConfirmPanel.centerYAnchor.constraint(equalTo: closeConfirmOverlay.centerYAnchor),
            closeConfirmPanel.widthAnchor.constraint(equalToConstant: 330),

            closeConfirmTitle.topAnchor.constraint(equalTo: closeConfirmPanel.topAnchor, constant: 18),
            closeConfirmTitle.leadingAnchor.constraint(equalTo: closeConfirmPanel.leadingAnchor, constant: 18),
            closeConfirmTitle.trailingAnchor.constraint(equalTo: closeConfirmPanel.trailingAnchor, constant: -18),

            closeConfirmBody.topAnchor.constraint(equalTo: closeConfirmTitle.bottomAnchor, constant: 8),
            closeConfirmBody.leadingAnchor.constraint(equalTo: closeConfirmTitle.leadingAnchor),
            closeConfirmBody.trailingAnchor.constraint(equalTo: closeConfirmTitle.trailingAnchor),

            closeConfirmCancelButton.topAnchor.constraint(equalTo: closeConfirmBody.bottomAnchor, constant: 18),
            closeConfirmCancelButton.leadingAnchor.constraint(equalTo: closeConfirmPanel.leadingAnchor, constant: 18),
            closeConfirmCancelButton.bottomAnchor.constraint(equalTo: closeConfirmPanel.bottomAnchor, constant: -18),
            closeConfirmCancelButton.widthAnchor.constraint(equalToConstant: 130),
            closeConfirmCancelButton.heightAnchor.constraint(equalToConstant: 34),

            closeConfirmTurnOffButton.topAnchor.constraint(equalTo: closeConfirmCancelButton.topAnchor),
            closeConfirmTurnOffButton.leadingAnchor.constraint(equalTo: closeConfirmCancelButton.trailingAnchor, constant: 12),
            closeConfirmTurnOffButton.trailingAnchor.constraint(equalTo: closeConfirmPanel.trailingAnchor, constant: -18),
            closeConfirmTurnOffButton.heightAnchor.constraint(equalTo: closeConfirmCancelButton.heightAnchor),
        ])

        navButton.target = self
        navButton.action = #selector(toggleSessionsClicked)
        canvasToggleButton.target = self
        canvasToggleButton.action = #selector(toggleCanvasClicked)
        newSessionButton.target = self
        newSessionButton.action = #selector(newSessionClicked)
        latestSessionButton.target = self
        latestSessionButton.action = #selector(continueSessionClicked)
        answerStyleSaveButton.target = self
        answerStyleSaveButton.action = #selector(saveAnswerStyleClicked)
        hideButton.target = self
        hideButton.action = #selector(hideClicked)
        closeButton.target = self
        closeButton.action = #selector(closeClicked)
        closeConfirmCancelButton.target = self
        closeConfirmCancelButton.action = #selector(cancelCloseConfirmClicked)
        closeConfirmTurnOffButton.target = self
        closeConfirmTurnOffButton.action = #selector(confirmTurnOffClicked)
        opacitySlider.target = self
        opacitySlider.action = #selector(opacityChanged)
        composer.onSubmit = { [weak self] in self?.askClicked() }
        composer.onMeasuredHeight = { [weak self] height in self?.setComposerTextHeight(height) }
        recordingButton.target = self
        recordingButton.action = #selector(recordingClicked)
        askButton.target = self
        askButton.action = #selector(askClicked)
        analyzeButton.target = self
        analyzeButton.action = #selector(analyzeClicked)
        attachButton.target = self
        attachButton.action = #selector(attachClicked)
        instructionsButton.target = self
        instructionsButton.action = #selector(instructionsClicked)

        sessionDrawer.isHidden = true
        canvasPane.isHidden = true
        canvasToggleButton.isHidden = true
        canvasPane.onCollapse = { [weak self] in self?.setCanvasOpen(false) }
        styleHeaderIconButton(navButton, symbol: "sidebar.left", fallback: "[]")
        styleHeaderIconButton(canvasToggleButton, symbol: "sidebar.right", fallback: "|")
        styleHeaderIconButton(newSessionButton, symbol: "square.and.pencil", fallback: "+")
        styleControlButton(latestSessionButton, symbol: "clock.arrow.circlepath", accent: false)
        styleControlButton(answerStyleSaveButton, symbol: "checkmark", accent: true)
        styleControlButton(recordingButton, symbol: "waveform", accent: false)
        styleControlButton(instructionsButton, symbol: "text.bubble", accent: false)
        styleIconButton(attachButton, symbol: "plus", fallback: "+")
        styleControlButton(analyzeButton, symbol: "sparkle.magnifyingglass", accent: false)
        styleIconButton(askButton, symbol: "arrow.up", fallback: "↑", accent: true)
        styleHeaderIconButton(hideButton, symbol: "eye.slash", fallback: "-")
        styleHeaderIconButton(closeButton, symbol: "xmark", fallback: "x")
        configureTooltips()
        setContextItems([])
        setTranscriptState("IDLE", active: false)
    }
    required init?(coder: NSCoder) { fatalError() }

    override func layout() {
        super.layout()
        resizeTranscriptLabelToContent()
    }

    private func configureHeader() {
        headerBar.wantsLayer = true
        headerBar.layer?.backgroundColor = NSColor(red: 0.016, green: 0.019, blue: 0.025, alpha: 0.98).cgColor
        headerBar.layer?.cornerRadius = 21
        headerBar.layer?.borderWidth = 1
        headerBar.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.20).cgColor
        headerBar.layer?.shadowColor = NSColor.black.cgColor
        headerBar.layer?.shadowOpacity = 0.24
        headerBar.layer?.shadowRadius = 18
        headerBar.layer?.shadowOffset = NSSize(width: 0, height: -6)

        headerStack.orientation = .horizontal
        headerStack.alignment = .centerY
        headerStack.distribution = .fill
        headerStack.spacing = 7
        headerSpacer.setContentHuggingPriority(.defaultLow, for: .horizontal)
        headerSpacer.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        brandStack.orientation = .vertical
        brandStack.alignment = .leading
        brandStack.distribution = .fill
        brandStack.spacing = -1
        brandStack.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        brandStack.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        titleLabel.font = NSFont.systemFont(ofSize: 13.5, weight: .bold)
        titleLabel.textColor = BlueyTheme.text
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.maximumNumberOfLines = 1
        titleLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        statusLabel.font = NSFont.systemFont(ofSize: 9.5, weight: .semibold)
        statusLabel.textColor = BlueyTheme.textDim
        statusLabel.lineBreakMode = .byTruncatingTail
        statusLabel.maximumNumberOfLines = 1
        statusLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        modelMenu.addItems(withTitles: ["Auto", "Instant", "Balanced", "Deep"])
        modelMenu.selectItem(at: 0)
        modelMenu.isBordered = false
        modelMenu.wantsLayer = true
        modelMenu.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.075).cgColor
        modelMenu.layer?.cornerRadius = 15
        modelMenu.layer?.borderWidth = 1
        modelMenu.layer?.borderColor = NSColor.white.withAlphaComponent(0.12).cgColor
        modelMenu.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        modelMenu.contentTintColor = BlueyTheme.text
        modelMenu.setContentHuggingPriority(.defaultLow, for: .horizontal)
        modelMenu.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        styleHeaderBadge(routeBadge, textColor: BlueyTheme.cyan)
        routeBadge.toolTip = "Auto Router classification and selected lane"

        styleHeaderBadge(knowledgeBadge, textColor: BlueyTheme.text)
        knowledgeBadge.toolTip = "Knowledge base status for attached documents"

        balanceLabel.font = NSFont.monospacedSystemFont(ofSize: 10.5, weight: .bold)
        balanceLabel.textColor = BlueyTheme.text
        balanceLabel.alignment = .center
        balanceLabel.lineBreakMode = .byTruncatingMiddle
        balanceLabel.maximumNumberOfLines = 1
        balanceLabel.setContentHuggingPriority(.defaultLow, for: .horizontal)
        balanceLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        balanceLabel.wantsLayer = true
        balanceLabel.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.055).cgColor
        balanceLabel.layer?.cornerRadius = 14
        balanceLabel.layer?.borderWidth = 1
        balanceLabel.layer?.borderColor = NSColor.white.withAlphaComponent(0.10).cgColor
    }

    private func configureContextRows() {
        transcriptStrip.wantsLayer = true
        transcriptStrip.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.16).cgColor
        transcriptStrip.layer?.cornerRadius = 13
        transcriptStrip.layer?.borderWidth = 1
        transcriptStrip.layer?.borderColor = BlueyTheme.hairline.cgColor

        transcriptActivityDot.wantsLayer = true
        transcriptActivityDot.layer?.cornerRadius = 3.5
        transcriptActivityDot.layer?.backgroundColor = BlueyTheme.textDim.withAlphaComponent(0.55).cgColor
        transcriptActivityDot.layer?.shadowColor = BlueyTheme.cyan.cgColor
        transcriptActivityDot.layer?.shadowOpacity = 0
        transcriptActivityDot.layer?.shadowRadius = 7
        transcriptActivityDot.layer?.shadowOffset = .zero

        transcriptStateLabel.font = NSFont.monospacedSystemFont(ofSize: 9.5, weight: .bold)
        transcriptStateLabel.textColor = BlueyTheme.textDim
        transcriptStateLabel.alignment = .left
        transcriptStateLabel.lineBreakMode = .byTruncatingTail

        transcriptScroll.drawsBackground = false
        transcriptScroll.hasVerticalScroller = false
        transcriptScroll.hasHorizontalScroller = true
        transcriptScroll.autohidesScrollers = true
        transcriptScroll.borderType = .noBorder
        transcriptScroll.scrollerStyle = .overlay

        transcriptLabel.isBezeled = false
        transcriptLabel.drawsBackground = false
        transcriptLabel.font = NSFont.systemFont(ofSize: 11.5, weight: .medium)
        transcriptLabel.textColor = BlueyTheme.textDim
        transcriptLabel.lineBreakMode = .byClipping
        transcriptLabel.maximumNumberOfLines = 1
        transcriptLabel.alignment = .left
        if let cell = transcriptLabel.cell as? NSTextFieldCell {
            cell.isScrollable = true
            cell.wraps = false
            cell.lineBreakMode = .byClipping
        }

        attachmentStack.orientation = .horizontal
        attachmentStack.alignment = .centerY
        attachmentStack.spacing = 6
        attachmentStack.edgeInsets = NSEdgeInsets(top: 3, left: 4, bottom: 3, right: 4)

        attachmentStrip.drawsBackground = false
        attachmentStrip.hasVerticalScroller = false
        attachmentStrip.hasHorizontalScroller = true
        attachmentStrip.autohidesScrollers = true
        attachmentStrip.borderType = .noBorder
        attachmentStrip.documentView = attachmentStack
        attachmentStrip.scrollerStyle = .overlay
        attachmentStrip.isHidden = true
    }

    private func styleDrawer() {
        sessionDrawer.wantsLayer = true
        sessionDrawer.layer?.backgroundColor = NSColor(red: 0.035, green: 0.040, blue: 0.050, alpha: 0.98).cgColor
        sessionDrawer.layer?.cornerRadius = 16
        sessionDrawer.layer?.borderWidth = 1
        sessionDrawer.layer?.borderColor = BlueyTheme.hairline.cgColor
        sessionDrawer.layer?.shadowColor = NSColor.black.cgColor
        sessionDrawer.layer?.shadowOpacity = 0.26
        sessionDrawer.layer?.shadowRadius = 18
        sessionDrawer.layer?.shadowOffset = NSSize(width: 0, height: -8)
        sessionDrawer.layer?.zPosition = 10

        drawerTitleLabel.font = NSFont.systemFont(ofSize: 13, weight: .bold)
        drawerTitleLabel.textColor = BlueyTheme.text
        drawerSubtitleLabel.font = NSFont.systemFont(ofSize: 10.5, weight: .medium)
        drawerSubtitleLabel.textColor = BlueyTheme.textDim
        drawerSubtitleLabel.lineBreakMode = .byWordWrapping
        drawerSubtitleLabel.maximumNumberOfLines = 2

        sessionStack.orientation = .vertical
        sessionStack.alignment = .centerX
        sessionStack.spacing = 6
        sessionStack.edgeInsets = NSEdgeInsets(top: 2, left: 0, bottom: 2, right: 0)

        sessionScroll.drawsBackground = false
        sessionScroll.hasVerticalScroller = true
        sessionScroll.hasHorizontalScroller = false
        sessionScroll.autohidesScrollers = true
        sessionScroll.borderType = .noBorder
        sessionScroll.documentView = sessionStack
        sessionScroll.scrollerStyle = .overlay

        answerStyleLabel.font = NSFont.systemFont(ofSize: 10.5, weight: .bold)
        answerStyleLabel.textColor = BlueyTheme.textDim
        answerStyleBox.placeholderString = "Concise, structured, implementation-first..."
        answerStyleBox.font = NSFont.systemFont(ofSize: 11.5, weight: .medium)
        answerStyleBox.isBezeled = false
        answerStyleBox.drawsBackground = false
        answerStyleBox.focusRingType = .none
        answerStyleBox.textColor = BlueyTheme.text
        answerStyleBox.placeholderAttributedString = NSAttributedString(
            string: "Concise, structured, implementation-first...",
            attributes: [.foregroundColor: BlueyTheme.textDim.withAlphaComponent(0.78)])
        answerStyleBox.wantsLayer = true
        answerStyleBox.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.18).cgColor
        answerStyleBox.layer?.cornerRadius = 10
        answerStyleBox.layer?.borderWidth = 1
        answerStyleBox.layer?.borderColor = BlueyTheme.hairline.cgColor
    }

    private func configureComposer() {
        composerBar.wantsLayer = true
        composerBar.layer?.backgroundColor = NSColor(red: 0.026, green: 0.030, blue: 0.038, alpha: 0.98).cgColor
        composerBar.layer?.cornerRadius = 28
        composerBar.layer?.borderWidth = 1
        composerBar.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.16).cgColor

        composerSurface.wantsLayer = true
        composerSurface.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.055).cgColor
        composerSurface.layer?.cornerRadius = 20
        composerSurface.layer?.borderWidth = 1
        composerSurface.layer?.borderColor = NSColor.white.withAlphaComponent(0.10).cgColor

        opacityControl.wantsLayer = true
        opacityControl.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.055).cgColor
        opacityControl.layer?.cornerRadius = 18
        opacityControl.layer?.borderWidth = 1
        opacityControl.layer?.borderColor = NSColor.white.withAlphaComponent(0.08).cgColor
        opacityControl.toolTip = "Overlay opacity"
        opacityLabel.stringValue = "%"
        opacityLabel.font = NSFont.systemFont(ofSize: 10, weight: .semibold)
        opacityLabel.textColor = BlueyTheme.textDim
        opacityLabel.alignment = .left
        opacityValueLabel.font = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .semibold)
        opacityValueLabel.textColor = BlueyTheme.textDim
        opacityValueLabel.alignment = .right
        opacitySlider.controlSize = .small
        opacitySlider.wantsLayer = true
        opacitySlider.toolTip = "Overlay opacity"

        composer.placeholder = "Ask anything..."
        composer.setContentHuggingPriority(.defaultLow, for: .horizontal)
        composer.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        for control in [
            recordingButton,
            opacityControl,
            instructionsButton,
            attachButton,
            analyzeButton,
            askButton,
        ] {
            control.setContentHuggingPriority(.required, for: .horizontal)
            control.setContentCompressionResistancePriority(.required, for: .horizontal)
        }
    }

    private func configureCloseConfirm() {
        closeConfirmOverlay.isHidden = true
        closeConfirmOverlay.wantsLayer = true
        closeConfirmOverlay.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.52).cgColor
        closeConfirmOverlay.layer?.zPosition = 100

        closeConfirmPanel.wantsLayer = true
        closeConfirmPanel.layer?.backgroundColor = BlueyTheme.panelDeep.cgColor
        closeConfirmPanel.layer?.cornerRadius = 18
        closeConfirmPanel.layer?.borderWidth = 1
        closeConfirmPanel.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.30).cgColor
        closeConfirmPanel.layer?.shadowColor = NSColor.black.cgColor
        closeConfirmPanel.layer?.shadowOpacity = 0.34
        closeConfirmPanel.layer?.shadowRadius = 22
        closeConfirmPanel.layer?.shadowOffset = .zero

        closeConfirmTitle.font = NSFont.systemFont(ofSize: 16, weight: .bold)
        closeConfirmTitle.textColor = BlueyTheme.text
        closeConfirmBody.font = NSFont.systemFont(ofSize: 12.5, weight: .medium)
        closeConfirmBody.textColor = BlueyTheme.textDim
        closeConfirmBody.maximumNumberOfLines = 3

        styleControlButton(closeConfirmCancelButton, symbol: "xmark", accent: false)
        styleControlButton(closeConfirmTurnOffButton, symbol: "power", accent: true)
        closeConfirmCancelButton.toolTip = "Keep Bluey running"
        closeConfirmTurnOffButton.toolTip = "Turn Bluey off. Run bluey on to start again."
    }

    private func configureTooltips() {
        navButton.toolTip = "Show recordings"
        newSessionButton.toolTip = "Start a new recording"
        modelMenu.toolTip = "Choose routing lane"
        canvasToggleButton.toolTip = "Open or collapse the canvas"
        balanceLabel.toolTip = "Remaining Bluey balance"
        hideButton.toolTip = "Hide to pill"
        closeButton.toolTip = "Turn Bluey off. Run bluey on to start again."
        recordingButton.toolTip = "Start or stop listening"
        instructionsButton.toolTip = "How Bluey should answer"
        attachButton.toolTip = "Attach files"
        analyzeButton.toolTip = "Analyse screen"
        askButton.toolTip = "Send"
        latestSessionButton.toolTip = "Continue the latest recording"
        answerStyleSaveButton.toolTip = "Save answer style for this session"
    }

    private func styleControlButton(_ button: NSButton, symbol: String, accent: Bool) {
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 16
        button.layer?.backgroundColor = accent
            ? NSColor(red: 0.07, green: 0.19, blue: 0.24, alpha: 0.98).cgColor
            : BlueyTheme.surfaceRaised.cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = (accent ? BlueyTheme.cyan.withAlphaComponent(0.55) : BlueyTheme.hairline).cgColor
        button.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        button.attributedTitle = NSAttributedString(
            string: button.title,
            attributes: [
                .font: button.font ?? NSFont.systemFont(ofSize: 12, weight: .bold),
                .foregroundColor: BlueyTheme.text,
            ])
        button.contentTintColor = BlueyTheme.cyan
        if let image = symbolImage(symbol) {
            image.isTemplate = true
            button.image = image
        }
        button.imagePosition = .imageLeading
        button.imageScaling = .scaleProportionallyDown
        button.alignment = .center
    }

    private func styleHeaderBadge(_ label: NSTextField, textColor: NSColor) {
        label.font = NSFont.systemFont(ofSize: 11.3, weight: .bold)
        label.textColor = textColor
        label.alignment = .center
        label.lineBreakMode = .byTruncatingMiddle
        label.maximumNumberOfLines = 1
        label.setContentHuggingPriority(.defaultLow, for: .horizontal)
        label.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        label.wantsLayer = true
        label.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.052).cgColor
        label.layer?.cornerRadius = 13
        label.layer?.borderWidth = 1
        label.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.16).cgColor
    }

    private func styleHeaderIconButton(_ button: NSButton, symbol: String, fallback: String) {
        button.title = fallback
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 15
        button.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.070).cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = NSColor.white.withAlphaComponent(0.12).cgColor
        button.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        button.contentTintColor = BlueyTheme.text
        if let image = symbolImage(symbol) {
            image.isTemplate = true
            button.title = ""
            button.image = image
            button.imagePosition = .imageOnly
            button.imageScaling = .scaleProportionallyDown
        } else {
            button.attributedTitle = NSAttributedString(
                string: fallback,
                attributes: [
                    .font: button.font ?? NSFont.systemFont(ofSize: 12, weight: .bold),
                    .foregroundColor: BlueyTheme.textDim,
                ])
        }
        button.alignment = .center
    }

    private func styleIconButton(_ button: NSButton, symbol: String, fallback: String, accent: Bool = false) {
        button.title = fallback
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 16
        button.layer?.backgroundColor = accent
            ? NSColor(red: 0.84, green: 0.92, blue: 0.96, alpha: 0.95).cgColor
            : BlueyTheme.surfaceRaised.cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = (accent ? NSColor.white.withAlphaComponent(0.18) : BlueyTheme.hairline).cgColor
        button.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        button.contentTintColor = accent ? NSColor.black.withAlphaComponent(0.82) : BlueyTheme.cyan
        if let image = symbolImage(symbol) {
            image.isTemplate = true
            button.title = ""
            button.image = image
            button.imagePosition = .imageOnly
            button.imageScaling = .scaleProportionallyDown
        } else {
            button.attributedTitle = NSAttributedString(
                string: fallback,
                attributes: [
                    .font: button.font ?? NSFont.systemFont(ofSize: 12, weight: .bold),
                    .foregroundColor: BlueyTheme.text,
                ])
        }
        button.alignment = .center
    }

    @objc private func hideClicked() { onClose?() }

    @objc private func closeClicked() {
        closeConfirmOverlay.isHidden = false
        closeConfirmOverlay.alphaValue = 0
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.12
            closeConfirmOverlay.animator().alphaValue = 1
        }
    }

    @objc private func cancelCloseConfirmClicked() {
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.10
            closeConfirmOverlay.animator().alphaValue = 0
        }, completionHandler: { [weak self] in
            guard let self else { return }
            self.closeConfirmOverlay.isHidden = true
            self.closeConfirmOverlay.alphaValue = 1
        })
    }

    @objc private func confirmTurnOffClicked() {
        emitSimple("close_requested")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) {
            NSApp.terminate(nil)
        }
    }

    @objc private func opacityChanged() {
        applyOpacity(opacitySlider.doubleValue)
    }

    func applyOpacity(_ opacity: Double) {
        let value = min(max(opacity, 0.50), 1.0)
        if abs(opacitySlider.doubleValue - value) > 0.001 {
            opacitySlider.doubleValue = value
        }
        opacityValueLabel.stringValue = "\(Int((value * 100.0).rounded()))%"
        onOpacityChanged?(value)
    }

    private func setComposerTextHeight(_ rawHeight: CGFloat) {
        let textHeight = min(max(rawHeight, 46), 122)
        guard abs((composerTextHeightConstraint?.constant ?? 0) - textHeight) > 0.5 else { return }
        composerTextHeightConstraint?.constant = textHeight
        composerBarHeightConstraint?.constant = textHeight + 62
        needsLayout = true
        layoutSubtreeIfNeeded()
    }

    @objc private func toggleSessionsClicked() {
        sessionDrawer.isHidden.toggle()
        statusLabel.stringValue = sessionDrawer.isHidden ? statusLabel.stringValue : "Sessions"
    }

    @objc private func toggleCanvasClicked() {
        guard latestCanvas != nil else { return }
        setCanvasOpen(!canvasOpen)
    }

    @objc private func newSessionClicked() {
        resetSessionSurface()
        composer.clearText()
        statusLabel.stringValue = "New recording"
        sessionDrawer.isHidden = true
        emitSimple("session_new_requested")
    }

    @objc private func continueSessionClicked() {
        sessionDrawer.isHidden = true
        statusLabel.stringValue = "Latest recording"
        emitSimple("session_continue_requested")
    }

    @objc private func saveAnswerStyleClicked() {
        let text = answerStyleBox.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        emitInstructions(text: text)
        statusLabel.stringValue = text.isEmpty ? "Default style" : "Answer style saved"
    }

    @objc private func recordingClicked() {
        if recordingActive {
            emitSimple("recording_stop_requested")
            recordingActive = false
            recordingButton.title = "Listen"
            statusLabel.stringValue = "Paused"
            composer.placeholder = "Ask anything..."
            setTranscriptState("PAUSED", active: false)
            styleControlButton(recordingButton, symbol: "waveform", accent: false)
        } else {
            emitSimple("recording_start_requested")
            recordingActive = true
            recordingButton.title = "Stop"
            statusLabel.stringValue = "Listening"
            composer.placeholder = "Listening... type a follow-up anytime"
            setTranscriptState("LISTENING", active: true)
            styleControlButton(recordingButton, symbol: "stop.fill", accent: true)
        }
    }

    @objc private func askClicked() {
        let raw = composer.string.trimmingCharacters(in: .whitespacesAndNewlines)
        let q = raw.isEmpty
            ? "Answer the latest clear question or useful context from this Bluey session."
            : raw
        composer.clearText()
        let route = selectedRoute()
        updateRouteBadge(for: q, selectedRoute: route)
        emitAsk(question: q, provider: route.provider, model: route.model, mode: route.mode)
    }

    @objc private func analyzeClicked() {
        routeBadge.stringValue = "Vision · deep"
        statusLabel.stringValue = "Reading screen"
        emitSimple("analyze_screen_requested")
    }

    @objc private func attachClicked() {
        setKnowledgeBadge("KB loading", accent: BlueyTheme.warning)
        showKnowledgePlaceholder("Indexing selected files...")
        emitSimple("attach_requested")
    }

    @objc private func instructionsClicked() {
        sessionDrawer.isHidden = false
        window?.makeFirstResponder(answerStyleBox)
    }

    func setBalanceLabel(_ label: String) {
        let clean = label.trimmingCharacters(in: .whitespacesAndNewlines)
        balanceLabel.stringValue = clean.isEmpty ? "Balance --" : clean
    }

    private func setKnowledgeBadge(_ text: String, accent: NSColor) {
        knowledgeBadge.stringValue = text
        knowledgeBadge.textColor = accent
        knowledgeBadge.layer?.borderColor = accent.withAlphaComponent(0.30).cgColor
        knowledgeBadge.layer?.backgroundColor = accent.withAlphaComponent(0.08).cgColor
    }

    private func showKnowledgePlaceholder(_ text: String) {
        for view in attachmentStack.arrangedSubviews {
            attachmentStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        attachmentStrip.isHidden = false

        let chip = NSTextField(labelWithString: text)
        chip.translatesAutoresizingMaskIntoConstraints = false
        chip.font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
        chip.textColor = BlueyTheme.textDim
        chip.alignment = .center
        chip.lineBreakMode = .byTruncatingTail
        chip.wantsLayer = true
        chip.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.035).cgColor
        chip.layer?.cornerRadius = 12
        chip.layer?.borderWidth = 1
        chip.layer?.borderColor = BlueyTheme.hairline.cgColor
        attachmentStack.addArrangedSubview(chip)
        NSLayoutConstraint.activate([
            chip.heightAnchor.constraint(equalToConstant: 28),
            chip.widthAnchor.constraint(greaterThanOrEqualToConstant: 210),
        ])
    }

    private func setTranscriptState(_ text: String, active: Bool) {
        transcriptStateLabel.stringValue = text
        transcriptStateLabel.textColor = active ? BlueyTheme.green : BlueyTheme.textDim
        transcriptActivityDot.layer?.backgroundColor = (active ? BlueyTheme.green : BlueyTheme.textDim.withAlphaComponent(0.55)).cgColor
        transcriptActivityDot.layer?.shadowOpacity = active ? 0.45 : 0
        transcriptStrip.layer?.borderColor = (active ? BlueyTheme.green.withAlphaComponent(0.26) : BlueyTheme.hairline).cgColor
    }

    private func updateRouteBadge(
        for question: String,
        selectedRoute: (provider: String?, model: String?, mode: String?)
    ) {
        let manual = (selectedRoute.provider ?? "auto").lowercased() != "auto"
        if manual {
            switch selectedRoute.model ?? selectedRoute.provider ?? "Manual" {
            case let value where value.contains("mini"):
                routeBadge.stringValue = "Instant · manual"
            case let value where value.contains("sonnet"):
                routeBadge.stringValue = "Deep · manual"
            default:
                routeBadge.stringValue = "Manual lane"
            }
            routeBadge.textColor = BlueyTheme.text
            return
        }

        let lower = question.lowercased()
        let words = lower.split { $0.isWhitespace || $0.isNewline }.count
        let vision = lower.contains("screen") || lower.contains("screenshot") || lower.contains("image")
        let code = looksLikeCode(lower) || lower.contains("leetcode") || lower.contains("debug")
        let design = looksLikeSystemDesign(lower) || lower.contains("architecture")
        let hard = words > 80 || design || lower.contains("tradeoff") || lower.contains("scale")
        let label: String
        if vision {
            label = "Vision · deep"
        } else if hard {
            label = "Auto · hard"
        } else if code {
            label = "Auto · medium"
        } else {
            label = "Auto · easy"
        }
        routeBadge.stringValue = label
        routeBadge.textColor = hard || vision ? BlueyTheme.warning : BlueyTheme.cyan
    }

    private func routeBadgeText(for artifact: OverlayArtifact) -> String {
        switch artifact.artifactType {
        case "code": return "Code · canvas"
        case "system_design": return "Design · canvas"
        case "screen": return "Vision · canvas"
        case "document": return "Docs · canvas"
        default: return "Auto · canvas"
        }
    }

    func setContextItems(_ items: [OverlayContextItem]) {
        for view in attachmentStack.arrangedSubviews {
            attachmentStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }

        attachmentStrip.isHidden = false
        guard !items.isEmpty else {
            setKnowledgeBadge("KB empty", accent: BlueyTheme.textDim)
            showKnowledgePlaceholder("Knowledge base empty · attach docs")
            return
        }

        setKnowledgeBadge("KB \(items.count) loaded", accent: BlueyTheme.green)

        for item in items {
            attachmentStack.addArrangedSubview(makeAttachmentChip(item))
        }
    }

    func setSessions(_ sessions: [OverlaySessionItem]) {
        sessionItems = sessions
        renameField = nil
        for view in sessionStack.arrangedSubviews {
            sessionStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }

        if sessions.isEmpty {
            let empty = NSTextField(wrappingLabelWithString: "No saved recordings yet.")
            empty.font = NSFont.systemFont(ofSize: 11.5, weight: .medium)
            empty.textColor = BlueyTheme.textDim
            empty.alignment = .center
            empty.translatesAutoresizingMaskIntoConstraints = false
            sessionStack.addArrangedSubview(empty)
            empty.widthAnchor.constraint(equalTo: sessionStack.widthAnchor, constant: -20).isActive = true
            return
        }

        for session in sessions {
            let row = makeSessionRow(session)
            sessionStack.addArrangedSubview(row)
            row.widthAnchor.constraint(equalTo: sessionStack.widthAnchor, constant: -2).isActive = true
        }
    }

    func resetSessionSurface() {
        feed.clear()
        setContextItems([])
        transcriptSnippets.removeAll()
        updateTranscriptStripText("Live captions preview", scrollToEnd: false)
        setTranscriptState("IDLE", active: false)
        routeBadge.stringValue = "Auto · ready"
        latestCanvas = nil
        setCanvasOpen(false)
        canvasToggleButton.isHidden = true
    }

    func pushCard(_ card: RenderedCard) {
        feed.push(card)
        routeCanvasIfNeeded(card)
    }

    func updateCard(id: String, body: String, done: Bool, costLabel: String?, artifact: OverlayArtifact?) {
        guard let card = feed.update(id: id, body: body, done: done, costLabel: costLabel, artifact: artifact) else {
            return
        }
        if let artifact {
            routeBadge.stringValue = routeBadgeText(for: artifact)
        } else if !done {
            statusLabel.stringValue = "Answer streaming"
        }
        routeCanvasIfNeeded(card)
    }

    private func routeCanvasIfNeeded(_ card: RenderedCard) {
        guard let artifact = makeCanvasArtifact(from: card) else { return }
        latestCanvas = artifact
        canvasPane.render(artifact)
        canvasToggleButton.isHidden = false
        if shouldAutoOpenCanvas(for: card, artifact: artifact) {
            setCanvasOpen(true)
        }
    }

    private func shouldAutoOpenCanvas(for card: RenderedCard, artifact: CanvasArtifact) -> Bool {
        if card.artifact != nil { return true }
        guard card.kind == "answer" else { return false }
        switch artifact.kind {
        case .code, .systemDesign, .screen:
            return true
        case .document, .structured:
            return false
        }
    }

    private func setCanvasOpen(_ open: Bool) {
        canvasOpen = open
        canvasPane.isHidden = !open
        canvasWidthConstraint?.constant = open ? 310 : 0
        canvasToggleButton.contentTintColor = open ? BlueyTheme.cyan : BlueyTheme.textDim
        if open {
            ensureRoomForCanvas()
        } else {
            restoreCompactWidth()
        }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.16
            self.layoutSubtreeIfNeeded()
        }
    }

    private func ensureRoomForCanvas() {
        guard let window else { return }
        let targetWidth = ExpandedPanelMetrics.maxCanvasWidth
        let screen = window.screen?.visibleFrame
            ?? NSScreen.main?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let clampedTargetWidth = ExpandedPanelMetrics.fittingWidth(for: screen, preferred: targetWidth)
        let minimumWidth = ExpandedPanelMetrics.fittingMinimumWidth(for: screen, targetWidth: clampedTargetWidth)
        if let overlayWindow = window as? OverlayWindow {
            overlayWindow.minimumFrameWidth = minimumWidth
            overlayWindow.maximumFrameWidth = clampedTargetWidth
        }
        window.minSize = NSSize(width: minimumWidth, height: window.minSize.height)
        window.contentMinSize = NSSize(width: minimumWidth, height: window.contentMinSize.height)
        window.maxSize = NSSize(width: clampedTargetWidth, height: window.maxSize.height)
        window.contentMaxSize = NSSize(width: clampedTargetWidth, height: window.contentMaxSize.height)
        guard window.frame.width < clampedTargetWidth else { return }
        var frame = window.frame
        frame.size.width = clampedTargetWidth
        frame.origin.x = min(max(screen.minX + 12, frame.origin.x), screen.maxX - frame.width - 12)
        window.setFrame(frame, display: true, animate: true)
    }

    private func restoreCompactWidth() {
        guard let window else { return }
        let screen = window.screen?.visibleFrame
            ?? NSScreen.main?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let compactWidth = ExpandedPanelMetrics.fittingWidth(
            for: screen,
            preferred: ExpandedPanelMetrics.maxCompactWidth)
        let minimumWidth = ExpandedPanelMetrics.fittingMinimumWidth(for: screen, targetWidth: compactWidth)
        if let overlayWindow = window as? OverlayWindow {
            overlayWindow.minimumFrameWidth = minimumWidth
            overlayWindow.maximumFrameWidth = compactWidth
        }
        window.minSize = NSSize(width: minimumWidth, height: window.minSize.height)
        window.contentMinSize = NSSize(width: minimumWidth, height: window.contentMinSize.height)
        window.maxSize = NSSize(width: compactWidth, height: window.maxSize.height)
        window.contentMaxSize = NSSize(width: compactWidth, height: window.contentMaxSize.height)
        guard window.frame.width > compactWidth else { return }
        var frame = window.frame
        frame.size.width = compactWidth
        window.setFrame(frame, display: true, animate: true)
    }

    private func makeCanvasArtifact(from card: RenderedCard) -> CanvasArtifact? {
        guard card.kind == "answer" || card.kind == "context" || card.kind == "system" else {
            return nil
        }
        let body = card.body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !body.isEmpty else { return nil }

        if let artifact = card.artifact {
            let kind = CanvasKind.fromArtifactType(artifact.artifactType)
            let confidence = artifact.confidence.map { "Confidence \(Int(($0 * 100).rounded()))%" }
            return CanvasArtifact(
                kind: kind,
                title: artifact.title.isEmpty ? kind.title : artifact.title,
                subtitle: confidence ?? kind.subtitle,
                content: artifact.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                    ? body
                    : artifact.body,
                sourceCardId: card.id)
        }

        let lower = body.lowercased()
        let codeBlocks = extractCodeBlocks(from: body)
        if !codeBlocks.isEmpty || looksLikeCode(lower) {
            return CanvasArtifact(
                kind: .code,
                title: "Code canvas",
                subtitle: "Code, tests, complexity, and implementation notes",
                content: formatCodeCanvas(body: body, codeBlocks: codeBlocks),
                sourceCardId: card.id)
        }

        if looksLikeSystemDesign(lower) {
            return CanvasArtifact(
                kind: .systemDesign,
                title: "System design canvas",
                subtitle: "Architecture, tradeoffs, APIs, data, and scale",
                content: formatStructuredCanvas(body, fallbackHeading: "System Design"),
                sourceCardId: card.id)
        }

        if looksLikeScreenAnalysis(lower) {
            return CanvasArtifact(
                kind: .screen,
                title: "Screen analysis",
                subtitle: "Detected context and answerable details",
                content: formatStructuredCanvas(body, fallbackHeading: "Screen Context"),
                sourceCardId: card.id)
        }

        if card.kind == "context" || looksLikeDocumentWork(lower) {
            return CanvasArtifact(
                kind: .document,
                title: "Document notes",
                subtitle: "Attached context distilled for this session",
                content: formatStructuredCanvas(body, fallbackHeading: "Document Context"),
                sourceCardId: card.id)
        }

        if body.count > 950 && hasStructuredShape(body) {
            return CanvasArtifact(
                kind: .structured,
                title: "Workspace",
                subtitle: "Long-form answer kept beside the chat",
                content: formatStructuredCanvas(body, fallbackHeading: "Details"),
                sourceCardId: card.id)
        }

        return nil
    }

    private func appendTranscriptSnippet(_ card: RenderedCard) {
        let title = card.title.trimmingCharacters(in: .whitespacesAndNewlines)
        let body = card.body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !body.isEmpty else { return }

        let label = title.isEmpty ? "Transcript" : title
        transcriptSnippets.append("\(label): \(body)")
        if transcriptSnippets.count > 6 {
            transcriptSnippets.removeFirst(transcriptSnippets.count - 6)
        }
        setTranscriptState(recordingActive ? "TRANSCRIBING" : "CAPTURED", active: recordingActive)
        updateTranscriptStripText(transcriptSnippets.joined(separator: "   "), scrollToEnd: true)
    }

    private func updateTranscriptStripText(_ text: String, scrollToEnd: Bool) {
        transcriptLabel.stringValue = text
        resizeTranscriptLabelToContent()
        guard scrollToEnd else {
            transcriptScroll.contentView.scroll(to: .zero)
            transcriptScroll.reflectScrolledClipView(transcriptScroll.contentView)
            return
        }
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.resizeTranscriptLabelToContent()
            let maxX = max(0, self.transcriptLabel.frame.width - self.transcriptScroll.contentView.bounds.width)
            self.transcriptScroll.contentView.scroll(to: NSPoint(x: maxX, y: 0))
            self.transcriptScroll.reflectScrolledClipView(self.transcriptScroll.contentView)
        }
    }

    private func resizeTranscriptLabelToContent() {
        let viewport = max(0, transcriptScroll.contentView.bounds.width)
        let height = max(22, transcriptScroll.contentView.bounds.height)
        let font = transcriptLabel.font ?? NSFont.systemFont(ofSize: 11.5, weight: .medium)
        let textWidth = ceil((transcriptLabel.stringValue as NSString).size(
            withAttributes: [.font: font]).width) + 24
        transcriptLabel.frame = NSRect(
            x: 0,
            y: max(0, (height - 18) / 2),
            width: max(viewport, textWidth),
            height: 18)
    }

    private func makeAttachmentChip(_ item: OverlayContextItem) -> NSView {
        let chip = NSView()
        chip.translatesAutoresizingMaskIntoConstraints = false
        chip.wantsLayer = true
        chip.layer?.backgroundColor = BlueyTheme.surfaceRaised.cgColor
        chip.layer?.cornerRadius = 12
        chip.layer?.borderWidth = 1
        chip.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.18).cgColor

        let icon = NSImageView()
        icon.translatesAutoresizingMaskIntoConstraints = false
        icon.imageScaling = .scaleProportionallyDown
        icon.contentTintColor = fileAccent(for: item.kind)
        if let image = symbolImage(fileSymbol(for: item.kind)) {
            image.isTemplate = true
            icon.image = image
        }

        let title = NSTextField(labelWithString: item.title.isEmpty ? "Attached file" : item.title)
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
        title.textColor = BlueyTheme.text
        title.lineBreakMode = .byTruncatingMiddle
        title.maximumNumberOfLines = 1

        let kind = NSTextField(labelWithString: item.kind.uppercased())
        kind.translatesAutoresizingMaskIntoConstraints = false
        kind.font = NSFont.monospacedSystemFont(ofSize: 8.5, weight: .bold)
        kind.textColor = BlueyTheme.textDim
        kind.stringValue = "LOADED · \(item.kind.uppercased())"

        chip.addSubview(icon)
        chip.addSubview(title)
        chip.addSubview(kind)
        NSLayoutConstraint.activate([
            chip.heightAnchor.constraint(equalToConstant: 28),
            chip.widthAnchor.constraint(lessThanOrEqualToConstant: 190),
            chip.widthAnchor.constraint(greaterThanOrEqualToConstant: 112),

            icon.leadingAnchor.constraint(equalTo: chip.leadingAnchor, constant: 8),
            icon.centerYAnchor.constraint(equalTo: chip.centerYAnchor),
            icon.widthAnchor.constraint(equalToConstant: 16),
            icon.heightAnchor.constraint(equalToConstant: 16),

            title.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: 7),
            title.topAnchor.constraint(equalTo: chip.topAnchor, constant: 4),
            title.trailingAnchor.constraint(equalTo: chip.trailingAnchor, constant: -8),

            kind.leadingAnchor.constraint(equalTo: title.leadingAnchor),
            kind.topAnchor.constraint(equalTo: title.bottomAnchor, constant: -1),
            kind.trailingAnchor.constraint(lessThanOrEqualTo: title.trailingAnchor),
        ])
        return chip
    }

    private func makeSessionRow(_ session: OverlaySessionItem) -> NSView {
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false
        row.wantsLayer = true
        row.layer?.backgroundColor = session.isActive
            ? BlueyTheme.cyanSoft.cgColor
            : NSColor.white.withAlphaComponent(0.035).cgColor
        row.layer?.cornerRadius = 12
        row.layer?.borderWidth = 1
        row.layer?.borderColor = session.isActive
            ? BlueyTheme.cyan.withAlphaComponent(0.34).cgColor
            : BlueyTheme.hairline.cgColor

        if editingSessionId == session.id {
            return configureRenameRow(row, session: session)
        }

        let openButton = NSButton(title: "", target: self, action: #selector(sessionRowClicked(_:)))
        openButton.translatesAutoresizingMaskIntoConstraints = false
        openButton.isBordered = false
        openButton.tag = sessionIndex(session.id)

        let title = NSTextField(labelWithString: session.title)
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
        title.textColor = BlueyTheme.text
        title.lineBreakMode = .byTruncatingTail

        let subtitle = NSTextField(labelWithString: session.subtitle)
        subtitle.translatesAutoresizingMaskIntoConstraints = false
        subtitle.font = NSFont.systemFont(ofSize: 9.5, weight: .medium)
        subtitle.textColor = BlueyTheme.textDim
        subtitle.lineBreakMode = .byTruncatingTail

        let rename = NSButton(title: "", target: self, action: #selector(renameSessionClicked(_:)))
        rename.translatesAutoresizingMaskIntoConstraints = false
        rename.isBordered = false
        rename.tag = sessionIndex(session.id)
        rename.contentTintColor = BlueyTheme.cyan
        if let image = symbolImage("pencil") {
            image.isTemplate = true
            rename.image = image
            rename.imagePosition = .imageOnly
            rename.imageScaling = .scaleProportionallyDown
        } else {
            rename.title = "Edit"
            rename.font = NSFont.systemFont(ofSize: 9, weight: .bold)
        }

        row.addSubview(openButton)
        row.addSubview(title)
        row.addSubview(subtitle)
        row.addSubview(rename)
        NSLayoutConstraint.activate([
            row.heightAnchor.constraint(equalToConstant: 52),

            openButton.topAnchor.constraint(equalTo: row.topAnchor),
            openButton.leadingAnchor.constraint(equalTo: row.leadingAnchor),
            openButton.bottomAnchor.constraint(equalTo: row.bottomAnchor),
            openButton.trailingAnchor.constraint(equalTo: rename.leadingAnchor),

            title.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 10),
            title.topAnchor.constraint(equalTo: row.topAnchor, constant: 8),
            title.trailingAnchor.constraint(equalTo: rename.leadingAnchor, constant: -6),

            subtitle.leadingAnchor.constraint(equalTo: title.leadingAnchor),
            subtitle.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 2),
            subtitle.trailingAnchor.constraint(equalTo: title.trailingAnchor),

            rename.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -6),
            rename.centerYAnchor.constraint(equalTo: row.centerYAnchor),
            rename.widthAnchor.constraint(equalToConstant: 28),
            rename.heightAnchor.constraint(equalToConstant: 28),
        ])
        return row
    }

    private func configureRenameRow(_ row: NSView, session: OverlaySessionItem) -> NSView {
        let field = NSTextField()
        field.translatesAutoresizingMaskIntoConstraints = false
        field.stringValue = session.title
        field.font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
        field.textColor = BlueyTheme.text
        field.isBezeled = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.target = self
        field.action = #selector(saveInlineRenameClicked(_:))
        field.tag = sessionIndex(session.id)
        renameField = field

        let save = NSButton(title: "", target: self, action: #selector(saveInlineRenameClicked(_:)))
        save.translatesAutoresizingMaskIntoConstraints = false
        save.isBordered = false
        save.tag = sessionIndex(session.id)
        save.contentTintColor = BlueyTheme.cyan
        save.toolTip = "Save recording name"
        if let image = symbolImage("checkmark") {
            image.isTemplate = true
            save.image = image
            save.imagePosition = .imageOnly
            save.imageScaling = .scaleProportionallyDown
        } else {
            save.title = "Save"
            save.font = NSFont.systemFont(ofSize: 9, weight: .bold)
        }

        row.addSubview(field)
        row.addSubview(save)
        NSLayoutConstraint.activate([
            row.heightAnchor.constraint(equalToConstant: 44),
            field.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 10),
            field.centerYAnchor.constraint(equalTo: row.centerYAnchor),
            field.trailingAnchor.constraint(equalTo: save.leadingAnchor, constant: -6),
            field.heightAnchor.constraint(equalToConstant: 28),

            save.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -6),
            save.centerYAnchor.constraint(equalTo: row.centerYAnchor),
            save.widthAnchor.constraint(equalToConstant: 28),
            save.heightAnchor.constraint(equalToConstant: 28),
        ])
        DispatchQueue.main.async { [weak self, weak field] in
            guard self?.editingSessionId == session.id else { return }
            self?.window?.makeFirstResponder(field)
            field?.selectText(nil)
        }
        return row
    }

    private func sessionIndex(_ id: String) -> Int {
        sessionItems.firstIndex(where: { $0.id == id }) ?? -1
    }

    @objc private func sessionRowClicked(_ sender: NSButton) {
        guard sender.tag >= 0, sender.tag < sessionItems.count else { return }
        let session = sessionItems[sender.tag]
        sessionDrawer.isHidden = true
        statusLabel.stringValue = session.title
        emitSessionOpen(id: session.id)
    }

    @objc private func renameSessionClicked(_ sender: NSButton) {
        guard sender.tag >= 0, sender.tag < sessionItems.count else { return }
        let session = sessionItems[sender.tag]
        editingSessionId = session.id
        setSessions(sessionItems)
    }

    @objc private func saveInlineRenameClicked(_ sender: NSControl) {
        guard sender.tag >= 0, sender.tag < sessionItems.count else { return }
        let session = sessionItems[sender.tag]
        let title = (renameField?.stringValue ?? session.title)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return }
        sessionItems[sender.tag] = OverlaySessionItem(
            id: session.id,
            title: title,
            subtitle: session.subtitle,
            isActive: session.isActive)
        editingSessionId = nil
        emitSessionRename(id: session.id, title: title)
        setSessions(sessionItems)
    }

    private func fileSymbol(for kind: String) -> String {
        switch kind {
        case "image", "diagram": return "photo"
        case "code": return "curlybraces"
        case "text": return "doc.plaintext"
        case "document": return "doc.text"
        default: return "doc"
        }
    }

    private func fileAccent(for kind: String) -> NSColor {
        switch kind {
        case "image", "diagram": return NSColor(red: 0.58, green: 0.74, blue: 1.0, alpha: 1.0)
        case "code": return NSColor(red: 0.58, green: 1.0, blue: 0.74, alpha: 1.0)
        case "text": return NSColor(red: 1.0, green: 0.82, blue: 0.42, alpha: 1.0)
        case "document": return NSColor(red: 1.0, green: 0.43, blue: 0.34, alpha: 1.0)
        default: return BlueyTheme.cyan
        }
    }

    private func extractCodeBlocks(from text: String) -> [String] {
        var blocks: [String] = []
        var current: [String] = []
        var inFence = false

        for line in text.components(separatedBy: .newlines) {
            if line.trimmingCharacters(in: .whitespaces).hasPrefix("```") {
                if inFence {
                    blocks.append(current.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines))
                    current.removeAll()
                }
                inFence.toggle()
                continue
            }
            if inFence {
                current.append(line)
            }
        }

        return blocks.filter { !$0.isEmpty }
    }

    private func looksLikeCode(_ lower: String) -> Bool {
        let signals = [
            "class solution",
            "def ",
            "function ",
            "const ",
            "let ",
            "public ",
            "private ",
            "time complexity",
            "space complexity",
            "test case",
            "edge case",
            "sql",
        ]
        return signals.filter { lower.contains($0) }.count >= 2
    }

    private func looksLikeSystemDesign(_ lower: String) -> Bool {
        let signals = [
            "system design",
            "architecture",
            "api",
            "database",
            "cache",
            "queue",
            "scale",
            "latency",
            "throughput",
            "tradeoff",
            "shard",
            "load balancer",
            "microservice",
            "event-driven",
        ]
        return signals.filter { lower.contains($0) }.count >= 3
    }

    private func looksLikeScreenAnalysis(_ lower: String) -> Bool {
        lower.contains("screenshot")
            || lower.contains("screen context")
            || lower.contains("analyse screen")
            || lower.contains("analyze screen")
            || lower.contains("image shows")
    }

    private func looksLikeDocumentWork(_ lower: String) -> Bool {
        lower.contains("attached document")
            || lower.contains("pdf")
            || lower.contains("resume")
            || lower.contains("document context")
            || lower.contains("source:")
    }

    private func hasStructuredShape(_ text: String) -> Bool {
        let lines = text.components(separatedBy: .newlines)
        let structured = lines.filter { line in
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            return trimmed.hasPrefix("- ")
                || trimmed.hasPrefix("* ")
                || trimmed.hasPrefix("#")
                || trimmed.range(of: #"^\d+[\.\)]\s"#, options: .regularExpression) != nil
        }
        return structured.count >= 3
    }

    private func formatCodeCanvas(body: String, codeBlocks: [String]) -> String {
        let notes = stripCodeFences(from: body)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        var sections: [String] = []

        if !codeBlocks.isEmpty {
            sections.append("CODE\n----\n" + codeBlocks.joined(separator: "\n\n// ---\n\n"))
        }

        if !notes.isEmpty {
            sections.append("NOTES\n-----\n" + notes)
        }

        return sections.isEmpty ? body : sections.joined(separator: "\n\n")
    }

    private func stripCodeFences(from text: String) -> String {
        var lines: [String] = []
        var inFence = false
        for line in text.components(separatedBy: .newlines) {
            if line.trimmingCharacters(in: .whitespaces).hasPrefix("```") {
                inFence.toggle()
                continue
            }
            if !inFence {
                lines.append(line)
            }
        }
        return lines.joined(separator: "\n")
    }

    private func formatStructuredCanvas(_ body: String, fallbackHeading: String) -> String {
        let clean = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty else { return fallbackHeading }
        if clean.hasPrefix("#") || clean.uppercased().hasPrefix(fallbackHeading.uppercased()) {
            return clean
        }
        return "\(fallbackHeading)\n" + String(repeating: "-", count: fallbackHeading.count) + "\n" + clean
    }

    private func selectedRoute() -> (provider: String?, model: String?, mode: String?) {
        switch modelMenu.indexOfSelectedItem {
        case 1:
            return ("openai", "gpt-4o-mini", "general")
        case 2:
            return ("anthropic", "claude-3-5-sonnet-latest", "general")
        case 3:
            return ("anthropic", "claude-3-7-sonnet-latest", "general")
        default:
            return ("auto", nil, "general")
        }
    }
}

// MARK: - Coordinator

private final class OverlayApp {
    private var pillWindow: OverlayWindow!
    private var expandedWindow: OverlayWindow?
    private var pillView: PillView!
    private var expandedView: ExpandedPanelView?

    /// Pending boot card, if a Boot command arrived before windows materialised.
    private var pendingBoot: (title: String, lines: [String])?

    func start() {
        // Pill window: compact, parked at the top-right by default.
        let pillSize = NSSize(width: 110, height: 30)
        let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1920, height: 1080)
        let pillOrigin = NSPoint(
            x: screen.maxX - pillSize.width - 16,
            y: screen.maxY - pillSize.height - 12)
        pillWindow = OverlayWindow(
            contentRect: NSRect(origin: pillOrigin, size: pillSize),
            draggable: true)

        pillView = PillView(frame: NSRect(origin: .zero, size: pillSize))
        pillWindow.contentView = pillView
        pillView.statusText = "Bluey"
        pillView.onClick = { [weak self] in self?.expand() }

        if captureVisibleForDebug {
            NSApp.activate(ignoringOtherApps: true)
        }
        bringPillToFront()
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { [weak self] in
            self?.bringPillToFront()
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.75) { [weak self] in
            self?.bringPillToFront()
        }

        emitReady()
        emitLifecycle("started", detail: "capture_excluded=\(!captureVisibleForDebug)")
        startParentWatchdog()
        startIpcLoop()
    }

    private func bringPillToFront() {
        pillWindow.setIsVisible(true)
        pillWindow.orderFrontRegardless()
        pillWindow.makeKeyAndOrderFront(nil)
        pillView.needsDisplay = true
        pillView.needsLayout = true
        pillView.layoutSubtreeIfNeeded()
        pillView.displayIfNeeded()
        pillWindow.displayIfNeeded()
    }

    private func startParentWatchdog() {
        // LaunchServices helper apps are reparented by macOS, so getppid()
        // is not a reliable daemon-liveness signal in socket IPC mode.
        // The socket reader below exits on EOF when the daemon goes away.
        if overlaySocketPath != nil { return }

        Timer.scheduledTimer(withTimeInterval: 2.0, repeats: true) { timer in
            if getppid() == 1 {
                timer.invalidate()
                NSApp.terminate(nil)
            }
        }
    }

    private func expand() {
        ensureExpandedWindow()
        // Reposition expanded just below pill's current frame so the user's
        // dragging is honoured.
        guard let expandedWindow else { return }
        if let pillFrame = pillWindow?.frame {
            let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1920, height: 1080)
            let inset: CGFloat = 12
            var ef = expandedWindow.frame
            ef.origin.x = pillFrame.maxX - ef.width
            ef.origin.x = min(max(screen.minX + inset, ef.origin.x), screen.maxX - ef.width - inset)
            ef.origin.y = pillFrame.minY - ef.height - 8
            ef.origin.y = min(max(screen.minY + inset, ef.origin.y), screen.maxY - ef.height - inset)
            expandedWindow.setFrame(ef, display: true)
        }
        expandedWindow.orderFrontRegardless()
        emitSimple("shown")
        emitLifecycle("expanded")
    }

    private func ensureExpandedWindow() {
        guard expandedWindow == nil else { return }
        let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1920, height: 1080)
        let expandedWidth = ExpandedPanelMetrics.fittingWidth(
            for: screen,
            preferred: ExpandedPanelMetrics.maxCompactWidth)
        let minimumWidth = ExpandedPanelMetrics.fittingMinimumWidth(for: screen, targetWidth: expandedWidth)
        let expandedSize = NSSize(width: expandedWidth, height: ExpandedPanelMetrics.height)
        let pillFrame = pillWindow?.frame ?? NSRect(
            x: screen.midX - 66,
            y: screen.maxY - 60,
            width: 110,
            height: 30)
        let expandedOrigin = NSPoint(
            x: screen.midX - expandedSize.width / 2,
            y: pillFrame.minY - expandedSize.height - 8)
        let window = OverlayWindow(
            contentRect: NSRect(origin: expandedOrigin, size: expandedSize),
            draggable: false)
        window.minimumFrameWidth = minimumWidth
        window.lockedFrameHeight = expandedSize.height
        // The expanded surface must stay compact vertically; otherwise AppKit can
        // grow the borderless window to satisfy dense feed/composer constraints.
        // Width can still expand intentionally for the canvas panel.
        let maxExpandedWidth = expandedWidth
        window.minSize = NSSize(width: minimumWidth, height: expandedSize.height)
        window.maxSize = NSSize(width: maxExpandedWidth, height: expandedSize.height)
        window.contentMinSize = NSSize(width: minimumWidth, height: expandedSize.height)
        window.contentMaxSize = NSSize(width: maxExpandedWidth, height: expandedSize.height)
        window.maximumFrameWidth = maxExpandedWidth
        let view = ExpandedPanelView(frame: NSRect(origin: .zero, size: expandedSize))
        window.contentView = view
        view.onClose = { [weak self] in self?.collapse() }
        view.onOpacityChanged = { [weak self] opacity in
            let value = CGFloat(opacity)
            self?.pillWindow?.alphaValue = value
            self?.expandedWindow?.alphaValue = value
        }
        expandedWindow = window
        expandedView = view
        if let pending = pendingBoot {
            pushBootCard(title: pending.title, lines: pending.lines)
            pendingBoot = nil
        }
    }

    private func collapse() {
        expandedWindow?.orderOut(nil)
        emitSimple("hidden")
        emitLifecycle("collapsed")
    }

    func handleCommand(_ cmd: OverlayCommand) {
        switch cmd {
        case .ping:
            emitSimple("pong")
        case .show:
            expand()
        case .hide:
            // Codex Stage 18 commit 5: smooth fade on the visible windows
            // + center-screen restore-toast for 2s.
            pillWindow?.fadeOutAndHide()
            expandedWindow?.fadeOutAndHide()
            RestoreToast.shared.show()
            emitLifecycle("hidden")

        case .toggle:
            if expandedWindow?.isVisible == true { collapse() } else { expand() }
        case .clear:
            expandedView?.resetSessionSurface()
        case .boot(let title, let lines):
            pushBootCard(title: title, lines: lines)
        case .setOpacity(let o):
            let value = min(max(o, 0.50), 1.0)
            pillWindow.alphaValue = CGFloat(value)
            expandedWindow?.alphaValue = CGFloat(value)
            expandedView?.applyOpacity(value)
        case .setPosition(let pos):
            applyPosition(pos)
        case .setBalance(let label):
            expandedView?.setBalanceLabel(label)
            pillView?.setBalanceLabel(label)
        case .setContextItems(let items):
            expandedView?.setContextItems(items)
        case .setSessions(let sessions):
            expandedView?.setSessions(sessions)
        case .pushCard(let card):
            ensureExpandedWindow()
            expandedView?.pushCard(RenderedCard(
                id: card.id, kind: card.kind, title: card.title,
                body: card.body, done: true, costLabel: card.costLabel,
                artifact: card.artifact))
        case .updateCard(let id, let body, let done, let costLabel, let artifact):
            ensureExpandedWindow()
            expandedView?.updateCard(id: id, body: body, done: done, costLabel: costLabel, artifact: artifact)
        case .shutdown:
            emitLifecycle("shutdown")
            NSApp.terminate(nil)
        case .unknown:
            break // log-only on stderr happens elsewhere; silently drop.
        }
    }

    private func pushBootCard(title: String, lines: [String]) {
        guard let view = expandedView else {
            pendingBoot = (title, lines)
            return
        }
        let body = lines.joined(separator: "\n")
        let card = RenderedCard(
            id: UUID().uuidString,
            kind: "system",
            title: title,
            body: body,
            done: true,
            costLabel: nil,
            artifact: nil)
        view.pushCard(card)
        // Briefly flash the pill to indicate boot activity.
        pillView?.dotColor = NSColor.systemBlue
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in
            self?.pillView?.dotColor = NSColor.systemGreen
        }
    }

    private func applyPosition(_ pos: String) {
        guard let screen = NSScreen.main?.visibleFrame else { return }
        let pillSize = pillWindow.frame.size
        let inset: CGFloat = 12
        let origin: NSPoint
        switch pos {
        case "top_left":     origin = NSPoint(x: screen.minX + inset,                  y: screen.maxY - pillSize.height - inset)
        case "top_right":    origin = NSPoint(x: screen.maxX - pillSize.width - inset, y: screen.maxY - pillSize.height - inset)
        case "bottom_left":  origin = NSPoint(x: screen.minX + inset,                  y: screen.minY + inset)
        case "bottom_right": origin = NSPoint(x: screen.maxX - pillSize.width - inset, y: screen.minY + inset)
        case "center":       origin = NSPoint(x: screen.midX - pillSize.width / 2,     y: screen.midY - pillSize.height / 2)
        default:             origin = NSPoint(x: screen.midX - pillSize.width / 2,     y: screen.maxY - pillSize.height - inset)
        }
        pillWindow.setFrameOrigin(origin)
    }

    private func startIpcLoop() {
        // Background thread reads NDJSON from the socket in production, or
        // stdin in tests/manual protocol checks, then dispatches commands onto
        // the main thread because AppKit must run on main.
        let handle = ipcInputHandle ?? FileHandle.standardInput
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            var buffer = Data()
            while true {
                let chunk = handle.availableData
                if chunk.isEmpty {
                    DispatchQueue.main.async { NSApp.terminate(nil) }
                    return
                }
                buffer.append(chunk)
                while let nl = buffer.firstIndex(of: 0x0A) {
                    let lineData = buffer.subdata(in: 0..<nl)
                    buffer.removeSubrange(0...nl)
                    if let line = String(data: lineData, encoding: .utf8), !line.isEmpty {
                        let cmd = parseCommand(line)
                        DispatchQueue.main.async { [weak self] in
                            self?.handleCommand(cmd)
                        }
                    }
                }
            }
        }
    }
}

// MARK: - Entry point

private final class AppDelegate: NSObject, NSApplicationDelegate {
    private let coord = OverlayApp()
    func applicationDidFinishLaunching(_ notification: Notification) {
        connectIpcIfNeeded()
        coord.start()
    }
}

private let app = NSApplication.shared
app.setActivationPolicy(captureVisibleForDebug ? .regular : .accessory)
private let delegate = AppDelegate()
app.delegate = delegate
if captureVisibleForDebug {
    app.activate(ignoringOtherApps: true)
}
app.run()


// ─── Codex Stage 18 commit 5: smooth fade + restore toast ───────────────

extension NSWindow {
    func fadeOutAndHide(duration: TimeInterval = 0.25) {
        NSAnimationContext.runAnimationGroup({ ctx in
            ctx.duration = duration
            ctx.timingFunction = CAMediaTimingFunction(name: .easeOut)
            self.animator().alphaValue = 0
        }, completionHandler: {
            self.orderOut(nil)
            self.alphaValue = 1.0  // restore for next show
        })
    }

    func fadeInAndShow(duration: TimeInterval = 0.18) {
        self.alphaValue = 0
        self.makeKeyAndOrderFront(nil)
        NSAnimationContext.runAnimationGroup { ctx in
            ctx.duration = duration
            ctx.timingFunction = CAMediaTimingFunction(name: .easeOut)
            self.animator().alphaValue = 1.0
        }
    }
}

private final class RestoreToast {
    static let shared = RestoreToast()
    private var window: NSWindow?
    private var dismissTimer: Timer?

    func show() {
        // Singleton: dismiss any existing toast first.
        dismiss(animated: false)

        guard let mainScreen = NSScreen.main else { return }
        let screenFrame = mainScreen.visibleFrame
        let toastWidth: CGFloat = 280
        let toastHeight: CGFloat = 44
        let frame = NSRect(
            x: screenFrame.midX - toastWidth / 2,
            y: screenFrame.minY + 80,
            width: toastWidth,
            height: toastHeight
        )

        let win = NSWindow(
            contentRect: frame,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        win.isOpaque = false
        win.backgroundColor = .clear
        win.level = .statusBar
        win.ignoresMouseEvents = true
        win.collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]

        let bg = NSVisualEffectView(frame: NSRect(origin: .zero, size: frame.size))
        bg.material = .hudWindow
        bg.blendingMode = .behindWindow
        bg.state = .active
        bg.wantsLayer = true
        bg.layer?.cornerRadius = 12
        bg.layer?.masksToBounds = true

        let label = NSTextField(labelWithString: "Bluey hidden — press F19 to restore")
        label.alignment = .center
        label.font = NSFont.systemFont(ofSize: 13, weight: .medium)
        label.textColor = NSColor(white: 0.95, alpha: 1.0)
        label.frame = NSRect(x: 12, y: 12, width: frame.size.width - 24, height: 20)
        bg.addSubview(label)

        win.contentView = bg
        win.alphaValue = 0
        win.orderFront(nil)
        NSAnimationContext.runAnimationGroup { ctx in
            ctx.duration = 0.18
            ctx.timingFunction = CAMediaTimingFunction(name: .easeOut)
            win.animator().alphaValue = 1.0
        }
        self.window = win

        // Dismiss after 2 seconds.
        dismissTimer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: false) { [weak self] _ in
            self?.dismiss(animated: true)
        }
    }

    func dismiss(animated: Bool) {
        dismissTimer?.invalidate()
        dismissTimer = nil
        guard let win = window else { return }
        if animated {
            NSAnimationContext.runAnimationGroup({ ctx in
                ctx.duration = 0.18
                ctx.timingFunction = CAMediaTimingFunction(name: .easeOut)
                win.animator().alphaValue = 0
            }, completionHandler: {
                win.orderOut(nil)
                self.window = nil
            })
        } else {
            win.orderOut(nil)
            window = nil
        }
    }
}

// Convenience for OverlayWindowController to call.
extension NSWindow {
    func showRestoreToast() {
        RestoreToast.shared.show()
    }
}
