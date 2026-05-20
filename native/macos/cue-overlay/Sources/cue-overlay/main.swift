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

private func symbolImage(_ name: String) -> NSImage? {
    guard let image = NSImage(systemSymbolName: name, accessibilityDescription: nil) else {
        return nil
    }
    return image.withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 13, weight: .semibold)) ?? image
}

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

private struct OverlayContextItem {
    let id: String
    let title: String
    let kind: String
    let path: String?
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
        NSGraphicsContext.saveGraphicsState()

        let outer = bounds.insetBy(dx: 1, dy: 1)
        let path = NSBezierPath(roundedRect: outer, xRadius: outer.height / 2, yRadius: outer.height / 2)
        let shadow = NSShadow()
        shadow.shadowColor = NSColor(red: 0.08, green: 0.58, blue: 0.90, alpha: 0.16)
        shadow.shadowBlurRadius = 8
        shadow.shadowOffset = .zero
        shadow.set()

        let bg = NSGradient(colors: [
            NSColor(red: 0.015, green: 0.045, blue: 0.075, alpha: 0.96),
            NSColor(red: 0.025, green: 0.090, blue: 0.135, alpha: 0.93),
        ])
        bg?.draw(in: path, angle: 0)

        NSGraphicsContext.restoreGraphicsState()

        NSColor(red: 0.26, green: 0.74, blue: 0.95, alpha: 0.55).setStroke()
        path.lineWidth = 1
        path.stroke()

        let inner = outer.insetBy(dx: 2, dy: 2)
        let innerPath = NSBezierPath(roundedRect: inner, xRadius: inner.height / 2, yRadius: inner.height / 2)
        NSColor.white.withAlphaComponent(0.06).setStroke()
        innerPath.lineWidth = 1
        innerPath.stroke()

        drawLogo(in: NSRect(x: 5, y: 3, width: 18, height: 18))

        let labelRect = NSRect(x: 30, y: 4.5, width: bounds.width - 44, height: 15)
        let label = statusText as NSString
        let labelAttrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 12, weight: .semibold),
            .foregroundColor: NSColor(red: 0.92, green: 0.98, blue: 1.0, alpha: 1.0),
        ]
        label.draw(in: labelRect, withAttributes: labelAttrs)

        let labelWidth = ceil(label.size(withAttributes: labelAttrs).width)
        let dotSize: CGFloat = 5
        let dotX = min(labelRect.minX + labelWidth + 3, bounds.width - dotSize - 8)
        let dotRect = NSRect(
            x: dotX,
            y: bounds.height - dotSize - 5,
            width: dotSize,
            height: dotSize)
        let dotGlow = NSShadow()
        dotGlow.shadowColor = dotColor.withAlphaComponent(0.70)
        dotGlow.shadowBlurRadius = 6
        dotGlow.shadowOffset = .zero
        NSGraphicsContext.saveGraphicsState()
        dotGlow.set()
        dotColor.setFill()
        NSBezierPath(ovalIn: dotRect).fill()
        NSGraphicsContext.restoreGraphicsState()
        NSColor.white.withAlphaComponent(0.45).setStroke()
        NSBezierPath(ovalIn: dotRect.insetBy(dx: -1, dy: -1)).stroke()
    }

    private func drawLogo(in rect: NSRect) {
        let bgPath = NSBezierPath(roundedRect: rect, xRadius: 8, yRadius: 8)
        NSGradient(colors: [
            NSColor(red: 0.03, green: 0.12, blue: 0.23, alpha: 1.0),
            NSColor(red: 0.06, green: 0.36, blue: 0.52, alpha: 1.0),
        ])?.draw(in: bgPath, angle: -40)
        NSColor(red: 0.42, green: 0.92, blue: 1.0, alpha: 0.9).setStroke()
        bgPath.lineWidth = 1.5
        bgPath.stroke()

        let screen = rect.insetBy(dx: 4.5, dy: 5.5)
        let screenPath = NSBezierPath(roundedRect: screen, xRadius: 4.5, yRadius: 4.5)
        NSColor(red: 0.015, green: 0.055, blue: 0.10, alpha: 1.0).setFill()
        screenPath.fill()
        NSColor(red: 0.36, green: 0.90, blue: 1.0, alpha: 0.95).setStroke()
        screenPath.lineWidth = 1.4
        screenPath.stroke()

        let prompt = NSBezierPath()
        prompt.move(to: NSPoint(x: screen.minX + 3.5, y: screen.midY + 3.5))
        prompt.line(to: NSPoint(x: screen.minX + 7, y: screen.midY))
        prompt.line(to: NSPoint(x: screen.minX + 3.5, y: screen.midY - 3.5))
        NSColor.white.setStroke()
        prompt.lineWidth = 1.8
        prompt.lineCapStyle = .round
        prompt.lineJoinStyle = .round
        prompt.stroke()

        let cursor = NSBezierPath()
        cursor.move(to: NSPoint(x: screen.minX + 9.5, y: screen.midY - 3.5))
        cursor.line(to: NSPoint(x: screen.maxX - 3.5, y: screen.midY - 3.5))
        NSColor(red: 0.45, green: 0.96, blue: 1.0, alpha: 1.0).setStroke()
        cursor.lineWidth = 2
        cursor.lineCapStyle = .round
        cursor.stroke()

        let sparkle = NSBezierPath()
        let cx = rect.maxX - 4
        let cy = rect.maxY - 5
        sparkle.move(to: NSPoint(x: cx, y: cy + 3))
        sparkle.line(to: NSPoint(x: cx, y: cy - 3))
        sparkle.move(to: NSPoint(x: cx - 3, y: cy))
        sparkle.line(to: NSPoint(x: cx + 3, y: cy))
        NSColor(red: 0.55, green: 1.0, blue: 0.62, alpha: 0.95).setStroke()
        sparkle.lineWidth = 1.4
        sparkle.lineCapStyle = .round
        sparkle.stroke()
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
}

