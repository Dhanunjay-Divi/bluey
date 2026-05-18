// bluey-overlay-macos / cue-overlay-macos
//
// Native macOS overlay process. Speaks NDJSON IPC over stdin/stdout with the
// Bluey daemon. Provides:
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
import Foundation

// MARK: - Protocol

private struct CueCard: Decodable {
    let id: String
    let kind: String
    let title: String
    let body: String
    let createdAt: String?
    let source: String?

    enum CodingKeys: String, CodingKey {
        case id, kind, title, body
        case createdAt = "created_at"
        case source
    }
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
    case pushCard(CueCard)
    case updateCard(id: String, body: String, done: Bool)
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
        return .updateCard(id: id, body: body, done: done)
    default:
        return .unknown(line)
    }
}

/// Outbound events to the daemon. Every event carries the session token
/// embedded as the top-level "token" field; the daemon's
/// validate_and_decode_overlay_line function rejects events without it.
private let sessionToken: String = ProcessInfo.processInfo
    .environment["BLUEY_OVERLAY_SESSION_TOKEN"] ?? ""

private func emitEvent(_ payload: [String: Any]) {
    var withToken = payload
    if !sessionToken.isEmpty {
        withToken["token"] = sessionToken
    }
    guard let data = try? JSONSerialization.data(withJSONObject: withToken),
          let json = String(data: data, encoding: .utf8)
    else { return }
    FileHandle.standardOutput.write((json + "\n").data(using: .utf8)!)
}

private func emitReady() {
    emitEvent([
        "type": "ready",
        "platform": "macos",
        "capture_excluded": true,
    ])
}

private func emitSimple(_ type: String) {
    emitEvent(["type": type])
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

private func emitCardRendered(id: String) {
    emitEvent(["type": "card_rendered", "id": id])
}

// MARK: - Capture-excluded NSWindow

/// Borderless, transparent, always-on-top, excluded from screen capture.
/// Configured for either the small pill or the expanded feed depending on
/// the size passed at construction time.
private final class OverlayWindow: NSWindow {
    init(contentRect: NSRect, draggable: Bool) {
        super.init(
            contentRect: contentRect,
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        self.isOpaque = false
        self.backgroundColor = .clear
        self.hasShadow = true
        self.level = .floating
        self.collectionBehavior = [
            .canJoinAllSpaces,
            .stationary,
            .ignoresCycle,
            .fullScreenAuxiliary,
        ]
        self.isMovableByWindowBackground = draggable
        self.hidesOnDeactivate = false
        // sharingType = .none excludes the window from ScreenCaptureKit /
        // legacy CGWindowList captures so screenshares do not show it.
        self.sharingType = .none
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }
}

// MARK: - Pill view

private final class PillView: NSView {
    var statusText: String = "Bluey" { didSet { needsDisplay = true } }
    var dotColor: NSColor = NSColor.systemGreen { didSet { needsDisplay = true } }
    var onClick: (() -> Void)?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        self.wantsLayer = true
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        // Pill background.
        let bg = NSColor(white: 0.05, alpha: 0.92)
        let path = NSBezierPath(roundedRect: bounds, xRadius: bounds.height / 2, yRadius: bounds.height / 2)
        bg.setFill()
        path.fill()

        // Border.
        NSColor(white: 1.0, alpha: 0.10).setStroke()
        path.lineWidth = 1
        path.stroke()

        // Status dot.
        let dotSize: CGFloat = 8
        let dotRect = NSRect(
            x: 12,
            y: (bounds.height - dotSize) / 2,
            width: dotSize, height: dotSize)
        dotColor.setFill()
        NSBezierPath(ovalIn: dotRect).fill()

        // Label "Bluey ▾"
        let label = "\(statusText)  ▾" as NSString
        let attrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 12, weight: .medium),
            .foregroundColor: NSColor.white,
        ]
        let textRect = NSRect(
            x: 28, y: 0,
            width: bounds.width - 36,
            height: bounds.height)
        let para = NSMutableParagraphStyle()
        para.alignment = .center
        var allAttrs = attrs
        allAttrs[.paragraphStyle] = para
        label.draw(
            in: textRect.insetBy(dx: 0, dy: (bounds.height - 16) / 2),
            withAttributes: allAttrs)
    }

    override func mouseDown(with event: NSEvent) {
        // If user is dragging (movableByWindowBackground), AppKit handles it;
        // a click without movement triggers onClick.
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
                if abs(dx) > 4 || abs(dy) > 4 { didDrag = true }
                window?.performDrag(with: next)
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
}

