// bluey-overlay-macos / cue-overlay-macos
//
// Native macOS overlay process. Speaks NDJSON IPC over a local Unix socket
// when BLUEY_OVERLAY_SOCKET is set, with stdin/stdout retained for test stubs
// and manual protocol checks. Provides:
//
//   - A compact centered pill (collapsed default state), draggable, click to
//     disappear into the full feed.
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
import QuartzCore

// MARK: - Visual system

private func normalizedCardKind(_ kind: String) -> String {
    kind
        .trimmingCharacters(in: .whitespacesAndNewlines)
        .replacingOccurrences(of: "-", with: "_")
        .lowercased()
}

private func displayTranscriptText(_ text: String) -> String {
    let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
    if trimmed.hasPrefix("[dev audio:") {
        return ""
    }
    return trimmed
        .replacingOccurrences(
            of: #"(?i)\b(?:system|mic|microphone|audio)\s*:\s*"#,
            with: "",
            options: .regularExpression)
        .replacingOccurrences(of: #"\s+"#, with: " ", options: .regularExpression)
        .trimmingCharacters(in: .whitespacesAndNewlines)
}

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
    static let danger = NSColor(red: 1.0, green: 0.38, blue: 0.44, alpha: 1.0)

    static func accent(for kind: String) -> NSColor {
        switch normalizedCardKind(kind) {
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

private func blueyMaterialAlpha(_ base: CGFloat, opacity: CGFloat, floor: CGFloat = 0.02) -> CGFloat {
    min(1.0, max(floor, base * min(max(opacity, 0.50), 1.0)))
}

private final class VerticallyCenteredTextFieldCell: NSTextFieldCell {
    override func drawingRect(forBounds rect: NSRect) -> NSRect {
        var drawingRect = super.drawingRect(forBounds: rect)
        let textSize = cellSize(forBounds: rect)
        drawingRect.origin.y = rect.origin.y + max(0, (rect.height - textSize.height) / 2)
        drawingRect.size.height = min(rect.height, textSize.height)
        return drawingRect
    }
}

private func useCenteredSingleLineCell(_ label: NSTextField) {
    let oldCell = label.cell as? NSTextFieldCell
    let cell = VerticallyCenteredTextFieldCell(textCell: label.stringValue)
    cell.font = label.font
    cell.textColor = label.textColor
    cell.alignment = label.alignment
    cell.lineBreakMode = oldCell?.lineBreakMode ?? label.lineBreakMode
    cell.usesSingleLineMode = true
    cell.wraps = false
    cell.isScrollable = false
    label.cell = cell
    label.isBezeled = false
    label.drawsBackground = false
    label.isEditable = false
    label.isSelectable = false
}

private enum ExpandedPanelMetrics {
    static let maxCompactWidth: CGFloat = 820
    static let minCompactWidth: CGFloat = 680
    static let maxCanvasWidth: CGFloat = 960
    static let height: CGFloat = 520
    static let minHeight: CGFloat = 440
    static let screenInset: CGFloat = 32
    static let cornerRadius: CGFloat = 28

    static func fittingWidth(for screen: NSRect, preferred: CGFloat) -> CGFloat {
        let available = max(360, screen.width - screenInset * 2)
        return min(preferred, available)
    }

    static func fittingHeight(for screen: NSRect, preferred: CGFloat = height) -> CGFloat {
        let available = max(minHeight, screen.height - screenInset * 2)
        return min(preferred, available)
    }

    static func fittingMinimumWidth(for screen: NSRect, targetWidth: CGFloat) -> CGFloat {
        let available = max(360, screen.width - screenInset * 2)
        return min(minCompactWidth, targetWidth, available)
    }

    static func compactFrame(in visibleFrame: NSRect) -> NSRect {
        let width = fittingWidth(for: visibleFrame, preferred: maxCompactWidth)
        let height = fittingHeight(for: visibleFrame)
        return fitExpandedFrameToVisibleScreen(
            NSRect(
                x: visibleFrame.midX - width / 2,
                y: visibleFrame.midY - height / 2,
                width: width,
                height: height),
            visibleFrame: visibleFrame)
    }

    static func fitExpandedFrameToVisibleScreen(_ frame: NSRect, visibleFrame: NSRect) -> NSRect {
        var fitted = frame
        let availableWidth = max(360, visibleFrame.width - screenInset * 2)
        let availableHeight = max(minHeight, visibleFrame.height - screenInset * 2)
        fitted.size.width = min(max(360, fitted.size.width), availableWidth)
        fitted.size.height = min(max(minHeight, fitted.size.height), availableHeight)
        fitted.origin.x = min(
            max(visibleFrame.minX + screenInset, fitted.origin.x),
            visibleFrame.maxX - fitted.size.width - screenInset)
        fitted.origin.y = min(
            max(visibleFrame.minY + screenInset, fitted.origin.y),
            visibleFrame.maxY - fitted.size.height - screenInset)
        return fitted
    }
}

private enum PillMetrics {
    static let size = NSSize(width: 158, height: 32)

    static func centeredFrame(in visibleFrame: NSRect) -> NSRect {
        NSRect(
            x: visibleFrame.midX - size.width / 2,
            y: visibleFrame.midY - size.height / 2,
            width: size.width,
            height: size.height)
    }
}

private enum OverlayScreenPlacement {
    static let fallbackVisibleFrame = NSRect(x: 0, y: 0, width: 1920, height: 1080)

    static func activeVisibleFrame() -> NSRect {
        let mouse = NSEvent.mouseLocation
        if let screen = NSScreen.screens.first(where: { $0.frame.contains(mouse) }) {
            return screen.visibleFrame
        }
        return NSScreen.main?.visibleFrame
            ?? NSScreen.screens.first?.visibleFrame
            ?? fallbackVisibleFrame
    }

    static func centeredFrame(size: NSSize, in visibleFrame: NSRect) -> NSRect {
        NSRect(
            x: visibleFrame.midX - size.width / 2,
            y: visibleFrame.midY - size.height / 2,
            width: size.width,
            height: size.height)
    }
}

private enum BlueyBrandAsset {
    static let logoSvg = #"""
<svg viewBox="0 0 96 96" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Bluey logo">
  <defs>
    <linearGradient id="shell" x1="12" y1="10" x2="86" y2="88" gradientUnits="userSpaceOnUse">
      <stop stop-color="#08121c"/>
      <stop offset=".58" stop-color="#092236"/>
      <stop offset="1" stop-color="#0f4059"/>
    </linearGradient>
    <linearGradient id="glass" x1="24" y1="22" x2="76" y2="70" gradientUnits="userSpaceOnUse">
      <stop stop-color="#0b1a28"/>
      <stop offset="1" stop-color="#06101b"/>
    </linearGradient>
    <linearGradient id="edge" x1="16" y1="15" x2="82" y2="83" gradientUnits="userSpaceOnUse">
      <stop stop-color="#85f7ff"/>
      <stop offset=".5" stop-color="#2eb8db"/>
      <stop offset="1" stop-color="#4b6fff"/>
    </linearGradient>
    <linearGradient id="signal" x1="60" y1="20" x2="78" y2="38" gradientUnits="userSpaceOnUse">
      <stop stop-color="#91ffb0"/>
      <stop offset="1" stop-color="#49e8ff"/>
    </linearGradient>
    <filter id="soft-glow" x="-30%" y="-30%" width="160%" height="160%" color-interpolation-filters="sRGB">
      <feDropShadow dx="0" dy="0" stdDeviation="2.4" flood-color="#69ecff" flood-opacity=".46"/>
      <feDropShadow dx="0" dy="12" stdDeviation="12" flood-color="#020811" flood-opacity=".48"/>
    </filter>
  </defs>
  <rect x="7" y="7" width="82" height="82" rx="25" fill="url(#shell)"/>
  <rect x="8.5" y="8.5" width="79" height="79" rx="23.5" fill="none" stroke="#71e9ff" stroke-opacity=".24"/>
  <path d="M18 46c0-16 13-29 29-29h6c12 0 22 10 22 22v16c0 12-10 22-22 22H39c-12 0-21-9-21-21V46Z" fill="#07101a" opacity=".58"/>
  <rect x="23" y="25" width="50" height="42" rx="12" fill="url(#glass)" stroke="url(#edge)" stroke-width="3.25" filter="url(#soft-glow)"/>
  <path d="M33 39.5 41.25 48 33 56.5" fill="none" stroke="#f4fbff" stroke-width="5.2" stroke-linecap="round" stroke-linejoin="round"/>
  <path d="M47.5 57.5h14" stroke="#bcefff" stroke-width="5.2" stroke-linecap="round"/>
  <path d="M69 19.5 71.15 27 78.5 29.3 71.15 31.7 69 39.5 66.85 31.7 59.5 29.3 66.85 27Z" fill="url(#signal)"/>
  <circle cx="74.5" cy="23.5" r="2.2" fill="#9dff9e"/>
</svg>
"""#

    static let wordmarkSvg = #"""
<svg viewBox="0 0 292 100" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="bluey">
  <defs>
    <linearGradient id="wordmark-fill" x1="0" y1="8" x2="292" y2="88" gradientUnits="userSpaceOnUse">
      <stop stop-color="#f8fdff"/>
      <stop offset=".32" stop-color="#bdefff"/>
      <stop offset=".68" stop-color="#63d8ff"/>
      <stop offset="1" stop-color="#4f8dff"/>
    </linearGradient>
    <filter id="wordmark-glow" x="-16" y="-18" width="324" height="132" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB">
      <feDropShadow dx="0" dy="0" stdDeviation="3.5" flood-color="#63d8ff" flood-opacity=".30"/>
      <feDropShadow dx="0" dy="10" stdDeviation="10" flood-color="#020811" flood-opacity=".55"/>
    </filter>
    <mask id="wordmark-cut-mask" maskUnits="userSpaceOnUse" x="0" y="0" width="292" height="100">
      <rect width="292" height="100" fill="#fff"/>
      <path d="M29.6 41.4h20.2l-7.2 7.5H22.2l7.4-7.5Z" fill="#000"/>
      <path d="M185.4 43.2h27.8l-6.7 7.2h-28.2l7.1-7.2Z" fill="#000"/>
      <path d="M252.5 53.4h21.2l-7.6 8.5h-21.4l7.8-8.5Z" fill="#000"/>
    </mask>
  </defs>
  <g fill="url(#wordmark-fill)" fill-rule="evenodd" filter="url(#wordmark-glow)" mask="url(#wordmark-cut-mask)">
    <path d="M5 8h13v24c5.4-6.7 12.2-10 20.6-10 16.3 0 27.4 11.6 27.4 27.2C66 65.1 54.5 75 38.7 75c-9 0-16.1-3.3-21.2-9.9L15.7 73H5V8Zm13 41.2c0 9.8 7 16.9 16.8 16.9 9.6 0 16.3-6.9 16.3-16.8 0-10.1-6.7-17.1-16.3-17.1C25 32.2 18 39.3 18 49.2Z"/>
    <path d="M76 8h13v65H76V8Z"/>
    <path d="M103 23h13v27.2c0 9.6 5.1 15.2 14.1 15.2 8.8 0 14.7-5.9 14.7-15.7V23h13v50h-10.7l-1.4-8.4C140.7 71.1 133.6 75 124.4 75 109.8 75 103 65.6 103 51.4V23Z"/>
    <path d="M171 48.6C171 32.8 182.7 22 198.2 22 214.7 22 224 33.9 224 48.5c0 1.7-.1 3.4-.4 5H184.2c1.7 7.3 7.4 11.3 15.5 11.3 6.2 0 11.4-2 15.8-5.6l5.6 8.4C215.2 72.5 207.8 75 198.5 75 181.2 75 171 64.2 171 48.6Zm13.3-5.1h27.8c-1.2-7.4-6.2-11.9-13.8-11.9-7.3 0-12.4 4.6-14 11.9Z"/>
    <path d="M232 23h14.2l13.2 31.9L273 23h13.8l-31.2 70H242l10.3-23.1L232 23Z"/>
  </g>
</svg>
"""#

    static func image(from svg: String) -> NSImage? {
        NSImage(data: Data(svg.utf8))
    }
}

private final class BlueyLogoView: NSImageView {
    init() {
        super.init(frame: .zero)
        image = BlueyBrandAsset.image(from: BlueyBrandAsset.logoSvg)
        imageScaling = .scaleProportionallyUpOrDown
        setContentHuggingPriority(.required, for: .horizontal)
        setContentCompressionResistancePriority(.required, for: .horizontal)
    }

    required init?(coder: NSCoder) { fatalError() }
}

private final class BlueyWordmarkView: NSImageView {
    init() {
        super.init(frame: .zero)
        image = BlueyBrandAsset.image(from: BlueyBrandAsset.wordmarkSvg)
        imageScaling = .scaleProportionallyUpOrDown
        setContentHuggingPriority(.required, for: .horizontal)
        setContentCompressionResistancePriority(.required, for: .horizontal)
    }

    required init?(coder: NSCoder) { fatalError() }
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

    override var acceptsFirstResponder: Bool { true }

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
        let chars = event.charactersIgnoringModifiers ?? ""
        let isReturn = event.keyCode == 36 || event.keyCode == 76 || chars == "\r" || chars == "\n"
        if isReturn {
            if event.modifierFlags.contains(.shift) {
                insertNewline(nil)
            } else {
                // Enter submits. Cmd+Enter is shown as the explicit shortcut,
                // and Shift+Enter keeps a multiline thought inside the composer.
                onSubmit?()
            }
            return
        }
        super.keyDown(with: event)
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        super.mouseDown(with: event)
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

private final class ComposerSurfaceView: NSView {
    weak var composer: ComposerTextView?

    override var acceptsFirstResponder: Bool { true }

    override func mouseDown(with event: NSEvent) {
        if let composer {
            window?.makeFirstResponder(composer)
        }
        super.mouseDown(with: event)
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
    case listeningStateChanged(String)
    case transcriptPartial(source: String, text: String)
    case transcriptFinal(source: String, text: String)
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
    case "listening_state_changed":
        return .listeningStateChanged(obj["state"] as? String ?? "idle")
    case "transcript_partial":
        return .transcriptPartial(
            source: obj["source"] as? String ?? "audio",
            text: obj["text"] as? String ?? "")
    case "transcript_final":
        return .transcriptFinal(
            source: obj["source"] as? String ?? "audio",
            text: obj["text"] as? String ?? "")
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
#if DEBUG
    let devEnabled = argumentFlag("--bluey-dev-overlay") || envFlag("BLUEY_DEV_OVERLAY")
    guard devEnabled else { return false }
    return argumentFlag("--bluey-overlay-capture-visible")
        || envFlag("BLUEY_OVERLAY_CAPTURE_VISIBLE")
        || envFlag("BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE")
#else
    return false
#endif
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

private func emitRemoveContext(id: String) {
    emitEvent(["type": "remove_context_requested", "id": id])
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

private func emitSessionDelete(id: String) {
    emitEvent(["type": "session_delete_requested", "id": id])
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
    var preserveProgrammaticFrameHeight = false
    var minimumFrameWidth: CGFloat?
    var maximumFrameWidth: CGFloat?
    var minimumFrameHeight: CGFloat?
    var maximumFrameHeight: CGFloat?
    var contentCornerRadius: CGFloat? {
        didSet { applyContentCornerMask() }
    }

    override var contentView: NSView? {
        didSet { applyContentCornerMask() }
    }

    init(contentRect: NSRect, draggable: Bool, resizable: Bool = false) {
        var style: NSWindow.StyleMask = [.borderless]
        if resizable {
            style.insert(.resizable)
        }
        super.init(
            contentRect: contentRect,
            styleMask: style,
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

    private func applyContentCornerMask() {
        guard let radius = contentCornerRadius, let contentView else { return }
        contentView.wantsLayer = true
        contentView.layer?.cornerRadius = radius
        contentView.layer?.masksToBounds = true
        if #available(macOS 10.15, *) {
            contentView.layer?.cornerCurve = .continuous
        }
        if let frameView = contentView.superview {
            frameView.wantsLayer = true
            frameView.layer?.cornerRadius = radius
            frameView.layer?.masksToBounds = true
            if #available(macOS 10.15, *) {
                frameView.layer?.cornerCurve = .continuous
            }
        }
    }

    override func setFrame(_ frameRect: NSRect, display displayFlag: Bool) {
        super.setFrame(clampedFrame(frameRect), display: displayFlag)
    }

    override func setFrame(_ frameRect: NSRect, display displayFlag: Bool, animate animateFlag: Bool) {
        super.setFrame(clampedFrame(frameRect), display: displayFlag, animate: animateFlag)
    }

    override func setContentSize(_ size: NSSize) {
        if clampingEnabled {
            var requestedSize = size
            if lockedFrameHeight == nil {
                // AppKit may try to satisfy dense feed/composer constraints by
                // growing the borderless window. Preserve the current height
                // for content-size fitting while still allowing real user
                // frame resizing through setFrame(_:display:).
                requestedSize.height = frame.height
            }
            super.setFrame(
                clampedFrame(NSRect(origin: frame.origin, size: requestedSize)),
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
        } else {
            if preserveProgrammaticFrameHeight,
               frame.height > self.frame.height,
               NSEvent.pressedMouseButtons == 0
            {
                clamped.size.height = self.frame.height
            }
            if let minimumFrameHeight {
                clamped.size.height = max(minimumFrameHeight, clamped.size.height)
            }
            if let maximumFrameHeight {
                clamped.size.height = min(maximumFrameHeight, clamped.size.height)
            }
        }

        guard clampingEnabled else {
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
        return ExpandedPanelMetrics.fitExpandedFrameToVisibleScreen(clamped, visibleFrame: visibleFrame)
    }

    private var clampingEnabled: Bool {
        lockedFrameHeight != nil
            || preserveProgrammaticFrameHeight
            || minimumFrameWidth != nil
            || maximumFrameWidth != nil
            || minimumFrameHeight != nil
            || maximumFrameHeight != nil
    }
}

private final class ModalBlockerView: NSView {
    var onEscape: (() -> Void)?

    override var acceptsFirstResponder: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard !isHidden, alphaValue > 0.01 else { return nil }
        return super.hitTest(point) ?? self
    }

    override func mouseDown(with event: NSEvent) {}
    override func mouseUp(with event: NSEvent) {}

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 {
            onEscape?()
            return
        }
        super.keyDown(with: event)
    }
}

private final class HeaderDragView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? {
        guard !isHidden, alphaValue > 0.01, bounds.contains(point) else { return nil }
        if let control = interactiveHit(in: self, point: point) {
            return control
        }
        return self
    }

    override func mouseDown(with event: NSEvent) {
        let localPoint = convert(event.locationInWindow, from: nil)
        if let hit = interactiveHit(in: self, point: localPoint), hit !== self {
            super.mouseDown(with: event)
            return
        }
        window?.performDrag(with: event)
    }

    private func interactiveHit(in view: NSView, point: NSPoint) -> NSView? {
        for subview in view.subviews.reversed() {
            guard !subview.isHidden, subview.alphaValue > 0.01 else { continue }
            let subPoint = subview.convert(point, from: view)
            guard subview.bounds.insetBy(dx: -6, dy: -6).contains(subPoint) else { continue }
            if preservesHeaderHit(for: subview) {
                return subview.hitTest(subPoint) ?? subview
            }
            if let nested = interactiveHit(in: subview, point: subPoint) {
                return nested
            }
        }
        return nil
    }

    private func preservesHeaderHit(for view: NSView) -> Bool {
        var current: NSView? = view
        while let candidate = current, candidate !== self {
            if candidate is NSButton
                || candidate is NSPopUpButton
                || candidate is NSSlider
                || candidate is NSScroller
                || candidate is NSTextView
            {
                return true
            }
            if let textField = candidate as? NSTextField, textField.isEditable {
                return true
            }
            current = candidate.superview
        }
        return false
    }
}

private final class CopyCardButton: NSButton {
    var copyText = ""
}

private final class RemoveAttachmentButton: NSButton {
    var contextId = ""
}

// MARK: - Pill view

private enum PillRunState: Equatable {
    case ready
    case connecting
    case listening
    case paused
    case failed

    init(listeningState: String) {
        switch listeningState.lowercased() {
        case "connecting":
            self = .connecting
        case "listening":
            self = .listening
        case "paused":
            self = .paused
        case "failed":
            self = .failed
        default:
            self = .ready
        }
    }

    var dotColor: NSColor {
        switch self {
        case .ready:
            return BlueyTheme.green
        case .connecting:
            return BlueyTheme.warning
        case .listening:
            return BlueyTheme.green
        case .paused:
            return BlueyTheme.textDim.withAlphaComponent(0.72)
        case .failed:
            return BlueyTheme.danger
        }
    }

    var symbolName: String {
        switch self {
        case .ready:
            return "play.fill"
        case .connecting:
            return "bolt.horizontal.fill"
        case .listening:
            return "pause.fill"
        case .paused:
            return "play.fill"
        case .failed:
            return "exclamationmark"
        }
    }

    var symbolColor: NSColor {
        switch self {
        case .ready, .paused:
            return BlueyTheme.text
        case .connecting:
            return BlueyTheme.warning
        case .listening:
            return BlueyTheme.green
        case .failed:
            return BlueyTheme.danger
        }
    }

    var accessibilityLabel: String {
        switch self {
        case .ready:
            return "Bluey ready"
        case .connecting:
            return "Bluey connecting"
        case .listening:
            return "Bluey listening"
        case .paused:
            return "Bluey paused"
        case .failed:
            return "Bluey needs attention"
        }
    }
}

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
        // The collapsed pill is intentionally identity-only. Balance lives in
        // the expanded header so the launcher stays compact and scannable.
        setAccessibilityLabel(statusText)
    }
    func setBalanceLabel(_ label: String) {
        balanceText = label
    }
    var dotColor: NSColor = NSColor.systemGreen {
        didSet {
            dotView.layer?.backgroundColor = dotColor.cgColor
            dotView.layer?.shadowColor = dotColor.cgColor
            needsDisplay = true
        }
    }
    var onClick: (() -> Void)?
    var onRunToggle: (() -> Void)?
    var onAsk: (() -> Void)?
    var onEnd: (() -> Void)?
    private var runState: PillRunState = .ready

    private let logoMark = BlueyLogoView()
    private let wordmarkView = BlueyWordmarkView()
    private let dotView = NSView()
    private let controlRail = NSView()
    private let styleButton = NSButton(title: "", target: nil, action: nil)
    private let runButton = NSButton(title: "", target: nil, action: nil)
    private let endButton = NSButton(title: "", target: nil, action: nil)
    private var backgroundOpacity: CGFloat = 0.94

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor
        layer?.cornerRadius = frameRect.height / 2
        layer?.borderWidth = 0
        layer?.shadowColor = NSColor(red: 0.10, green: 0.70, blue: 0.96, alpha: 1.0).cgColor
        layer?.shadowOpacity = 0.18
        layer?.shadowRadius = 8
        layer?.shadowOffset = .zero

        logoMark.wantsLayer = true
        logoMark.layer?.shadowColor = NSColor(red: 0.26, green: 0.84, blue: 1.0, alpha: 1.0).cgColor
        logoMark.layer?.shadowOpacity = 0.22
        logoMark.layer?.shadowRadius = 8
        logoMark.layer?.shadowOffset = .zero
        addSubview(logoMark)
        addSubview(wordmarkView)

        dotView.wantsLayer = true
        dotView.layer?.backgroundColor = dotColor.cgColor
        dotView.layer?.cornerRadius = 4
        dotView.layer?.shadowColor = dotColor.cgColor
        dotView.layer?.shadowOpacity = 0.62
        dotView.layer?.shadowRadius = 6
        dotView.layer?.shadowOffset = .zero
        addSubview(dotView)

        controlRail.wantsLayer = true
        controlRail.layer?.backgroundColor = NSColor.white.withAlphaComponent(materialAlpha(0.040)).cgColor
        controlRail.layer?.cornerRadius = 12
        controlRail.layer?.borderWidth = 1
        controlRail.layer?.borderColor = NSColor.white.withAlphaComponent(materialAlpha(0.085)).cgColor
        addSubview(controlRail)

        configureMiniButton(styleButton, symbol: "text.cursor", fallback: "?", tint: BlueyTheme.cyan)
        configureRunButton()
        configureMiniButton(endButton, symbol: "power", fallback: "×", tint: BlueyTheme.textDim)

        styleButton.toolTip = "Ask a question"
        runButton.toolTip = "Start or pause listening"
        endButton.toolTip = "Turn Bluey off"
        styleButton.target = self
        styleButton.action = #selector(styleClicked)
        runButton.target = self
        runButton.action = #selector(runClicked)
        endButton.target = self
        endButton.action = #selector(endClicked)
        controlRail.addSubview(styleButton)
        controlRail.addSubview(runButton)
        controlRail.addSubview(endButton)
        updateRunStateDisplay()
    }
    required init?(coder: NSCoder) { fatalError() }

    override func layout() {
        super.layout()
        layer?.cornerRadius = bounds.height / 2

        let logoSide: CGFloat = 25
        logoMark.frame = NSRect(x: 5, y: (bounds.height - logoSide) / 2, width: logoSide, height: logoSide)

        let railWidth: CGFloat = 65
        controlRail.frame = NSRect(
            x: bounds.width - railWidth - 5,
            y: (bounds.height - 24) / 2,
            width: railWidth,
            height: 24)
        controlRail.layer?.cornerRadius = 12

        let buttonSide: CGFloat = 20
        styleButton.frame = NSRect(x: 3, y: 2, width: buttonSide, height: buttonSide)
        runButton.frame = NSRect(x: 23.5, y: 2, width: buttonSide, height: buttonSide)
        endButton.frame = NSRect(x: 42, y: 2, width: buttonSide, height: buttonSide)

        wordmarkView.frame = NSRect(x: 36, y: (bounds.height - 18) / 2 + 1, width: 50, height: 18)
        let dotSize: CGFloat = 7
        let dotX = min(wordmarkView.frame.maxX + 2, controlRail.frame.minX - dotSize - 7)
        dotView.frame = NSRect(x: dotX, y: bounds.midY + 4.5, width: dotSize, height: dotSize)
        dotView.layer?.cornerRadius = dotSize / 2
    }

    func setRunState(_ state: PillRunState) {
        runState = state
        dotColor = state.dotColor
        updateRunStateDisplay()
    }

    private func updateRunStateDisplay() {
        configureRunButton()
        runButton.layer?.backgroundColor = runState.symbolColor.withAlphaComponent(
            materialAlpha(runState == .listening ? 0.18 : 0.07)).cgColor
        runButton.layer?.borderColor = runState.symbolColor.withAlphaComponent(materialAlpha(0.25)).cgColor
        runButton.layer?.shadowColor = runState.symbolColor.cgColor
        runButton.layer?.shadowOpacity = runState == .listening ? 0.34 : 0
        runButton.layer?.shadowRadius = runState == .listening ? 7 : 0
        runButton.layer?.shadowOffset = .zero
        updateRunPulseAnimation()
        setAccessibilityLabel(runState.accessibilityLabel)
        needsLayout = true
    }

    private func updateRunPulseAnimation() {
        let isActive = runState == .listening || runState == .connecting
        if isActive {
            if dotView.layer?.animation(forKey: "bluey-dot-pulse") == nil {
                let dotPulse = CABasicAnimation(keyPath: "opacity")
                dotPulse.fromValue = 0.45
                dotPulse.toValue = 1.0
                dotPulse.duration = 0.58
                dotPulse.autoreverses = true
                dotPulse.repeatCount = .infinity
                dotView.layer?.add(dotPulse, forKey: "bluey-dot-pulse")
            }
            if runButton.layer?.animation(forKey: "bluey-run-glow") == nil {
                let glow = CABasicAnimation(keyPath: "shadowRadius")
                glow.fromValue = 4
                glow.toValue = 10
                glow.duration = 0.64
                glow.autoreverses = true
                glow.repeatCount = .infinity
                runButton.layer?.add(glow, forKey: "bluey-run-glow")
            }
        } else {
            dotView.layer?.removeAnimation(forKey: "bluey-dot-pulse")
            runButton.layer?.removeAnimation(forKey: "bluey-run-glow")
            runButton.layer?.shadowOpacity = 0
            runButton.layer?.shadowRadius = 0
        }
    }

    private func configureMiniButton(_ button: NSButton, symbol: String, fallback: String, tint: NSColor) {
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 9
        button.layer?.backgroundColor = NSColor.white.withAlphaComponent(materialAlpha(0.035)).cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = NSColor.white.withAlphaComponent(materialAlpha(0.08)).cgColor
        button.contentTintColor = tint
        if let image = symbolImage(symbol) {
            image.isTemplate = true
            button.image = image
            button.imagePosition = .imageOnly
            button.title = ""
        } else {
            button.image = nil
            button.attributedTitle = NSAttributedString(
                string: fallback,
                attributes: [
                    .font: NSFont.systemFont(ofSize: 10.5, weight: .heavy),
                    .foregroundColor: tint,
                ])
        }
    }

    private func configureRunButton() {
        configureMiniButton(runButton, symbol: "", fallback: runState == .listening ? "Ⅱ" : "▶", tint: runState.symbolColor)
        runButton.image = nil
        runButton.attributedTitle = NSAttributedString(
            string: runState == .listening ? "Ⅱ" : "▶",
            attributes: [
                .font: NSFont.systemFont(ofSize: runState == .listening ? 11.5 : 10.0, weight: .heavy),
                .foregroundColor: runState.symbolColor,
            ])
        runButton.alignment = .center
    }

    private func materialAlpha(_ base: CGFloat, floor: CGFloat = 0.0) -> CGFloat {
        blueyMaterialAlpha(base, opacity: backgroundOpacity, floor: floor)
    }

    func applyBackgroundOpacity(_ opacity: Double) {
        backgroundOpacity = min(max(CGFloat(opacity), 0.50), 1.0)
        controlRail.layer?.backgroundColor = NSColor.white.withAlphaComponent(materialAlpha(0.040)).cgColor
        controlRail.layer?.borderColor = NSColor.white.withAlphaComponent(materialAlpha(0.085)).cgColor
        configureMiniButton(styleButton, symbol: "text.cursor", fallback: "?", tint: BlueyTheme.cyan)
        configureMiniButton(endButton, symbol: "power", fallback: "×", tint: BlueyTheme.textDim)
        updateRunStateDisplay()
        needsDisplay = true
    }

    @objc private func runClicked() { onRunToggle?() }

    @objc private func styleClicked() { onAsk?() }

    @objc private func endClicked() { onEnd?() }

    override func draw(_ dirtyRect: NSRect) {
        NSGraphicsContext.saveGraphicsState()

        let outer = bounds.insetBy(dx: 0.85, dy: 0.85)
        let radius = outer.height / 2
        let path = NSBezierPath(roundedRect: outer, xRadius: radius, yRadius: radius)
        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(materialAlpha(0.34, floor: 0.10))
        shadow.shadowBlurRadius = 9
        shadow.shadowOffset = .zero
        shadow.set()

        let bg = NSGradient(colors: [
            NSColor(red: 0.007, green: 0.011, blue: 0.017, alpha: materialAlpha(0.98)),
            NSColor(red: 0.014, green: 0.026, blue: 0.032, alpha: materialAlpha(0.95)),
            NSColor(red: 0.007, green: 0.010, blue: 0.015, alpha: materialAlpha(0.99)),
        ])
        bg?.draw(in: path, angle: -12)

        NSGraphicsContext.restoreGraphicsState()

        NSColor(red: 0.26, green: 0.74, blue: 0.96, alpha: materialAlpha(0.38)).setStroke()
        path.lineWidth = 1.0
        path.stroke()

        let inner = outer.insetBy(dx: 1.5, dy: 1.5)
        let innerPath = NSBezierPath(roundedRect: inner, xRadius: inner.height / 2, yRadius: inner.height / 2)
        NSColor.white.withAlphaComponent(materialAlpha(0.050)).setStroke()
        innerPath.lineWidth = 0.7
        innerPath.stroke()

        let gloss = NSBezierPath(roundedRect: outer.insetBy(dx: 2, dy: 2), xRadius: radius - 2, yRadius: radius - 2)
        NSGradient(colors: [
            NSColor.white.withAlphaComponent(materialAlpha(0.08)),
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
    var onOpenURL: ((URL) -> Void)?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.backgroundColor = BlueyTheme.panel.cgColor
        layer?.cornerRadius = 16
        layer?.masksToBounds = true
        layer?.borderWidth = 1
        layer?.borderColor = BlueyTheme.hairline.cgColor

        stack.orientation = .vertical
        stack.alignment = .centerX
        stack.spacing = 12
        stack.edgeInsets = NSEdgeInsets(top: 20, left: 0, bottom: 16, right: 0)
        stack.translatesAutoresizingMaskIntoConstraints = false

        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        scroll.borderType = .noBorder
        scroll.drawsBackground = false
        scroll.documentView = stack
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.contentView.wantsLayer = true
        scroll.contentView.layer?.masksToBounds = true
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

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    func push(_ card: RenderedCard) {
        if normalizedCardKind(card.kind) == "transcript" {
            onTranscript?(card)
            pushTranscriptCard(card)
            emitCardRendered(id: card.id)
            return
        }
        if loginURL(from: card) != nil {
            removeAllCards()
        }
        cards.append(card)
        emptyState.isHidden = true
        let view = makeCardView(card)
        stack.addArrangedSubview(view)
        view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        scrollToBottom()
        emitCardRendered(id: card.id)
    }

    func pushTranscript(source: String, body: String) {
        let cleanBody = displayTranscriptText(body)
        guard !cleanBody.isEmpty else { return }
        let card = RenderedCard(
            id: "transcript-\(UUID().uuidString)",
            kind: "transcript",
            title: source,
            body: cleanBody,
            done: true,
            costLabel: nil,
            artifact: nil)
        pushTranscriptCard(card)
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
        replaceCardView(at: idx)
        scrollToBottom()
        return cards[idx]
    }

    func clear() {
        removeAllCards()
        emptyState.isHidden = false
    }

    func forwardScrollWheel(_ event: NSEvent) {
        scroll.scrollWheel(with: event)
    }

    override func scrollWheel(with event: NSEvent) {
        scroll.scrollWheel(with: event)
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard bounds.contains(point) else { return nil }
        return super.hitTest(point) ?? scroll
    }

    func hasCopyControl(atScreenPoint screenPoint: NSPoint) -> Bool {
        guard let window else { return false }
        let windowPoint = window.convertPoint(fromScreen: screenPoint)
        let localPoint = convert(windowPoint, from: nil)
        guard bounds.contains(localPoint) else { return false }

        var hit: NSView? = hitTest(localPoint)
        while let view = hit {
            if view is CopyCardButton {
                return true
            }
            hit = view.superview
        }
        return false
    }

    func applyBackgroundOpacity(_ opacity: CGFloat) {
        layer?.backgroundColor = BlueyTheme.panel
            .withAlphaComponent(blueyMaterialAlpha(0.95, opacity: opacity))
            .cgColor
    }

    private func removeAllCards() {
        cards.removeAll()
        for view in stack.arrangedSubviews {
            stack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
    }

    private func pushTranscriptCard(_ input: RenderedCard) {
        let source = transcriptCardSource(input)
        let cleanBody = displayTranscriptText(input.body)
        guard !cleanBody.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        let card = RenderedCard(
            id: input.id,
            kind: input.kind,
            title: source,
            body: cleanBody,
            done: input.done,
            costLabel: input.costLabel,
            artifact: input.artifact)

        emptyState.isHidden = true
        if let lastIndex = cards.indices.last,
           normalizedCardKind(cards[lastIndex].kind) == "transcript",
           transcriptCardSource(cards[lastIndex]) == source {
            let merged = mergedTranscriptBody(cards[lastIndex].body, card.body)
            guard merged != cards[lastIndex].body else {
                scrollToBottom()
                return
            }
            cards[lastIndex].body = merged
            replaceCardView(at: lastIndex)
            scrollToBottom()
            return
        }

        cards.append(card)
        let view = makeCardView(card)
        stack.addArrangedSubview(view)
        view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        scrollToBottom()
    }

    private func replaceCardView(at idx: Int) {
        guard stack.arrangedSubviews.indices.contains(idx) else { return }
        let existing = stack.arrangedSubviews[idx]
        stack.removeArrangedSubview(existing)
        existing.removeFromSuperview()
        let view = makeCardView(cards[idx])
        stack.insertArrangedSubview(view, at: idx)
        view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
    }

    private func transcriptCardSource(_ card: RenderedCard) -> String {
        let title = card.title.trimmingCharacters(in: .whitespacesAndNewlines)
        if !title.isEmpty {
            let lower = title.lowercased()
            if lower.contains("microphone") || lower.contains("mic") || lower == "user" {
                return "Mic"
            }
            if lower.contains("system") {
                return "System"
            }
            return title.capitalized
        }
        return "Audio"
    }

    private func mergedTranscriptBody(_ existing: String, _ incoming: String) -> String {
        let old = existing.trimmingCharacters(in: .whitespacesAndNewlines)
        let new = incoming.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !old.isEmpty else { return new }
        guard !new.isEmpty else { return old }

        let oldNorm = normalizeTranscriptBody(old)
        let newNorm = normalizeTranscriptBody(new)
        if oldNorm == newNorm || oldNorm.contains(newNorm) { return old }
        if newNorm.contains(oldNorm) { return new }

        let oldWords = old.split(whereSeparator: { $0.isWhitespace }).map(String.init)
        let newWords = new.split(whereSeparator: { $0.isWhitespace }).map(String.init)
        let maxOverlap = min(oldWords.count, newWords.count, 8)
        if maxOverlap > 0 {
            for count in stride(from: maxOverlap, through: 1, by: -1) {
                let suffix = oldWords.suffix(count).joined(separator: " ")
                let prefix = newWords.prefix(count).joined(separator: " ")
                if normalizeTranscriptBody(suffix) == normalizeTranscriptBody(prefix) {
                    return (oldWords + newWords.dropFirst(count)).joined(separator: " ")
                }
            }
        }
        return "\(old) \(new)"
    }

    private func normalizeTranscriptBody(_ value: String) -> String {
        value
            .lowercased()
            .replacingOccurrences(of: #"[^a-z0-9]+"#, with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespacesAndNewlines)
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
        let kind = normalizedCardKind(card.kind)
        let accent = BlueyTheme.accent(for: kind)
        let rightAligned = isUserSide(card)
        let answerLike = kind == "answer"
        let signInLike = loginURL(from: card) != nil
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
            : accent.withAlphaComponent(answerLike ? 0.24 : 0.14).cgColor
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

        let signInURL = signInLike ? loginURL(from: card) : nil
        let rawBody = card.body.isEmpty && !card.done ? "Thinking..." : card.body
        let bodyText = signInURL == nil
            ? chatBody(for: card, rawBody: rawBody)
            : signInBody(from: rawBody)
        let bodyLabel = NSTextField(wrappingLabelWithString: bodyText)
        bodyLabel.font = bodyFont(for: card)
        bodyLabel.textColor = rightAligned ? NSColor.black : BlueyTheme.text
        bodyLabel.alignment = signInURL == nil ? .left : .center
        bodyLabel.translatesAutoresizingMaskIntoConstraints = false
        bodyLabel.preferredMaxLayoutWidth = signInURL == nil ? (rightAligned ? 360 : 480) : 430

        let statusLabel = NSTextField(labelWithString: statusText(for: card))
        statusLabel.font = NSFont.monospacedSystemFont(ofSize: 9.5, weight: .semibold)
        statusLabel.textColor = rightAligned ? NSColor.black.withAlphaComponent(0.46) : BlueyTheme.textDim
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        let copyButton = signInURL == nil && !rightAligned
            ? makeCopyCardButton(text: rawBody.isEmpty ? bodyText : rawBody, rightAligned: rightAligned)
            : nil

        let signInButton: NSButton? = signInURL.map { url in
            let button = NSButton(title: "Open login", target: self, action: #selector(openURLButtonClicked(_:)))
            button.translatesAutoresizingMaskIntoConstraints = false
            button.identifier = NSUserInterfaceItemIdentifier(url.absoluteString)
            styleSignInButton(button)
            return button
        }
        if signInURL != nil {
            bubble.layer?.backgroundColor = NSColor(red: 0.020, green: 0.030, blue: 0.040, alpha: 0.98).cgColor
            bubble.layer?.borderWidth = 1
            bubble.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.30).cgColor
            metaLabel.isHidden = true
            statusLabel.isHidden = true
            titleLabel.font = NSFont.systemFont(ofSize: 15.5, weight: .bold)
            titleLabel.alignment = .center
            bodyLabel.preferredMaxLayoutWidth = 360
        }

        row.addSubview(bubble)
        bubble.addSubview(metaLabel)
        bubble.addSubview(titleLabel)
        bubble.addSubview(bodyLabel)
        bubble.addSubview(statusLabel)
        if let copyButton {
            bubble.addSubview(copyButton)
        }
        if let signInButton {
            bubble.addSubview(signInButton)
        }

        let leading = bubble.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 8)
        let trailing = bubble.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -8)
        if signInLike {
            leading.priority = .defaultLow
            trailing.priority = .defaultLow
        } else if rightAligned {
            leading.priority = .defaultLow
            trailing.priority = .required
        } else {
            leading.priority = .required
            trailing.priority = .defaultLow
        }

        var constraints: [NSLayoutConstraint] = [
            row.heightAnchor.constraint(greaterThanOrEqualTo: bubble.heightAnchor),
            bubble.topAnchor.constraint(equalTo: row.topAnchor),
            bubble.bottomAnchor.constraint(equalTo: row.bottomAnchor),
            leading,
            trailing,
            bubble.widthAnchor.constraint(lessThanOrEqualTo: row.widthAnchor, multiplier: signInLike ? 0.62 : (answerLike ? 0.90 : (rightAligned ? 0.70 : 0.78))),
            bubble.widthAnchor.constraint(greaterThanOrEqualToConstant: signInLike ? 330 : (answerLike ? 240 : 170)),
        ]
        if signInLike {
            constraints.append(contentsOf: [
                bubble.centerXAnchor.constraint(equalTo: row.centerXAnchor),
                titleLabel.topAnchor.constraint(equalTo: bubble.topAnchor, constant: 18),
                titleLabel.leadingAnchor.constraint(equalTo: bubble.leadingAnchor, constant: 18),
                titleLabel.trailingAnchor.constraint(equalTo: bubble.trailingAnchor, constant: -18),

                bodyLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 12),
                bodyLabel.leadingAnchor.constraint(equalTo: bubble.leadingAnchor, constant: 28),
                bodyLabel.trailingAnchor.constraint(equalTo: bubble.trailingAnchor, constant: -28),
            ])
        } else {
            constraints.append(contentsOf: [
                metaLabel.topAnchor.constraint(equalTo: bubble.topAnchor, constant: 10),
                metaLabel.leadingAnchor.constraint(equalTo: bubble.leadingAnchor, constant: 14),

                titleLabel.centerYAnchor.constraint(equalTo: metaLabel.centerYAnchor),
                titleLabel.leadingAnchor.constraint(equalTo: metaLabel.trailingAnchor, constant: 8),
                titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: statusLabel.leadingAnchor, constant: -10),

                statusLabel.centerYAnchor.constraint(equalTo: metaLabel.centerYAnchor),

                bodyLabel.topAnchor.constraint(equalTo: metaLabel.bottomAnchor, constant: 8),
                bodyLabel.leadingAnchor.constraint(equalTo: metaLabel.leadingAnchor),
                bodyLabel.trailingAnchor.constraint(equalTo: bubble.trailingAnchor, constant: -14),
            ])
            if let copyButton {
                constraints.append(contentsOf: [
                    statusLabel.trailingAnchor.constraint(lessThanOrEqualTo: copyButton.leadingAnchor, constant: -6),
                    copyButton.centerYAnchor.constraint(equalTo: metaLabel.centerYAnchor),
                    copyButton.trailingAnchor.constraint(equalTo: bubble.trailingAnchor, constant: -10),
                    copyButton.widthAnchor.constraint(equalToConstant: 22),
                    copyButton.heightAnchor.constraint(equalToConstant: 22),
                ])
            } else {
                constraints.append(statusLabel.trailingAnchor.constraint(equalTo: bubble.trailingAnchor, constant: -14))
            }
        }
        if let signInButton {
            constraints.append(contentsOf: [
                bodyLabel.bottomAnchor.constraint(equalTo: signInButton.topAnchor, constant: -12),
                signInButton.centerXAnchor.constraint(equalTo: bubble.centerXAnchor),
                signInButton.bottomAnchor.constraint(equalTo: bubble.bottomAnchor, constant: -14),
                signInButton.widthAnchor.constraint(equalToConstant: 150),
                signInButton.heightAnchor.constraint(equalToConstant: 38),
            ])
        } else {
            constraints.append(bodyLabel.bottomAnchor.constraint(equalTo: bubble.bottomAnchor, constant: -12))
        }
        NSLayoutConstraint.activate(constraints)
        return row
    }

    private func makeCopyCardButton(text: String, rightAligned: Bool) -> CopyCardButton {
        let button = CopyCardButton(title: "", target: self, action: #selector(copyCardClicked(_:)))
        button.translatesAutoresizingMaskIntoConstraints = false
        button.copyText = text
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 10
        button.layer?.backgroundColor = rightAligned
            ? NSColor.black.withAlphaComponent(0.06).cgColor
            : NSColor.white.withAlphaComponent(0.045).cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = rightAligned
            ? NSColor.black.withAlphaComponent(0.10).cgColor
            : BlueyTheme.hairline.cgColor
        button.contentTintColor = rightAligned
            ? NSColor.black.withAlphaComponent(0.56)
            : BlueyTheme.textDim
        if let image = symbolImage("doc.on.doc") {
            image.isTemplate = true
            button.image = image
            button.imagePosition = .imageOnly
            button.imageScaling = .scaleProportionallyDown
        } else {
            button.title = "Copy"
        }
        button.toolTip = "Copy this message"
        return button
    }

    @objc private func copyCardClicked(_ sender: CopyCardButton) {
        let text = sender.copyText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    private func styleSignInButton(_ button: NSButton) {
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 17
        button.layer?.backgroundColor = NSColor(red: 0.64, green: 0.93, blue: 1.0, alpha: 0.96).cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = NSColor.white.withAlphaComponent(0.20).cgColor
        button.font = NSFont.systemFont(ofSize: 12.5, weight: .bold)
        button.attributedTitle = NSAttributedString(
            string: "Open login",
            attributes: [
                .font: button.font ?? NSFont.systemFont(ofSize: 12.5, weight: .bold),
                .foregroundColor: NSColor.black.withAlphaComponent(0.86),
            ])
        button.contentTintColor = NSColor.black.withAlphaComponent(0.82)
        if let image = symbolImage("arrow.up.right") {
            image.isTemplate = true
            button.image = image
            button.imagePosition = .imageTrailing
            button.imageScaling = .scaleProportionallyDown
        }
        button.imageHugsTitle = true
        button.alignment = .center
        button.toolTip = "Open Bluey login"
    }

    @objc private func openURLButtonClicked(_ sender: NSButton) {
        guard
            let raw = sender.identifier?.rawValue,
            let url = URL(string: raw)
        else { return }
        onOpenURL?(url)
    }

    private func kindLabel(_ card: RenderedCard) -> String {
        switch normalizedCardKind(card.kind) {
        case "answer":      return "BLUEY"
        case "question":    return "YOU"
        case "action_item": return "ACTION"
        case "decision":    return "DECISION"
        case "context":     return "CONTEXT"
        case "transcript":
            let source = card.title.trimmingCharacters(in: .whitespacesAndNewlines)
            return source.isEmpty ? "AUDIO" : source.uppercased()
        case "warning":     return "WARNING"
        case "system":      return "SYSTEM"
        default:            return "BLUEY"
        }
    }

    private func displayTitle(for card: RenderedCard) -> String {
        switch normalizedCardKind(card.kind) {
        case "answer", "question", "transcript":
            return ""
        default:
            return card.title.isEmpty ? kindTitle(card.kind) : card.title
        }
    }

    private func isUserSide(_ card: RenderedCard) -> Bool {
        let kind = normalizedCardKind(card.kind)
        return kind == "question" || kind == "transcript"
    }

    private func bodyFont(for card: RenderedCard) -> NSFont {
        return NSFont.systemFont(ofSize: 13.5, weight: .regular)
    }

    private func chatBody(for card: RenderedCard, rawBody: String) -> String {
        guard normalizedCardKind(card.kind) == "answer" else { return rawBody }

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
        if loginURL(from: card) != nil { return "login" }
        if let costLabel = card.costLabel, !costLabel.isEmpty { return costLabel }
        switch normalizedCardKind(card.kind) {
        case "answer":   return ""
        case "question": return "sent"
        default:         return ""
        }
    }

    private func kindTitle(_ kind: String) -> String {
        switch normalizedCardKind(kind) {
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

    private func signInBody(from text: String) -> String {
        text.components(separatedBy: .newlines)
            .filter { !$0.trimmingCharacters(in: .whitespaces).hasPrefix("login_url:") }
            .joined(separator: "\n")
            .replacingOccurrences(of: "knowledge base", with: "documents")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func loginURL(from card: RenderedCard) -> URL? {
        guard normalizedCardKind(card.kind) == "system" else { return nil }
        for line in card.body.components(separatedBy: .newlines) {
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            let candidate: String
            if trimmed.hasPrefix("login_url:") {
                candidate = trimmed
                    .replacingOccurrences(of: "login_url:", with: "")
                    .trimmingCharacters(in: .whitespacesAndNewlines)
            } else if trimmed.hasPrefix("https://") || trimmed.hasPrefix("http://") {
                candidate = trimmed
            } else {
                continue
            }
            if
                let url = URL(string: candidate),
                let scheme = url.scheme?.lowercased(),
                ["http", "https"].contains(scheme)
            {
                return url
            }
        }
        return nil
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
    private let fullWindowButton = NSButton(title: "", target: nil, action: nil)
    private let copyButton = NSButton(title: "", target: nil, action: nil)
    private let closeButton = NSButton(title: "", target: nil, action: nil)
    private let scroll = NSScrollView()
    private let textView = NSTextView()
    private var currentText = ""
    private var fullWindow = false

    var onCollapse: (() -> Void)?
    var onToggleFullWindow: (() -> Void)?

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
        fullWindowButton.translatesAutoresizingMaskIntoConstraints = false
        copyButton.translatesAutoresizingMaskIntoConstraints = false
        closeButton.translatesAutoresizingMaskIntoConstraints = false
        scroll.translatesAutoresizingMaskIntoConstraints = false

        addSubview(header)
        header.addSubview(iconView)
        header.addSubview(titleLabel)
        header.addSubview(subtitleLabel)
        header.addSubview(fullWindowButton)
        header.addSubview(copyButton)
        header.addSubview(closeButton)
        addSubview(scroll)

        iconView.imageScaling = .scaleProportionallyDown
        iconView.contentTintColor = BlueyTheme.cyan

        titleLabel.font = NSFont.systemFont(ofSize: 12.5, weight: .bold)
        titleLabel.textColor = BlueyTheme.text
        subtitleLabel.font = NSFont.systemFont(ofSize: 10.5, weight: .medium)
        subtitleLabel.textColor = BlueyTheme.textDim
        subtitleLabel.lineBreakMode = .byTruncatingTail

        styleCanvasHeaderButton(copyButton, symbol: "doc.on.doc", fallback: "C")
        copyButton.toolTip = "Copy canvas"
        copyButton.target = self
        copyButton.action = #selector(copyClicked)

        styleCanvasHeaderButton(fullWindowButton, symbol: "arrow.up.left.and.arrow.down.right", fallback: "[]")
        fullWindowButton.toolTip = "Expand canvas"
        fullWindowButton.target = self
        fullWindowButton.action = #selector(fullWindowClicked)

        styleCanvasHeaderButton(closeButton, symbol: "chevron.right", fallback: "<")
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

            copyButton.trailingAnchor.constraint(equalTo: closeButton.leadingAnchor, constant: -6),
            copyButton.topAnchor.constraint(equalTo: header.topAnchor),
            copyButton.widthAnchor.constraint(equalToConstant: 26),
            copyButton.heightAnchor.constraint(equalToConstant: 26),

            fullWindowButton.trailingAnchor.constraint(equalTo: copyButton.leadingAnchor, constant: -6),
            fullWindowButton.topAnchor.constraint(equalTo: header.topAnchor),
            fullWindowButton.widthAnchor.constraint(equalToConstant: 26),
            fullWindowButton.heightAnchor.constraint(equalToConstant: 26),

            titleLabel.leadingAnchor.constraint(equalTo: iconView.trailingAnchor, constant: 8),
            titleLabel.topAnchor.constraint(equalTo: header.topAnchor),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: fullWindowButton.leadingAnchor, constant: -8),

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

    func applyBackgroundOpacity(_ opacity: CGFloat) {
        layer?.backgroundColor = NSColor(
            red: 0.020,
            green: 0.024,
            blue: 0.031,
            alpha: blueyMaterialAlpha(0.98, opacity: opacity)
        ).cgColor
    }

    func render(_ artifact: CanvasArtifact) {
        titleLabel.stringValue = artifact.title
        subtitleLabel.stringValue = artifact.subtitle
        currentText = artifact.content
        if let image = symbolImage(artifact.kind.icon) {
            image.isTemplate = true
            iconView.image = image
        }
        textView.string = artifact.content
        textView.scrollRangeToVisible(NSRange(location: 0, length: 0))
    }

    func setFullWindow(_ value: Bool) {
        fullWindow = value
        let symbol = value
            ? "arrow.down.right.and.arrow.up.left"
            : "arrow.up.left.and.arrow.down.right"
        styleCanvasHeaderButton(fullWindowButton, symbol: symbol, fallback: value ? "><" : "[]")
        fullWindowButton.toolTip = value ? "Restore canvas size" : "Expand canvas"
    }

    private func styleCanvasHeaderButton(_ button: NSButton, symbol: String, fallback: String) {
        button.title = ""
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 12
        button.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.040).cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = BlueyTheme.hairline.cgColor
        button.contentTintColor = BlueyTheme.textDim
        button.font = NSFont.systemFont(ofSize: 10.5, weight: .bold)
        if let image = symbolImage(symbol) {
            image.isTemplate = true
            button.image = image
            button.imagePosition = .imageOnly
            button.imageScaling = .scaleProportionallyDown
        } else {
            button.image = nil
            button.attributedTitle = NSAttributedString(
                string: fallback,
                attributes: [
                    .font: button.font ?? NSFont.systemFont(ofSize: 10.5, weight: .bold),
                    .foregroundColor: BlueyTheme.textDim,
                ])
        }
        button.imageHugsTitle = true
        button.alignment = .center
    }

    @objc private func copyClicked() {
        let text = currentText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    @objc private func fullWindowClicked() {
        onToggleFullWindow?()
    }

    @objc private func collapseClicked() {
        onCollapse?()
    }
}

// MARK: - Expanded panel (feed + composer)

private final class ExpandedPanelView: NSView, NSTextFieldDelegate {
    let feed: FeedView
    let workspace: NSView
    let canvasPane: CanvasPaneView
    let toastView: NSView
    let toastTitleLabel: NSTextField
    let toastBodyLabel: NSTextField
    let headerBar: HeaderDragView
    let headerStack: NSStackView
    let brandStack: NSStackView
    let headerLogo: BlueyLogoView
    let headerWordmark: BlueyWordmarkView
    let headerSpacer: NSView
    let statusLabel: NSTextField
    let modelMenu: NSPopUpButton
    let routeBadge: NSTextField
    let knowledgeBadge: NSTextField
    let balanceLabel: NSTextField
    let fullSizeButton: NSButton
    let canvasToggleButton: NSButton
    let navButton: NSButton
    let newSessionButton: NSButton
    let sessionDrawer: NSView
    let drawerTitleLabel: NSTextField
    let drawerSubtitleLabel: NSTextField
    let drawerCloseButton: NSButton
    let latestSessionButton: NSButton
    let sessionScroll: NSScrollView
    let sessionStack: NSStackView
    let answerStyleOverlay: NSView
    let answerStylePanel: NSView
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
    var onListeningStateChanged: ((PillRunState) -> Void)?
    private var recordingActive = false
    private var transcriptSnippets: [String] = []
    private var latestLiveTranscriptLine: String?
    private var sessionItems: [OverlaySessionItem] = []
    private var editingSessionId: String?
    private var pendingDeleteSessionId: String?
    private var renameField: NSTextField?
    private var canvasWidthConstraint: NSLayoutConstraint?
    private var composerBarHeightConstraint: NSLayoutConstraint?
    private var composerTextHeightConstraint: NSLayoutConstraint?
    private var attachmentStripHeightConstraint: NSLayoutConstraint?
    private var toastHideWorkItem: DispatchWorkItem?
    private var knowledgeIndexTimer: Timer?
    private var knowledgeIndexFrame = 0
    private var audioPulseTimer: Timer?
    private var audioPulseFrame = 0
    private let knowledgeIndexFrames = [
        "Indexing · ● 101",
        "Indexing · ● 010",
        "Indexing · ● 111",
        "Indexing · ● 001",
    ]
    private var latestCanvas: CanvasArtifact?
    private var canvasOpen = false
    private var canvasFullWindow = false
    private var preCanvasFullWindowFrame: NSRect?
    private var windowFullSize = false
    private var preWindowFullSizeFrame: NSRect?
    private struct ResizeEdges: OptionSet {
        let rawValue: Int
        static let left = ResizeEdges(rawValue: 1 << 0)
        static let right = ResizeEdges(rawValue: 1 << 1)
        static let top = ResizeEdges(rawValue: 1 << 2)
        static let bottom = ResizeEdges(rawValue: 1 << 3)
    }
    private var activeResizeEdges: ResizeEdges = []
    private var resizeStartMouse = NSPoint.zero
    private var resizeStartFrame = NSRect.zero
    private let resizeHitSize: CGFloat = 10
    private var backgroundOpacity: CGFloat = 0.94

    override init(frame frameRect: NSRect) {
        feed = FeedView(frame: .zero)
        workspace = NSView()
        canvasPane = CanvasPaneView(frame: .zero)
        toastView = NSView()
        toastTitleLabel = NSTextField(labelWithString: "")
        toastBodyLabel = NSTextField(wrappingLabelWithString: "")
        headerBar = HeaderDragView()
        headerStack = NSStackView()
        brandStack = NSStackView()
        headerLogo = BlueyLogoView()
        headerWordmark = BlueyWordmarkView()
        headerSpacer = NSView()
        statusLabel = NSTextField(labelWithString: "New recording")
        modelMenu = NSPopUpButton(frame: .zero, pullsDown: false)
        routeBadge = NSTextField(labelWithString: "● Ready")
        knowledgeBadge = NSTextField(labelWithString: "Docs empty")
        balanceLabel = NSTextField(labelWithString: "Balance --")
        fullSizeButton = NSButton(title: "", target: nil, action: nil)
        canvasToggleButton = NSButton(title: "", target: nil, action: nil)
        navButton = NSButton(title: "", target: nil, action: nil)
        newSessionButton = NSButton(title: "", target: nil, action: nil)
        sessionDrawer = NSView()
        drawerTitleLabel = NSTextField(labelWithString: "Recordings")
        drawerSubtitleLabel = NSTextField(labelWithString: "Click to continue. Pencil to rename.")
        drawerCloseButton = NSButton(title: "", target: nil, action: nil)
        latestSessionButton = NSButton(title: "Continue latest", target: nil, action: nil)
        sessionScroll = NSScrollView()
        sessionStack = NSStackView()
        answerStyleOverlay = ModalBlockerView()
        answerStylePanel = NSView()
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
        composerSurface = ComposerSurfaceView()
        composer = ComposerTextView(frame: .zero, textContainer: nil)
        recordingButton = NSButton(title: "Listen", target: nil, action: nil)
        askButton = NSButton(title: "Answer ⌘↵", target: nil, action: nil)
        analyzeButton = NSButton(title: "Screen", target: nil, action: nil)
        attachButton = NSButton(title: "", target: nil, action: nil)
        instructionsButton = NSButton(title: "Tone", target: nil, action: nil)
        opacityControl = NSView()
        opacityLabel = NSTextField(labelWithString: "Opacity")
        opacitySlider = NSSlider(value: 0.94, minValue: 0.50, maxValue: 1.0, target: nil, action: nil)
        opacityValueLabel = NSTextField(labelWithString: "94")
        hideButton = NSButton(title: "", target: nil, action: nil)
        closeButton = NSButton(title: "x", target: nil, action: nil)
        closeConfirmOverlay = ModalBlockerView()
        closeConfirmPanel = NSView()
        closeConfirmTitle = NSTextField(labelWithString: "Turn Bluey off?")
        closeConfirmBody = NSTextField(wrappingLabelWithString: "This closes Bluey completely. To start again, run: bluey on")
        closeConfirmCancelButton = NSButton(title: "Cancel", target: nil, action: nil)
        closeConfirmTurnOffButton = NSButton(title: "Turn Off", target: nil, action: nil)

        super.init(frame: frameRect)

        (answerStyleOverlay as? ModalBlockerView)?.onEscape = { [weak self] in
            self?.dismissAnswerStyleEditor(animated: true)
        }
        (closeConfirmOverlay as? ModalBlockerView)?.onEscape = { [weak self] in
            self?.dismissCloseConfirm(animated: true)
        }

        applyShellChrome()
        layer?.backgroundColor = NSColor(red: 0.010, green: 0.012, blue: 0.016, alpha: 0.94).cgColor
        layer?.masksToBounds = true
        layer?.borderWidth = 1
        layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.14).cgColor
        layer?.shadowColor = NSColor.black.cgColor
        layer?.shadowOpacity = 0.28
        layer?.shadowRadius = 24
        layer?.shadowOffset = .zero
        workspace.wantsLayer = true
        workspace.layer?.backgroundColor = NSColor.clear.cgColor
        workspace.layer?.masksToBounds = true

        configureHeader()
        configureSystemToast()
        configureContextRows()
        configureComposer()
        configureFixedChromeLayoutPriorities()
        configureCloseConfirm()
        styleDrawer()
        feed.onTranscript = { [weak self] card in
            self?.appendTranscriptSnippet(card)
        }
        feed.onOpenURL = { url in
            NSWorkspace.shared.open(url)
        }

        for view in [
            headerBar,
            headerStack,
            brandStack,
            headerLogo,
            headerWordmark,
            headerSpacer,
            statusLabel,
            modelMenu,
            routeBadge,
            knowledgeBadge,
            balanceLabel,
            fullSizeButton,
            canvasToggleButton,
            navButton,
            newSessionButton,
            sessionDrawer,
            drawerTitleLabel,
            drawerSubtitleLabel,
            drawerCloseButton,
            latestSessionButton,
            sessionScroll,
            sessionStack,
            answerStyleOverlay,
            answerStylePanel,
            answerStyleLabel,
            answerStyleBox,
            answerStyleSaveButton,
            workspace,
            feed,
            canvasPane,
            toastView,
            toastTitleLabel,
            toastBodyLabel,
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
        brandStack.addArrangedSubview(headerWordmark)
        brandStack.addArrangedSubview(statusLabel)
        for view in [
            navButton,
            newSessionButton,
            headerLogo,
            brandStack,
            routeBadge,
            knowledgeBadge,
            headerSpacer,
            canvasToggleButton,
            balanceLabel,
            fullSizeButton,
            hideButton,
            closeButton,
        ] {
            headerStack.addArrangedSubview(view)
        }
        addSubview(workspace)
        workspace.addSubview(feed)
        workspace.addSubview(canvasPane)
        addSubview(toastView)
        toastView.addSubview(toastTitleLabel)
        toastView.addSubview(toastBodyLabel)
        addSubview(sessionDrawer)
        sessionDrawer.addSubview(drawerTitleLabel)
        sessionDrawer.addSubview(drawerSubtitleLabel)
        sessionDrawer.addSubview(drawerCloseButton)
        sessionDrawer.addSubview(latestSessionButton)
        sessionDrawer.addSubview(sessionScroll)
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
        composerSurface.addSubview(recordingButton)
        composerSurface.addSubview(askButton)
        composerBar.addSubview(attachButton)
        composerBar.addSubview(instructionsButton)
        composerBar.addSubview(opacityControl)
        opacityControl.addSubview(opacityLabel)
        opacityControl.addSubview(opacitySlider)
        opacityControl.addSubview(opacityValueLabel)
        composerBar.addSubview(modelMenu)
        composerBar.addSubview(analyzeButton)
        // Add the header late in the root view so it paints above the scroll
        // workspace. Full-screen modal overlays are added after this.
        addSubview(headerBar)
        addSubview(answerStyleOverlay)
        answerStyleOverlay.addSubview(answerStylePanel)
        answerStylePanel.addSubview(answerStyleLabel)
        answerStylePanel.addSubview(answerStyleBox)
        answerStylePanel.addSubview(answerStyleSaveButton)
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
        toastView.layer?.zPosition = 70

        let canvasWidth = canvasPane.widthAnchor.constraint(equalToConstant: 0)
        canvasWidthConstraint = canvasWidth
        let composerTextHeight = composerSurface.heightAnchor.constraint(equalToConstant: 42)
        let composerBarHeight = composerBar.heightAnchor.constraint(equalToConstant: 94)
        let attachmentStripHeight = attachmentStrip.heightAnchor.constraint(equalToConstant: 0)
        composerTextHeightConstraint = composerTextHeight
        composerBarHeightConstraint = composerBarHeight
        attachmentStripHeightConstraint = attachmentStripHeight

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

            headerLogo.widthAnchor.constraint(equalToConstant: 28),
            headerLogo.heightAnchor.constraint(equalToConstant: 28),

            headerWordmark.widthAnchor.constraint(equalToConstant: 62),
            headerWordmark.heightAnchor.constraint(equalToConstant: 22),

            brandStack.widthAnchor.constraint(greaterThanOrEqualToConstant: 72),
            brandStack.widthAnchor.constraint(lessThanOrEqualToConstant: 128),

            routeBadge.widthAnchor.constraint(greaterThanOrEqualToConstant: 86),
            routeBadge.widthAnchor.constraint(lessThanOrEqualToConstant: 126),
            routeBadge.heightAnchor.constraint(equalToConstant: 24),

            knowledgeBadge.widthAnchor.constraint(greaterThanOrEqualToConstant: 86),
            knowledgeBadge.widthAnchor.constraint(lessThanOrEqualToConstant: 126),
            knowledgeBadge.heightAnchor.constraint(equalToConstant: 24),

            closeButton.widthAnchor.constraint(equalToConstant: 26),
            closeButton.heightAnchor.constraint(equalToConstant: 26),

            hideButton.widthAnchor.constraint(equalToConstant: 26),
            hideButton.heightAnchor.constraint(equalToConstant: 26),

            fullSizeButton.widthAnchor.constraint(equalToConstant: 26),
            fullSizeButton.heightAnchor.constraint(equalToConstant: 26),

            balanceLabel.widthAnchor.constraint(greaterThanOrEqualToConstant: 76),
            balanceLabel.widthAnchor.constraint(lessThanOrEqualToConstant: 106),
            balanceLabel.heightAnchor.constraint(equalToConstant: 24),

            canvasToggleButton.widthAnchor.constraint(equalToConstant: 30),
            canvasToggleButton.heightAnchor.constraint(equalToConstant: 30),

            workspace.topAnchor.constraint(equalTo: headerBar.bottomAnchor, constant: 12),
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

            toastView.topAnchor.constraint(equalTo: workspace.topAnchor, constant: 14),
            toastView.centerXAnchor.constraint(equalTo: centerXAnchor),
            toastView.widthAnchor.constraint(lessThanOrEqualTo: widthAnchor, multiplier: 0.52),
            toastView.widthAnchor.constraint(greaterThanOrEqualToConstant: 280),

            toastTitleLabel.topAnchor.constraint(equalTo: toastView.topAnchor, constant: 12),
            toastTitleLabel.leadingAnchor.constraint(equalTo: toastView.leadingAnchor, constant: 16),
            toastTitleLabel.trailingAnchor.constraint(equalTo: toastView.trailingAnchor, constant: -16),

            toastBodyLabel.topAnchor.constraint(equalTo: toastTitleLabel.bottomAnchor, constant: 5),
            toastBodyLabel.leadingAnchor.constraint(equalTo: toastTitleLabel.leadingAnchor),
            toastBodyLabel.trailingAnchor.constraint(equalTo: toastTitleLabel.trailingAnchor),
            toastBodyLabel.bottomAnchor.constraint(equalTo: toastView.bottomAnchor, constant: -13),

            sessionDrawer.topAnchor.constraint(equalTo: topAnchor, constant: 10),
            sessionDrawer.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            sessionDrawer.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            sessionDrawer.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -10),

            drawerTitleLabel.topAnchor.constraint(equalTo: sessionDrawer.topAnchor, constant: 14),
            drawerTitleLabel.leadingAnchor.constraint(equalTo: sessionDrawer.leadingAnchor, constant: 14),
            drawerTitleLabel.trailingAnchor.constraint(equalTo: drawerCloseButton.leadingAnchor, constant: -10),

            drawerCloseButton.topAnchor.constraint(equalTo: sessionDrawer.topAnchor, constant: 10),
            drawerCloseButton.trailingAnchor.constraint(equalTo: sessionDrawer.trailingAnchor, constant: -10),
            drawerCloseButton.widthAnchor.constraint(equalToConstant: 30),
            drawerCloseButton.heightAnchor.constraint(equalToConstant: 30),

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
            sessionScroll.bottomAnchor.constraint(equalTo: sessionDrawer.bottomAnchor, constant: -12),

            sessionStack.leadingAnchor.constraint(equalTo: sessionScroll.contentView.leadingAnchor),
            sessionStack.topAnchor.constraint(equalTo: sessionScroll.contentView.topAnchor),
            sessionStack.trailingAnchor.constraint(equalTo: sessionScroll.contentView.trailingAnchor),
            sessionStack.bottomAnchor.constraint(lessThanOrEqualTo: sessionScroll.contentView.bottomAnchor),
            sessionStack.widthAnchor.constraint(equalTo: sessionScroll.widthAnchor),

            answerStyleOverlay.topAnchor.constraint(equalTo: topAnchor),
            answerStyleOverlay.leadingAnchor.constraint(equalTo: leadingAnchor),
            answerStyleOverlay.trailingAnchor.constraint(equalTo: trailingAnchor),
            answerStyleOverlay.bottomAnchor.constraint(equalTo: bottomAnchor),

            answerStylePanel.centerXAnchor.constraint(equalTo: workspace.centerXAnchor),
            answerStylePanel.centerYAnchor.constraint(equalTo: workspace.centerYAnchor),
            answerStylePanel.widthAnchor.constraint(equalToConstant: 360),

            answerStyleLabel.topAnchor.constraint(equalTo: answerStylePanel.topAnchor, constant: 18),
            answerStyleLabel.leadingAnchor.constraint(equalTo: answerStylePanel.leadingAnchor, constant: 18),
            answerStyleLabel.trailingAnchor.constraint(equalTo: answerStylePanel.trailingAnchor, constant: -18),

            answerStyleBox.topAnchor.constraint(equalTo: answerStyleLabel.bottomAnchor, constant: 12),
            answerStyleBox.leadingAnchor.constraint(equalTo: answerStylePanel.leadingAnchor, constant: 18),
            answerStyleBox.trailingAnchor.constraint(equalTo: answerStylePanel.trailingAnchor, constant: -18),
            answerStyleBox.heightAnchor.constraint(equalToConstant: 48),

            answerStyleSaveButton.topAnchor.constraint(equalTo: answerStyleBox.bottomAnchor, constant: 14),
            answerStyleSaveButton.leadingAnchor.constraint(equalTo: answerStylePanel.leadingAnchor, constant: 18),
            answerStyleSaveButton.trailingAnchor.constraint(equalTo: answerStylePanel.trailingAnchor, constant: -18),
            answerStyleSaveButton.bottomAnchor.constraint(equalTo: answerStylePanel.bottomAnchor, constant: -18),
            answerStyleSaveButton.heightAnchor.constraint(equalToConstant: 36),

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
            attachmentStripHeight,

            attachmentStack.leadingAnchor.constraint(equalTo: attachmentStrip.contentView.leadingAnchor),
            attachmentStack.topAnchor.constraint(equalTo: attachmentStrip.contentView.topAnchor),
            attachmentStack.bottomAnchor.constraint(equalTo: attachmentStrip.contentView.bottomAnchor),
            attachmentStack.heightAnchor.constraint(equalTo: attachmentStrip.heightAnchor),

            composerBar.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            composerBar.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            composerBar.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -10),
            composerBarHeight,

            composerSurface.topAnchor.constraint(equalTo: composerBar.topAnchor, constant: 8),
            composerSurface.leadingAnchor.constraint(equalTo: composerBar.leadingAnchor, constant: 12),
            composerSurface.trailingAnchor.constraint(equalTo: composerBar.trailingAnchor, constant: -12),
            composerTextHeight,

            composer.topAnchor.constraint(equalTo: composerSurface.topAnchor, constant: 2),
            composer.leadingAnchor.constraint(equalTo: composerSurface.leadingAnchor, constant: 14),
            composer.trailingAnchor.constraint(equalTo: recordingButton.leadingAnchor, constant: -8),
            composer.bottomAnchor.constraint(equalTo: composerSurface.bottomAnchor, constant: -2),

            recordingButton.trailingAnchor.constraint(equalTo: askButton.leadingAnchor, constant: -6),
            recordingButton.centerYAnchor.constraint(equalTo: composerSurface.centerYAnchor),
            recordingButton.widthAnchor.constraint(equalToConstant: 80),
            recordingButton.heightAnchor.constraint(equalToConstant: 30),

            askButton.trailingAnchor.constraint(equalTo: composerSurface.trailingAnchor, constant: -7),
            askButton.centerYAnchor.constraint(equalTo: composerSurface.centerYAnchor),
            askButton.widthAnchor.constraint(equalToConstant: 106),
            askButton.heightAnchor.constraint(equalToConstant: 32),

            attachButton.leadingAnchor.constraint(equalTo: composerBar.leadingAnchor, constant: 12),
            attachButton.bottomAnchor.constraint(equalTo: composerBar.bottomAnchor, constant: -8),
            attachButton.widthAnchor.constraint(equalToConstant: 32),
            attachButton.heightAnchor.constraint(equalToConstant: 32),

            instructionsButton.leadingAnchor.constraint(equalTo: attachButton.trailingAnchor, constant: 8),
            instructionsButton.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            instructionsButton.widthAnchor.constraint(equalToConstant: 80),
            instructionsButton.heightAnchor.constraint(equalToConstant: 32),

            opacityControl.leadingAnchor.constraint(equalTo: instructionsButton.trailingAnchor, constant: 8),
            opacityControl.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            opacityControl.widthAnchor.constraint(equalToConstant: 144),
            opacityControl.heightAnchor.constraint(equalToConstant: 32),

            opacityLabel.leadingAnchor.constraint(equalTo: opacityControl.leadingAnchor, constant: 12),
            opacityLabel.centerYAnchor.constraint(equalTo: opacityControl.centerYAnchor),
            opacityLabel.widthAnchor.constraint(equalToConstant: 46),

            opacitySlider.leadingAnchor.constraint(equalTo: opacityLabel.trailingAnchor, constant: 8),
            opacitySlider.centerYAnchor.constraint(equalTo: opacityControl.centerYAnchor),
            opacitySlider.trailingAnchor.constraint(equalTo: opacityValueLabel.leadingAnchor, constant: -7),
            opacitySlider.heightAnchor.constraint(equalToConstant: 20),

            opacityValueLabel.trailingAnchor.constraint(equalTo: opacityControl.trailingAnchor, constant: -10),
            opacityValueLabel.centerYAnchor.constraint(equalTo: opacityControl.centerYAnchor),
            opacityValueLabel.widthAnchor.constraint(equalToConstant: 22),

            analyzeButton.trailingAnchor.constraint(equalTo: composerBar.trailingAnchor, constant: -14),
            analyzeButton.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            analyzeButton.widthAnchor.constraint(equalToConstant: 84),
            analyzeButton.heightAnchor.constraint(equalToConstant: 32),

            modelMenu.trailingAnchor.constraint(equalTo: analyzeButton.leadingAnchor, constant: -7),
            modelMenu.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            modelMenu.widthAnchor.constraint(greaterThanOrEqualToConstant: 118),
            modelMenu.widthAnchor.constraint(lessThanOrEqualToConstant: 144),
            modelMenu.heightAnchor.constraint(equalToConstant: 32),

            opacityControl.trailingAnchor.constraint(lessThanOrEqualTo: modelMenu.leadingAnchor, constant: -10),

            closeConfirmOverlay.topAnchor.constraint(equalTo: topAnchor),
            closeConfirmOverlay.leadingAnchor.constraint(equalTo: leadingAnchor),
            closeConfirmOverlay.trailingAnchor.constraint(equalTo: trailingAnchor),
            closeConfirmOverlay.bottomAnchor.constraint(equalTo: bottomAnchor),

            closeConfirmPanel.centerXAnchor.constraint(equalTo: workspace.centerXAnchor),
            closeConfirmPanel.centerYAnchor.constraint(equalTo: workspace.centerYAnchor),
            closeConfirmPanel.widthAnchor.constraint(equalToConstant: 360),

            closeConfirmTitle.topAnchor.constraint(equalTo: closeConfirmPanel.topAnchor, constant: 18),
            closeConfirmTitle.leadingAnchor.constraint(equalTo: closeConfirmPanel.leadingAnchor, constant: 18),
            closeConfirmTitle.trailingAnchor.constraint(equalTo: closeConfirmPanel.trailingAnchor, constant: -18),

            closeConfirmBody.topAnchor.constraint(equalTo: closeConfirmTitle.bottomAnchor, constant: 8),
            closeConfirmBody.leadingAnchor.constraint(equalTo: closeConfirmTitle.leadingAnchor),
            closeConfirmBody.trailingAnchor.constraint(equalTo: closeConfirmTitle.trailingAnchor),

            closeConfirmCancelButton.topAnchor.constraint(equalTo: closeConfirmBody.bottomAnchor, constant: 18),
            closeConfirmCancelButton.leadingAnchor.constraint(equalTo: closeConfirmPanel.leadingAnchor, constant: 18),
            closeConfirmCancelButton.bottomAnchor.constraint(equalTo: closeConfirmPanel.bottomAnchor, constant: -18),
            closeConfirmCancelButton.widthAnchor.constraint(equalToConstant: 150),
            closeConfirmCancelButton.heightAnchor.constraint(equalToConstant: 34),

            closeConfirmTurnOffButton.topAnchor.constraint(equalTo: closeConfirmCancelButton.topAnchor),
            closeConfirmTurnOffButton.leadingAnchor.constraint(equalTo: closeConfirmCancelButton.trailingAnchor, constant: 12),
            closeConfirmTurnOffButton.trailingAnchor.constraint(equalTo: closeConfirmPanel.trailingAnchor, constant: -18),
            closeConfirmTurnOffButton.heightAnchor.constraint(equalTo: closeConfirmCancelButton.heightAnchor),
        ])

        navButton.target = self
        navButton.action = #selector(toggleSessionsClicked)
        drawerCloseButton.target = self
        drawerCloseButton.action = #selector(closeSessionsClicked)
        canvasToggleButton.target = self
        canvasToggleButton.action = #selector(toggleCanvasClicked)
        newSessionButton.target = self
        newSessionButton.action = #selector(newSessionClicked)
        latestSessionButton.target = self
        latestSessionButton.action = #selector(continueSessionClicked)
        answerStyleSaveButton.target = self
        answerStyleSaveButton.action = #selector(saveAnswerStyleClicked)
        answerStyleBox.delegate = self
        hideButton.target = self
        hideButton.action = #selector(hideClicked)
        fullSizeButton.target = self
        fullSizeButton.action = #selector(fullSizeClicked)
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
        (composerSurface as? ComposerSurfaceView)?.composer = composer
        recordingButton.target = self
        recordingButton.action = #selector(recordingClicked)
        askButton.target = self
        askButton.action = #selector(askClicked)
        askButton.keyEquivalent = "\r"
        askButton.keyEquivalentModifierMask = [.command]
        analyzeButton.target = self
        analyzeButton.action = #selector(analyzeClicked)
        attachButton.target = self
        attachButton.action = #selector(attachClicked)
        instructionsButton.target = self
        instructionsButton.action = #selector(instructionsClicked)

        sessionDrawer.isHidden = true
        answerStyleOverlay.isHidden = true
        canvasPane.isHidden = true
        canvasToggleButton.isHidden = true
        canvasPane.onCollapse = { [weak self] in self?.setCanvasOpen(false) }
        canvasPane.onToggleFullWindow = { [weak self] in self?.toggleCanvasFullWindow() }
        canvasPane.setFullWindow(false)
        styleHeaderIconButton(navButton, symbol: "sidebar.left", fallback: "[]")
        styleHeaderIconButton(drawerCloseButton, symbol: "xmark", fallback: "x")
        styleHeaderIconButton(canvasToggleButton, symbol: "sidebar.right", fallback: "|")
        styleHeaderIconButton(newSessionButton, symbol: "square.and.pencil", fallback: "+")
        styleControlButton(latestSessionButton, symbol: "clock.arrow.circlepath", accent: false)
        styleControlButton(answerStyleSaveButton, symbol: "checkmark", accent: true)
        styleControlButton(recordingButton, symbol: "waveform", accent: false)
        styleControlButton(instructionsButton, symbol: "text.bubble", accent: false)
        styleIconButton(attachButton, symbol: "plus", fallback: "+")
        styleControlButton(analyzeButton, symbol: "sparkle.magnifyingglass", accent: false)
        styleControlButton(askButton, symbol: "arrow.up", accent: true)
        styleHeaderIconButton(hideButton, symbol: "eye.slash", fallback: "-")
        styleHeaderIconButton(closeButton, symbol: "xmark", fallback: "x")
        updateFullSizeButtonChrome()
        configureTooltips()
        setContextItems([])
        setTranscriptState("IDLE", active: false)
        applyOpacity(opacitySlider.doubleValue)
    }
    required init?(coder: NSCoder) { fatalError() }

    deinit {
        audioPulseTimer?.invalidate()
        knowledgeIndexTimer?.invalidate()
        toastHideWorkItem?.cancel()
    }

    override func layout() {
        super.layout()
        applyShellChrome()
        keepFixedChromeInBounds()
        if canvasOpen {
            updateCanvasWidth()
        }
        resizeTranscriptLabelToContent()
    }

    override func resetCursorRects() {
        super.resetCursorRects()
        guard closeConfirmOverlay.isHidden, answerStyleOverlay.isHidden else { return }
        addCursorRect(NSRect(x: bounds.width - resizeHitSize, y: 0, width: resizeHitSize, height: bounds.height), cursor: .resizeLeftRight)
        addCursorRect(NSRect(x: 0, y: 0, width: bounds.width, height: resizeHitSize), cursor: .resizeUpDown)
    }

    private func applyShellChrome() {
        wantsLayer = true
        layer?.cornerRadius = ExpandedPanelMetrics.cornerRadius
        layer?.masksToBounds = true
        if #available(macOS 10.15, *) {
            layer?.cornerCurve = .continuous
        }
    }

    private func materialAlpha(_ base: CGFloat, floor: CGFloat = 0.0) -> CGFloat {
        blueyMaterialAlpha(base, opacity: backgroundOpacity, floor: floor)
    }

    private func refreshBackgroundChrome() {
        layer?.backgroundColor = NSColor(
            red: 0.010,
            green: 0.012,
            blue: 0.016,
            alpha: backgroundOpacity
        ).cgColor
        headerBar.layer?.backgroundColor = NSColor(
            red: 0.018,
            green: 0.022,
            blue: 0.030,
            alpha: materialAlpha(0.99, floor: 0.96)
        ).cgColor
        modelMenu.layer?.backgroundColor = NSColor.white.withAlphaComponent(materialAlpha(0.075)).cgColor
        modelMenu.layer?.borderColor = NSColor.white.withAlphaComponent(materialAlpha(0.12)).cgColor
        balanceLabel.layer?.backgroundColor = NSColor.clear.cgColor
        balanceLabel.layer?.borderColor = NSColor.clear.cgColor
        toastView.layer?.backgroundColor = NSColor(
            red: 0.018,
            green: 0.023,
            blue: 0.030,
            alpha: materialAlpha(0.96)
        ).cgColor
        transcriptStrip.layer?.backgroundColor = NSColor.black.withAlphaComponent(materialAlpha(0.16)).cgColor
        sessionDrawer.layer?.backgroundColor = NSColor(
            red: 0.035,
            green: 0.040,
            blue: 0.050,
            alpha: materialAlpha(0.98)
        ).cgColor
        answerStylePanel.layer?.backgroundColor = BlueyTheme.panelDeep
            .withAlphaComponent(materialAlpha(0.97))
            .cgColor
        composerBar.layer?.backgroundColor = NSColor(
            red: 0.014,
            green: 0.016,
            blue: 0.022,
            alpha: materialAlpha(0.94)
        ).cgColor
        composerSurface.layer?.backgroundColor = NSColor.white.withAlphaComponent(materialAlpha(0.050)).cgColor
        composerSurface.layer?.borderColor = NSColor.white.withAlphaComponent(materialAlpha(0.12)).cgColor
        opacityControl.layer?.backgroundColor = NSColor.clear.cgColor
        opacityControl.layer?.borderColor = NSColor.clear.cgColor
        closeConfirmPanel.layer?.backgroundColor = BlueyTheme.panelDeep
            .withAlphaComponent(materialAlpha(0.97))
            .cgColor
        feed.applyBackgroundOpacity(backgroundOpacity)
        canvasPane.applyBackgroundOpacity(backgroundOpacity)
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53, dismissActiveOverlay() {
            return
        }
        super.keyDown(with: event)
    }

    override func scrollWheel(with event: NSEvent) {
        let localPoint = convert(event.locationInWindow, from: nil)
        if rectForView(feed).contains(localPoint) {
            feed.forwardScrollWheel(event)
            return
        }
        if rectForView(canvasPane).contains(localPoint) {
            canvasPane.scrollWheel(with: event)
            return
        }
        if !sessionDrawer.isHidden, rectForView(sessionScroll).contains(localPoint) {
            sessionScroll.scrollWheel(with: event)
            return
        }
        super.scrollWheel(with: event)
    }

    override func mouseDown(with event: NSEvent) {
        let localPoint = convert(event.locationInWindow, from: nil)
        let edges = resizeEdges(at: localPoint)
        if !edges.isEmpty, let window {
            activeResizeEdges = edges
            resizeStartMouse = NSEvent.mouseLocation
            resizeStartFrame = window.frame
            return
        }
        super.mouseDown(with: event)
    }

    override func mouseDragged(with event: NSEvent) {
        guard !activeResizeEdges.isEmpty, let window else {
            super.mouseDragged(with: event)
            return
        }
        let currentMouse = NSEvent.mouseLocation
        let deltaX = currentMouse.x - resizeStartMouse.x
        let deltaY = currentMouse.y - resizeStartMouse.y
        var frame = resizeStartFrame

        if activeResizeEdges.contains(.left) {
            frame.origin.x += deltaX
            frame.size.width -= deltaX
        }
        if activeResizeEdges.contains(.right) {
            frame.size.width += deltaX
        }
        if activeResizeEdges.contains(.bottom) {
            frame.origin.y += deltaY
            frame.size.height -= deltaY
        }
        if activeResizeEdges.contains(.top) {
            frame.size.height += deltaY
        }

        frame = clampedResizeFrame(frame, from: resizeStartFrame, edges: activeResizeEdges, window: window)
        let verticalResize = activeResizeEdges.contains(.top) || activeResizeEdges.contains(.bottom)
        if let overlayWindow = window as? OverlayWindow, !verticalResize {
            let topLeft = NSPoint(x: frame.minX, y: resizeStartFrame.maxY)
            overlayWindow.lockedFrameHeight = resizeStartFrame.height
            window.setFrame(frame, display: true)
            window.setFrameTopLeftPoint(topLeft)
            overlayWindow.lockedFrameHeight = nil
        } else {
            window.setFrame(frame, display: true)
        }
    }

    override func mouseUp(with event: NSEvent) {
        if !activeResizeEdges.isEmpty {
            (window as? OverlayWindow)?.lockedFrameHeight = nil
            activeResizeEdges = []
            return
        }
        super.mouseUp(with: event)
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        guard control === answerStyleBox else { return false }
        if commandSelector == #selector(NSResponder.cancelOperation(_:)) {
            dismissAnswerStyleEditor(animated: true)
            return true
        }
        return false
    }

    func isInteractiveAtScreenPoint(_ screenPoint: NSPoint) -> Bool {
        guard let window else { return false }
        let windowPoint = window.convertPoint(fromScreen: screenPoint)
        let localPoint = convert(windowPoint, from: nil)
        guard bounds.contains(localPoint) else { return false }

        if !closeConfirmOverlay.isHidden {
            return true
        }
        if !answerStyleOverlay.isHidden {
            return true
        }
        if !sessionDrawer.isHidden {
            let drawerPoint = sessionDrawer.convert(localPoint, from: self)
            return sessionDrawer.bounds.contains(drawerPoint)
        }
        if !resizeEdges(at: localPoint).isEmpty {
            return true
        }
        if hitsExplicitInteractiveChrome(at: localPoint) {
            return true
        }
        return hasInteractiveView(at: localPoint)
            || feed.hasCopyControl(atScreenPoint: screenPoint)
    }

    private func hitsExplicitInteractiveChrome(at localPoint: NSPoint) -> Bool {
        let controls: [NSView] = [
            headerBar,
            navButton,
            newSessionButton,
            canvasToggleButton,
            balanceLabel,
            fullSizeButton,
            hideButton,
            closeButton,
            routeBadge,
            knowledgeBadge,
            feed,
            canvasPane,
            transcriptStrip,
            attachmentStrip,
            composerSurface,
            composer,
            recordingButton,
            askButton,
            attachButton,
            instructionsButton,
            opacityControl,
            opacitySlider,
            modelMenu,
            analyzeButton,
        ]
        return controls.contains { view in
            guard !view.isHidden, view.alphaValue > 0.01 else { return false }
            let rect = view.convert(view.bounds, to: self).insetBy(dx: -8, dy: -8)
            return rect.contains(localPoint)
        }
    }

    private func rectForView(_ view: NSView) -> NSRect {
        view.convert(view.bounds, to: self)
    }

    private func hasInteractiveView(at localPoint: NSPoint) -> Bool {
        var hit: NSView? = hitTest(localPoint)
        while let view = hit {
            if view === self || view === workspace || view === headerBar || view === composerBar {
                hit = view.superview
                continue
            }
            if view is NSButton
                || view is NSPopUpButton
                || view is NSSlider
                || view is NSScroller
                || view is NSTextView
            {
                return true
            }
            if let textField = view as? NSTextField, textField.isEditable {
                return true
            }
            hit = view.superview
        }
        return false
    }

    private func resizeEdges(at point: NSPoint) -> ResizeEdges {
        guard bounds.contains(point),
              closeConfirmOverlay.isHidden,
              answerStyleOverlay.isHidden,
              canStartResize(at: point)
        else {
            return []
        }
        var edges: ResizeEdges = []
        if point.x >= bounds.width - resizeHitSize {
            edges.insert(.right)
        }
        if point.y <= resizeHitSize {
            edges.insert(.bottom)
        }
        return edges
    }

    private func canStartResize(at point: NSPoint) -> Bool {
        if headerBar.frame.insetBy(dx: -4, dy: -4).contains(point) {
            return false
        }
        if composerBar.frame.insetBy(dx: -4, dy: -4).contains(point) {
            return false
        }
        if transcriptStrip.frame.insetBy(dx: -4, dy: -4).contains(point) {
            return false
        }
        if !sessionDrawer.isHidden {
            let drawerPoint = sessionDrawer.convert(point, from: self)
            if sessionDrawer.bounds.contains(drawerPoint) {
                return false
            }
        }
        if !canvasPane.isHidden {
            let canvasPoint = canvasPane.convert(point, from: self)
            if canvasPane.bounds.contains(canvasPoint) {
                return false
            }
        }
        return point.x >= bounds.width - resizeHitSize || point.y <= resizeHitSize
    }

    private func clampedResizeFrame(
        _ proposed: NSRect,
        from start: NSRect,
        edges: ResizeEdges,
        window: NSWindow
    ) -> NSRect {
        var frame = proposed
        let minWidth = max(window.minSize.width, 360)
        let minHeight = max(window.minSize.height, ExpandedPanelMetrics.minHeight)
        let maxWidth = window.maxSize.width > 0 ? window.maxSize.width : CGFloat.greatestFiniteMagnitude
        let maxHeight = window.maxSize.height > 0 ? window.maxSize.height : CGFloat.greatestFiniteMagnitude

        if frame.width < minWidth {
            if edges.contains(.left) {
                frame.origin.x = start.maxX - minWidth
            }
            frame.size.width = minWidth
        } else if frame.width > maxWidth {
            if edges.contains(.left) {
                frame.origin.x = start.maxX - maxWidth
            }
            frame.size.width = maxWidth
        }

        if frame.height < minHeight {
            if edges.contains(.bottom) {
                frame.origin.y = start.maxY - minHeight
            }
            frame.size.height = minHeight
        } else if frame.height > maxHeight {
            if edges.contains(.bottom) {
                frame.origin.y = start.maxY - maxHeight
            }
            frame.size.height = maxHeight
        }

        return frame
    }

    /// The expanded overlay is a bounded, resizable tool surface. Header,
    /// transcript, attachments, and composer are chrome; only the workspace
    /// may compress/scroll as content grows.
    private func configureFixedChromeLayoutPriorities() {
        for chrome in [headerBar, transcriptStrip, attachmentStrip, composerBar] {
            chrome.setContentHuggingPriority(.required, for: .vertical)
            chrome.setContentCompressionResistancePriority(.required, for: .vertical)
        }
        workspace.setContentHuggingPriority(.defaultLow, for: .vertical)
        workspace.setContentCompressionResistancePriority(.defaultLow, for: .vertical)
        feed.setContentHuggingPriority(.defaultLow, for: .vertical)
        feed.setContentCompressionResistancePriority(.defaultLow, for: .vertical)
        canvasPane.setContentHuggingPriority(.defaultLow, for: .vertical)
        canvasPane.setContentCompressionResistancePriority(.defaultLow, for: .vertical)
    }

    private func keepFixedChromeInBounds() {
        // Defensive guard for AppKit/autolayout edge cases. The window can
        // report content bounds taller than the visible frame in some launch
        // paths, so pin fixed chrome to the real visible height and keep its
        // stacking order above dense transcript/card content.
        guard bounds.height >= ExpandedPanelMetrics.minHeight else { return }
        headerBar.isHidden = false
        headerBar.layer?.zPosition = 1_000
        headerStack.layer?.zPosition = 1_001
        transcriptStrip.layer?.zPosition = 900
        attachmentStrip.layer?.zPosition = 900
        composerBar.layer?.zPosition = 1_000
        let visibleHeight = min(bounds.height, window?.frame.height ?? bounds.height)
        headerBar.frame = NSRect(
            x: 10,
            y: max(10, visibleHeight - 52),
            width: max(0, bounds.width - 20),
            height: 42)
        headerStack.frame = headerBar.bounds.insetBy(dx: 9, dy: 4)
        if composerBar.frame.minY < 0 || composerBar.frame.maxY > bounds.height {
            let height = composerBarHeightConstraint?.constant ?? 94
            composerBar.frame = NSRect(
                x: 10,
                y: 10,
                width: max(0, bounds.width - 20),
                height: height)
        }
        let workspaceBottom = transcriptStrip.frame.maxY + 8
        let workspaceTop = headerBar.frame.minY - 12
        if workspaceTop > workspaceBottom {
            workspace.frame = NSRect(
                x: 10,
                y: workspaceBottom,
                width: max(0, bounds.width - 20),
                height: workspaceTop - workspaceBottom)
        }
    }

    private func configureHeader() {
        headerBar.wantsLayer = true
        headerBar.layer?.backgroundColor = NSColor(red: 0.018, green: 0.022, blue: 0.030, alpha: 0.99).cgColor
        headerBar.layer?.cornerRadius = 21
        headerBar.layer?.borderWidth = 1
        headerBar.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.18).cgColor
        headerBar.layer?.shadowColor = NSColor.black.cgColor
        headerBar.layer?.shadowOpacity = 0.18
        headerBar.layer?.shadowRadius = 14
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

        styleHeaderBadge(routeBadge, textColor: BlueyTheme.green)
        routeBadge.toolTip = "Auto Router classification and selected lane"

        styleHeaderBadge(knowledgeBadge, textColor: BlueyTheme.text)
        knowledgeBadge.toolTip = "Attached document status"

        balanceLabel.font = NSFont.monospacedSystemFont(ofSize: 10.5, weight: .bold)
        balanceLabel.textColor = BlueyTheme.text
        balanceLabel.alignment = .center
        balanceLabel.lineBreakMode = .byTruncatingMiddle
        balanceLabel.maximumNumberOfLines = 1
        useCenteredSingleLineCell(balanceLabel)
        balanceLabel.setContentHuggingPriority(.defaultLow, for: .horizontal)
        balanceLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        balanceLabel.wantsLayer = true
        balanceLabel.layer?.backgroundColor = NSColor.clear.cgColor
        balanceLabel.layer?.cornerRadius = 0
        balanceLabel.layer?.borderWidth = 0
        balanceLabel.layer?.borderColor = NSColor.clear.cgColor
    }

    private func configureSystemToast() {
        toastView.wantsLayer = true
        toastView.layer?.backgroundColor = NSColor(red: 0.018, green: 0.023, blue: 0.030, alpha: 0.96).cgColor
        toastView.layer?.cornerRadius = 18
        toastView.layer?.borderWidth = 1
        toastView.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.28).cgColor
        toastView.layer?.shadowColor = NSColor.black.cgColor
        toastView.layer?.shadowOpacity = 0.24
        toastView.layer?.shadowRadius = 18
        toastView.layer?.shadowOffset = NSSize(width: 0, height: -8)
        toastView.alphaValue = 0
        toastView.isHidden = true

        toastTitleLabel.font = NSFont.systemFont(ofSize: 13, weight: .bold)
        toastTitleLabel.textColor = BlueyTheme.text
        toastTitleLabel.alignment = .center
        toastTitleLabel.maximumNumberOfLines = 1
        toastTitleLabel.lineBreakMode = .byTruncatingTail

        toastBodyLabel.font = NSFont.systemFont(ofSize: 11.5, weight: .medium)
        toastBodyLabel.textColor = BlueyTheme.textDim
        toastBodyLabel.alignment = .center
        toastBodyLabel.maximumNumberOfLines = 3
        toastBodyLabel.lineBreakMode = .byWordWrapping
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
        sessionDrawer.layer?.zPosition = 1_500

        answerStyleOverlay.isHidden = true
        answerStyleOverlay.wantsLayer = true
        answerStyleOverlay.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.44).cgColor
        answerStyleOverlay.layer?.zPosition = 2_000

        answerStylePanel.wantsLayer = true
        answerStylePanel.layer?.backgroundColor = BlueyTheme.panelDeep.cgColor
        answerStylePanel.layer?.cornerRadius = 18
        answerStylePanel.layer?.borderWidth = 1
        answerStylePanel.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.30).cgColor
        answerStylePanel.layer?.shadowColor = NSColor.black.cgColor
        answerStylePanel.layer?.shadowOpacity = 0.32
        answerStylePanel.layer?.shadowRadius = 20
        answerStylePanel.layer?.shadowOffset = .zero

        drawerTitleLabel.font = NSFont.systemFont(ofSize: 14, weight: .bold)
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
        answerStyleLabel.alignment = .center
        answerStyleLabel.stringValue = "How Bluey should answer"
        answerStyleBox.placeholderString = "Natural, concise, interview-ready..."
        answerStyleBox.font = NSFont.systemFont(ofSize: 11.5, weight: .medium)
        answerStyleBox.isBezeled = false
        answerStyleBox.drawsBackground = true
        answerStyleBox.focusRingType = .none
        answerStyleBox.backgroundColor = NSColor.white.withAlphaComponent(0.98)
        answerStyleBox.textColor = NSColor.black.withAlphaComponent(0.96)
        answerStyleBox.alignment = .center
        answerStyleBox.placeholderAttributedString = NSAttributedString(
            string: "Natural, concise, interview-ready...",
            attributes: [.foregroundColor: NSColor.black.withAlphaComponent(0.60)])
        answerStyleBox.wantsLayer = true
        answerStyleBox.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.98).cgColor
        answerStyleBox.layer?.cornerRadius = 10
        answerStyleBox.layer?.borderWidth = 1
        answerStyleBox.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.24).cgColor
        answerStyleBox.layer?.masksToBounds = true
    }

    private func configureComposer() {
        composerBar.wantsLayer = true
        composerBar.layer?.backgroundColor = NSColor(red: 0.014, green: 0.016, blue: 0.022, alpha: 0.94).cgColor
        composerBar.layer?.cornerRadius = 24
        composerBar.layer?.borderWidth = 1
        composerBar.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.22).cgColor
        composerBar.layer?.shadowColor = NSColor.black.cgColor
        composerBar.layer?.shadowOpacity = 0.22
        composerBar.layer?.shadowRadius = 18
        composerBar.layer?.shadowOffset = .zero

        composerSurface.wantsLayer = true
        composerSurface.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.050).cgColor
        composerSurface.layer?.cornerRadius = 18
        composerSurface.layer?.borderWidth = 1
        composerSurface.layer?.borderColor = NSColor.white.withAlphaComponent(0.12).cgColor

        opacityControl.wantsLayer = true
        opacityControl.layer?.backgroundColor = NSColor.clear.cgColor
        opacityControl.layer?.cornerRadius = 16
        opacityControl.layer?.borderWidth = 0
        opacityControl.layer?.borderColor = NSColor.clear.cgColor
        opacityControl.toolTip = "Overlay opacity"
        opacityLabel.stringValue = "Opacity"
        opacityLabel.font = NSFont.systemFont(ofSize: 10.2, weight: .semibold)
        opacityLabel.textColor = BlueyTheme.textDim
        opacityLabel.alignment = .left
        opacityValueLabel.font = NSFont.monospacedDigitSystemFont(ofSize: 10.2, weight: .semibold)
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
        closeConfirmOverlay.layer?.zPosition = 2_100

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
        closeConfirmTitle.alignment = .center
        closeConfirmBody.font = NSFont.systemFont(ofSize: 12.5, weight: .medium)
        closeConfirmBody.textColor = BlueyTheme.textDim
        closeConfirmBody.alignment = .center
        closeConfirmBody.maximumNumberOfLines = 3

        styleControlButton(closeConfirmCancelButton, symbol: "xmark", accent: false)
        styleControlButton(closeConfirmTurnOffButton, symbol: "power", accent: true)
        closeConfirmCancelButton.toolTip = "Keep Bluey running"
        closeConfirmTurnOffButton.toolTip = "Turn Bluey off. Run bluey on to start again."
    }

    private func configureTooltips() {
        navButton.toolTip = "Show recordings"
        drawerCloseButton.toolTip = "Close recordings"
        newSessionButton.toolTip = "Start a new recording"
        modelMenu.toolTip = "Choose routing lane"
        canvasToggleButton.toolTip = "Open or collapse the canvas"
        balanceLabel.toolTip = "Remaining Bluey balance"
        fullSizeButton.toolTip = windowFullSize ? "Restore Bluey size" : "Make Bluey full size"
        hideButton.toolTip = "Hide to pill"
        closeButton.toolTip = "Turn Bluey off. Run bluey on to start again."
        recordingButton.toolTip = "Start or stop listening"
        instructionsButton.toolTip = "How Bluey should answer"
        attachButton.toolTip = "Attach files"
        analyzeButton.toolTip = "Analyse screen"
        askButton.toolTip = "Answer with Cmd+Enter. Enter also answers; Shift+Enter adds a new line."
        latestSessionButton.toolTip = "Continue the latest recording"
        answerStyleSaveButton.toolTip = "Save answer style for this session"
    }

    private func styleControlButton(_ button: NSButton, symbol: String, accent: Bool) {
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 16
        button.layer?.backgroundColor = accent
            ? NSColor(red: 0.07, green: 0.19, blue: 0.24, alpha: 0.98).cgColor
            : NSColor.white.withAlphaComponent(0.070).cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = (accent ? BlueyTheme.cyan.withAlphaComponent(0.55) : NSColor.white.withAlphaComponent(0.12)).cgColor
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
        button.imageHugsTitle = true
        button.imageScaling = .scaleProportionallyDown
        button.alignment = .center
    }

    private func styleHeaderBadge(_ label: NSTextField, textColor: NSColor) {
        label.font = NSFont.systemFont(ofSize: 10.8, weight: .bold)
        label.textColor = textColor
        label.alignment = .center
        label.lineBreakMode = .byTruncatingMiddle
        label.maximumNumberOfLines = 1
        if let cell = label.cell as? NSTextFieldCell {
            cell.alignment = .center
            cell.lineBreakMode = .byTruncatingMiddle
            cell.usesSingleLineMode = true
            cell.wraps = false
        }
        useCenteredSingleLineCell(label)
        label.setContentHuggingPriority(.defaultLow, for: .horizontal)
        label.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        label.wantsLayer = true
        label.layer?.backgroundColor = NSColor.clear.cgColor
        label.layer?.cornerRadius = 0
        label.layer?.borderWidth = 0
        label.layer?.borderColor = NSColor.clear.cgColor
    }

    private func styleHeaderIconButton(_ button: NSButton, symbol: String, fallback: String) {
        button.title = fallback
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 0
        button.layer?.backgroundColor = NSColor.clear.cgColor
        button.layer?.borderWidth = 0
        button.layer?.borderColor = NSColor.clear.cgColor
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
        button.imageHugsTitle = true
        button.alignment = .center
    }

    private func styleIconButton(_ button: NSButton, symbol: String, fallback: String, accent: Bool = false) {
        button.title = fallback
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 0
        button.layer?.backgroundColor = NSColor.clear.cgColor
        button.layer?.borderWidth = 0
        button.layer?.borderColor = NSColor.clear.cgColor
        button.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        button.contentTintColor = accent ? BlueyTheme.text : BlueyTheme.cyan
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
        button.imageHugsTitle = true
        button.alignment = .center
    }

    @objc private func hideClicked() { onClose?() }

    @objc private func closeClicked() {
        showTurnOffConfirmation()
    }

    @objc private func fullSizeClicked() {
        toggleWindowFullSize()
    }

    func showTurnOffConfirmation() {
        pendingDeleteSessionId = nil
        closeConfirmTitle.stringValue = "Turn Bluey off?"
        closeConfirmBody.stringValue = "This closes Bluey completely. To start again, run: bluey on"
        closeConfirmTurnOffButton.title = "Turn Off"
        closeConfirmTurnOffButton.target = self
        closeConfirmTurnOffButton.action = #selector(confirmTurnOffClicked)
        closeConfirmTurnOffButton.toolTip = "Turn Bluey off. Run bluey on to start again."
        styleControlButton(closeConfirmTurnOffButton, symbol: "power", accent: true)
        presentConfirmationOverlay()
    }

    private func presentConfirmationOverlay() {
        dismissAnswerStyleEditor(animated: false)
        closeConfirmOverlay.isHidden = false
        closeConfirmOverlay.alphaValue = 0
        updateBackgroundControlsEnabledForModalState()
        window?.makeFirstResponder(closeConfirmOverlay)
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.12
            closeConfirmOverlay.animator().alphaValue = 1
        }
    }

    @objc private func cancelCloseConfirmClicked() {
        pendingDeleteSessionId = nil
        dismissCloseConfirm(animated: true)
    }

    private func dismissCloseConfirm(animated: Bool) {
        guard !closeConfirmOverlay.isHidden else { return }
        guard animated else {
            closeConfirmOverlay.isHidden = true
            closeConfirmOverlay.alphaValue = 1
            updateBackgroundControlsEnabledForModalState()
            return
        }
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.10
            closeConfirmOverlay.animator().alphaValue = 0
        }, completionHandler: { [weak self] in
            guard let self else { return }
            self.closeConfirmOverlay.isHidden = true
            self.closeConfirmOverlay.alphaValue = 1
            self.updateBackgroundControlsEnabledForModalState()
        })
    }

    @objc private func confirmTurnOffClicked() {
        emitSimple("close_requested")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) {
            NSApp.terminate(nil)
        }
    }

    @objc private func confirmDeleteSessionClicked() {
        guard let id = pendingDeleteSessionId else { return }
        if let index = sessionItems.firstIndex(where: { $0.id == id }) {
            sessionItems.remove(at: index)
            setSessions(sessionItems)
        }
        editingSessionId = nil
        pendingDeleteSessionId = nil
        dismissCloseConfirm(animated: true)
        statusLabel.stringValue = "Session deleted"
        emitSessionDelete(id: id)
    }

    @objc private func opacityChanged() {
        applyOpacity(opacitySlider.doubleValue)
    }

    func applyOpacity(_ opacity: Double) {
        let value = min(max(opacity, 0.50), 1.0)
        backgroundOpacity = CGFloat(value)
        if abs(opacitySlider.doubleValue - value) > 0.001 {
            opacitySlider.doubleValue = value
        }
        opacityValueLabel.stringValue = "\(Int((value * 100.0).rounded()))"
        refreshBackgroundChrome()
        onOpacityChanged?(value)
    }

    private func setComposerTextHeight(_ rawHeight: CGFloat) {
        let textHeight = min(max(rawHeight, 42), 88)
        guard abs((composerTextHeightConstraint?.constant ?? 0) - textHeight) > 0.5 else { return }
        composerTextHeightConstraint?.constant = textHeight
        composerBarHeightConstraint?.constant = textHeight + 52
        needsLayout = true
        layoutSubtreeIfNeeded()
    }

    @objc private func toggleSessionsClicked() {
        sessionDrawer.isHidden.toggle()
        statusLabel.stringValue = sessionDrawer.isHidden ? statusLabel.stringValue : "Sessions"
    }

    @objc private func closeSessionsClicked() {
        sessionDrawer.isHidden = true
    }

    @objc private func toggleCanvasClicked() {
        guard latestCanvas != nil else { return }
        if canvasOpen {
            setCanvasOpen(false)
        } else {
            setCanvasOpen(true)
        }
    }

    private func toggleCanvasFullWindow() {
        guard canvasOpen, window != nil else { return }
        if canvasFullWindow {
            restoreCanvasWindow()
        } else {
            expandCanvasWindow()
        }
    }

    @objc private func newSessionClicked() {
        let preserveFrame = !canvasOpen
        let previousFrame = preserveFrame ? window?.frame : nil
        resetSessionSurface()
        composer.clearText()
        statusLabel.stringValue = "New recording"
        sessionDrawer.isHidden = true
        if let previousFrame {
            layoutSubtreeIfNeeded()
            window?.setFrame(previousFrame, display: true)
        }
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
        dismissAnswerStyleEditor(animated: true)
    }

    @discardableResult
    private func dismissActiveOverlay() -> Bool {
        if !closeConfirmOverlay.isHidden {
            dismissCloseConfirm(animated: true)
            return true
        }
        if !answerStyleOverlay.isHidden {
            dismissAnswerStyleEditor(animated: true)
            return true
        }
        return false
    }

    private func dismissAnswerStyleEditor(animated: Bool) {
        guard !answerStyleOverlay.isHidden else { return }
        guard animated else {
            answerStyleOverlay.isHidden = true
            answerStyleOverlay.alphaValue = 1
            updateBackgroundControlsEnabledForModalState()
            return
        }
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.10
            answerStyleOverlay.animator().alphaValue = 0
        }, completionHandler: { [weak self] in
            guard let self else { return }
            self.answerStyleOverlay.isHidden = true
            self.answerStyleOverlay.alphaValue = 1
            self.updateBackgroundControlsEnabledForModalState()
        })
    }

    @objc private func recordingClicked() {
        if recordingActive {
            emitSimple("recording_stop_requested")
            recordingActive = false
            onListeningStateChanged?(.paused)
            recordingButton.title = "Listen"
            statusLabel.stringValue = "Paused"
            composer.placeholder = "Ask anything..."
            styleControlButton(recordingButton, symbol: "waveform", accent: false)
            setTranscriptState("PAUSED", active: false)
        } else {
            emitSimple("recording_start_requested")
            recordingActive = false
            onListeningStateChanged?(.connecting)
            recordingButton.title = "Starting"
            statusLabel.stringValue = "Starting audio"
            composer.placeholder = "Starting audio..."
            styleControlButton(recordingButton, symbol: "waveform", accent: false)
            setTranscriptState("STARTING", active: true)
            seedTranscriptPreviewIfEmpty("Starting mic + system audio...")
        }
    }

    @objc private func askClicked() {
        let raw = composer.string.trimmingCharacters(in: .whitespacesAndNewlines)
        let q: String
        if raw.isEmpty {
            guard let transcriptQuestion = transcriptQuestionForAnswer() else {
                showSystemToast(for: RenderedCard(
                    id: "empty-answer-\(UUID().uuidString)",
                    kind: "system",
                    title: "Nothing to answer yet",
                    body: "Type a question or start Listen so Bluey has transcript context.",
                    done: true,
                    costLabel: nil,
                    artifact: nil))
                window?.makeFirstResponder(composer)
                return
            }
            q = transcriptQuestion
        } else {
            q = raw
        }
        composer.clearText()
        let route = selectedRoute()
        updateRouteBadge(for: q, selectedRoute: route)
        emitAsk(question: q, provider: route.provider, model: route.model, mode: route.mode)
        window?.makeFirstResponder(composer)
    }

    @objc private func analyzeClicked() {
        routeBadge.stringValue = "Vision · deep"
        statusLabel.stringValue = "Reading screen"
        emitSimple("analyze_screen_requested")
    }

    @objc private func attachClicked() {
        setKnowledgeBadge("Docs loading", accent: BlueyTheme.warning)
        showKnowledgePlaceholder("Indexing selected files...")
        emitSimple("attach_requested")
    }

    @objc private func removeAttachmentClicked(_ sender: RemoveAttachmentButton) {
        let id = sender.contextId.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !id.isEmpty else { return }
        setKnowledgeBadge("Docs updating", accent: BlueyTheme.warning)
        emitRemoveContext(id: id)
    }

    @objc private func instructionsClicked() {
        openAnswerStyleEditor()
    }

    func openAnswerStyleEditor() {
        dismissCloseConfirm(animated: false)
        answerStyleOverlay.isHidden = false
        answerStyleOverlay.alphaValue = 0
        updateBackgroundControlsEnabledForModalState()
        window?.makeFirstResponder(answerStyleBox)
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.12
            answerStyleOverlay.animator().alphaValue = 1
        }
    }

    func focusComposerForQuestion() {
        dismissAnswerStyleEditor(animated: false)
        dismissCloseConfirm(animated: false)
        composer.placeholder = recordingActive
            ? "Ask while Bluey listens..."
            : "Ask anything..."
        window?.makeFirstResponder(composer)
    }

    private func updateBackgroundControlsEnabledForModalState() {
        let hasModal = !answerStyleOverlay.isHidden || !closeConfirmOverlay.isHidden
        setBackgroundControlsEnabled(!hasModal)
    }

    private func setBackgroundControlsEnabled(_ isEnabled: Bool) {
        let controls: [NSControl] = [
            navButton,
            newSessionButton,
            latestSessionButton,
            canvasToggleButton,
            modelMenu,
            fullSizeButton,
            recordingButton,
            askButton,
            analyzeButton,
            attachButton,
            instructionsButton,
            opacitySlider,
            hideButton,
            closeButton,
        ]
        for control in controls {
            control.isEnabled = isEnabled
            control.alphaValue = isEnabled ? 1.0 : 0.45
        }
        composer.isEditable = isEnabled
        composer.alphaValue = isEnabled ? 1.0 : 0.55
    }

    func setBalanceLabel(_ label: String) {
        let clean = label.trimmingCharacters(in: .whitespacesAndNewlines)
        balanceLabel.stringValue = clean.isEmpty ? "Balance --" : clean
    }

    func showSignedOutLogin(url: URL?) {
        statusLabel.stringValue = "Login needed"
        routeBadge.stringValue = "Sign in"
        routeBadge.textColor = BlueyTheme.warning
        routeBadge.layer?.borderColor = NSColor.clear.cgColor
        routeBadge.layer?.backgroundColor = NSColor.clear.cgColor
        balanceLabel.stringValue = "Login"
        setKnowledgeBadge("Docs locked", accent: BlueyTheme.textDim)
        composer.placeholder = url == nil ? "Sign in to use managed answers..." : "Sign in, then ask anything..."
        statusLabel.toolTip = "Cloud answers, balance, sync, and documents unlock after login"
    }

    func showSignedInReady() {
        statusLabel.stringValue = recordingActive ? "Listening" : "Ready"
        statusLabel.toolTip = nil
        routeBadge.stringValue = "● Ready"
        routeBadge.textColor = BlueyTheme.green
        routeBadge.layer?.borderColor = NSColor.clear.cgColor
        routeBadge.layer?.backgroundColor = NSColor.clear.cgColor
        if balanceLabel.stringValue == "Login" {
            balanceLabel.stringValue = "Balance --"
        }
        if knowledgeBadge.stringValue == "Docs locked" {
            setKnowledgeBadge("Docs empty", accent: BlueyTheme.textDim)
        }
        composer.placeholder = recordingActive
            ? "Listening... type a follow-up anytime"
            : "Ask anything..."
    }

    private func setKnowledgeBadge(_ text: String, accent: NSColor) {
        if text.localizedCaseInsensitiveContains("loading")
            || text.localizedCaseInsensitiveContains("updating")
            || text.localizedCaseInsensitiveContains("indexing")
        {
            startKnowledgeIndexing()
            return
        }
        stopKnowledgeIndexing()
        knowledgeBadge.stringValue = text
        knowledgeBadge.textColor = accent
        knowledgeBadge.layer?.borderColor = NSColor.clear.cgColor
        knowledgeBadge.layer?.backgroundColor = NSColor.clear.cgColor
    }

    private func startKnowledgeIndexing() {
        knowledgeIndexTimer?.invalidate()
        knowledgeIndexFrame = 0
        applyKnowledgeIndexFrame()
        knowledgeIndexTimer = Timer.scheduledTimer(withTimeInterval: 0.42, repeats: true) { [weak self] _ in
            guard let self else { return }
            self.knowledgeIndexFrame = (self.knowledgeIndexFrame + 1) % self.knowledgeIndexFrames.count
            self.applyKnowledgeIndexFrame()
        }
    }

    private func applyKnowledgeIndexFrame() {
        let frame = knowledgeIndexFrames[knowledgeIndexFrame % knowledgeIndexFrames.count]
        knowledgeBadge.stringValue = frame
        knowledgeBadge.textColor = BlueyTheme.green
        knowledgeBadge.layer?.borderColor = NSColor.clear.cgColor
        knowledgeBadge.layer?.backgroundColor = NSColor.clear.cgColor
        knowledgeBadge.toolTip = "Indexing attached documents"
    }

    private func stopKnowledgeIndexing() {
        knowledgeIndexTimer?.invalidate()
        knowledgeIndexTimer = nil
        knowledgeBadge.toolTip = "Attached document status"
    }

    private func showKnowledgePlaceholder(_ text: String) {
        for view in attachmentStack.arrangedSubviews {
            attachmentStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        attachmentStrip.isHidden = false
        attachmentStripHeightConstraint?.constant = 34

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
        layoutSubtreeIfNeeded()
    }

    private func setTranscriptState(_ text: String, active: Bool) {
        transcriptStateLabel.stringValue = text
        transcriptStateLabel.textColor = active ? BlueyTheme.green : BlueyTheme.textDim
        transcriptActivityDot.layer?.backgroundColor = (active ? BlueyTheme.green : BlueyTheme.textDim.withAlphaComponent(0.55)).cgColor
        transcriptActivityDot.layer?.shadowOpacity = active ? 0.45 : 0
        transcriptStrip.layer?.borderColor = (active ? BlueyTheme.green.withAlphaComponent(0.26) : BlueyTheme.hairline).cgColor
        setAudioPulseActive(active)
    }

    private func setAudioPulseActive(_ active: Bool) {
        if active {
            if audioPulseTimer == nil {
                audioPulseFrame = 0
                let timer = Timer(timeInterval: 0.34, repeats: true) { [weak self] _ in
                    guard let self else { return }
                    self.audioPulseFrame = (self.audioPulseFrame + 1) % 4
                    self.applyAudioPulseFrame(active: true)
                }
                RunLoop.main.add(timer, forMode: .common)
                audioPulseTimer = timer
            }
            applyAudioPulseFrame(active: true)
        } else {
            audioPulseTimer?.invalidate()
            audioPulseTimer = nil
            audioPulseFrame = 0
            applyAudioPulseFrame(active: false)
        }
    }

    private func applyAudioPulseFrame(active: Bool) {
        guard active else {
            transcriptStrip.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.16).cgColor
            transcriptActivityDot.layer?.shadowOpacity = 0
            transcriptActivityDot.layer?.shadowRadius = 7
            recordingButton.layer?.shadowOpacity = 0
            recordingButton.layer?.shadowRadius = 0
            return
        }

        let phases: [CGFloat] = [0.00, 0.32, 0.68, 0.32]
        let phase = phases[audioPulseFrame % phases.count]
        let fill = 0.13 + (0.12 * phase)
        let border = 0.36 + (0.28 * phase)
        transcriptStrip.layer?.backgroundColor = BlueyTheme.green.withAlphaComponent(0.030 + (0.030 * phase)).cgColor
        transcriptStrip.layer?.borderColor = BlueyTheme.green.withAlphaComponent(border).cgColor
        transcriptActivityDot.layer?.backgroundColor = BlueyTheme.green.cgColor
        transcriptActivityDot.layer?.shadowColor = BlueyTheme.green.cgColor
        transcriptActivityDot.layer?.shadowOpacity = Float(0.50 + (0.32 * phase))
        transcriptActivityDot.layer?.shadowRadius = 7 + (5 * phase)
        recordingButton.layer?.backgroundColor = BlueyTheme.green.withAlphaComponent(fill).cgColor
        recordingButton.layer?.borderColor = BlueyTheme.green.withAlphaComponent(border).cgColor
        recordingButton.layer?.shadowColor = BlueyTheme.green.cgColor
        recordingButton.layer?.shadowOpacity = Float(0.18 + (0.22 * phase))
        recordingButton.layer?.shadowRadius = 7 + (6 * phase)
        recordingButton.layer?.shadowOffset = .zero
        recordingButton.contentTintColor = BlueyTheme.green
    }

    private func seedTranscriptPreviewIfEmpty(_ text: String) {
        guard transcriptSnippets.isEmpty else { return }
        updateTranscriptStripText(text, scrollToEnd: false)
    }

    func setListeningState(_ state: PillRunState) {
        switch state {
        case .connecting:
            recordingActive = false
            recordingButton.title = "Starting"
            statusLabel.stringValue = "Connecting"
            composer.placeholder = "Connecting audio..."
            updateAudioRouteBadge("● Starting", accent: BlueyTheme.green)
            styleControlButton(recordingButton, symbol: "waveform", accent: false)
            setTranscriptState("STARTING", active: true)
            seedTranscriptPreviewIfEmpty("Starting mic + system audio...")
        case .listening:
            recordingActive = true
            recordingButton.title = "Stop"
            statusLabel.stringValue = "Listening"
            composer.placeholder = "Listening... type a follow-up anytime"
            updateAudioRouteBadge("● Listening", accent: BlueyTheme.green)
            styleControlButton(recordingButton, symbol: "stop.fill", accent: true)
            setTranscriptState("LISTENING", active: true)
            seedTranscriptPreviewIfEmpty("Mic + System live. Captions appear here.")
        case .paused:
            recordingActive = false
            recordingButton.title = "Listen"
            statusLabel.stringValue = "Paused"
            composer.placeholder = "Ask anything..."
            updateAudioRouteBadge("● Paused", accent: BlueyTheme.textDim)
            styleControlButton(recordingButton, symbol: "waveform", accent: false)
            setTranscriptState("PAUSED", active: false)
            seedTranscriptPreviewIfEmpty("Live captions preview")
        case .failed:
            recordingActive = false
            recordingButton.title = "Listen"
            statusLabel.stringValue = "Audio needs attention"
            composer.placeholder = "Ask anything..."
            updateAudioRouteBadge("● Audio", accent: BlueyTheme.warning)
            styleControlButton(recordingButton, symbol: "waveform", accent: false)
            setTranscriptState("FAILED", active: false)
        case .ready:
            recordingActive = false
            recordingButton.title = "Listen"
            statusLabel.stringValue = "Ready"
            composer.placeholder = "Ask anything..."
            updateAudioRouteBadge("● Ready", accent: BlueyTheme.green)
            styleControlButton(recordingButton, symbol: "waveform", accent: false)
            setTranscriptState("IDLE", active: false)
            seedTranscriptPreviewIfEmpty("Live captions preview")
        }
    }

    private func updateAudioRouteBadge(_ text: String, accent: NSColor) {
        routeBadge.stringValue = text
        routeBadge.textColor = accent
        routeBadge.layer?.borderColor = NSColor.clear.cgColor
        routeBadge.layer?.backgroundColor = NSColor.clear.cgColor
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
        case "code": return "Code"
        case "system_design": return "Design"
        case "screen": return "Vision"
        case "document": return "Docs"
        default: return "Auto"
        }
    }

    func setContextItems(_ items: [OverlayContextItem]) {
        for view in attachmentStack.arrangedSubviews {
            attachmentStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }

        attachmentStrip.isHidden = false
        guard !items.isEmpty else {
            setKnowledgeBadge("Docs empty", accent: BlueyTheme.textDim)
            attachmentStrip.isHidden = true
            attachmentStripHeightConstraint?.constant = 0
            layoutSubtreeIfNeeded()
            return
        }

        setKnowledgeBadge("Docs \(items.count) ready", accent: BlueyTheme.green)
        attachmentStripHeightConstraint?.constant = 34

        for item in items {
            attachmentStack.addArrangedSubview(makeAttachmentChip(item))
        }
        layoutSubtreeIfNeeded()
    }

    func setSessions(_ sessions: [OverlaySessionItem]) {
        sessionItems = sessions
        renameField = nil
        for view in sessionStack.arrangedSubviews {
            sessionStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }

        if sessions.isEmpty {
            let empty = NSTextField(wrappingLabelWithString: "No saved recordings on this device yet.")
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
        hideSystemToast(immediately: true)
        setContextItems([])
        transcriptSnippets.removeAll()
        latestLiveTranscriptLine = nil
        updateTranscriptStripText("Live captions preview", scrollToEnd: false)
        setTranscriptState("IDLE", active: false)
        routeBadge.stringValue = "● Ready"
        routeBadge.textColor = BlueyTheme.green
        latestCanvas = nil
        setCanvasOpen(false)
        canvasToggleButton.isHidden = true
    }

    func pushCard(_ card: RenderedCard) {
        if shouldRenderAsToast(card) {
            showSystemToast(for: card)
            emitCardRendered(id: card.id)
            return
        }
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

    private func shouldRenderAsToast(_ card: RenderedCard) -> Bool {
        let kind = normalizedCardKind(card.kind)
        guard (kind == "system" || kind == "warning"), actionableLoginURL(from: card) == nil else {
            return false
        }
        let title = card.title.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if title.contains("recap") || title.contains("summary") {
            return false
        }
        let plainBody = card.body.trimmingCharacters(in: .whitespacesAndNewlines)
        return plainBody.count <= 360
    }

    private func actionableLoginURL(from card: RenderedCard) -> URL? {
        guard normalizedCardKind(card.kind) == "system" else { return nil }
        for line in card.body.components(separatedBy: .newlines) {
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            let candidate: String
            if trimmed.hasPrefix("login_url:") {
                candidate = trimmed
                    .replacingOccurrences(of: "login_url:", with: "")
                    .trimmingCharacters(in: .whitespacesAndNewlines)
            } else if trimmed.hasPrefix("https://") || trimmed.hasPrefix("http://") {
                candidate = trimmed
            } else {
                continue
            }
            if
                let url = URL(string: candidate),
                let scheme = url.scheme?.lowercased(),
                ["http", "https"].contains(scheme)
            {
                return url
            }
        }
        return nil
    }

    private func showSystemToast(for card: RenderedCard) {
        toastHideWorkItem?.cancel()
        let title = card.title.trimmingCharacters(in: .whitespacesAndNewlines)
        let body = systemToastBody(card.body)
        toastTitleLabel.stringValue = title.isEmpty ? "Bluey" : title
        toastBodyLabel.stringValue = body
        statusLabel.stringValue = toastTitleLabel.stringValue

        toastView.isHidden = false
        toastView.animator().alphaValue = 1

        let work = DispatchWorkItem { [weak self] in
            self?.hideSystemToast(immediately: false)
        }
        toastHideWorkItem = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 4.0, execute: work)
    }

    private func hideSystemToast(immediately: Bool) {
        toastHideWorkItem?.cancel()
        toastHideWorkItem = nil
        guard !toastView.isHidden else { return }
        if immediately {
            toastView.alphaValue = 0
            toastView.isHidden = true
            return
        }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.18
            toastView.animator().alphaValue = 0
        } completionHandler: { [weak self] in
            self?.toastView.isHidden = true
        }
    }

    private func systemToastBody(_ body: String) -> String {
        var lines: [String] = []
        for raw in body.components(separatedBy: .newlines) {
            let line = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !line.isEmpty, !line.hasPrefix("login_url:") else { continue }
            lines.append(line)
        }
        let joined = lines.joined(separator: " · ")
            .replacingOccurrences(of: "knowledge base", with: "documents")
        if joined.count <= 190 {
            return joined
        }
        let end = joined.index(joined.startIndex, offsetBy: 187)
        return String(joined[..<end]) + "..."
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
        if !open, canvasFullWindow {
            restoreCanvasWindow()
        }
        canvasOpen = open
        canvasPane.isHidden = !open
        updateCanvasWidth()
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

    private func toggleWindowFullSize() {
        guard window != nil else { return }
        if windowFullSize {
            restoreWindowFromFullSize()
        } else {
            expandWindowFullSize()
        }
    }

    private func updateFullSizeButtonChrome() {
        let symbol = windowFullSize
            ? "arrow.down.right.and.arrow.up.left"
            : "arrow.up.left.and.arrow.down.right"
        let fallback = windowFullSize ? "↙" : "↗"
        styleHeaderIconButton(fullSizeButton, symbol: symbol, fallback: fallback)
        fullSizeButton.toolTip = windowFullSize ? "Restore Bluey size" : "Make Bluey full size"
    }

    private func expandWindowFullSize() {
        guard let window else { return }
        if preWindowFullSizeFrame == nil {
            preWindowFullSizeFrame = window.frame
        }
        windowFullSize = true
        updateFullSizeButtonChrome()

        let screen = window.screen?.visibleFrame
            ?? NSScreen.main?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let maxWidth = max(ExpandedPanelMetrics.minCompactWidth, screen.width - ExpandedPanelMetrics.screenInset * 2)
        let maxHeight = max(ExpandedPanelMetrics.minHeight, screen.height - ExpandedPanelMetrics.screenInset * 2)
        if let overlayWindow = window as? OverlayWindow {
            overlayWindow.minimumFrameWidth = min(ExpandedPanelMetrics.minCompactWidth, maxWidth)
            overlayWindow.maximumFrameWidth = maxWidth
            overlayWindow.minimumFrameHeight = ExpandedPanelMetrics.minHeight
            overlayWindow.maximumFrameHeight = maxHeight
        }
        window.minSize = NSSize(width: min(ExpandedPanelMetrics.minCompactWidth, maxWidth), height: ExpandedPanelMetrics.minHeight)
        window.contentMinSize = window.minSize
        window.maxSize = NSSize(width: maxWidth, height: maxHeight)
        window.contentMaxSize = window.maxSize

        var frame = NSRect(
            x: screen.midX - maxWidth / 2,
            y: screen.midY - maxHeight / 2,
            width: maxWidth,
            height: maxHeight)
        frame = ExpandedPanelMetrics.fitExpandedFrameToVisibleScreen(frame, visibleFrame: screen)
        updateCanvasWidth()
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.16
            context.timingFunction = CAMediaTimingFunction(name: .easeOut)
            window.animator().setFrame(frame, display: true)
            self.layoutSubtreeIfNeeded()
        }
    }

    private func restoreWindowFromFullSize() {
        guard let window else { return }
        windowFullSize = false
        let targetFrame = preWindowFullSizeFrame
        preWindowFullSizeFrame = nil
        restoreCompactWidth()
        updateCanvasWidth()
        updateFullSizeButtonChrome()
        guard let targetFrame else { return }
        let screen = window.screen?.visibleFrame
            ?? NSScreen.main?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let fittedFrame = ExpandedPanelMetrics.fitExpandedFrameToVisibleScreen(targetFrame, visibleFrame: screen)
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.16
            context.timingFunction = CAMediaTimingFunction(name: .easeOut)
            window.animator().setFrame(fittedFrame, display: true)
            self.layoutSubtreeIfNeeded()
        }
    }

    private func updateCanvasWidth() {
        guard canvasOpen else {
            canvasWidthConstraint?.constant = 0
            return
        }
        let available = max(0, bounds.width)
        if canvasFullWindow || windowFullSize {
            canvasWidthConstraint?.constant = min(max(380, available * 0.44), 560)
        } else {
            canvasWidthConstraint?.constant = 310
        }
    }

    private func expandCanvasWindow() {
        guard let window else { return }
        if preCanvasFullWindowFrame == nil {
            preCanvasFullWindowFrame = window.frame
        }
        canvasFullWindow = true
        canvasPane.setFullWindow(true)

        let screen = window.screen?.visibleFrame
            ?? NSScreen.main?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let maxWidth = max(360, screen.width - ExpandedPanelMetrics.screenInset * 2)
        let maxHeight = max(ExpandedPanelMetrics.minHeight, screen.height - ExpandedPanelMetrics.screenInset * 2)
        if let overlayWindow = window as? OverlayWindow {
            overlayWindow.minimumFrameWidth = min(ExpandedPanelMetrics.minCompactWidth, maxWidth)
            overlayWindow.maximumFrameWidth = maxWidth
            overlayWindow.minimumFrameHeight = ExpandedPanelMetrics.minHeight
            overlayWindow.maximumFrameHeight = maxHeight
        }
        window.minSize = NSSize(width: min(ExpandedPanelMetrics.minCompactWidth, maxWidth), height: ExpandedPanelMetrics.minHeight)
        window.contentMinSize = window.minSize
        window.maxSize = NSSize(width: maxWidth, height: maxHeight)
        window.contentMaxSize = window.maxSize
        var frame = NSRect(
            x: screen.midX - maxWidth / 2,
            y: screen.midY - maxHeight / 2,
            width: maxWidth,
            height: maxHeight)
        frame = ExpandedPanelMetrics.fitExpandedFrameToVisibleScreen(frame, visibleFrame: screen)
        updateCanvasWidth()
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.16
            context.timingFunction = CAMediaTimingFunction(name: .easeOut)
            window.animator().setFrame(frame, display: true)
            self.layoutSubtreeIfNeeded()
        }
    }

    private func restoreCanvasWindow() {
        guard let window else { return }
        canvasFullWindow = false
        canvasPane.setFullWindow(false)
        let targetFrame = preCanvasFullWindowFrame
        preCanvasFullWindowFrame = nil
        restoreCompactWidth()
        updateCanvasWidth()
        if let targetFrame {
            NSAnimationContext.runAnimationGroup { context in
                context.duration = 0.16
                context.timingFunction = CAMediaTimingFunction(name: .easeOut)
                window.animator().setFrame(targetFrame, display: true)
                self.layoutSubtreeIfNeeded()
            }
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
        let maximumWidth = max(clampedTargetWidth, screen.width - ExpandedPanelMetrics.screenInset * 2)
        let maximumHeight = max(ExpandedPanelMetrics.minHeight, screen.height - ExpandedPanelMetrics.screenInset * 2)
        if let overlayWindow = window as? OverlayWindow {
            overlayWindow.minimumFrameWidth = minimumWidth
            overlayWindow.maximumFrameWidth = maximumWidth
            overlayWindow.minimumFrameHeight = ExpandedPanelMetrics.minHeight
            overlayWindow.maximumFrameHeight = maximumHeight
        }
        window.minSize = NSSize(width: minimumWidth, height: ExpandedPanelMetrics.minHeight)
        window.contentMinSize = NSSize(width: minimumWidth, height: ExpandedPanelMetrics.minHeight)
        window.maxSize = NSSize(width: maximumWidth, height: maximumHeight)
        window.contentMaxSize = NSSize(width: maximumWidth, height: maximumHeight)
        guard window.frame.width < clampedTargetWidth else { return }
        var frame = window.frame
        frame.size.width = clampedTargetWidth
        frame.origin.x = min(max(screen.minX + 12, frame.origin.x), screen.maxX - frame.width - 12)
        frame = ExpandedPanelMetrics.fitExpandedFrameToVisibleScreen(frame, visibleFrame: screen)
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
        let maximumWidth = max(compactWidth, screen.width - ExpandedPanelMetrics.screenInset * 2)
        let maximumHeight = max(ExpandedPanelMetrics.minHeight, screen.height - ExpandedPanelMetrics.screenInset * 2)
        if let overlayWindow = window as? OverlayWindow {
            overlayWindow.minimumFrameWidth = minimumWidth
            overlayWindow.maximumFrameWidth = maximumWidth
            overlayWindow.minimumFrameHeight = ExpandedPanelMetrics.minHeight
            overlayWindow.maximumFrameHeight = maximumHeight
        }
        window.minSize = NSSize(width: minimumWidth, height: ExpandedPanelMetrics.minHeight)
        window.contentMinSize = NSSize(width: minimumWidth, height: ExpandedPanelMetrics.minHeight)
        window.maxSize = NSSize(width: maximumWidth, height: maximumHeight)
        window.contentMaxSize = NSSize(width: maximumWidth, height: maximumHeight)
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
        let body = displayTranscriptText(card.body)
        guard !body.isEmpty else { return }

        let label = transcriptSourceLabel(title)
        rememberTranscriptForAnswer(label: label, body: body, final: true)
        setTranscriptState(recordingActive ? "TRANSCRIBING" : "CAPTURED", active: recordingActive)
        updateTranscriptStripText("Captured · \(label) · \(body)", scrollToEnd: true)
    }

    func appendLiveTranscript(source: String, text: String, final: Bool) {
        let body = displayTranscriptText(text)
        let label = transcriptSourceLabel(source)
        let state = recordingActive ? "TRANSCRIBING" : (final ? "CAPTURED" : "HEARD")
        let prefix = recordingActive ? "Transcribing" : (final ? "Captured" : "Heard")
        guard !body.isEmpty else {
            setTranscriptState(state, active: recordingActive)
            updateTranscriptStripText("\(prefix) · \(label) audio is live", scrollToEnd: false)
            return
        }
        rememberTranscriptForAnswer(label: label, body: body, final: final)
        if final {
            feed.pushTranscript(source: label, body: body)
        }
        setTranscriptState(state, active: recordingActive)
        updateTranscriptStripText("\(prefix) · \(label) · \(body)", scrollToEnd: true)
    }

    private func rememberTranscriptForAnswer(label: String, body: String, final: Bool) {
        let cleanBody = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanBody.isEmpty else { return }
        let line = "\(label): \(cleanBody)"
        if final {
            latestLiveTranscriptLine = nil
            if let last = transcriptSnippets.last,
               shouldReplaceTranscriptMemoryLine(last, with: line) {
                transcriptSnippets[transcriptSnippets.count - 1] = longerTranscriptMemoryLine(last, line)
            } else if transcriptSnippets.last != line {
                transcriptSnippets.append(line)
            }
            if transcriptSnippets.count > 12 {
                transcriptSnippets.removeFirst(transcriptSnippets.count - 12)
            }
        } else {
            latestLiveTranscriptLine = line
        }
    }

    private func transcriptQuestionForAnswer() -> String? {
        var lines = transcriptSnippets
        if let live = latestLiveTranscriptLine, lines.last != live {
            lines.append(live)
        }
        let joined = compactTranscriptQuestionLines(Array(lines.suffix(12)))
            .joined(separator: "\n")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !joined.isEmpty else { return nil }
        return joined
    }

    private func compactTranscriptQuestionLines(_ lines: [String]) -> [String] {
        var order: [String] = []
        var bodies: [String: String] = [:]
        for line in lines {
            let parsed = parsedTranscriptMemoryLine(line)
            let label = parsed.label
            let body = parsed.body
            guard !body.isEmpty else { continue }
            if bodies[label] == nil {
                order.append(label)
                bodies[label] = body
            } else if let existing = bodies[label] {
                bodies[label] = mergedTranscriptBody(existing, body)
            }
        }
        return order.compactMap { label in
            guard let body = bodies[label]?.trimmingCharacters(in: .whitespacesAndNewlines), !body.isEmpty else {
                return nil
            }
            return "\(label): \(body)"
        }
    }

    private func parsedTranscriptMemoryLine(_ line: String) -> (label: String, body: String) {
        let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let colon = trimmed.firstIndex(of: ":") else {
            return ("Audio", trimmed)
        }
        let label = String(trimmed[..<colon]).trimmingCharacters(in: .whitespacesAndNewlines)
        let bodyStart = trimmed.index(after: colon)
        let body = String(trimmed[bodyStart...]).trimmingCharacters(in: .whitespacesAndNewlines)
        return (label.isEmpty ? "Audio" : label, body)
    }

    private func mergedTranscriptBody(_ existing: String, _ incoming: String) -> String {
        let old = existing.trimmingCharacters(in: .whitespacesAndNewlines)
        let new = incoming.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !old.isEmpty else { return new }
        guard !new.isEmpty else { return old }

        let oldNorm = normalizeTranscriptMemoryLine(old)
        let newNorm = normalizeTranscriptMemoryLine(new)
        if oldNorm == newNorm || oldNorm.contains(newNorm) { return old }
        if newNorm.contains(oldNorm) { return new }

        let oldWords = old.split(whereSeparator: { $0.isWhitespace }).map(String.init)
        let newWords = new.split(whereSeparator: { $0.isWhitespace }).map(String.init)
        let maxOverlap = min(oldWords.count, newWords.count, 8)
        if maxOverlap > 0 {
            for count in stride(from: maxOverlap, through: 1, by: -1) {
                let suffix = oldWords.suffix(count).joined(separator: " ")
                let prefix = newWords.prefix(count).joined(separator: " ")
                if normalizeTranscriptMemoryLine(suffix) == normalizeTranscriptMemoryLine(prefix) {
                    return (oldWords + newWords.dropFirst(count)).joined(separator: " ")
                }
            }
        }

        return "\(old) \(new)"
    }

    private func shouldReplaceTranscriptMemoryLine(_ old: String, with new: String) -> Bool {
        let oldNorm = normalizeTranscriptMemoryLine(old)
        let newNorm = normalizeTranscriptMemoryLine(new)
        guard !oldNorm.isEmpty, !newNorm.isEmpty else { return false }
        return oldNorm == newNorm
            || oldNorm.hasPrefix(newNorm)
            || newNorm.hasPrefix(oldNorm)
            || oldNorm.contains(newNorm)
            || newNorm.contains(oldNorm)
    }

    private func longerTranscriptMemoryLine(_ first: String, _ second: String) -> String {
        first.count >= second.count ? first : second
    }

    private func normalizeTranscriptMemoryLine(_ value: String) -> String {
        value
            .lowercased()
            .replacingOccurrences(of: #"[^a-z0-9]+"#, with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func transcriptSourceLabel(_ raw: String) -> String {
        let clean = raw
            .replacingOccurrences(of: "_", with: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let lower = clean.lowercased()
        if lower.contains("microphone") || lower.contains("mic") || lower == "user" {
            return "Mic"
        }
        if lower.contains("system") {
            return "System"
        }
        return clean.isEmpty ? "Audio" : clean.capitalized
    }

    private func updateTranscriptStripText(_ text: String, scrollToEnd: Bool) {
        transcriptLabel.attributedStringValue = attributedTranscriptStripText(text)
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

    private func attributedTranscriptStripText(_ text: String) -> NSAttributedString {
        let font = transcriptLabel.font ?? NSFont.systemFont(ofSize: 11.5, weight: .medium)
        let sourceFont = NSFont.systemFont(ofSize: 11.5, weight: .bold)
        let attributed = NSMutableAttributedString(
            string: text,
            attributes: [
                .font: font,
                .foregroundColor: BlueyTheme.textDim,
            ])
        for label in ["Transcribing", "Captured", "Heard", "Starting", "Mic", "System", "Audio"]
            where attributed.string.hasPrefix(label) || attributed.string == label {
            let range = NSRange(location: 0, length: label.count)
            attributed.addAttributes(
                [
                    .font: sourceFont,
                    .foregroundColor: BlueyTheme.green,
                ],
                range: range)
            break
        }
        return attributed
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

        let remove = RemoveAttachmentButton(title: "", target: self, action: #selector(removeAttachmentClicked(_:)))
        remove.translatesAutoresizingMaskIntoConstraints = false
        remove.contextId = item.id
        remove.isBordered = false
        remove.wantsLayer = true
        remove.layer?.cornerRadius = 9
        remove.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.04).cgColor
        remove.layer?.borderWidth = 1
        remove.layer?.borderColor = BlueyTheme.hairline.cgColor
        remove.contentTintColor = BlueyTheme.textDim
        if let image = symbolImage("xmark") {
            image.isTemplate = true
            remove.image = image
            remove.imagePosition = .imageOnly
            remove.imageScaling = .scaleProportionallyDown
        } else {
            remove.title = "x"
        }
        remove.toolTip = "Remove this document"

        chip.addSubview(icon)
        chip.addSubview(title)
        chip.addSubview(kind)
        chip.addSubview(remove)
        NSLayoutConstraint.activate([
            chip.heightAnchor.constraint(equalToConstant: 28),
            chip.widthAnchor.constraint(lessThanOrEqualToConstant: 214),
            chip.widthAnchor.constraint(greaterThanOrEqualToConstant: 134),

            icon.leadingAnchor.constraint(equalTo: chip.leadingAnchor, constant: 8),
            icon.centerYAnchor.constraint(equalTo: chip.centerYAnchor),
            icon.widthAnchor.constraint(equalToConstant: 16),
            icon.heightAnchor.constraint(equalToConstant: 16),

            title.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: 7),
            title.topAnchor.constraint(equalTo: chip.topAnchor, constant: 4),
            title.trailingAnchor.constraint(equalTo: remove.leadingAnchor, constant: -6),

            kind.leadingAnchor.constraint(equalTo: title.leadingAnchor),
            kind.topAnchor.constraint(equalTo: title.bottomAnchor, constant: -1),
            kind.trailingAnchor.constraint(lessThanOrEqualTo: title.trailingAnchor),

            remove.trailingAnchor.constraint(equalTo: chip.trailingAnchor, constant: -7),
            remove.centerYAnchor.constraint(equalTo: chip.centerYAnchor),
            remove.widthAnchor.constraint(equalToConstant: 18),
            remove.heightAnchor.constraint(equalToConstant: 18),
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
        rename.toolTip = "Rename recording"
        if let image = symbolImage("pencil") {
            image.isTemplate = true
            rename.image = image
            rename.imagePosition = .imageOnly
            rename.imageScaling = .scaleProportionallyDown
        } else {
            rename.title = "Edit"
            rename.font = NSFont.systemFont(ofSize: 9, weight: .bold)
        }

        let delete = NSButton(title: "", target: self, action: #selector(deleteSessionClicked(_:)))
        delete.translatesAutoresizingMaskIntoConstraints = false
        delete.isBordered = false
        delete.tag = sessionIndex(session.id)
        delete.contentTintColor = BlueyTheme.warning
        delete.toolTip = "Delete recording"
        if let image = symbolImage("trash") {
            image.isTemplate = true
            delete.image = image
            delete.imagePosition = .imageOnly
            delete.imageScaling = .scaleProportionallyDown
        } else {
            delete.title = "Del"
            delete.font = NSFont.systemFont(ofSize: 9, weight: .bold)
        }

        row.addSubview(openButton)
        row.addSubview(title)
        row.addSubview(subtitle)
        row.addSubview(rename)
        row.addSubview(delete)
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

            rename.centerYAnchor.constraint(equalTo: row.centerYAnchor),
            rename.widthAnchor.constraint(equalToConstant: 28),
            rename.heightAnchor.constraint(equalToConstant: 28),

            delete.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -6),
            delete.centerYAnchor.constraint(equalTo: row.centerYAnchor),
            delete.widthAnchor.constraint(equalToConstant: 28),
            delete.heightAnchor.constraint(equalToConstant: 28),

            rename.trailingAnchor.constraint(equalTo: delete.leadingAnchor, constant: -2),
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

    @objc private func deleteSessionClicked(_ sender: NSButton) {
        guard sender.tag >= 0, sender.tag < sessionItems.count else { return }
        let session = sessionItems[sender.tag]
        pendingDeleteSessionId = session.id
        closeConfirmTitle.stringValue = "Delete recording?"
        closeConfirmBody.stringValue = "Remove \"\(session.title)\" from this device. This cannot be undone."
        closeConfirmTurnOffButton.title = "Delete"
        closeConfirmTurnOffButton.target = self
        closeConfirmTurnOffButton.action = #selector(confirmDeleteSessionClicked)
        closeConfirmTurnOffButton.toolTip = "Delete this saved recording"
        styleControlButton(closeConfirmTurnOffButton, symbol: "trash", accent: true)
        presentConfirmationOverlay()
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
    private var expandedPassthroughTimer: Timer?
    private var currentRunState: PillRunState = .ready
    private var overlayOpacity = 0.94

    /// Pending boot card, if a Boot command arrived before windows materialised.
    private var pendingBoot: (title: String, lines: [String])?

    func start() {
        // Pill window: compact launcher, centered by default.
        let pillSize = PillMetrics.size
        let screen = OverlayScreenPlacement.activeVisibleFrame()
        pillWindow = OverlayWindow(
            contentRect: PillMetrics.centeredFrame(in: screen),
            draggable: true)
        pillWindow.contentCornerRadius = pillSize.height / 2

        pillView = PillView(frame: NSRect(origin: .zero, size: pillSize))
        pillWindow.contentView = pillView
        pillView.statusText = "Bluey"
        pillView.setRunState(currentRunState)
        pillView.applyBackgroundOpacity(overlayOpacity)
        pillView.onClick = { [weak self] in self?.expand() }
        pillView.onRunToggle = { [weak self] in self?.toggleListeningFromPill() }
        pillView.onAsk = { [weak self] in self?.expandAndFocusQuestion() }
        pillView.onEnd = { [weak self] in self?.expandAndConfirmTurnOff() }

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
        startExpandedPassthroughTracking()
        startIpcLoop()
    }

    private func bringPillToFront() {
        centerPillOnMainScreen()
        pillWindow.setIsVisible(true)
        pillWindow.orderFrontRegardless()
        pillWindow.makeKeyAndOrderFront(nil)
        pillView.needsDisplay = true
        pillView.needsLayout = true
        pillView.layoutSubtreeIfNeeded()
        pillView.displayIfNeeded()
        pillWindow.displayIfNeeded()
    }

    private func centerPillOnMainScreen() {
        let screen = OverlayScreenPlacement.activeVisibleFrame()
        pillWindow.setFrame(PillMetrics.centeredFrame(in: screen), display: true)
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

    private func startExpandedPassthroughTracking() {
        expandedPassthroughTimer?.invalidate()
        expandedPassthroughTimer = Timer.scheduledTimer(withTimeInterval: 0.06, repeats: true) { [weak self] _ in
            guard
                let self,
                let expandedWindow = self.expandedWindow,
                expandedWindow.isVisible
            else { return }

            // The expanded panel is an interactive control surface. Earlier
            // builds tried to make non-control regions pass clicks through by
            // flipping the whole NSWindow's ignoresMouseEvents flag from a
            // timer. In practice that made timing-sensitive controls feel
            // broken: a click could arrive while the entire window was still
            // ignoring events. Keep the expanded window clickable and reserve
            // full click-through for the collapsed pill/hidden states.
            if expandedWindow.ignoresMouseEvents {
                expandedWindow.ignoresMouseEvents = false
            }
        }
    }

    private func expand() {
        ensureExpandedWindow()
        guard let expandedWindow else { return }
        placeExpandedWindowForOpen()
        pillWindow?.orderOut(nil)
        expandedWindow.ignoresMouseEvents = false
        expandedWindow.orderFrontRegardless()
        expandedWindow.makeKeyAndOrderFront(nil)
        DispatchQueue.main.async { [weak self] in
            self?.placeExpandedWindowForOpen()
        }
        emitSimple("shown")
        emitLifecycle("expanded")
    }

    private func ensureExpandedWindow() {
        guard expandedWindow == nil else { return }
        let screen = OverlayScreenPlacement.activeVisibleFrame()
        let expandedFrame = ExpandedPanelMetrics.compactFrame(in: screen)
        let expandedWidth = expandedFrame.width
        let minimumWidth = ExpandedPanelMetrics.fittingMinimumWidth(for: screen, targetWidth: expandedWidth)
        let window = OverlayWindow(
            contentRect: expandedFrame,
            draggable: true,
            resizable: false)
        window.contentCornerRadius = ExpandedPanelMetrics.cornerRadius
        window.preserveProgrammaticFrameHeight = true
        let maxExpandedWidth = max(minimumWidth, screen.width - ExpandedPanelMetrics.screenInset * 2)
        let maxExpandedHeight = max(ExpandedPanelMetrics.minHeight, screen.height - ExpandedPanelMetrics.screenInset * 2)
        window.minimumFrameWidth = minimumWidth
        window.maximumFrameWidth = maxExpandedWidth
        window.minimumFrameHeight = ExpandedPanelMetrics.minHeight
        window.maximumFrameHeight = maxExpandedHeight
        window.minSize = NSSize(width: minimumWidth, height: ExpandedPanelMetrics.minHeight)
        window.maxSize = NSSize(width: maxExpandedWidth, height: maxExpandedHeight)
        window.contentMinSize = NSSize(width: minimumWidth, height: ExpandedPanelMetrics.minHeight)
        window.contentMaxSize = NSSize(width: maxExpandedWidth, height: maxExpandedHeight)
        let view = ExpandedPanelView(frame: NSRect(origin: .zero, size: expandedFrame.size))
        view.autoresizingMask = [.width, .height]
        window.contentView = view
        view.onClose = { [weak self] in self?.collapse() }
        view.onListeningStateChanged = { [weak self] state in
            self?.setRunState(state)
        }
        view.onOpacityChanged = { [weak self] opacity in
            self?.overlayOpacity = opacity
            self?.pillView?.applyBackgroundOpacity(opacity)
        }
        expandedWindow = window
        expandedView = view
        view.setListeningState(currentRunState)
        view.applyOpacity(overlayOpacity)
        if let pending = pendingBoot {
            pushBootCard(title: pending.title, lines: pending.lines)
            pendingBoot = nil
        }
    }

    private func placeExpandedWindowForOpen() {
        guard let expandedWindow else { return }
        let screen = OverlayScreenPlacement.activeVisibleFrame()
        expandedWindow.setFrame(ExpandedPanelMetrics.compactFrame(in: screen), display: true)
    }

    private func collapse() {
        expandedWindow?.ignoresMouseEvents = false
        expandedWindow?.orderOut(nil)
        bringPillToFront()
        emitSimple("hidden")
        emitLifecycle("collapsed")
    }

    private func setRunState(_ state: PillRunState) {
        currentRunState = state
        pillView?.setRunState(state)
        expandedView?.setListeningState(state)
    }

    private func toggleListeningFromPill() {
        if currentRunState == .listening || currentRunState == .connecting {
            emitSimple("recording_stop_requested")
            setRunState(.paused)
        } else {
            emitSimple("recording_start_requested")
            setRunState(.listening)
        }
    }

    private func expandAndFocusQuestion() {
        expand()
        expandedView?.focusComposerForQuestion()
    }

    private func expandAndConfirmTurnOff() {
        expand()
        expandedView?.showTurnOffConfirmation()
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
            overlayOpacity = value
            pillView?.applyBackgroundOpacity(value)
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
        case .listeningStateChanged(let state):
            let runState = PillRunState(listeningState: state)
            setRunState(runState)
        case .transcriptPartial(let source, let text):
            let runState = PillRunState(listeningState: "listening")
            setRunState(runState)
            expandedView?.appendLiveTranscript(source: source, text: text, final: false)
        case .transcriptFinal(let source, let text):
            expandedView?.appendLiveTranscript(source: source, text: text, final: true)
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
        let signInURL = loginURL(from: lines)
        let body = lines.joined(separator: "\n")
        let card = RenderedCard(
            id: UUID().uuidString,
            kind: "system",
            title: title,
            body: body,
            done: true,
            costLabel: nil,
            artifact: nil)
        if signInURL != nil || title.localizedCaseInsensitiveContains("sign in") {
            view.showSignedOutLogin(url: signInURL)
            pillView?.dotColor = BlueyTheme.warning
        } else {
            view.showSignedInReady()
        }
        view.pushCard(card)
        if signInURL == nil {
            pillView?.dotColor = NSColor.systemGreen
        }
    }

    private func loginURL(from lines: [String]) -> URL? {
        for line in lines {
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            let candidate: String
            if trimmed.hasPrefix("login_url:") {
                candidate = trimmed
                    .replacingOccurrences(of: "login_url:", with: "")
                    .trimmingCharacters(in: .whitespacesAndNewlines)
            } else if trimmed.hasPrefix("https://") || trimmed.hasPrefix("http://") {
                candidate = trimmed
            } else {
                continue
            }
            if
                let url = URL(string: candidate),
                let scheme = url.scheme?.lowercased(),
                ["http", "https"].contains(scheme)
            {
                return url
            }
        }
        return nil
    }

    private func applyPosition(_ pos: String) {
        let screen = OverlayScreenPlacement.activeVisibleFrame()
        let pillSize = pillWindow.frame.size
        let inset: CGFloat = 12
        let origin: NSPoint
        switch pos {
        case "top_left":     origin = NSPoint(x: screen.minX + inset,                  y: screen.maxY - pillSize.height - inset)
        case "top_right":    origin = NSPoint(x: screen.maxX - pillSize.width - inset, y: screen.maxY - pillSize.height - inset)
        case "bottom_left":  origin = NSPoint(x: screen.minX + inset,                  y: screen.minY + inset)
        case "bottom_right": origin = NSPoint(x: screen.maxX - pillSize.width - inset, y: screen.minY + inset)
        case "center":       origin = NSPoint(x: screen.midX - pillSize.width / 2,     y: screen.midY - pillSize.height / 2)
        default:             origin = PillMetrics.centeredFrame(in: screen).origin
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
