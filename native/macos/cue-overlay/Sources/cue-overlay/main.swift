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
    static let cyan = NSColor(red: 0.42, green: 0.92, blue: 1.0, alpha: 1.0)
    static let cyanSoft = NSColor(red: 0.42, green: 0.92, blue: 1.0, alpha: 0.28)
    static let panel = NSColor(red: 0.012, green: 0.030, blue: 0.045, alpha: 0.94)
    static let panelDeep = NSColor(red: 0.006, green: 0.018, blue: 0.030, alpha: 0.96)
    static let surface = NSColor(red: 0.030, green: 0.070, blue: 0.100, alpha: 0.90)
    static let surfaceRaised = NSColor(red: 0.045, green: 0.095, blue: 0.130, alpha: 0.92)
    static let text = NSColor(red: 0.92, green: 0.98, blue: 1.0, alpha: 1.0)
    static let textDim = NSColor(red: 0.62, green: 0.78, blue: 0.88, alpha: 1.0)
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

        drawLogo(in: NSRect(x: 6, y: 4, width: 20, height: 20))

        let dotSize: CGFloat = 6
        let dotRect = NSRect(x: 34, y: (bounds.height - dotSize) / 2, width: dotSize, height: dotSize)
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

        let labelRect = NSRect(x: 48, y: 5.5, width: bounds.width - 66, height: 17)
        let label = statusText as NSString
        label.draw(in: labelRect, withAttributes: [
            .font: NSFont.systemFont(ofSize: 12, weight: .semibold),
            .foregroundColor: NSColor(red: 0.92, green: 0.98, blue: 1.0, alpha: 1.0),
        ])

        let chevron = "⌄" as NSString
        chevron.draw(in: NSRect(x: bounds.width - 16, y: 5, width: 10, height: 14), withAttributes: [
            .font: NSFont.systemFont(ofSize: 12, weight: .semibold),
            .foregroundColor: NSColor(red: 0.67, green: 0.86, blue: 0.96, alpha: 0.78),
        ])
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

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.backgroundColor = BlueyTheme.panel.cgColor
        layer?.cornerRadius = 18
        layer?.borderWidth = 1
        layer?.borderColor = BlueyTheme.cyanSoft.cgColor

        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 10
        stack.edgeInsets = NSEdgeInsets(top: 14, left: 14, bottom: 14, right: 14)
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
        let accent = BlueyTheme.accent(for: card.kind)
        let container = NSView()
        container.wantsLayer = true
        container.layer?.backgroundColor = BlueyTheme.surface.cgColor
        container.layer?.cornerRadius = 12
        container.layer?.borderWidth = 1
        container.layer?.borderColor = accent.withAlphaComponent(0.18).cgColor
        container.layer?.shadowColor = NSColor.black.cgColor
        container.layer?.shadowOpacity = 0.18
        container.layer?.shadowRadius = 10
        container.layer?.shadowOffset = NSSize(width: 0, height: -4)
        container.translatesAutoresizingMaskIntoConstraints = false

        let rail = NSView()
        rail.wantsLayer = true
        rail.layer?.backgroundColor = accent.withAlphaComponent(0.86).cgColor
        rail.layer?.cornerRadius = 2
        rail.translatesAutoresizingMaskIntoConstraints = false

        let metaLabel = NSTextField(labelWithString: kindLabel(card))
        metaLabel.font = NSFont.monospacedSystemFont(ofSize: 10, weight: .bold)
        metaLabel.textColor = accent
        metaLabel.translatesAutoresizingMaskIntoConstraints = false

        let titleLabel = NSTextField(labelWithString: card.title.isEmpty ? kindTitle(card.kind) : card.title)
        titleLabel.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
        titleLabel.textColor = BlueyTheme.text
        titleLabel.translatesAutoresizingMaskIntoConstraints = false

        let bodyText = card.done ? card.body : "\(card.body)\n"
        let bodyLabel = NSTextField(wrappingLabelWithString: bodyText)
        bodyLabel.font = NSFont.systemFont(ofSize: 13.5)
        bodyLabel.textColor = BlueyTheme.text
        bodyLabel.translatesAutoresizingMaskIntoConstraints = false
        bodyLabel.preferredMaxLayoutWidth = 420

        let statusLabel = NSTextField(labelWithString: card.done ? "" : "streaming")
        statusLabel.font = NSFont.monospacedSystemFont(ofSize: 10, weight: .semibold)
        statusLabel.textColor = BlueyTheme.textDim
        statusLabel.translatesAutoresizingMaskIntoConstraints = false

        container.addSubview(rail)
        container.addSubview(metaLabel)
        container.addSubview(titleLabel)
        container.addSubview(bodyLabel)
        container.addSubview(statusLabel)
        NSLayoutConstraint.activate([
            rail.topAnchor.constraint(equalTo: container.topAnchor, constant: 10),
            rail.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 10),
            rail.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -10),
            rail.widthAnchor.constraint(equalToConstant: 3),

            metaLabel.topAnchor.constraint(equalTo: container.topAnchor, constant: 9),
            metaLabel.leadingAnchor.constraint(equalTo: rail.trailingAnchor, constant: 10),

            titleLabel.topAnchor.constraint(equalTo: metaLabel.bottomAnchor, constant: 3),
            titleLabel.leadingAnchor.constraint(equalTo: metaLabel.leadingAnchor),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: container.trailingAnchor, constant: -12),

            statusLabel.centerYAnchor.constraint(equalTo: metaLabel.centerYAnchor),
            statusLabel.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -12),

            bodyLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 7),
            bodyLabel.leadingAnchor.constraint(equalTo: metaLabel.leadingAnchor),
            bodyLabel.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -12),
            bodyLabel.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -11),
        ])
        return container
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
    let sessionShell: NSView
    let sessionTitleLabel: NSTextField
    let sessionSubtitleLabel: NSTextField
    let newSessionButton: NSButton
    let continueSessionButton: NSButton
    let composerShell: NSView
    let composer: NSTextField
    let askButton: NSButton
    let analyzeButton: NSButton
    let attachButton: NSButton
    let instructionsButton: NSButton
    let recapButton: NSButton
    let closeButton: NSButton

    var onClose: (() -> Void)?

    override init(frame frameRect: NSRect) {
        feed = FeedView(frame: .zero)
        headerBar = NSView()
        titleLabel = NSTextField(labelWithString: "Bluey")
        statusLabel = NSTextField(labelWithString: "Choose session · Auto route ready")
        sessionShell = NSView()
        sessionTitleLabel = NSTextField(labelWithString: "Start clean or continue where you left off.")
        sessionSubtitleLabel = NSTextField(labelWithString: "New creates a fresh workspace. Continue restores the latest saved session.")
        newSessionButton = NSButton(title: "New", target: nil, action: nil)
        continueSessionButton = NSButton(title: "Continue", target: nil, action: nil)
        composerShell = NSView()
        composer = NSTextField()
        composer.placeholderString = "Ask anything from audio, page, or files..."
        composer.font = NSFont.systemFont(ofSize: 14, weight: .medium)
        composer.isBezeled = false
        composer.drawsBackground = false
        composer.textColor = BlueyTheme.text
        composer.placeholderAttributedString = NSAttributedString(
            string: "Ask anything from audio, page, or files...",
            attributes: [.foregroundColor: BlueyTheme.textDim])

        askButton           = NSButton(title: "Answer", target: nil, action: nil)
        analyzeButton       = NSButton(title: "Analyse", target: nil, action: nil)
        attachButton        = NSButton(title: "Attach", target: nil, action: nil)
        instructionsButton  = NSButton(title: "Rules",  target: nil, action: nil)
        recapButton         = NSButton(title: "Recap",  target: nil, action: nil)
        closeButton         = NSButton(title: "✕",            target: nil, action: nil)

        super.init(frame: frameRect)

        wantsLayer = true
        layer?.backgroundColor = BlueyTheme.panelDeep.cgColor
        layer?.cornerRadius = 22
        layer?.borderWidth = 1
        layer?.borderColor = BlueyTheme.cyanSoft.cgColor

        headerBar.wantsLayer = true
        headerBar.layer?.backgroundColor = BlueyTheme.surface.cgColor
        headerBar.layer?.cornerRadius = 16
        headerBar.layer?.borderWidth = 1
        headerBar.layer?.borderColor = BlueyTheme.cyanSoft.withAlphaComponent(0.75).cgColor

        titleLabel.font = NSFont.systemFont(ofSize: 15, weight: .bold)
        titleLabel.textColor = BlueyTheme.text
        statusLabel.font = NSFont.systemFont(ofSize: 11, weight: .semibold)
        statusLabel.textColor = BlueyTheme.textDim

        sessionShell.wantsLayer = true
        sessionShell.layer?.backgroundColor = BlueyTheme.surface.cgColor
        sessionShell.layer?.cornerRadius = 16
        sessionShell.layer?.borderWidth = 1
        sessionShell.layer?.borderColor = BlueyTheme.cyanSoft.withAlphaComponent(0.65).cgColor

        sessionTitleLabel.font = NSFont.systemFont(ofSize: 13, weight: .bold)
        sessionTitleLabel.textColor = BlueyTheme.text
        sessionSubtitleLabel.font = NSFont.systemFont(ofSize: 10.5, weight: .medium)
        sessionSubtitleLabel.textColor = BlueyTheme.textDim
        sessionSubtitleLabel.lineBreakMode = .byTruncatingTail

        composerShell.wantsLayer = true
        composerShell.layer?.backgroundColor = NSColor(red: 0.006, green: 0.020, blue: 0.035, alpha: 0.92).cgColor
        composerShell.layer?.cornerRadius = 16
        composerShell.layer?.borderWidth = 1
        composerShell.layer?.borderColor = BlueyTheme.cyanSoft.withAlphaComponent(0.70).cgColor

        headerBar.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        sessionShell.translatesAutoresizingMaskIntoConstraints = false
        sessionTitleLabel.translatesAutoresizingMaskIntoConstraints = false
        sessionSubtitleLabel.translatesAutoresizingMaskIntoConstraints = false
        newSessionButton.translatesAutoresizingMaskIntoConstraints = false
        continueSessionButton.translatesAutoresizingMaskIntoConstraints = false
        feed.translatesAutoresizingMaskIntoConstraints = false
        composerShell.translatesAutoresizingMaskIntoConstraints = false
        composer.translatesAutoresizingMaskIntoConstraints = false
        askButton.translatesAutoresizingMaskIntoConstraints = false
        analyzeButton.translatesAutoresizingMaskIntoConstraints = false
        attachButton.translatesAutoresizingMaskIntoConstraints = false
        instructionsButton.translatesAutoresizingMaskIntoConstraints = false
        recapButton.translatesAutoresizingMaskIntoConstraints = false
        closeButton.translatesAutoresizingMaskIntoConstraints = false

        let sessionButtons = NSStackView(views: [newSessionButton, continueSessionButton])
        sessionButtons.orientation = .horizontal
        sessionButtons.spacing = 6
        sessionButtons.distribution = .fillEqually
        sessionButtons.translatesAutoresizingMaskIntoConstraints = false

        let buttonRow = NSStackView(views: [askButton, analyzeButton, attachButton, instructionsButton, recapButton])
        buttonRow.orientation = .horizontal
        buttonRow.spacing = 7
        buttonRow.distribution = .fillEqually
        buttonRow.translatesAutoresizingMaskIntoConstraints = false

        headerBar.addSubview(titleLabel)
        headerBar.addSubview(statusLabel)
        sessionShell.addSubview(sessionTitleLabel)
        sessionShell.addSubview(sessionSubtitleLabel)
        sessionShell.addSubview(sessionButtons)
        addSubview(feed)
        addSubview(headerBar)
        addSubview(sessionShell)
        addSubview(closeButton)
        addSubview(composerShell)
        composerShell.addSubview(composer)
        addSubview(buttonRow)

        NSLayoutConstraint.activate([
            headerBar.topAnchor.constraint(equalTo: topAnchor, constant: 10),
            headerBar.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            headerBar.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -46),
            headerBar.heightAnchor.constraint(equalToConstant: 42),

            titleLabel.leadingAnchor.constraint(equalTo: headerBar.leadingAnchor, constant: 14),
            titleLabel.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor, constant: -5),
            statusLabel.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            statusLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 0),

            closeButton.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            closeButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            closeButton.widthAnchor.constraint(equalToConstant: 30),
            closeButton.heightAnchor.constraint(equalToConstant: 30),

            sessionShell.topAnchor.constraint(equalTo: headerBar.bottomAnchor, constant: 8),
            sessionShell.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            sessionShell.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            sessionShell.heightAnchor.constraint(equalToConstant: 58),

            sessionTitleLabel.leadingAnchor.constraint(equalTo: sessionShell.leadingAnchor, constant: 14),
            sessionTitleLabel.topAnchor.constraint(equalTo: sessionShell.topAnchor, constant: 10),
            sessionTitleLabel.trailingAnchor.constraint(lessThanOrEqualTo: sessionButtons.leadingAnchor, constant: -10),

            sessionSubtitleLabel.leadingAnchor.constraint(equalTo: sessionTitleLabel.leadingAnchor),
            sessionSubtitleLabel.topAnchor.constraint(equalTo: sessionTitleLabel.bottomAnchor, constant: 2),
            sessionSubtitleLabel.trailingAnchor.constraint(lessThanOrEqualTo: sessionButtons.leadingAnchor, constant: -10),

            sessionButtons.trailingAnchor.constraint(equalTo: sessionShell.trailingAnchor, constant: -10),
            sessionButtons.centerYAnchor.constraint(equalTo: sessionShell.centerYAnchor),
            sessionButtons.widthAnchor.constraint(equalToConstant: 154),
            sessionButtons.heightAnchor.constraint(equalToConstant: 32),

            feed.topAnchor.constraint(equalTo: sessionShell.bottomAnchor, constant: 10),
            feed.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            feed.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            feed.bottomAnchor.constraint(equalTo: composerShell.topAnchor, constant: -10),

            composerShell.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            composerShell.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            composerShell.bottomAnchor.constraint(equalTo: buttonRow.topAnchor, constant: -8),
            composerShell.heightAnchor.constraint(equalToConstant: 44),

            composer.leadingAnchor.constraint(equalTo: composerShell.leadingAnchor, constant: 14),
            composer.trailingAnchor.constraint(equalTo: composerShell.trailingAnchor, constant: -14),
            composer.centerYAnchor.constraint(equalTo: composerShell.centerYAnchor),
            composer.heightAnchor.constraint(equalToConstant: 24),

            buttonRow.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            buttonRow.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            buttonRow.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -10),
            buttonRow.heightAnchor.constraint(equalToConstant: 36),
        ])

        closeButton.target = self
        closeButton.action = #selector(closeClicked)
        styleIconButton(closeButton, symbol: "xmark", fallback: "x")

        newSessionButton.target = self
        newSessionButton.action = #selector(newSessionClicked)
        continueSessionButton.target = self
        continueSessionButton.action = #selector(continueSessionClicked)

        askButton.target = self
        askButton.action = #selector(askClicked)
        analyzeButton.target = self
        analyzeButton.action = #selector(analyzeClicked)
        attachButton.target = self
        attachButton.action = #selector(attachClicked)
        instructionsButton.target = self
        instructionsButton.action = #selector(instructionsClicked)
        recapButton.target = self
        recapButton.action = #selector(recapClicked)

        styleMiniButton(newSessionButton, symbol: "plus", accent: true)
        styleMiniButton(continueSessionButton, symbol: "clock.arrow.circlepath", accent: false)
        styleActionButton(askButton, symbol: "bolt.fill", accent: true)
        styleActionButton(analyzeButton, symbol: "sparkle.magnifyingglass", accent: false)
        styleActionButton(attachButton, symbol: "paperclip", accent: false)
        styleActionButton(instructionsButton, symbol: "note.text", accent: false)
        styleActionButton(recapButton, symbol: "list.bullet.rectangle", accent: false)
    }
    required init?(coder: NSCoder) { fatalError() }

    private func styleActionButton(_ button: NSButton, symbol: String, accent: Bool) {
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 13
        button.layer?.backgroundColor = accent
            ? NSColor(red: 0.08, green: 0.28, blue: 0.34, alpha: 0.95).cgColor
            : BlueyTheme.surfaceRaised.cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = (accent ? BlueyTheme.cyan : BlueyTheme.cyanSoft).cgColor
        button.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        button.contentTintColor = BlueyTheme.text
        button.image = symbolImage(symbol)
        button.imagePosition = .imageLeading
        button.imageScaling = .scaleProportionallyDown
        button.alignment = .center
    }

    private func styleMiniButton(_ button: NSButton, symbol: String, accent: Bool) {
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 12
        button.layer?.backgroundColor = accent
            ? NSColor(red: 0.08, green: 0.28, blue: 0.34, alpha: 0.96).cgColor
            : NSColor(red: 0.018, green: 0.050, blue: 0.075, alpha: 0.92).cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = (accent ? BlueyTheme.cyan : BlueyTheme.cyanSoft).cgColor
        button.font = NSFont.systemFont(ofSize: 11.5, weight: .bold)
        button.contentTintColor = BlueyTheme.text
        button.image = symbolImage(symbol)
        button.imagePosition = .imageLeading
        button.imageScaling = .scaleProportionallyDown
        button.alignment = .center
    }

    private func styleIconButton(_ button: NSButton, symbol: String, fallback: String) {
        button.title = fallback
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 15
        button.layer?.backgroundColor = BlueyTheme.surfaceRaised.cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = BlueyTheme.cyanSoft.cgColor
        button.font = NSFont.systemFont(ofSize: 13, weight: .bold)
        button.contentTintColor = BlueyTheme.text
        if let image = symbolImage(symbol) {
            button.title = ""
            button.image = image
            button.imagePosition = .imageOnly
            button.imageScaling = .scaleProportionallyDown
        }
    }

    @objc private func closeClicked() { onClose?() }

    @objc private func newSessionClicked() {
        emitSimple("session_new_requested")
    }

    @objc private func continueSessionClicked() {
        emitSimple("session_continue_requested")
    }

    @objc private func askClicked() {
        let raw = composer.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        let q = raw.isEmpty
            ? "Answer the latest clear question or useful context from this Bluey session."
            : raw
        composer.stringValue = ""
        emitAsk(question: q, provider: nil, model: nil, mode: nil)
    }

    @objc private func analyzeClicked() {
        emitSimple("analyze_screen_requested")
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
        let pillSize = NSSize(width: 112, height: 28)
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

        // Expanded window: anchored under the pill, compact enough to feel
        // like a command layer instead of a dashboard window.
        let expandedSize = NSSize(width: 460, height: 540)
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