private final class FeedView: NSView {
    private var cards: [RenderedCard] = []
    private let stack = NSStackView()
    private let scroll = NSScrollView()

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.backgroundColor = NSColor(white: 0.05, alpha: 0.94).cgColor
        layer?.cornerRadius = 14
        layer?.borderWidth = 1
        layer?.borderColor = NSColor(white: 1.0, alpha: 0.10).cgColor

        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 8
        stack.edgeInsets = NSEdgeInsets(top: 12, left: 12, bottom: 12, right: 12)
        stack.translatesAutoresizingMaskIntoConstraints = false

        scroll.hasVerticalScroller = true
        scroll.drawsBackground = false
        scroll.documentView = stack
        scroll.translatesAutoresizingMaskIntoConstraints = false
        addSubview(scroll)
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
        cards.append(card)
        stack.addArrangedSubview(makeCardView(card))
        scrollToBottom()
        emitCardRendered(id: card.id)
    }

    func update(id: String, body: String, done: Bool) {
        guard let idx = cards.firstIndex(where: { $0.id == id }) else { return }
        cards[idx].body = body
        cards[idx].done = done
        // Replace the corresponding subview.
        let existing = stack.arrangedSubviews[idx]
        stack.removeArrangedSubview(existing)
        existing.removeFromSuperview()
        stack.insertArrangedSubview(makeCardView(cards[idx]), at: idx)
        scrollToBottom()
    }

    func clear() {
        cards.removeAll()
        for v in stack.arrangedSubviews { v.removeFromSuperview() }
    }

    private func makeCardView(_ card: RenderedCard) -> NSView {
        let container = NSView()
        container.wantsLayer = true
        container.layer?.backgroundColor = NSColor(white: 0.10, alpha: 1.0).cgColor
        container.layer?.cornerRadius = 8
        container.translatesAutoresizingMaskIntoConstraints = false

        let titleLabel = NSTextField(labelWithString: "\(kindBadge(card.kind))  \(card.title)")
        titleLabel.font = NSFont.systemFont(ofSize: 11, weight: .semibold)
        titleLabel.textColor = NSColor(white: 0.85, alpha: 1.0)
        titleLabel.translatesAutoresizingMaskIntoConstraints = false

        let bodyLabel = NSTextField(wrappingLabelWithString: card.body)
        bodyLabel.font = NSFont.systemFont(ofSize: 13)
        bodyLabel.textColor = .white
        bodyLabel.translatesAutoresizingMaskIntoConstraints = false
        bodyLabel.preferredMaxLayoutWidth = 420

        container.addSubview(titleLabel)
        container.addSubview(bodyLabel)
        NSLayoutConstraint.activate([
            titleLabel.topAnchor.constraint(equalTo: container.topAnchor, constant: 8),
            titleLabel.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 10),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: container.trailingAnchor, constant: -10),
            bodyLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 4),
            bodyLabel.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 10),
            bodyLabel.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -10),
            bodyLabel.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -8),
        ])
        return container
    }

    private func kindBadge(_ kind: String) -> String {
        switch kind {
        case "answer":      return "💬"
        case "question":    return "❓"
        case "action_item": return "✅"
        case "decision":    return "📌"
        case "context":     return "📎"
        case "transcript":  return "🎙️"
        case "warning":     return "⚠️"
        case "system":      return "🔵"
        default:            return "•"
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

// MARK: - Expanded panel (feed + composer)

private final class ExpandedPanelView: NSView {
    let feed: FeedView
    let composer: NSTextField
    let askButton: NSButton
    let attachButton: NSButton
    let instructionsButton: NSButton
    let recapButton: NSButton
    let closeButton: NSButton

    var onClose: (() -> Void)?

    override init(frame frameRect: NSRect) {
        feed = FeedView(frame: .zero)
        composer = NSTextField()
        composer.placeholderString = "Ask Bluey…"
        composer.font = NSFont.systemFont(ofSize: 13)
        composer.bezelStyle = .roundedBezel

        askButton           = NSButton(title: "Ask",          target: nil, action: nil)
        attachButton        = NSButton(title: "Attach",       target: nil, action: nil)
        instructionsButton  = NSButton(title: "Instructions", target: nil, action: nil)
        recapButton         = NSButton(title: "Recap",        target: nil, action: nil)
        closeButton         = NSButton(title: "✕",            target: nil, action: nil)

        super.init(frame: frameRect)

        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor

        feed.translatesAutoresizingMaskIntoConstraints = false
        composer.translatesAutoresizingMaskIntoConstraints = false
        askButton.translatesAutoresizingMaskIntoConstraints = false
        attachButton.translatesAutoresizingMaskIntoConstraints = false
        instructionsButton.translatesAutoresizingMaskIntoConstraints = false
        recapButton.translatesAutoresizingMaskIntoConstraints = false
        closeButton.translatesAutoresizingMaskIntoConstraints = false

        let buttonRow = NSStackView(views: [askButton, attachButton, instructionsButton, recapButton])
        buttonRow.orientation = .horizontal
        buttonRow.spacing = 6
        buttonRow.translatesAutoresizingMaskIntoConstraints = false

        addSubview(feed)
        addSubview(closeButton)
        addSubview(composer)
        addSubview(buttonRow)

        NSLayoutConstraint.activate([
            closeButton.topAnchor.constraint(equalTo: topAnchor, constant: 8),
            closeButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            closeButton.widthAnchor.constraint(equalToConstant: 24),
            closeButton.heightAnchor.constraint(equalToConstant: 24),

            feed.topAnchor.constraint(equalTo: topAnchor, constant: 4),
            feed.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 4),
            feed.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -4),
            feed.bottomAnchor.constraint(equalTo: composer.topAnchor, constant: -8),

            composer.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            composer.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            composer.bottomAnchor.constraint(equalTo: buttonRow.topAnchor, constant: -6),
            composer.heightAnchor.constraint(equalToConstant: 26),

            buttonRow.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            buttonRow.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -8),
            buttonRow.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -8),
            buttonRow.heightAnchor.constraint(equalToConstant: 26),
        ])

        closeButton.target = self
        closeButton.action = #selector(closeClicked)
        closeButton.isBordered = false
        closeButton.font = NSFont.systemFont(ofSize: 14, weight: .bold)

        askButton.target = self
        askButton.action = #selector(askClicked)
        attachButton.target = self
        attachButton.action = #selector(attachClicked)
        instructionsButton.target = self
        instructionsButton.action = #selector(instructionsClicked)
        recapButton.target = self
        recapButton.action = #selector(recapClicked)
    }
    required init?(coder: NSCoder) { fatalError() }

    @objc private func closeClicked() { onClose?() }

    @objc private func askClicked() {
        let q = composer.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !q.isEmpty else { return }
        composer.stringValue = ""
        emitAsk(question: q, provider: nil, model: nil, mode: nil)
    }

    @objc private func attachClicked() {
        // AttachRequested is the entry-point event; the daemon will drive
        // its file picker logic. We do NOT open NSOpenPanel here \u2014 keeping
        // the daemon owning the dialog matches R12.2's state-machine model.
        emitSimple("attach_requested")
    }

    @objc private func instructionsClicked() {
        emitSimple("instructions_requested")
    }

    @objc private func recapClicked() {
        emitSimple("recap_requested")
    }
}