private final class FeedView: NSView {
    private var cards: [RenderedCard] = []
    private let stack = NSStackView()
    private let scroll = NSScrollView()
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
        let view = makeCardView(card)
        stack.addArrangedSubview(view)
        view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        scrollToBottom()
        if card.kind == "transcript" {
            onTranscript?(card)
        }
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
        let view = makeCardView(cards[idx])
        stack.insertArrangedSubview(view, at: idx)
        view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        scrollToBottom()
        if cards[idx].kind == "transcript" {
            onTranscript?(cards[idx])
        }
    }

    func clear() {
        cards.removeAll()
        for v in stack.arrangedSubviews { v.removeFromSuperview() }
    }

    private func makeCardView(_ card: RenderedCard) -> NSView {
        let accent = BlueyTheme.accent(for: card.kind)
        let rightAligned = isUserSide(card)
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false

        let bubble = NSView()
        bubble.wantsLayer = true
        bubble.layer?.backgroundColor = rightAligned
            ? NSColor(red: 0.90, green: 0.93, blue: 0.95, alpha: 0.96).cgColor
            : BlueyTheme.surface.cgColor
        bubble.layer?.cornerRadius = 16
        bubble.layer?.borderWidth = 1
        bubble.layer?.borderColor = rightAligned
            ? NSColor.white.withAlphaComponent(0.20).cgColor
            : accent.withAlphaComponent(card.kind == "answer" ? 0.24 : 0.14).cgColor
        bubble.layer?.shadowColor = NSColor.black.cgColor
        bubble.layer?.shadowOpacity = 0.16
        bubble.layer?.shadowRadius = 10
        bubble.layer?.shadowOffset = NSSize(width: 0, height: -4)
        bubble.translatesAutoresizingMaskIntoConstraints = false

        let metaLabel = NSTextField(labelWithString: kindLabel(card))
        metaLabel.font = NSFont.systemFont(ofSize: 11, weight: .bold)
        metaLabel.textColor = rightAligned ? NSColor.black.withAlphaComponent(0.58) : accent
        metaLabel.translatesAutoresizingMaskIntoConstraints = false

        let titleLabel = NSTextField(labelWithString: card.title.isEmpty ? kindTitle(card.kind) : card.title)
        titleLabel.font = NSFont.systemFont(ofSize: 12.5, weight: .semibold)
        titleLabel.textColor = rightAligned ? NSColor.black.withAlphaComponent(0.74) : BlueyTheme.text
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.lineBreakMode = .byTruncatingTail

        let bodyText = card.body.isEmpty && !card.done ? "Thinking..." : card.body
        let bodyLabel = NSTextField(wrappingLabelWithString: bodyText)
        bodyLabel.font = bodyFont(for: card)
        bodyLabel.textColor = rightAligned ? NSColor.black : BlueyTheme.text
        bodyLabel.translatesAutoresizingMaskIntoConstraints = false
        bodyLabel.preferredMaxLayoutWidth = 390

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
            bubble.widthAnchor.constraint(lessThanOrEqualTo: row.widthAnchor, multiplier: rightAligned ? 0.72 : 0.78),
            bubble.widthAnchor.constraint(greaterThanOrEqualToConstant: 190),

            metaLabel.topAnchor.constraint(equalTo: bubble.topAnchor, constant: 10),
            metaLabel.leadingAnchor.constraint(equalTo: bubble.leadingAnchor, constant: 14),

            titleLabel.centerYAnchor.constraint(equalTo: metaLabel.centerYAnchor),
            titleLabel.leadingAnchor.constraint(equalTo: metaLabel.leadingAnchor),
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

    private func isUserSide(_ card: RenderedCard) -> Bool {
        card.kind == "question" || card.kind == "transcript"
    }

    private func bodyFont(for card: RenderedCard) -> NSFont {
        if card.kind == "answer", card.body.contains("```") {
            return NSFont.monospacedSystemFont(ofSize: 12.5, weight: .regular)
        }
        return NSFont.systemFont(ofSize: 13.5, weight: .regular)
    }

    private func statusText(for card: RenderedCard) -> String {
        if !card.done { return "streaming..." }
        switch card.kind {
        case "answer":   return "cost syncing"
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

// MARK: - Expanded panel (feed + composer)

private final class ExpandedPanelView: NSView {
    let feed: FeedView
    let headerBar: NSView
    let titleLabel: NSTextField
    let statusLabel: NSTextField
    let modelMenu: NSPopUpButton
    let balanceLabel: NSTextField
    let navButton: NSButton
    let newSessionButton: NSButton
    let sessionDrawer: NSView
    let drawerTitleLabel: NSTextField
    let drawerSubtitleLabel: NSTextField
    let latestSessionButton: NSButton
    let transcriptStrip: NSView
    let transcriptLabel: NSTextField
    let attachmentStrip: NSScrollView
    let attachmentStack: NSStackView
    let composerBar: NSView
    let composer: NSTextField
    let recordingButton: NSButton
    let askButton: NSButton
    let analyzeButton: NSButton
    let attachButton: NSButton
    let instructionsButton: NSButton
    let closeButton: NSButton

    var onClose: (() -> Void)?
    private var recordingActive = false
    private var transcriptSnippets: [String] = []

    override init(frame frameRect: NSRect) {
        feed = FeedView(frame: .zero)
        headerBar = NSView()
        titleLabel = NSTextField(labelWithString: "Bluey")
        statusLabel = NSTextField(labelWithString: "New recording")
        modelMenu = NSPopUpButton(frame: .zero, pullsDown: false)
        balanceLabel = NSTextField(labelWithString: "Balance --")
        navButton = NSButton(title: "", target: nil, action: nil)
        newSessionButton = NSButton(title: "", target: nil, action: nil)
        sessionDrawer = NSView()
        drawerTitleLabel = NSTextField(labelWithString: "Chats")
        drawerSubtitleLabel = NSTextField(labelWithString: "Saved recordings stay here.")
        latestSessionButton = NSButton(title: "Latest recording", target: nil, action: nil)
        transcriptStrip = NSView()
        transcriptLabel = NSTextField(labelWithString: "Transcript will appear here while you listen")
        attachmentStrip = NSScrollView()
        attachmentStack = NSStackView()
        composerBar = NSView()
        composer = NSTextField()
        recordingButton = NSButton(title: "Listen", target: nil, action: nil)
        askButton = NSButton(title: "Answer", target: nil, action: nil)
        analyzeButton = NSButton(title: "", target: nil, action: nil)
        attachButton = NSButton(title: "", target: nil, action: nil)
        instructionsButton = NSButton(title: "", target: nil, action: nil)
        closeButton = NSButton(title: "x", target: nil, action: nil)

        super.init(frame: frameRect)

        wantsLayer = true
        layer?.backgroundColor = BlueyTheme.panelDeep.cgColor
        layer?.cornerRadius = 22
        layer?.borderWidth = 1
        layer?.borderColor = BlueyTheme.hairline.cgColor

        configureHeader()
        configureContextRows()
        configureComposer()
        feed.onTranscript = { [weak self] card in
            self?.appendTranscriptSnippet(card)
        }

        for view in [
            headerBar,
            titleLabel,
            statusLabel,
            modelMenu,
            balanceLabel,
            navButton,
            newSessionButton,
            sessionDrawer,
            drawerTitleLabel,
            drawerSubtitleLabel,
            latestSessionButton,
            feed,
            transcriptStrip,
            transcriptLabel,
            attachmentStrip,
            attachmentStack,
            composerBar,
            composer,
            recordingButton,
            askButton,
            analyzeButton,
            attachButton,
            instructionsButton,
            closeButton,
        ] {
            view.translatesAutoresizingMaskIntoConstraints = false
        }

        headerBar.addSubview(navButton)
        headerBar.addSubview(newSessionButton)
        headerBar.addSubview(modelMenu)
        headerBar.addSubview(balanceLabel)
        headerBar.addSubview(closeButton)
        addSubview(headerBar)
        addSubview(feed)
        addSubview(sessionDrawer)
        sessionDrawer.addSubview(drawerTitleLabel)
        sessionDrawer.addSubview(drawerSubtitleLabel)
        sessionDrawer.addSubview(latestSessionButton)
        addSubview(transcriptStrip)
        transcriptStrip.addSubview(transcriptLabel)
        addSubview(attachmentStrip)
        addSubview(composerBar)
        composerBar.addSubview(recordingButton)
        composerBar.addSubview(composer)
        composerBar.addSubview(attachButton)
        composerBar.addSubview(instructionsButton)
        composerBar.addSubview(analyzeButton)
        composerBar.addSubview(askButton)

        NSLayoutConstraint.activate([
            headerBar.topAnchor.constraint(equalTo: topAnchor, constant: 10),
            headerBar.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            headerBar.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            headerBar.heightAnchor.constraint(equalToConstant: 40),

            navButton.leadingAnchor.constraint(equalTo: headerBar.leadingAnchor, constant: 8),
            navButton.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            navButton.widthAnchor.constraint(equalToConstant: 32),
            navButton.heightAnchor.constraint(equalToConstant: 32),

            newSessionButton.leadingAnchor.constraint(equalTo: navButton.trailingAnchor, constant: 6),
            newSessionButton.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            newSessionButton.widthAnchor.constraint(equalToConstant: 32),
            newSessionButton.heightAnchor.constraint(equalToConstant: 32),

            modelMenu.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            modelMenu.leadingAnchor.constraint(equalTo: newSessionButton.trailingAnchor, constant: 14),
            modelMenu.widthAnchor.constraint(equalToConstant: 164),
            modelMenu.heightAnchor.constraint(equalToConstant: 32),

            closeButton.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            closeButton.trailingAnchor.constraint(equalTo: headerBar.trailingAnchor, constant: -8),
            closeButton.widthAnchor.constraint(equalToConstant: 28),
            closeButton.heightAnchor.constraint(equalToConstant: 28),

            balanceLabel.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            balanceLabel.trailingAnchor.constraint(equalTo: closeButton.leadingAnchor, constant: -8),
            balanceLabel.widthAnchor.constraint(greaterThanOrEqualToConstant: 92),
            balanceLabel.heightAnchor.constraint(equalToConstant: 28),
            modelMenu.trailingAnchor.constraint(lessThanOrEqualTo: balanceLabel.leadingAnchor, constant: -10),

            feed.topAnchor.constraint(equalTo: headerBar.bottomAnchor, constant: 8),
            feed.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            feed.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            feed.bottomAnchor.constraint(equalTo: transcriptStrip.topAnchor, constant: -8),

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
            latestSessionButton.heightAnchor.constraint(equalToConstant: 34),

            transcriptStrip.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            transcriptStrip.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            transcriptStrip.bottomAnchor.constraint(equalTo: attachmentStrip.topAnchor, constant: -6),
            transcriptStrip.heightAnchor.constraint(equalToConstant: 26),

            transcriptLabel.leadingAnchor.constraint(equalTo: transcriptStrip.leadingAnchor, constant: 12),
            transcriptLabel.trailingAnchor.constraint(equalTo: transcriptStrip.trailingAnchor, constant: -12),
            transcriptLabel.centerYAnchor.constraint(equalTo: transcriptStrip.centerYAnchor),

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
            composerBar.heightAnchor.constraint(equalToConstant: 52),

            recordingButton.leadingAnchor.constraint(equalTo: composerBar.leadingAnchor, constant: 8),
            recordingButton.centerYAnchor.constraint(equalTo: composerBar.centerYAnchor),
            recordingButton.widthAnchor.constraint(equalToConstant: 82),
            recordingButton.heightAnchor.constraint(equalToConstant: 36),

            askButton.trailingAnchor.constraint(equalTo: composerBar.trailingAnchor, constant: -8),
            askButton.centerYAnchor.constraint(equalTo: composerBar.centerYAnchor),
            askButton.widthAnchor.constraint(equalToConstant: 36),
            askButton.heightAnchor.constraint(equalToConstant: 36),

            analyzeButton.trailingAnchor.constraint(equalTo: askButton.leadingAnchor, constant: -7),
            analyzeButton.centerYAnchor.constraint(equalTo: composerBar.centerYAnchor),
            analyzeButton.widthAnchor.constraint(equalToConstant: 36),
            analyzeButton.heightAnchor.constraint(equalToConstant: 36),

            attachButton.trailingAnchor.constraint(equalTo: analyzeButton.leadingAnchor, constant: -7),
            attachButton.centerYAnchor.constraint(equalTo: composerBar.centerYAnchor),
            attachButton.widthAnchor.constraint(equalToConstant: 36),
            attachButton.heightAnchor.constraint(equalToConstant: 36),

            instructionsButton.trailingAnchor.constraint(equalTo: attachButton.leadingAnchor, constant: -7),
            instructionsButton.centerYAnchor.constraint(equalTo: composerBar.centerYAnchor),
            instructionsButton.widthAnchor.constraint(equalToConstant: 36),
            instructionsButton.heightAnchor.constraint(equalToConstant: 36),

            composer.leadingAnchor.constraint(equalTo: recordingButton.trailingAnchor, constant: 10),
            composer.trailingAnchor.constraint(equalTo: instructionsButton.leadingAnchor, constant: -10),
            composer.centerYAnchor.constraint(equalTo: composerBar.centerYAnchor),
            composer.heightAnchor.constraint(equalToConstant: 34),
        ])

        navButton.target = self
        navButton.action = #selector(toggleSessionsClicked)
        newSessionButton.target = self
        newSessionButton.action = #selector(newSessionClicked)
        latestSessionButton.target = self
        latestSessionButton.action = #selector(continueSessionClicked)
        closeButton.target = self
        closeButton.action = #selector(closeClicked)
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
        styleHeaderIconButton(navButton, symbol: "sidebar.left", fallback: "[]")
        styleHeaderIconButton(newSessionButton, symbol: "square.and.pencil", fallback: "+")
        styleDrawer()
        styleControlButton(latestSessionButton, symbol: "clock.arrow.circlepath", accent: false)
        styleControlButton(recordingButton, symbol: "waveform", accent: false)
        styleIconButton(instructionsButton, symbol: "text.badge.checkmark", fallback: "T")
        styleIconButton(attachButton, symbol: "paperclip", fallback: "+")
        styleIconButton(analyzeButton, symbol: "sparkle.magnifyingglass", fallback: "?")
        styleIconButton(askButton, symbol: "arrow.up", fallback: "^", accent: true)
        styleIconButton(closeButton, symbol: "xmark", fallback: "x")
    }
    required init?(coder: NSCoder) { fatalError() }

    private func configureHeader() {
        headerBar.wantsLayer = true
        headerBar.layer?.backgroundColor = NSColor(red: 0.030, green: 0.034, blue: 0.042, alpha: 0.96).cgColor
        headerBar.layer?.cornerRadius = 18
        headerBar.layer?.borderWidth = 1
        headerBar.layer?.borderColor = BlueyTheme.hairline.cgColor

        titleLabel.font = NSFont.systemFont(ofSize: 13, weight: .bold)
        titleLabel.textColor = BlueyTheme.text
        statusLabel.font = NSFont.systemFont(ofSize: 9.5, weight: .semibold)
        statusLabel.textColor = BlueyTheme.textDim

        titleLabel.isHidden = true
        statusLabel.isHidden = true

        modelMenu.addItems(withTitles: ["Auto", "Instant", "Balanced", "Deep"])
        modelMenu.selectItem(at: 0)
        modelMenu.isBordered = false
        modelMenu.wantsLayer = true
        modelMenu.layer?.backgroundColor = BlueyTheme.surfaceRaised.cgColor
        modelMenu.layer?.cornerRadius = 14
        modelMenu.layer?.borderWidth = 1
        modelMenu.layer?.borderColor = BlueyTheme.hairline.cgColor
        modelMenu.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        modelMenu.contentTintColor = BlueyTheme.text

        balanceLabel.font = NSFont.monospacedSystemFont(ofSize: 10.5, weight: .bold)
        balanceLabel.textColor = BlueyTheme.text
        balanceLabel.alignment = .center
        balanceLabel.wantsLayer = true
        balanceLabel.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.18).cgColor
        balanceLabel.layer?.cornerRadius = 14
        balanceLabel.layer?.borderWidth = 1
        balanceLabel.layer?.borderColor = BlueyTheme.hairline.cgColor
    }

    private func configureContextRows() {
        transcriptStrip.wantsLayer = true
        transcriptStrip.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.16).cgColor
        transcriptStrip.layer?.cornerRadius = 13
        transcriptStrip.layer?.borderWidth = 1
        transcriptStrip.layer?.borderColor = BlueyTheme.hairline.cgColor

        transcriptLabel.font = NSFont.systemFont(ofSize: 11.5, weight: .medium)
        transcriptLabel.textColor = BlueyTheme.textDim
        transcriptLabel.lineBreakMode = .byTruncatingHead
        transcriptLabel.maximumNumberOfLines = 1

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
    }

    private func configureComposer() {
        composerBar.wantsLayer = true
        composerBar.layer?.backgroundColor = NSColor(red: 0.030, green: 0.034, blue: 0.042, alpha: 0.98).cgColor
        composerBar.layer?.cornerRadius = 20
        composerBar.layer?.borderWidth = 1
        composerBar.layer?.borderColor = BlueyTheme.hairline.cgColor

        composer.placeholderString = "Ask anything..."
        composer.font = NSFont.systemFont(ofSize: 14, weight: .medium)
        composer.isBezeled = false
        composer.drawsBackground = false
        composer.focusRingType = .none
        composer.textColor = BlueyTheme.text
        composer.placeholderAttributedString = NSAttributedString(
            string: "Ask anything...",
            attributes: [.foregroundColor: BlueyTheme.textDim])
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

    private func styleHeaderIconButton(_ button: NSButton, symbol: String, fallback: String) {
        button.title = fallback
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 14
        button.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.035).cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = NSColor.white.withAlphaComponent(0.08).cgColor
        button.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        button.contentTintColor = BlueyTheme.textDim
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

    @objc private func closeClicked() { onClose?() }

    @objc private func toggleSessionsClicked() {
        sessionDrawer.isHidden.toggle()
        statusLabel.stringValue = sessionDrawer.isHidden ? statusLabel.stringValue : "Sessions"
    }

    @objc private func newSessionClicked() {
        resetSessionSurface()
        composer.stringValue = ""
        statusLabel.stringValue = "New recording"
        sessionDrawer.isHidden = true
        emitSimple("session_new_requested")
    }

    @objc private func continueSessionClicked() {
        sessionDrawer.isHidden = true
        statusLabel.stringValue = "Latest recording"
        emitSimple("session_continue_requested")
    }

    @objc private func recordingClicked() {
        if recordingActive {
            emitSimple("recording_stop_requested")
            recordingActive = false
            recordingButton.title = "Listen"
            statusLabel.stringValue = "Paused"
            styleControlButton(recordingButton, symbol: "waveform", accent: false)
        } else {
            emitSimple("recording_start_requested")
            recordingActive = true
            recordingButton.title = "Stop"
            statusLabel.stringValue = "Listening"
            styleControlButton(recordingButton, symbol: "stop.fill", accent: true)
        }
    }

    @objc private func askClicked() {
        let raw = composer.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        let q = raw.isEmpty
            ? "Answer the latest clear question or useful context from this Bluey session."
            : raw
        composer.stringValue = ""
        let route = selectedRoute()
        emitAsk(question: q, provider: route.provider, model: route.model, mode: route.mode)
    }

    @objc private func analyzeClicked() {
        emitSimple("analyze_screen_requested")
    }

    @objc private func attachClicked() {
        emitSimple("attach_requested")
    }

    @objc private func instructionsClicked() {
        emitSimple("instructions_requested")
    }

    func setBalanceLabel(_ label: String) {
        let clean = label.trimmingCharacters(in: .whitespacesAndNewlines)
        balanceLabel.stringValue = clean.isEmpty ? "Balance --" : clean
    }

    func setContextItems(_ items: [OverlayContextItem]) {
        for view in attachmentStack.arrangedSubviews {
            attachmentStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }

        attachmentStrip.isHidden = items.isEmpty
        guard !items.isEmpty else { return }

        for item in items {
            attachmentStack.addArrangedSubview(makeAttachmentChip(item))
        }
    }

    func resetSessionSurface() {
        feed.clear()
        setContextItems([])
        transcriptSnippets.removeAll()
        transcriptLabel.stringValue = "Transcript will appear here while you listen"
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
        transcriptLabel.stringValue = transcriptSnippets.joined(separator: "   ")
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
    private var expandedWindow: OverlayWindow!
    private var pillView: PillView!
    private var expandedView: ExpandedPanelView!

    /// Pending boot card, if a Boot command arrived before windows materialised.
    private var pendingBoot: (title: String, lines: [String])?

    func start() {
        // Pill window: compact, parked at the top-right by default.
        let pillSize = NSSize(width: 110, height: 24)
        let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1920, height: 1080)
        let pillOrigin = NSPoint(
            x: screen.maxX - pillSize.width - 14,
            y: screen.maxY - pillSize.height - 12)
        pillWindow = OverlayWindow(
            contentRect: NSRect(origin: pillOrigin, size: pillSize),
            draggable: true)

        pillView = PillView(frame: NSRect(origin: .zero, size: pillSize))
        pillWindow.contentView = pillView
        pillView.statusText = "Bluey"
        pillView.onClick = { [weak self] in self?.expand() }

        // Expanded window: anchored under the pill, compact enough to feel
        // like a command layer instead of a dashboard window.
        let expandedSize = NSSize(width: 620, height: 540)
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
        startParentWatchdog()
        startStdinLoop()
    }

    private func startParentWatchdog() {
        Timer.scheduledTimer(withTimeInterval: 2.0, repeats: true) { timer in
            if getppid() == 1 {
                timer.invalidate()
                NSApp.terminate(nil)
            }
        }
    }

    private func expand() {
        // Reposition expanded just below pill's current frame so the user's
        // dragging is honoured.
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
            expandedView?.resetSessionSurface()
        case .boot(let title, let lines):
            pushBootCard(title: title, lines: lines)
        case .setOpacity(let o):
            pillWindow.alphaValue = CGFloat(o)
            expandedWindow.alphaValue = CGFloat(o)
        case .setPosition(let pos):
            applyPosition(pos)
        case .setBalance(let label):
            expandedView?.setBalanceLabel(label)
        case .setContextItems(let items):
            expandedView?.setContextItems(items)
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