// MARK: - Coordinator

private final class OverlayApp {
    private var pillWindow: OverlayWindow!
    private var expandedWindow: OverlayWindow!
    private var pillView: PillView!
    private var expandedView: ExpandedPanelView!

    /// Pending boot card, if a Boot command arrived before windows materialised.
    private var pendingBoot: (title: String, lines: [String])?

    func start() {
        // Pill window: small, top center.
        let pillSize = NSSize(width: 160, height: 32)
        let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1920, height: 1080)
        let pillOrigin = NSPoint(
            x: screen.midX - pillSize.width / 2,
            y: screen.maxY - pillSize.height - 12)
        pillWindow = OverlayWindow(
            contentRect: NSRect(origin: pillOrigin, size: pillSize),
            draggable: true)

        pillView = PillView(frame: NSRect(origin: .zero, size: pillSize))
        pillWindow.contentView = pillView
        pillView.statusText = "Bluey"
        pillView.onClick = { [weak self] in self?.expand() }

        // Expanded window: larger, anchored under the pill.
        let expandedSize = NSSize(width: 480, height: 560)
        let expandedOrigin = NSPoint(
            x: screen.midX - expandedSize.width / 2,
            y: pillOrigin.y - expandedSize.height - 8)
        expandedWindow = OverlayWindow(
            contentRect: NSRect(origin: expandedOrigin, size: expandedSize),
            draggable: false)
        expandedView = ExpandedPanelView(
            frame: NSRect(origin: .zero, size: expandedSize))
        expandedWindow.contentView = expandedView
        expandedView.onClose = { [weak self] in self?.collapse() }

        pillWindow.orderFrontRegardless()
        // Expanded starts hidden.
        expandedWindow.orderOut(nil)

        // If a boot card arrived before windows existed, render it now.
        if let pending = pendingBoot {
            pushBootCard(title: pending.title, lines: pending.lines)
            pendingBoot = nil
        }

        emitReady()
        startStdinLoop()
    }

    private func expand() {
        // Reposition expanded just below pill's current frame so the user's
        // dragging is honoured.
        if let pillFrame = pillWindow?.frame {
            var ef = expandedWindow.frame
            ef.origin.x = pillFrame.midX - ef.width / 2
            ef.origin.y = pillFrame.minY - ef.height - 8
            expandedWindow.setFrame(ef, display: true)
        }
        expandedWindow.orderFrontRegardless()
        emitSimple("shown")
    }

    private func collapse() {
        expandedWindow.orderOut(nil)
        emitSimple("hidden")
    }

    func handleCommand(_ cmd: OverlayCommand) {
        switch cmd {
        case .ping:
            emitSimple("pong")
        case .show:
            expand()
        case .hide:
            collapse()
        case .toggle:
            if expandedWindow.isVisible { collapse() } else { expand() }
        case .clear:
            expandedView?.feed.clear()
        case .boot(let title, let lines):
            pushBootCard(title: title, lines: lines)
        case .setOpacity(let o):
            pillWindow.alphaValue = CGFloat(o)
            expandedWindow.alphaValue = CGFloat(o)
        case .setPosition(let pos):
            applyPosition(pos)
        case .pushCard(let card):
            expandedView?.feed.push(RenderedCard(
                id: card.id, kind: card.kind, title: card.title,
                body: card.body, done: true))
        case .updateCard(let id, let body, let done):
            expandedView?.feed.update(id: id, body: body, done: done)
        case .shutdown:
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
            done: true)
        view.feed.push(card)
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

    private func startStdinLoop() {
        // Background thread reads NDJSON from stdin and dispatches commands
        // onto the main thread (AppKit must run on main).
        let handle = FileHandle.standardInput
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
        coord.start()
    }
}

private let app = NSApplication.shared
app.setActivationPolicy(.accessory)
private let delegate = AppDelegate()
app.delegate = delegate
app.run()
