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

/// Redesign token palette — the *mechanical* translation of the CSS custom
/// properties in `docs/design/bluey-overlay-redesign-v2.html`. These are the
/// dashboard-exact tokens (one refined `#3B82F6` blue accent, white-alpha text,
/// hairlines) that replace the old neon-cyan `BlueyTheme` look on the panel.
/// NSColor RGB are css/255. This is the visual contract.
private enum Tok {
    // Text ramp.
    static let tx1 = NSColor.white.withAlphaComponent(0.96)
    static let tx2 = NSColor.white.withAlphaComponent(0.62)
    static let tx3 = NSColor.white.withAlphaComponent(0.40)
    static let tx4 = NSColor.white.withAlphaComponent(0.24)
    // Hairlines.
    static let hairline = NSColor.white.withAlphaComponent(0.10)
    static let hairlineStrong = NSColor.white.withAlphaComponent(0.17)
    static let glassHi = NSColor.white.withAlphaComponent(0.05)
    // Accent (#3B82F6 / #AFC9FB).
    static let accent = NSColor(red: 0.231, green: 0.510, blue: 0.965, alpha: 1.0)
    static let accentTx = NSColor(red: 0.686, green: 0.788, blue: 0.984, alpha: 1.0)
    static let accentBg = NSColor(red: 0.231, green: 0.510, blue: 0.965, alpha: 0.18)
    static let accentBgStrong = NSColor(red: 0.231, green: 0.510, blue: 0.965, alpha: 0.30)
    // Status.
    static let ok = NSColor(red: 0.231, green: 0.820, blue: 0.482, alpha: 1.0)
    static let warn = NSColor(red: 0.890, green: 0.663, blue: 0.247, alpha: 1.0)
    static let danger = NSColor(red: 0.898, green: 0.337, blue: 0.306, alpha: 1.0)
    // Code-span tint used in answers/diffs (#cfe0f5).
    static let codeTx = NSColor(red: 0.812, green: 0.878, blue: 0.961, alpha: 1.0)
    // Radii.
    static let rSm: CGFloat = 8
    static let rMd: CGFloat = 11
    static let rLg: CGFloat = 14
    static let rXl: CGFloat = 18
    static let r2xl: CGFloat = 24

    // The dark panel surface fill — near-opaque so it reads dark over ANY
    // wallpaper (the mockup only looks .52 because it sits over a dark desk).
    // This single alpha is the one knob for "dark vs washed-out".
    static let surfaceFill = NSColor(red: 0.066, green: 0.078, blue: 0.102, alpha: 0.97)
    // Dark modal fill (readable over the dim backdrop).
    static let modalFill = NSColor(red: 0.10, green: 0.115, blue: 0.14, alpha: 0.94)

    static func font(_ size: CGFloat, _ weight: NSFont.Weight = .regular) -> NSFont {
        NSFont.systemFont(ofSize: size, weight: weight)
    }

    static func mono(_ size: CGFloat, _ weight: NSFont.Weight = .regular) -> NSFont {
        NSFont.monospacedSystemFont(ofSize: size, weight: weight)
    }
}

/// Tracked, uppercase micro-label (CSS `letter-spacing` + `text-transform`).
private func trackedLabel(_ text: String, size: CGFloat, weight: NSFont.Weight, color: NSColor, tracking: CGFloat = 0.5) -> NSTextField {
    let label = NSTextField(labelWithString: "")
    label.translatesAutoresizingMaskIntoConstraints = false
    label.attributedStringValue = NSAttributedString(
        string: text,
        attributes: [
            .font: Tok.font(size, weight),
            .foregroundColor: color,
            .kern: tracking,
        ])
    label.lineBreakMode = .byTruncatingTail
    return label
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
                // Enter and Command+Enter both submit. Shift+Enter keeps a
                // multiline thought inside the composer.
                onSubmit?()
            }
            return
        }
        super.keyDown(with: event)
    }

    // Clicking the text view must make it first responder so keystrokes land.
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

/// The rounded background behind the composer. Clicking anywhere on it (not just
/// the text glyphs) focuses the composer, so the whole bar reads as one input.
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
    /// Project/workspace the session belongs to (redesign: shown as "project ·
    /// N turns"). `nil` when unassociated. Mirrors OverlaySessionItem.project.
    let project: String?
    /// Best-effort last-updated marker (epoch seconds or RFC3339). Drives the
    /// Today / Yesterday / Earlier date-group bucketing. Empty when unknown.
    let updatedAt: String
    /// Turn/exchange count when cheaply countable (shown as "N turns").
    let turnCount: Int?
    /// True when the user has pinned this session to the top of the list.
    let pinned: Bool

    init(
        id: String,
        title: String,
        subtitle: String,
        isActive: Bool,
        project: String? = nil,
        updatedAt: String = "",
        turnCount: Int? = nil,
        pinned: Bool = false
    ) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.isActive = isActive
        self.project = project
        self.updatedAt = updatedAt
        self.turnCount = turnCount
        self.pinned = pinned
    }
}

/// A BYOT (bring-your-own-token) billing disclosure the user MUST acknowledge
/// before a cloud agent attaches. Mirrors `OverlayCommand::PushBillingDisclosure`
/// in crates/cue-core/src/overlay.rs. The UI renders `disclosure` verbatim (the
/// legal copy) and echoes `vendorShort` + `pendingKind`/`pendingSessionId` back
/// in `billing_disclosure_responded`.
private struct BillingDisclosure {
    let vendorShort: String
    let vendorDisplayName: String
    let billingModel: String
    let disclosure: String
    let pendingKind: String
    let pendingSessionId: String?
}

// MARK: - Agent-bridge wire DTOs (Slice 5b)
//
// Shape-only mirrors of crates/cue-core/src/agent_ui.rs. They carry a
// connector's name / auth tier / readiness and a session id / title /
// timestamp — never an env value, token, or message body. Field names match
// the Rust serde `snake_case` form exactly so JSON decodes one-to-one.

private struct AgentSummary {
    let kind: String
    let displayName: String
    /// snake_case capability: drive / read_only / needs_trust / needs_reauth /
    /// cloud_blocked.
    let capability: String
    let connectorCount: Int
    let readyConnectorCount: Int
    let sessionCount: Int?
    let attached: Bool
}

private struct AgentSessionSummary {
    let id: String
    let title: String?
    let updatedAt: String
    /// Project/workspace path the session belongs to, when the source records
    /// it. `nil` when unassociated. Mirrors AgentSessionSummary.project.
    let project: String?

    init(id: String, title: String?, updatedAt: String, project: String? = nil) {
        self.id = id
        self.title = title
        self.updatedAt = updatedAt
        self.project = project
    }
}

private struct AgentConnectorInfo {
    let name: String
    /// snake_case auth tier: env_auth / hosted_oauth / none.
    let authTier: String
    let ready: Bool
}

/// Capability presentation: the snake_case `capability` string mapped to a
/// drawer chip label, accent color, and a "dimmed / non-tappable" flag for
/// `cloud_blocked`. Colors come straight from BlueyTheme (PLAN §9 surface 1).
private struct AgentCapability {
    let label: String
    let color: NSColor
    let dimmed: Bool

    init(_ raw: String) {
        switch raw {
        case "drive":
            label = "live"
            color = BlueyTheme.green
            dimmed = false
        case "read_only":
            label = "history only"
            color = BlueyTheme.textDim
            dimmed = false
        case "needs_trust":
            label = "needs trust"
            color = BlueyTheme.warning
            dimmed = false
        case "needs_reauth":
            label = "re-auth"
            color = BlueyTheme.warning
            dimmed = false
        case "cloud_blocked":
            label = "unavailable"
            color = NSColor(red: 1.0, green: 0.44, blue: 0.40, alpha: 1.0)
            dimmed = true
        default:
            label = raw.replacingOccurrences(of: "_", with: " ")
            color = BlueyTheme.textDim
            dimmed = false
        }
    }
}

/// Compact uppercase label for an agent kind, used on the pill-adjacent
/// header badge and the agent-answer card role badge (CLAUDE / CURSOR / …).
private func agentShortLabel(_ kind: String) -> String {
    switch kind {
    case "claude_code": return "CLAUDE"
    case "cursor": return "CURSOR"
    case "codex": return "CODEX"
    case "gemini": return "GEMINI"
    case "windsurf": return "WINDSURF"
    case "aider": return "AIDER"
    default:
        // Strip a trailing "_code" / "_cli" and uppercase the first token.
        let base = kind
            .replacingOccurrences(of: "_code", with: "")
            .replacingOccurrences(of: "_cli", with: "")
        let head = base.split(separator: "_").first.map(String.init) ?? base
        return head.uppercased()
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
    case setBalance(String)
    case setContextItems([OverlayContextItem])
    case setSessions([OverlaySessionItem])
    /// Paginated/searched session page (redesign History-at-scale). Carries the
    /// slice plus totals so the UI can render "show N more" + the result count.
    case setSessionsPage(
        sessions: [OverlaySessionItem],
        total: Int,
        offset: Int,
        hasMore: Bool,
        query: String)
    case listeningStateChanged(String)
    case transcriptPartial(source: String, text: String)
    case transcriptFinal(source: String, text: String)
    case pushCard(CueCard)
    case updateCard(id: String, body: String, done: Bool, costLabel: String?, artifact: OverlayArtifact?)
    case setAgents([AgentSummary])
    case setAgentSessions(kind: String, sessions: [AgentSessionSummary])
    case setAgentConnectors(kind: String, connectors: [AgentConnectorInfo])
    case pushFixProposal(FixProposal)
    case pushBillingDisclosure(BillingDisclosure)
    case shutdown
    case unknown(String)
}

/// A review-gated Fix proposal pushed by the daemon (Fix-button slice F4).
/// Mirrors `OverlayCommand::PushFixProposal` in crates/cue-core/src/overlay.rs.
/// `diff` is absent when the agent only proposed commands (no unified diff);
/// `applySupported` is false for agents that cannot be driven to apply.
private struct FixProposal {
    let proposalId: String
    let diagnosis: String
    let reasoning: String
    let fix: String
    let diff: String?
    let applySupported: Bool
}

/// Decode a `sessions` array into `[OverlaySessionItem]`, tolerating both the
/// legacy four-field shape and the redesigned shape (project / updated_at /
/// turn_count / pinned). Items without an id are dropped. Shared by
/// `set_sessions` and `set_sessions_page`.
private func parseSessionItems(_ raw: Any?) -> [OverlaySessionItem] {
    let rawSessions = raw as? [[String: Any]] ?? []
    return rawSessions.map { item in
        OverlaySessionItem(
            id: item["id"] as? String ?? "",
            title: item["title"] as? String ?? "Bluey session",
            subtitle: item["subtitle"] as? String ?? "",
            isActive: item["is_active"] as? Bool ?? false,
            project: item["project"] as? String,
            updatedAt: item["updated_at"] as? String ?? "",
            turnCount: (item["turn_count"] as? NSNumber)?.intValue,
            pinned: item["pinned"] as? Bool ?? false)
    }.filter { !$0.id.isEmpty }
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
        return .setSessions(parseSessionItems(obj["sessions"]))
    case "set_sessions_page":
        return .setSessionsPage(
            sessions: parseSessionItems(obj["sessions"]),
            total: (obj["total"] as? NSNumber)?.intValue ?? 0,
            offset: (obj["offset"] as? NSNumber)?.intValue ?? 0,
            hasMore: obj["has_more"] as? Bool ?? false,
            query: obj["query"] as? String ?? "")
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
    case "set_agents":
        let rawAgents = obj["agents"] as? [[String: Any]] ?? []
        let agents = rawAgents.map { item in
            AgentSummary(
                kind: item["kind"] as? String ?? "",
                displayName: item["display_name"] as? String ?? "Coding agent",
                capability: item["capability"] as? String ?? "read_only",
                connectorCount: item["connector_count"] as? Int ?? 0,
                readyConnectorCount: item["ready_connector_count"] as? Int ?? 0,
                sessionCount: item["session_count"] as? Int,
                attached: item["attached"] as? Bool ?? false
            )
        }.filter { !$0.kind.isEmpty }
        return .setAgents(agents)
    case "set_agent_sessions":
        let kind = obj["kind"] as? String ?? ""
        let rawSessions = obj["sessions"] as? [[String: Any]] ?? []
        let sessions = rawSessions.map { item in
            AgentSessionSummary(
                id: item["id"] as? String ?? "",
                title: item["title"] as? String,
                updatedAt: item["updated_at"] as? String ?? "",
                project: item["project"] as? String
            )
        }.filter { !$0.id.isEmpty }
        return .setAgentSessions(kind: kind, sessions: sessions)
    case "set_agent_connectors":
        let kind = obj["kind"] as? String ?? ""
        let rawConnectors = obj["connectors"] as? [[String: Any]] ?? []
        let connectors = rawConnectors.map { item in
            AgentConnectorInfo(
                name: item["name"] as? String ?? "connector",
                authTier: item["auth_tier"] as? String ?? "none",
                ready: item["ready"] as? Bool ?? false
            )
        }
        return .setAgentConnectors(kind: kind, connectors: connectors)
    case "push_fix_proposal":
        // Defensive decode: an unknown/missing proposal_id is unusable (the
        // approve/reject echo is id-matched upstream), so drop the command
        // rather than render an un-actionable card. The diff is optional and
        // is omitted from the wire form when absent -> nil.
        guard let proposalId = obj["proposal_id"] as? String, !proposalId.isEmpty else {
            return .unknown(line)
        }
        let proposal = FixProposal(
            proposalId: proposalId,
            diagnosis: obj["diagnosis"] as? String ?? "",
            reasoning: obj["reasoning"] as? String ?? "",
            fix: obj["fix"] as? String ?? "",
            diff: obj["diff"] as? String,
            applySupported: obj["apply_supported"] as? Bool ?? false
        )
        return .pushFixProposal(proposal)
    case "push_billing_disclosure":
        // The vendor short id is the consent key echoed back on accept/decline;
        // without it the modal is un-actionable, so drop the command.
        guard let vendorShort = obj["vendor_short"] as? String, !vendorShort.isEmpty else {
            return .unknown(line)
        }
        let disclosure = BillingDisclosure(
            vendorShort: vendorShort,
            vendorDisplayName: obj["vendor_display_name"] as? String ?? vendorShort,
            billingModel: obj["billing_model"] as? String ?? "byot",
            disclosure: obj["disclosure"] as? String ?? "",
            pendingKind: obj["pending_kind"] as? String ?? "",
            pendingSessionId: obj["pending_session_id"] as? String)
        return .pushBillingDisclosure(disclosure)
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

// MARK: - History-at-scale emit helpers (redesign)
//
// Mirror OverlayEvent::SessionsRequested / SessionPinRequested /
// SessionUnpinRequested in crates/cue-core/src/overlay.rs. The daemon answers a
// SessionsRequested with SetSessionsPage.

private func emitSessionsRequested(offset: Int, limit: Int, search: String) {
    emitEvent([
        "type": "sessions_requested",
        "offset": offset,
        "limit": limit,
        "search": search,
    ])
}

private func emitSessionPinRequested(id: String) {
    emitEvent(["type": "session_pin_requested", "id": id])
}

private func emitSessionUnpinRequested(id: String) {
    emitEvent(["type": "session_unpin_requested", "id": id])
}

// MARK: - Secondary-action emit helpers (the "+" menu, redesign)
//
// `recap_requested` / `active_page_capture_requested` / `instructions_requested`
// are type-only OverlayEvents; the daemon owns the side effect (recap pipeline,
// browser-page capture, opening the answer-style editor round-trip).

private func emitRecapRequested() {
    emitSimple("recap_requested")
}

private func emitActivePageCaptureRequested() {
    emitSimple("active_page_capture_requested")
}

private func emitInstructionsRequested() {
    emitSimple("instructions_requested")
}

// MARK: - Agent-bridge emit helpers (Slice 5b)
//
// Each event is a dict tagged with "type", matching OverlayEvent's serde
// `snake_case` form in crates/cue-core/src/overlay.rs. Optional fields are
// omitted (not sent as null) so the daemon's `#[serde(default)]` applies.

private func emitAgentListRequested() {
    emitEvent(["type": "agent_list_requested"])
}

private func emitAgentAttachRequested(kind: String, sessionId: String?) {
    var p: [String: Any] = ["type": "agent_attach_requested", "kind": kind]
    if let sessionId, !sessionId.isEmpty { p["session_id"] = sessionId }
    emitEvent(p)
}

private func emitAgentDetachRequested() {
    emitEvent(["type": "agent_detach_requested"])
}

private func emitAgentSessionsRequested(
    kind: String,
    offset: Int = 0,
    limit: Int = 0,
    search: String = ""
) {
    emitEvent([
        "type": "agent_sessions_requested",
        "kind": kind,
        "offset": offset,
        "limit": limit,
        "search": search,
    ])
}

private func emitAgentConnectorsRequested(kind: String) {
    emitEvent(["type": "agent_connectors_requested", "kind": kind])
}

private func emitConnectorReauthRequested(kind: String, name: String) {
    emitEvent(["type": "connector_reauth_requested", "kind": kind, "name": name])
}

// MARK: - Fix-button emit helpers (Slice F4)
//
// Mirror OverlayEvent::FixRequested / FixApprovalResponded. `card_id` is
// omitted (not null) when nil so the daemon's `#[serde(default)]` applies.

private func emitFixRequested(cardId: String?, question: String) {
    var p: [String: Any] = ["type": "fix_requested", "question": question]
    if let cardId, !cardId.isEmpty { p["card_id"] = cardId }
    emitEvent(p)
}

private func emitFixApprovalResponded(proposalId: String, approved: Bool) {
    emitEvent([
        "type": "fix_approval_responded",
        "proposal_id": proposalId,
        "approved": approved,
    ])
}

// MARK: - BYOT billing-disclosure emit helper (G4, redesign)
//
// Mirror OverlayEvent::BillingDisclosureResponded. The daemon resumes the
// pending attach (pending_kind / pending_session_id) on accept and records the
// vendor in accepted_byot_vendors; on decline it discards the pending attach.

private func emitBillingDisclosureResponded(
    vendorShort: String,
    accepted: Bool,
    pendingKind: String,
    pendingSessionId: String?
) {
    var p: [String: Any] = [
        "type": "billing_disclosure_responded",
        "vendor_short": vendorShort,
        "accepted": accepted,
        "pending_kind": pendingKind,
    ]
    if let pendingSessionId, !pendingSessionId.isEmpty {
        p["pending_session_id"] = pendingSessionId
    }
    emitEvent(p)
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
        guard let hit = super.hitTest(point) else { return self }
        if hit === self { return self }
        return preservesHeaderHit(for: hit) ? hit : self
    }

    override func mouseDown(with event: NSEvent) {
        window?.performDrag(with: event)
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

/// A history session row that exposes Rename / Delete via a right-click menu —
/// the redesigned rows are clean (pin + time only) like the mockup, so these
/// must-survive session actions live in a contextual menu instead of inline
/// buttons. The row stores its session id; the menu items carry it as `tag` is
/// index-based elsewhere, so we stash the id directly on the menu items.
private final class SessionRowView: NSView {
    var sessionId = ""
    weak var rowMenuTarget: AnyObject?
    var renameAction: Selector?
    var deleteAction: Selector?

    override func menu(for event: NSEvent) -> NSMenu? {
        let menu = NSMenu()
        if let renameAction {
            let item = NSMenuItem(title: "Rename", action: renameAction, keyEquivalent: "")
            item.target = rowMenuTarget
            item.representedObject = sessionId
            menu.addItem(item)
        }
        if let deleteAction {
            let item = NSMenuItem(title: "Delete", action: deleteAction, keyEquivalent: "")
            item.target = rowMenuTarget
            item.representedObject = sessionId
            menu.addItem(item)
        }
        return menu.items.isEmpty ? nil : menu
    }
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

    /// Small cyan agent glyph shown at the pill's trailing edge while a coding
    /// agent is attached. Glyph-only — no text, so the pill never truncates
    /// (agent-bridge Slice 5b, PLAN §9 surface 4).
    var agentAttached: Bool = false {
        didSet {
            guard agentAttached != oldValue else { return }
            agentGlyph.isHidden = !agentAttached
            needsLayout = true
        }
    }

    private let logoMark = BlueyLogoView()
    private let wordmarkView = BlueyWordmarkView()
    private let dotView = NSView()
    private let controlRail = NSView()
    private let styleButton = NSButton(title: "", target: nil, action: nil)
    private let runButton = NSButton(title: "", target: nil, action: nil)
    private let endButton = NSButton(title: "", target: nil, action: nil)
    private let agentGlyph = NSImageView()
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

        // Agent-bridge: trailing-edge agent badge, hidden until an agent attaches.
        agentGlyph.wantsLayer = true
        agentGlyph.isHidden = true
        agentGlyph.imageScaling = .scaleProportionallyDown
        agentGlyph.contentTintColor = BlueyTheme.cyan
        agentGlyph.layer?.backgroundColor = BlueyTheme.cyan.withAlphaComponent(0.16).cgColor
        agentGlyph.layer?.cornerRadius = 9
        agentGlyph.layer?.borderWidth = 1
        agentGlyph.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.55).cgColor
        agentGlyph.toolTip = "A coding agent is attached"
        if let image = symbolImage("cpu") {
            image.isTemplate = true
            agentGlyph.image = image
        }
        addSubview(agentGlyph)
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

        // Agent-bridge: glyph-only agent badge tucked just left of the control
        // rail when an agent is attached, so the pill never truncates.
        let glyphSide: CGFloat = 18
        if agentAttached {
            let glyphX = controlRail.frame.minX - glyphSide - 6
            agentGlyph.frame = NSRect(
                x: glyphX, y: (bounds.height - glyphSide) / 2,
                width: glyphSide, height: glyphSide)
            agentGlyph.layer?.cornerRadius = glyphSide / 2
        }

        wordmarkView.frame = NSRect(x: 36, y: (bounds.height - 18) / 2 + 1, width: 50, height: 18)
        let dotSize: CGFloat = 7
        let trailingLimit = agentAttached ? agentGlyph.frame.minX - 6 : controlRail.frame.minX - 7
        let dotX = min(wordmarkView.frame.maxX + 2, trailingLimit - dotSize)
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

/// The user's one-shot decision on a Fix proposal card (Slice F4). `pending`
/// shows Approve/Reject; the terminal states disable both buttons so a proposal
/// can never be double-submitted.
private enum FixProposalState: Equatable {
    case pending
    case applying
    case discarded
}

private struct RenderedCard {
    let id: String
    let kind: String
    let title: String
    var body: String
    var done: Bool
    var costLabel: String?
    var artifact: OverlayArtifact?
    /// Free-form provenance from CueCard.source (e.g. "claude_code agent").
    /// When an answer card's source names a coding agent, the role badge and
    /// status reflect that agent instead of BLUEY (agent-bridge Slice 5b).
    var source: String?
    /// Set only for kind == "fix_proposal" cards (Slice F4): the proposal
    /// payload plus the user's pending/applying/discarded decision.
    var fixProposal: FixProposal? = nil
    var fixState: FixProposalState = .pending
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
    /// Fired when the user taps **Fix** on an agent answer card (Slice F4).
    /// Carries the source card id + its body text (the problem to fix).
    var onFixRequested: ((_ cardId: String, _ question: String) -> Void)?
    /// Fired whenever the rendered card set changes, so the panel can recompute
    /// the context bar ("transcript · N screen · N turns").
    var onContentChanged: (() -> Void)?
    /// Interactive controls inside cards (Fix / Approve / Reject buttons). The
    /// feed/workspace region is normally click-through; the panel consults
    /// `hasInteractiveControl(at:)` so only these button frames capture the
    /// mouse, leaving the rest of the feed transparent to the app underneath.
    private let interactiveControls = NSHashTable<NSView>.weakObjects()

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        // The feed floats ON the panel's dark aurora surface — it is fully
        // transparent (no fill, no border). Readability comes from the dark
        // panel behind it, exactly like the mockup `.feed`.
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor

        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 0
        // `.feed` padding 16/16/8.
        stack.edgeInsets = NSEdgeInsets(top: 16, left: 16, bottom: 8, right: 16)
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

    /// Register a button so the panel's pass-through tracking treats its frame
    /// as interactive. Enabled state is re-checked live at hit-test time, so a
    /// disabled (already-submitted) button stops capturing the mouse.
    private func registerInteractive(_ control: NSView) {
        interactiveControls.add(control)
    }

    /// True when `point` (in FeedView coordinates) lands on an enabled card
    /// button. Used by ExpandedPanelView.isInteractiveAtScreenPoint so card
    /// affordances are clickable without making the whole feed opaque to mouse
    /// events. Disabled buttons (terminal proposal states) return false.
    func hasInteractiveControl(at point: NSPoint) -> Bool {
        guard let hit = hitTest(point) else { return false }
        var node: NSView? = hit
        while let current = node {
            if interactiveControls.contains(current) {
                if let control = current as? NSControl { return control.isEnabled }
                return true
            }
            node = current.superview
        }
        return false
    }

    /// Counts used by the panel's context bar.
    var transcriptTurnCount: Int { cards.filter { normalizedCardKind($0.kind) == "transcript" }.count }
    var screenTurnCount: Int { cards.filter { isScreenCard($0) }.count }
    var conversationTurnCount: Int {
        cards.filter {
            let k = normalizedCardKind($0.kind)
            return k == "question" || k == "answer"
        }.count
    }
    var hasCards: Bool { !cards.isEmpty }

    func push(_ card: RenderedCard) {
        // Transcripts now flow INTO the thread as HEARD turns (the continuous
        // timeline). We still notify the panel so it can update audio chrome.
        if normalizedCardKind(card.kind) == "transcript" {
            onTranscript?(card)
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
        onContentChanged?()
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
        rebuildSubview(at: idx)
        scrollToBottom()
        onContentChanged?()
        return cards[idx]
    }

    func clear() {
        removeAllCards()
        emptyState.isHidden = false
        onContentChanged?()
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
        // The feed is transparent; nothing to dim. Kept for API parity.
    }

    private func removeAllCards() {
        cards.removeAll()
        interactiveControls.removeAllObjects()
        for view in stack.arrangedSubviews {
            stack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
    }

    /// Rebuild a single turn subview in place (used by streaming updates + fix
    /// state transitions), preserving the connector-line "last turn" logic.
    private func rebuildSubview(at idx: Int) {
        guard idx >= 0, idx < stack.arrangedSubviews.count else { return }
        let existing = stack.arrangedSubviews[idx]
        stack.removeArrangedSubview(existing)
        existing.removeFromSuperview()
        let view = makeCardView(cards[idx])
        stack.insertArrangedSubview(view, at: idx)
        view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
    }

    /// Transition a Fix proposal card to a terminal/transient state (Slice F4)
    /// and rebuild just its subview so the buttons reflect the new state. The
    /// proposal id doubles as the card id, so we match on it directly.
    private func setFixState(proposalId: String, to state: FixProposalState) {
        guard let idx = cards.firstIndex(where: {
            $0.kind == "fix_proposal" && $0.id == proposalId
        }) else { return }
        guard case .pending = cards[idx].fixState else { return } // one-shot
        cards[idx].fixState = state
        rebuildSubview(at: idx)
    }

    private func configureEmptyState() {
        emptyState.translatesAutoresizingMaskIntoConstraints = false
        emptyState.wantsLayer = true
        emptyState.layer?.backgroundColor = NSColor.clear.cgColor
        addSubview(emptyState)

        let badge = trackedLabel("BLUEY", size: 10.5, weight: .heavy, color: Tok.accentTx, tracking: 0.6)
        badge.alignment = .center

        let title = NSTextField(labelWithString: "Ask, listen, or share a screen")
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = Tok.font(18, .semibold)
        title.textColor = Tok.tx1
        title.alignment = .center

        let subtitle = NSTextField(labelWithString: "What's said, the screen you share, and your questions all flow into one thread.")
        subtitle.translatesAutoresizingMaskIntoConstraints = false
        subtitle.font = Tok.font(12.5, .regular)
        subtitle.textColor = Tok.tx3
        subtitle.alignment = .center
        subtitle.maximumNumberOfLines = 2
        subtitle.lineBreakMode = .byWordWrapping

        emptyState.addSubview(badge)
        emptyState.addSubview(title)
        emptyState.addSubview(subtitle)

        NSLayoutConstraint.activate([
            emptyState.centerXAnchor.constraint(equalTo: centerXAnchor),
            emptyState.centerYAnchor.constraint(equalTo: centerYAnchor, constant: -12),
            emptyState.widthAnchor.constraint(lessThanOrEqualTo: widthAnchor, multiplier: 0.78),

            badge.topAnchor.constraint(equalTo: emptyState.topAnchor),
            badge.centerXAnchor.constraint(equalTo: emptyState.centerXAnchor),

            title.topAnchor.constraint(equalTo: badge.bottomAnchor, constant: 8),
            title.leadingAnchor.constraint(equalTo: emptyState.leadingAnchor),
            title.trailingAnchor.constraint(equalTo: emptyState.trailingAnchor),

            subtitle.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 8),
            subtitle.leadingAnchor.constraint(equalTo: emptyState.leadingAnchor),
            subtitle.trailingAnchor.constraint(equalTo: emptyState.trailingAnchor),
            subtitle.bottomAnchor.constraint(equalTo: emptyState.bottomAnchor),
        ])
    }

    // MARK: - Turn rendering (the continuous timeline thread)

    /// A turn kind drives the rail icon/color, label, and body styling.
    private enum TurnKind {
        case heard      // transcript → "INTERVIEWER · HEARD", italic
        case screen     // screen-capture answer → "SCREEN · ⌥S" + capture chip
        case you        // your question → "YOU · ASKED"
        case bluey      // an answer → "BLUEY" (or agent label)
        case note       // context/decision/action/warning/system → its own label
    }

    private func turnKind(for card: RenderedCard) -> TurnKind {
        switch normalizedCardKind(card.kind) {
        case "transcript": return .heard
        case "question":   return .you
        case "answer":     return isScreenCard(card) ? .screen : .bluey
        case "context", "decision", "action_item", "warning", "system":
            return .note
        default:           return .bluey
        }
    }

    /// An answer is a "screen" turn when its provenance names a screen/vision
    /// capture (the ⌥S path). The daemon stamps `source` on screen answers.
    private func isScreenCard(_ card: RenderedCard) -> Bool {
        guard normalizedCardKind(card.kind) == "answer" else { return false }
        let src = (card.source ?? "").lowercased()
        if src.contains("screen") || src.contains("vision") || src.contains("capture") {
            return true
        }
        let lower = card.body.lowercased()
        return lower.contains("from the shared screen") || lower.contains("captured from")
    }

    private func makeCardView(_ card: RenderedCard) -> NSView {
        // Sign-in cards keep their dedicated centered call-to-action card.
        if normalizedCardKind(card.kind) == "system", loginURL(from: card) != nil {
            return makeSignInTurn(card)
        }
        // Review-gated Fix proposals render as their own `.fix` card, inside a
        // bluey turn rail.
        if normalizedCardKind(card.kind) == "fix_proposal", let proposal = card.fixProposal {
            return makeTurn(card: card, kind: .bluey) { container in
                let fixView = self.makeFixProposalView(proposal, state: card.fixState)
                self.pin(fixView, in: container)
            }
        }
        let kind = turnKind(for: card)
        return makeTurn(card: card, kind: kind) { body in
            self.makeTurnBody(card: card, kind: kind, into: body)
        }
    }

    /// Build the `.turn` scaffold: a 16×16 rail icon at the left, a vertical
    /// hairline connector down to the next turn, a label row, then the body.
    private func makeTurn(card: RenderedCard, kind: TurnKind, body buildBody: (NSView) -> Void) -> NSView {
        let turn = NSView()
        turn.translatesAutoresizingMaskIntoConstraints = false

        // Rail icon chip.
        let rail = NSView()
        rail.translatesAutoresizingMaskIntoConstraints = false
        rail.wantsLayer = true
        rail.layer?.cornerRadius = 5
        let (railFill, railTint, symbol) = railStyle(kind)
        rail.layer?.backgroundColor = railFill.cgColor
        let icon = NSImageView()
        icon.translatesAutoresizingMaskIntoConstraints = false
        if let image = symbolImage(symbol) {
            image.isTemplate = true
            icon.image = image
        }
        icon.contentTintColor = railTint
        icon.imageScaling = .scaleProportionallyDown
        rail.addSubview(icon)

        // Connector line (hidden on the last turn — toggled after layout via
        // the stack's last-arranged check in scrollToBottom()).
        let connector = NSView()
        connector.translatesAutoresizingMaskIntoConstraints = false
        connector.wantsLayer = true
        connector.layer?.backgroundColor = Tok.hairline.cgColor
        connector.identifier = NSUserInterfaceItemIdentifier("turn-connector")

        // Label row.
        let labelRow = NSView()
        labelRow.translatesAutoresizingMaskIntoConstraints = false
        let (labelText, labelColor) = turnLabel(card: card, kind: kind)
        let label = trackedLabel(labelText, size: 10, weight: .heavy, color: labelColor, tracking: 0.5)
        let time = trackedLabel(turnTime(card), size: 10, weight: .regular, color: Tok.tx4, tracking: 0)
        time.alignment = .right
        labelRow.addSubview(label)
        labelRow.addSubview(time)

        // Body container.
        let bodyContainer = NSView()
        bodyContainer.translatesAutoresizingMaskIntoConstraints = false
        buildBody(bodyContainer)

        turn.addSubview(rail)
        turn.addSubview(connector)
        turn.addSubview(labelRow)
        turn.addSubview(bodyContainer)

        NSLayoutConstraint.activate([
            // `.turn` padding-left 26; rail at left 8 top 3, 16×16.
            rail.leadingAnchor.constraint(equalTo: turn.leadingAnchor, constant: 0),
            rail.topAnchor.constraint(equalTo: turn.topAnchor, constant: 1),
            rail.widthAnchor.constraint(equalToConstant: 16),
            rail.heightAnchor.constraint(equalToConstant: 16),
            icon.centerXAnchor.constraint(equalTo: rail.centerXAnchor),
            icon.centerYAnchor.constraint(equalTo: rail.centerYAnchor),
            icon.widthAnchor.constraint(equalToConstant: 10),
            icon.heightAnchor.constraint(equalToConstant: 10),

            connector.centerXAnchor.constraint(equalTo: rail.centerXAnchor),
            connector.topAnchor.constraint(equalTo: rail.bottomAnchor, constant: 4),
            connector.widthAnchor.constraint(equalToConstant: 1),
            connector.bottomAnchor.constraint(equalTo: turn.bottomAnchor, constant: 0),

            labelRow.leadingAnchor.constraint(equalTo: turn.leadingAnchor, constant: 26),
            labelRow.trailingAnchor.constraint(equalTo: turn.trailingAnchor),
            labelRow.topAnchor.constraint(equalTo: turn.topAnchor),
            labelRow.heightAnchor.constraint(equalToConstant: 14),

            label.leadingAnchor.constraint(equalTo: labelRow.leadingAnchor),
            label.centerYAnchor.constraint(equalTo: labelRow.centerYAnchor),
            label.trailingAnchor.constraint(lessThanOrEqualTo: time.leadingAnchor, constant: -8),
            time.trailingAnchor.constraint(equalTo: labelRow.trailingAnchor),
            time.centerYAnchor.constraint(equalTo: labelRow.centerYAnchor),

            bodyContainer.leadingAnchor.constraint(equalTo: turn.leadingAnchor, constant: 26),
            bodyContainer.trailingAnchor.constraint(equalTo: turn.trailingAnchor),
            bodyContainer.topAnchor.constraint(equalTo: labelRow.bottomAnchor, constant: 3),
            // `.turn` margin-bottom 15.
            bodyContainer.bottomAnchor.constraint(equalTo: turn.bottomAnchor, constant: -15),
        ])
        return turn
    }

    private func railStyle(_ kind: TurnKind) -> (fill: NSColor, tint: NSColor, symbol: String) {
        switch kind {
        case .heard:
            return (NSColor.white.withAlphaComponent(0.07), Tok.tx3, "waveform")
        case .screen:
            return (Tok.accentBg, Tok.accentTx, "rectangle.on.rectangle")
        case .you:
            return (Tok.glassHi, Tok.tx2, "person")
        case .bluey:
            return (Tok.accentBg, Tok.accentTx, "sparkle")
        case .note:
            return (Tok.glassHi, Tok.tx3, "doc.text")
        }
    }

    private func turnLabel(card: RenderedCard, kind: TurnKind) -> (String, NSColor) {
        switch kind {
        case .heard:
            let who = audioWho(card.source)
            return ("\(who) · HEARD", Tok.tx3)
        case .screen:
            return ("SCREEN · ⌥S", Tok.accentTx)
        case .you:
            return ("YOU · ASKED", Tok.tx2)
        case .bluey:
            let label = agentLabel(from: card.source) ?? "BLUEY"
            return (label, Tok.accentTx)
        case .note:
            return (noteLabel(card), Tok.tx3)
        }
    }

    private func audioWho(_ source: String?) -> String {
        let lower = (source ?? "").lowercased()
        if lower.contains("system") { return "SCREEN AUDIO" }
        if lower.contains("mic") || lower.contains("microphone") { return "YOU" }
        return "HEARD"
    }

    private func noteLabel(_ card: RenderedCard) -> String {
        switch normalizedCardKind(card.kind) {
        case "context":     return "CONTEXT"
        case "decision":    return "DECISION"
        case "action_item": return "ACTION"
        case "warning":     return "WARNING"
        case "system":      return "BLUEY"
        default:            return "NOTE"
        }
    }

    private func turnTime(_ card: RenderedCard) -> String {
        if !card.done { return "…" }
        if let cost = card.costLabel, !cost.isEmpty { return cost }
        return ""
    }

    /// Build the body for a turn (everything to the right of the rail, below the
    /// label row), branching on the turn kind.
    private func makeTurnBody(card: RenderedCard, kind: TurnKind, into container: NSView) {
        switch kind {
        case .heard:
            let text = displayTranscriptText(card.body)
            let body = makeBodyLabel(text.isEmpty ? "…" : "“\(text)”", font: NSFontManager.shared.convert(Tok.font(12.5, .regular), toHaveTrait: .italicFontMask), color: Tok.tx2)
            pin(body, in: container)

        case .you:
            let body = makeBodyLabel(card.body, font: Tok.font(13, .regular), color: Tok.tx1)
            pin(body, in: container)

        case .screen:
            // Capture chip, then the answer body.
            let chip = makeCaptureChip(card)
            let rawBody = card.body.isEmpty && !card.done ? "Reading the shared screen…" : chatBody(for: card, rawBody: card.body)
            let body = makeAnswerLabel(rawBody, baseSize: 13.5)
            container.addSubview(chip)
            container.addSubview(body)
            chip.translatesAutoresizingMaskIntoConstraints = false
            body.translatesAutoresizingMaskIntoConstraints = false
            NSLayoutConstraint.activate([
                chip.topAnchor.constraint(equalTo: container.topAnchor),
                chip.leadingAnchor.constraint(equalTo: container.leadingAnchor),
                chip.trailingAnchor.constraint(lessThanOrEqualTo: container.trailingAnchor),
                body.topAnchor.constraint(equalTo: chip.bottomAnchor, constant: 7),
                body.leadingAnchor.constraint(equalTo: container.leadingAnchor),
                body.trailingAnchor.constraint(equalTo: container.trailingAnchor),
                body.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            ])
            maybeAttachFix(card: card, to: container, below: body)

        case .bluey:
            let rawBody = card.body.isEmpty && !card.done ? "Thinking…" : chatBody(for: card, rawBody: card.body)
            let body = makeAnswerLabel(rawBody, baseSize: 13.5)
            pin(body, in: container, allowFix: card)

        case .note:
            let title = card.title.trimmingCharacters(in: .whitespacesAndNewlines)
            let bodyText = card.body.trimmingCharacters(in: .whitespacesAndNewlines)
            let combined = title.isEmpty ? bodyText : (bodyText.isEmpty ? title : "\(title)\n\(bodyText)")
            let body = makeBodyLabel(combined.isEmpty ? "—" : combined, font: Tok.font(13, .regular), color: Tok.tx1)
            pin(body, in: container)
        }
    }

    private func pin(_ view: NSView, in container: NSView, allowFix card: RenderedCard? = nil) {
        container.addSubview(view)
        view.translatesAutoresizingMaskIntoConstraints = false
        let bottom = view.bottomAnchor.constraint(equalTo: container.bottomAnchor)
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: container.topAnchor),
            view.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: container.trailingAnchor),
        ])
        if let card { maybeAttachFix(card: card, to: container, below: view, bottomToDeactivate: bottom) }
        else { bottom.isActive = true }
    }

    /// Attach a compact "Fix" affordance under a final agent answer (Slice F4).
    private func maybeAttachFix(card: RenderedCard, to container: NSView, below anchorView: NSView, bottomToDeactivate: NSLayoutConstraint? = nil) {
        let answerLike = normalizedCardKind(card.kind) == "answer"
        let showFix = answerLike && card.done && agentLabel(from: card.source) != nil
        guard showFix else {
            (bottomToDeactivate ?? anchorView.bottomAnchor.constraint(equalTo: container.bottomAnchor)).isActive = true
            return
        }
        bottomToDeactivate?.isActive = false
        let fix = NSButton(title: "Fix", target: self, action: #selector(fixButtonClicked(_:)))
        fix.translatesAutoresizingMaskIntoConstraints = false
        fix.identifier = NSUserInterfaceItemIdentifier(card.id)
        fix.toolTip = "Ask your agent to propose a fix for this answer"
        styleFixButton(fix)
        container.addSubview(fix)
        registerInteractive(fix)
        NSLayoutConstraint.activate([
            fix.topAnchor.constraint(equalTo: anchorView.bottomAnchor, constant: 8),
            fix.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            fix.heightAnchor.constraint(equalToConstant: 26),
            fix.widthAnchor.constraint(greaterThanOrEqualToConstant: 64),
            fix.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])
    }

    /// `.scap` capture chip: a 46×30 thumbnail + a two-line caption.
    private func makeCaptureChip(_ card: RenderedCard) -> NSView {
        let chip = NSView()
        chip.wantsLayer = true
        chip.layer?.backgroundColor = Tok.glassHi.cgColor
        chip.layer?.cornerRadius = 9
        chip.layer?.borderWidth = 1
        chip.layer?.borderColor = Tok.hairline.cgColor

        let thumb = NSView()
        thumb.translatesAutoresizingMaskIntoConstraints = false
        thumb.wantsLayer = true
        thumb.layer?.backgroundColor = NSColor(red: 0.47, green: 0.55, blue: 0.78, alpha: 0.16).cgColor
        thumb.layer?.cornerRadius = 5
        thumb.layer?.borderWidth = 1
        thumb.layer?.borderColor = Tok.hairline.cgColor
        let thumbIcon = NSImageView()
        thumbIcon.translatesAutoresizingMaskIntoConstraints = false
        if let image = symbolImage("viewfinder") {
            image.isTemplate = true
            thumbIcon.image = image
        }
        thumbIcon.contentTintColor = Tok.tx3
        thumb.addSubview(thumbIcon)

        let titleText = card.title.trimmingCharacters(in: .whitespacesAndNewlines)
        let title = NSTextField(labelWithString: titleText.isEmpty ? "Shared screen" : titleText)
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = Tok.font(12, .semibold)
        title.textColor = Tok.tx1
        title.lineBreakMode = .byTruncatingTail

        let sub = NSTextField(labelWithString: "captured from shared screen")
        sub.translatesAutoresizingMaskIntoConstraints = false
        sub.font = Tok.font(11.5, .regular)
        sub.textColor = Tok.tx2

        chip.addSubview(thumb)
        chip.addSubview(title)
        chip.addSubview(sub)
        NSLayoutConstraint.activate([
            chip.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),
            thumb.leadingAnchor.constraint(equalTo: chip.leadingAnchor, constant: 10),
            thumb.centerYAnchor.constraint(equalTo: chip.centerYAnchor),
            thumb.widthAnchor.constraint(equalToConstant: 46),
            thumb.heightAnchor.constraint(equalToConstant: 30),
            thumbIcon.centerXAnchor.constraint(equalTo: thumb.centerXAnchor),
            thumbIcon.centerYAnchor.constraint(equalTo: thumb.centerYAnchor),
            thumbIcon.widthAnchor.constraint(equalToConstant: 14),
            thumbIcon.heightAnchor.constraint(equalToConstant: 14),

            title.leadingAnchor.constraint(equalTo: thumb.trailingAnchor, constant: 9),
            title.topAnchor.constraint(equalTo: chip.topAnchor, constant: 7),
            title.trailingAnchor.constraint(equalTo: chip.trailingAnchor, constant: -10),
            sub.leadingAnchor.constraint(equalTo: title.leadingAnchor),
            sub.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 1),
            sub.trailingAnchor.constraint(equalTo: chip.trailingAnchor, constant: -10),
            sub.bottomAnchor.constraint(equalTo: chip.bottomAnchor, constant: -7),
        ])
        return chip
    }

    private func makeBodyLabel(_ text: String, font: NSFont, color: NSColor) -> NSTextField {
        let label = NSTextField(wrappingLabelWithString: text)
        label.translatesAutoresizingMaskIntoConstraints = false
        label.font = font
        label.textColor = color
        label.alignment = .left
        label.preferredMaxLayoutWidth = 470
        return label
    }

    /// An answer label that lightly renders **bold** spans and `code` spans
    /// (the mockup's `.ans2 b` 600 + `.code` mono tint) on top of the body text.
    private func makeAnswerLabel(_ text: String, baseSize: CGFloat) -> NSTextField {
        let label = NSTextField(wrappingLabelWithString: "")
        label.translatesAutoresizingMaskIntoConstraints = false
        label.attributedStringValue = attributedAnswer(text, baseSize: baseSize)
        label.isSelectable = true
        label.preferredMaxLayoutWidth = 470
        return label
    }

    /// Render simple Markdown emphasis: `**bold**` → weight 600, `` `code` `` →
    /// monospaced tinted span. Anything else is plain `tx-1`.
    private func attributedAnswer(_ text: String, baseSize: CGFloat) -> NSAttributedString {
        let base = Tok.font(baseSize, .regular)
        let bold = Tok.font(baseSize, .semibold)
        let mono = Tok.mono(baseSize - 1.5, .regular)
        let result = NSMutableAttributedString()

        func appendPlain(_ chunk: String) {
            // Within a plain chunk, also pull out `code` spans.
            var rest = Substring(chunk)
            while let open = rest.firstIndex(of: "`") {
                let before = String(rest[rest.startIndex..<open])
                if !before.isEmpty {
                    result.append(NSAttributedString(string: before, attributes: [.font: base, .foregroundColor: Tok.tx1]))
                }
                let afterOpen = rest.index(after: open)
                if let close = rest[afterOpen...].firstIndex(of: "`") {
                    let code = String(rest[afterOpen..<close])
                    result.append(NSAttributedString(string: code, attributes: [.font: mono, .foregroundColor: Tok.codeTx]))
                    rest = rest[rest.index(after: close)...]
                } else {
                    result.append(NSAttributedString(string: String(rest[open...]), attributes: [.font: base, .foregroundColor: Tok.tx1]))
                    rest = rest[rest.endIndex...]
                }
            }
            if !rest.isEmpty {
                result.append(NSAttributedString(string: String(rest), attributes: [.font: base, .foregroundColor: Tok.tx1]))
            }
        }

        // Split on **bold** spans first.
        let parts = text.components(separatedBy: "**")
        for (idx, part) in parts.enumerated() {
            if idx % 2 == 1 {
                result.append(NSAttributedString(string: part, attributes: [.font: bold, .foregroundColor: Tok.tx1]))
            } else {
                appendPlain(part)
            }
        }
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineSpacing = 2
        result.addAttribute(.paragraphStyle, value: paragraph, range: NSRange(location: 0, length: result.length))
        return result
    }

    /// Sign-in turn: a centered accent call-to-action card with an Open login
    /// button (must-survive feature).
    private func makeSignInTurn(_ card: RenderedCard) -> NSView {
        let turn = NSView()
        turn.translatesAutoresizingMaskIntoConstraints = false

        let cardView = NSView()
        cardView.translatesAutoresizingMaskIntoConstraints = false
        cardView.wantsLayer = true
        cardView.layer?.backgroundColor = Tok.accentBg.cgColor
        cardView.layer?.cornerRadius = Tok.rLg
        cardView.layer?.borderWidth = 1
        cardView.layer?.borderColor = Tok.accentBgStrong.cgColor

        let title = NSTextField(labelWithString: card.title.isEmpty ? "Sign in to Bluey" : card.title)
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = Tok.font(15, .semibold)
        title.textColor = Tok.tx1
        title.alignment = .center

        let body = NSTextField(wrappingLabelWithString: signInBody(from: card.body))
        body.translatesAutoresizingMaskIntoConstraints = false
        body.font = Tok.font(12.5, .regular)
        body.textColor = Tok.tx2
        body.alignment = .center
        body.preferredMaxLayoutWidth = 360

        let url = loginURL(from: card)
        let button = NSButton(title: "Open login", target: self, action: #selector(openURLButtonClicked(_:)))
        button.translatesAutoresizingMaskIntoConstraints = false
        if let url { button.identifier = NSUserInterfaceItemIdentifier(url.absoluteString) }
        styleSignInButton(button)
        registerInteractive(button)

        turn.addSubview(cardView)
        cardView.addSubview(title)
        cardView.addSubview(body)
        cardView.addSubview(button)
        NSLayoutConstraint.activate([
            cardView.topAnchor.constraint(equalTo: turn.topAnchor),
            cardView.bottomAnchor.constraint(equalTo: turn.bottomAnchor, constant: -15),
            cardView.leadingAnchor.constraint(equalTo: turn.leadingAnchor),
            cardView.trailingAnchor.constraint(equalTo: turn.trailingAnchor),

            title.topAnchor.constraint(equalTo: cardView.topAnchor, constant: 16),
            title.leadingAnchor.constraint(equalTo: cardView.leadingAnchor, constant: 16),
            title.trailingAnchor.constraint(equalTo: cardView.trailingAnchor, constant: -16),

            body.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 10),
            body.leadingAnchor.constraint(equalTo: cardView.leadingAnchor, constant: 20),
            body.trailingAnchor.constraint(equalTo: cardView.trailingAnchor, constant: -20),

            button.topAnchor.constraint(equalTo: body.bottomAnchor, constant: 14),
            button.centerXAnchor.constraint(equalTo: cardView.centerXAnchor),
            button.bottomAnchor.constraint(equalTo: cardView.bottomAnchor, constant: -16),
            button.widthAnchor.constraint(equalToConstant: 150),
            button.heightAnchor.constraint(equalToConstant: 38),
        ])
        return turn
    }

    // MARK: Copy / sign-in / fix button styling (preserved, restyled to tokens)

    func makeCopyCardButton(text: String, rightAligned: Bool) -> CopyCardButton {
        let button = CopyCardButton(title: "", target: self, action: #selector(copyCardClicked(_:)))
        button.translatesAutoresizingMaskIntoConstraints = false
        button.copyText = text
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 8
        button.layer?.backgroundColor = Tok.glassHi.cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = Tok.hairline.cgColor
        button.contentTintColor = Tok.tx3
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
        button.layer?.cornerRadius = 11
        button.layer?.backgroundColor = Tok.accent.cgColor
        button.font = Tok.font(12.5, .semibold)
        button.attributedTitle = NSAttributedString(
            string: "Open login",
            attributes: [
                .font: Tok.font(12.5, .semibold),
                .foregroundColor: NSColor.white,
            ])
        button.contentTintColor = .white
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

    /// Small accent pill used for the per-answer **Fix** button.
    private func styleFixButton(_ button: NSButton) {
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 9
        button.layer?.backgroundColor = Tok.accentBg.cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = Tok.accentBgStrong.cgColor
        button.font = Tok.font(11, .semibold)
        button.contentTintColor = Tok.accentTx
        if let image = symbolImage("wrench.and.screwdriver") {
            image.isTemplate = true
            button.image = image
            button.imagePosition = .imageLeading
            button.imageHugsTitle = true
            button.imageScaling = .scaleProportionallyDown
        }
        button.attributedTitle = NSAttributedString(
            string: "Fix",
            attributes: [
                .font: Tok.font(11, .semibold),
                .foregroundColor: Tok.accentTx,
            ])
        button.alignment = .center
    }

    @objc private func fixButtonClicked(_ sender: NSButton) {
        guard let cardId = sender.identifier?.rawValue,
              let card = cards.first(where: { $0.id == cardId })
        else { return }
        onFixRequested?(cardId, card.body)
    }

    // MARK: Fix proposal card (Slice F4) — `.fix` card, restyled to tokens

    private func makeFixProposalView(_ proposal: FixProposal, state: FixProposalState) -> NSView {
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false

        let card = NSView()
        card.wantsLayer = true
        // `.fix` — warn-tinted card.
        card.layer?.backgroundColor = NSColor(red: 0.890, green: 0.663, blue: 0.247, alpha: 0.05).cgColor
        card.layer?.cornerRadius = Tok.rLg
        card.layer?.borderWidth = 1
        card.layer?.borderColor = Tok.warn.withAlphaComponent(0.30).cgColor
        card.translatesAutoresizingMaskIntoConstraints = false

        let header = NSView()
        header.translatesAutoresizingMaskIntoConstraints = false
        let headerIcon = NSImageView()
        headerIcon.translatesAutoresizingMaskIntoConstraints = false
        if let image = symbolImage("wand.and.stars") {
            image.isTemplate = true
            headerIcon.image = image
        }
        headerIcon.contentTintColor = Tok.warn
        let metaLabel = trackedLabel("PROPOSED FIX", size: 10.5, weight: .heavy, color: Tok.warn, tracking: 0.5)
        let stateChip = NSTextField(labelWithString: fixStateBadge(state))
        stateChip.translatesAutoresizingMaskIntoConstraints = false
        stateChip.font = Tok.font(10, .regular)
        stateChip.textColor = Tok.tx3
        stateChip.wantsLayer = true
        stateChip.layer?.borderWidth = 1
        stateChip.layer?.borderColor = Tok.hairline.cgColor
        stateChip.layer?.cornerRadius = 9
        useCenteredSingleLineCell(stateChip)
        header.addSubview(headerIcon)
        header.addSubview(metaLabel)
        header.addSubview(stateChip)

        let content = NSStackView()
        content.orientation = .vertical
        content.alignment = .leading
        content.spacing = 9
        content.translatesAutoresizingMaskIntoConstraints = false
        func addFullWidth(_ view: NSView) {
            content.addArrangedSubview(view)
            view.widthAnchor.constraint(equalTo: content.widthAnchor).isActive = true
        }
        addFullWidth(makeFixSection(title: "DIAGNOSIS", body: proposal.diagnosis))
        if !proposal.reasoning.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            addFullWidth(makeFixSection(title: "REASONING", body: proposal.reasoning))
        }
        if let diff = proposal.diff, !diff.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            addFullWidth(makeFixSectionHeader("FIX"))
            addFullWidth(makeDiffBlock(diff))
        } else {
            addFullWidth(makeFixSection(title: "FIX", body: proposal.fix))
        }
        addFullWidth(makeFixActionRow(proposal: proposal, state: state))

        row.addSubview(card)
        card.addSubview(header)
        card.addSubview(content)
        NSLayoutConstraint.activate([
            card.topAnchor.constraint(equalTo: row.topAnchor),
            card.bottomAnchor.constraint(equalTo: row.bottomAnchor),
            card.leadingAnchor.constraint(equalTo: row.leadingAnchor),
            card.trailingAnchor.constraint(equalTo: row.trailingAnchor),

            header.topAnchor.constraint(equalTo: card.topAnchor, constant: 13),
            header.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 14),
            header.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -14),
            header.heightAnchor.constraint(equalToConstant: 18),
            headerIcon.leadingAnchor.constraint(equalTo: header.leadingAnchor),
            headerIcon.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            headerIcon.widthAnchor.constraint(equalToConstant: 13),
            headerIcon.heightAnchor.constraint(equalToConstant: 13),
            metaLabel.leadingAnchor.constraint(equalTo: headerIcon.trailingAnchor, constant: 7),
            metaLabel.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            stateChip.trailingAnchor.constraint(equalTo: header.trailingAnchor),
            stateChip.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            stateChip.heightAnchor.constraint(equalToConstant: 18),
            stateChip.widthAnchor.constraint(greaterThanOrEqualToConstant: 88),

            content.topAnchor.constraint(equalTo: header.bottomAnchor, constant: 9),
            content.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 14),
            content.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -14),
            content.bottomAnchor.constraint(equalTo: card.bottomAnchor, constant: -14),
        ])
        return row
    }

    private func fixStateBadge(_ state: FixProposalState) -> String {
        switch state {
        case .pending:   return "awaiting review"
        case .applying:  return "applying…"
        case .discarded: return "discarded"
        }
    }

    private func makeFixSectionHeader(_ title: String) -> NSView {
        trackedLabel(title, size: 9.5, weight: .heavy, color: Tok.tx3, tracking: 0.6)
    }

    private func makeFixSection(title: String, body: String) -> NSView {
        let container = NSStackView()
        container.orientation = .vertical
        container.alignment = .leading
        container.spacing = 3
        container.translatesAutoresizingMaskIntoConstraints = false
        container.addArrangedSubview(makeFixSectionHeader(title))
        let text = body.trimmingCharacters(in: .whitespacesAndNewlines)
        let bodyLabel = NSTextField(wrappingLabelWithString: text.isEmpty ? "—" : text)
        bodyLabel.translatesAutoresizingMaskIntoConstraints = false
        bodyLabel.font = Tok.font(12.5, .regular)
        bodyLabel.textColor = Tok.tx1
        bodyLabel.preferredMaxLayoutWidth = 440
        container.addArrangedSubview(bodyLabel)
        bodyLabel.widthAnchor.constraint(equalTo: container.widthAnchor).isActive = true
        return container
    }

    /// Monospace diff block. `+`/`-` lines tinted green/red; hunk headers
    /// (`@@`) accent; everything else dim.
    private func makeDiffBlock(_ diff: String) -> NSView {
        let panel = NSView()
        panel.translatesAutoresizingMaskIntoConstraints = false
        panel.wantsLayer = true
        panel.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.28).cgColor
        panel.layer?.cornerRadius = 8
        panel.layer?.borderWidth = 1
        panel.layer?.borderColor = Tok.hairline.cgColor

        let label = NSTextField(labelWithString: "")
        label.translatesAutoresizingMaskIntoConstraints = false
        label.attributedStringValue = attributedDiff(diff)
        label.isEditable = false
        label.isSelectable = true
        label.drawsBackground = false
        label.isBezeled = false
        label.lineBreakMode = .byClipping
        label.maximumNumberOfLines = 0
        label.preferredMaxLayoutWidth = 420

        panel.addSubview(label)
        NSLayoutConstraint.activate([
            label.topAnchor.constraint(equalTo: panel.topAnchor, constant: 9),
            label.leadingAnchor.constraint(equalTo: panel.leadingAnchor, constant: 11),
            label.trailingAnchor.constraint(equalTo: panel.trailingAnchor, constant: -11),
            label.bottomAnchor.constraint(equalTo: panel.bottomAnchor, constant: -9),
        ])
        return panel
    }

    private func attributedDiff(_ diff: String) -> NSAttributedString {
        let mono = Tok.mono(11.5, .regular)
        let result = NSMutableAttributedString()
        let lines = diff.components(separatedBy: "\n")
        for (idx, line) in lines.enumerated() {
            let color: NSColor
            if line.hasPrefix("+++") || line.hasPrefix("---") {
                color = Tok.tx3
            } else if line.hasPrefix("@@") {
                color = Tok.accentTx
            } else if line.hasPrefix("+") {
                color = Tok.ok
            } else if line.hasPrefix("-") {
                color = Tok.danger
            } else {
                color = Tok.tx3
            }
            let suffix = idx == lines.count - 1 ? "" : "\n"
            result.append(NSAttributedString(
                string: line + suffix,
                attributes: [.font: mono, .foregroundColor: color]))
        }
        return result
    }

    private func makeFixActionRow(proposal: FixProposal, state: FixProposalState) -> NSView {
        let container = NSStackView()
        container.orientation = .vertical
        container.alignment = .leading
        container.spacing = 5
        container.translatesAutoresizingMaskIntoConstraints = false

        let buttonRow = NSStackView()
        buttonRow.orientation = .horizontal
        buttonRow.alignment = .centerY
        buttonRow.spacing = 8
        buttonRow.translatesAutoresizingMaskIntoConstraints = false

        let pending = { if case .pending = state { return true }; return false }()
        let approveEnabled = pending && proposal.applySupported

        let approve = NSButton(title: "Approve & apply", target: self, action: #selector(approveFixClicked(_:)))
        approve.translatesAutoresizingMaskIntoConstraints = false
        approve.identifier = NSUserInterfaceItemIdentifier(proposal.proposalId)
        approve.isEnabled = approveEnabled
        styleFixActionButton(approve, primary: true, enabled: approveEnabled)
        buttonRow.addArrangedSubview(approve)
        registerInteractive(approve)

        let reject = NSButton(title: "Reject", target: self, action: #selector(rejectFixClicked(_:)))
        reject.translatesAutoresizingMaskIntoConstraints = false
        reject.identifier = NSUserInterfaceItemIdentifier(proposal.proposalId)
        reject.isEnabled = pending
        styleFixActionButton(reject, primary: false, enabled: pending)
        buttonRow.addArrangedSubview(reject)
        registerInteractive(reject)

        NSLayoutConstraint.activate([
            approve.heightAnchor.constraint(equalToConstant: 30),
            approve.widthAnchor.constraint(greaterThanOrEqualToConstant: 124),
            reject.heightAnchor.constraint(equalToConstant: 30),
            reject.widthAnchor.constraint(greaterThanOrEqualToConstant: 84),
        ])
        container.addArrangedSubview(buttonRow)

        let captionText: String?
        switch state {
        case .pending:
            captionText = proposal.applySupported ? nil : "This agent can't apply automatically"
        case .applying:
            captionText = "Applying… sent to your agent"
        case .discarded:
            captionText = "Discarded — nothing was applied"
        }
        if let captionText {
            let caption = NSTextField(labelWithString: captionText)
            caption.translatesAutoresizingMaskIntoConstraints = false
            caption.font = Tok.font(10, .medium)
            caption.textColor = state == .applying ? Tok.accentTx : Tok.tx3
            caption.lineBreakMode = .byTruncatingTail
            container.addArrangedSubview(caption)
        }
        return container
    }

    private func styleFixActionButton(_ button: NSButton, primary: Bool, enabled: Bool) {
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 9
        if primary {
            button.layer?.backgroundColor = Tok.accent.cgColor
            button.layer?.borderWidth = 0
        } else {
            button.layer?.backgroundColor = NSColor.clear.cgColor
            button.layer?.borderWidth = 1
            button.layer?.borderColor = Tok.hairline.cgColor
        }
        button.alphaValue = enabled ? 1.0 : 0.4
        let titleColor: NSColor = primary ? .white : Tok.tx2
        button.attributedTitle = NSAttributedString(
            string: button.title,
            attributes: [
                .font: Tok.font(12, .semibold),
                .foregroundColor: titleColor,
            ])
        button.contentTintColor = titleColor
        button.alignment = .center
    }

    @objc private func approveFixClicked(_ sender: NSButton) {
        guard sender.isEnabled, let proposalId = sender.identifier?.rawValue else { return }
        setFixState(proposalId: proposalId, to: .applying)
        emitFixApprovalResponded(proposalId: proposalId, approved: true)
    }

    @objc private func rejectFixClicked(_ sender: NSButton) {
        guard sender.isEnabled, let proposalId = sender.identifier?.rawValue else { return }
        setFixState(proposalId: proposalId, to: .discarded)
        emitFixApprovalResponded(proposalId: proposalId, approved: false)
    }

    // MARK: Provenance + body shaping (preserved)

    /// Detect a coding-agent provenance inside a free-form CueCard.source and
    /// return its uppercase badge label, or nil for plain Bluey answers.
    private func agentLabel(from source: String?) -> String? {
        guard let source, !source.isEmpty else { return nil }
        let lower = source.lowercased()
        guard lower.contains("agent") || lower.contains("claude_code")
            || lower.contains("cursor") || lower.contains("codex")
            || lower.contains("gemini") || lower.contains("windsurf")
            || lower.contains("aider")
        else { return nil }
        let known: [(needle: String, label: String)] = [
            ("claude_code", "CLAUDE"),
            ("claude", "CLAUDE"),
            ("cursor", "CURSOR"),
            ("codex", "CODEX"),
            ("gemini", "GEMINI"),
            ("windsurf", "WINDSURF"),
            ("aider", "AIDER"),
        ]
        for entry in known where lower.contains(entry.needle) {
            return entry.label
        }
        return "AGENT"
    }

    private func chatBody(for card: RenderedCard, rawBody: String) -> String {
        guard normalizedCardKind(card.kind) == "answer" else { return rawBody }
        if let artifact = card.artifact {
            if artifact.artifactType == "code" {
                let notes = stripFencedCode(from: rawBody).trimmingCharacters(in: .whitespacesAndNewlines)
                return notes.isEmpty ? "I opened the code in the canvas." : notes + "\n\nCode opened in the canvas."
            }
            if rawBody.count > 1_100 {
                let prefix = String(rawBody.prefix(720)).trimmingCharacters(in: .whitespacesAndNewlines)
                return prefix + "\n\nFull \(artifact.title.lowercased()) opened in the canvas."
            }
        }
        if rawBody.contains("```") {
            let notes = stripFencedCode(from: rawBody).trimmingCharacters(in: .whitespacesAndNewlines)
            return notes.isEmpty ? "I opened the code in the canvas." : notes + "\n\nCode opened in the canvas."
        }
        if rawBody.count > 1_100 && hasStructuredShape(rawBody) {
            let prefix = String(rawBody.prefix(720)).trimmingCharacters(in: .whitespacesAndNewlines)
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
            if !inFence { lines.append(line) }
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
                candidate = trimmed.replacingOccurrences(of: "login_url:", with: "").trimmingCharacters(in: .whitespacesAndNewlines)
            } else if trimmed.hasPrefix("https://") || trimmed.hasPrefix("http://") {
                candidate = trimmed
            } else {
                continue
            }
            if let url = URL(string: candidate),
               let scheme = url.scheme?.lowercased(),
               ["http", "https"].contains(scheme) {
                return url
            }
        }
        return nil
    }

    private func scrollToBottom() {
        DispatchQueue.main.async { [weak self] in
            guard let s = self else { return }
            // Hide the connector under the final turn (timeline ends there).
            s.updateConnectors()
            let bottom = NSPoint(x: 0, y: max(0, s.stack.bounds.height - s.scroll.contentView.bounds.height))
            s.scroll.contentView.scroll(to: bottom)
            s.scroll.reflectScrolledClipView(s.scroll.contentView)
        }
    }

    /// Hide the connector hairline under the last turn so the thread visually
    /// terminates (mockup `.turn:last-child::before{display:none}`).
    private func updateConnectors() {
        let turns = stack.arrangedSubviews
        for (idx, turn) in turns.enumerated() {
            let isLast = idx == turns.count - 1
            for sub in turn.subviews where sub.identifier?.rawValue == "turn-connector" {
                sub.isHidden = isLast
            }
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
    // MARK: Body region (one of Ask / History / Agents visible at a time)
    private enum BodyTab: Int { case ask, history, agents }

    // Ask body.
    let feed: FeedView
    let workspace: NSView
    let canvasPane: CanvasPaneView

    // Surface sublayers.
    private let auroraLayer = CAGradientLayer()
    private let topHairline = CALayer()

    // Header (`.ph`).
    let headerBar: HeaderDragView
    private let statusDot = NSView()
    private let listeningWave = NSView()
    private var waveBars: [NSView] = []
    private let brandLabel = NSTextField(labelWithString: "Bluey")
    private let viaLabel = NSTextField(labelWithString: "· managed")
    private let segContainer = NSView()
    private var segButtons: [NSButton] = []
    private let closeButton = NSButton()

    // Ask: context bar + composer + footer.
    private let contextBar = NSView()
    private let contextLabel = NSTextField(labelWithString: "")
    private let attachmentStrip = NSStackView()
    let composerBar: NSView
    let composerSurface: NSView
    let composer: ComposerTextView
    private let plusButton = NSButton()
    private let listenButton = NSButton()
    private let sendButton = NSButton()
    private let footerBar = NSView()
    private let footerModelLabel = NSTextField(labelWithString: "opus-4.8")
    private let footerConnectorsLabel = NSTextField(labelWithString: "")
    private let footerKeysLabel = NSTextField(labelWithString: "⌘↵ ask · ⌥ hide")

    // History body (`.hx`).
    private let historyContainer = NSView()
    private let searchField = NSTextField()
    private let historyScroll = NSScrollView()
    private let historyStack = NSStackView()

    // Agents body (`.ax`).
    private let agentsScroll = NSScrollView()
    private let agentsStack = NSStackView()

    // The "+" menu (secondary actions).
    private let plusMenu = NSView()

    // Modals (dark, over a dim backdrop).
    private let connectorSheetOverlay: ModalBlockerView
    private let connectorSheetPanel = NSView()
    private let connectorSheetTitle = NSTextField(labelWithString: "Inherited connectors")
    private let connectorSheetSummary = NSTextField(labelWithString: "")
    private let connectorSheetScroll = NSScrollView()
    private let connectorSheetStack = NSStackView()
    private let connectorSheetReauthLabel = NSTextField(labelWithString: "")
    private let connectorSheetCancelButton = NSButton()
    private let connectorSheetAttachButton = NSButton()

    private let billingOverlay: ModalBlockerView
    private let billingPanel = NSView()
    private let billingTitle = NSTextField(labelWithString: "")
    private let billingSubtitle = NSTextField(labelWithString: "Bring-your-own-key · please review.")
    private let billingDiscLabel = NSTextField(labelWithString: "")
    private let billingDeclineButton = NSButton()
    private let billingAcceptButton = NSButton()

    private let closeConfirmOverlay: ModalBlockerView
    private let closeConfirmPanel = NSView()
    private let closeConfirmTitle = NSTextField(labelWithString: "Turn Bluey off?")
    private let closeConfirmBody = NSTextField(wrappingLabelWithString: "This closes Bluey completely. To start again, run: bluey on")
    private let closeConfirmCancelButton = NSButton()
    private let closeConfirmTurnOffButton = NSButton()

    // System toast (transient cards).
    let toastView: NSView
    let toastTitleLabel: NSTextField
    let toastBodyLabel: NSTextField
    private var toastHideWorkItem: DispatchWorkItem?

    // MARK: External contract closures (consumed by OverlayApp)
    var onClose: (() -> Void)?
    var onOpacityChanged: ((Double) -> Void)?
    var onListeningStateChanged: ((PillRunState) -> Void)?
    var onAgentAttachmentChanged: ((Bool) -> Void)?

    // MARK: State
    private var currentTab: BodyTab = .ask
    private var recordingActive = false
    private var backgroundOpacity: CGFloat = 0.97
    private var transcriptCardId: String?

    // Sessions (History-at-scale).
    private var sessionItems: [OverlaySessionItem] = []
    private var sessionTotal = 0
    private var sessionHasMore = false
    private var sessionQuery = ""
    private var sessionsLoaded = false
    private var editingSessionId: String?
    private var pendingDeleteSessionId: String?
    private var renameField: NSTextField?

    // Agents.
    private enum AgentStage { case picker; case sessions(kind: String, displayName: String) }
    private var agentStage: AgentStage = .picker
    private var agentSummaries: [AgentSummary] = []
    private var agentListLoaded = false
    private var agentSessions: [AgentSessionSummary] = []
    private var agentSessionsLoaded = false
    private var attachedAgentKind: String?

    // Connector sheet / billing pending attach.
    private var pendingConnectorKind: String?
    private var pendingConnectorSessionId: String?
    private var pendingConnectorInfos: [AgentConnectorInfo] = []
    private var pendingConnectorsLoaded = false
    private var pendingBilling: BillingDisclosure?

    // Canvas.
    private var latestCanvas: CanvasArtifact?
    private var canvasOpen = false
    private var canvasFullWindow = false
    private var preCanvasFullWindowFrame: NSRect?
    private var canvasWidthConstraint: NSLayoutConstraint?

    // Composer height.
    private var composerTextHeightConstraint: NSLayoutConstraint?
    // Context bar height (collapses to 0 when there's nothing in context).
    private var contextBarHeightConstraint: NSLayoutConstraint?

    // Resize (bottom-right grip only — header drags).
    private struct ResizeEdges: OptionSet {
        let rawValue: Int
        static let right = ResizeEdges(rawValue: 1 << 1)
        static let bottom = ResizeEdges(rawValue: 1 << 3)
    }
    private var activeResizeEdges: ResizeEdges = []
    private var resizeStartMouse = NSPoint.zero
    private var resizeStartFrame = NSRect.zero
    private let resizeHitSize: CGFloat = 12

    override init(frame frameRect: NSRect) {
        feed = FeedView(frame: .zero)
        workspace = NSView()
        canvasPane = CanvasPaneView(frame: .zero)
        headerBar = HeaderDragView()
        composerBar = NSView()
        composerSurface = ComposerSurfaceView()
        composer = ComposerTextView(frame: .zero, textContainer: nil)
        connectorSheetOverlay = ModalBlockerView()
        billingOverlay = ModalBlockerView()
        closeConfirmOverlay = ModalBlockerView()
        toastView = NSView()
        toastTitleLabel = NSTextField(labelWithString: "")
        toastBodyLabel = NSTextField(wrappingLabelWithString: "")

        super.init(frame: frameRect)

        wantsLayer = true
        configureSurface()
        configureHeader()
        configureAskBody()
        configureComposer()
        configureFooter()
        configureContextBar()
        configurePlusMenu()
        configureHistoryBody()
        configureAgentsBody()
        configureConnectorSheet()
        configureBillingModal()
        configureCloseConfirm()
        configureSystemToast()
        assembleLayout()

        feed.onTranscript = { [weak self] _ in self?.markListening() }
        feed.onOpenURL = { url in NSWorkspace.shared.open(url) }
        feed.onFixRequested = { cardId, question in emitFixRequested(cardId: cardId, question: question) }
        feed.onContentChanged = { [weak self] in self?.updateContextBar() }
        canvasPane.onCollapse = { [weak self] in self?.setCanvasOpen(false) }
        canvasPane.onToggleFullWindow = { [weak self] in self?.toggleCanvasFullWindow() }

        composer.onSubmit = { [weak self] in self?.askClicked() }
        composer.onMeasuredHeight = { [weak self] height in self?.setComposerTextHeight(height) }
        composer.placeholder = "Ask a follow-up…"

        (connectorSheetOverlay as ModalBlockerView).onEscape = { [weak self] in self?.dismissConnectorSheet() }
        (billingOverlay as ModalBlockerView).onEscape = { [weak self] in self?.declineBilling() }
        (closeConfirmOverlay as ModalBlockerView).onEscape = { [weak self] in self?.dismissCloseConfirm(animated: true) }

        setBodyTab(.ask)
        updateContextBar()
    }
    required init?(coder: NSCoder) { fatalError() }

    // MARK: Phase 1 — Surface (dark aurora glass; the washout fix)

    private func configureSurface() {
        // THE surface — match the HTML EXACTLY. In the mockup the panel is a
        // clean, EVEN dark navy glass (rgba(17,20,26,.52)) sitting over the
        // dark `.desk` aurora gradient (#0b1220 → #160e24). The aurora belongs
        // to the DESK behind, NOT painted strongly on the panel — a strong
        // gradient on the panel makes the blotchy green/teal blob (the bug).
        // So: paint the panel as the desk's deep-navy gradient itself, even and
        // subtle, near-opaque so it reads dark over any wallpaper.
        layer?.backgroundColor = NSColor(red: 0.043, green: 0.071, blue: 0.125, alpha: 0.97).cgColor
        layer?.cornerRadius = Tok.r2xl
        layer?.masksToBounds = true
        if #available(macOS 10.15, *) { layer?.cornerCurve = .continuous }

        // The aurora as the DESK gradient: deep navy → deep violet, EVEN and
        // SUBTLE (matches linear-gradient(140deg,#0b1220,#160e24 60%,#0a0d14)
        // with the faint radial tints). Low-contrast so it's a calm dark field,
        // never a bright blob. This IS the panel's base — diagonal, gentle.
        auroraLayer.colors = [
            NSColor(red: 0.043, green: 0.071, blue: 0.125, alpha: 1.0).cgColor, // #0b1220 navy
            NSColor(red: 0.086, green: 0.055, blue: 0.141, alpha: 1.0).cgColor, // #160e24 violet
            NSColor(red: 0.039, green: 0.051, blue: 0.078, alpha: 1.0).cgColor, // #0a0d14 deep
        ]
        auroraLayer.locations = [0.0, 0.6, 1.0]
        auroraLayer.startPoint = CGPoint(x: 0.15, y: 0.0)
        auroraLayer.endPoint = CGPoint(x: 0.85, y: 1.0)
        auroraLayer.opacity = 0.96
        auroraLayer.cornerRadius = Tok.r2xl
        if #available(macOS 10.15, *) { auroraLayer.cornerCurve = .continuous }
        auroraLayer.masksToBounds = true
        layer?.addSublayer(auroraLayer)

        // 1px top-edge highlight hairline (.white α0.09).
        topHairline.backgroundColor = NSColor.white.withAlphaComponent(0.09).cgColor
        layer?.addSublayer(topHairline)

        // Window shadow (black α0.55, soft).
        layer?.shadowColor = NSColor.black.cgColor
        layer?.shadowOpacity = 0.55
        layer?.shadowRadius = 40
        layer?.shadowOffset = .zero
    }

    override func layout() {
        super.layout()
        auroraLayer.frame = bounds
        topHairline.frame = CGRect(x: 0, y: bounds.height - 1, width: bounds.width, height: 1)
    }

    func applyOpacity(_ opacity: Double) {
        // The dark fill alpha is the one knob for "dark vs washed-out". We keep
        // the panel firmly dark (floor 0.90) so it never washes out, while still
        // honoring the user's preference downward a touch.
        let value = min(max(CGFloat(opacity), 0.50), 1.0)
        backgroundOpacity = value
        // Drive the panel alpha via the aurora gradient layer (the visible
        // surface), keeping it firmly dark (floor 0.90). Don't repaint the base
        // with a different color — that would override the navy aurora base.
        let fillAlpha = blueyMaterialAlpha(1.0, opacity: value, floor: 0.90)
        layer?.backgroundColor = NSColor(red: 0.043, green: 0.071, blue: 0.125, alpha: fillAlpha).cgColor
        auroraLayer.opacity = Float(fillAlpha)
        canvasPane.applyBackgroundOpacity(value)
        onOpacityChanged?(Double(value))
    }

    // MARK: Phase 2 — Header (`.ph`): dot · Bluey · ·managed · [seg] · ✕

    private func configureHeader() {
        headerBar.translatesAutoresizingMaskIntoConstraints = false
        headerBar.wantsLayer = true
        headerBar.layer?.backgroundColor = NSColor.clear.cgColor
        // 1px hairline bottom.
        let hairline = NSView()
        hairline.translatesAutoresizingMaskIntoConstraints = false
        hairline.wantsLayer = true
        hairline.layer?.backgroundColor = Tok.hairline.cgColor

        statusDot.translatesAutoresizingMaskIntoConstraints = false
        statusDot.wantsLayer = true
        statusDot.layer?.cornerRadius = 3.5
        statusDot.layer?.backgroundColor = Tok.ok.cgColor

        // Listening waveform (shown instead of the dot while listening).
        listeningWave.translatesAutoresizingMaskIntoConstraints = false
        listeningWave.isHidden = true
        let heights: [CGFloat] = [5, 11, 7]
        for h in heights {
            let bar = NSView()
            bar.translatesAutoresizingMaskIntoConstraints = false
            bar.wantsLayer = true
            bar.layer?.cornerRadius = 1
            bar.layer?.backgroundColor = Tok.ok.cgColor
            listeningWave.addSubview(bar)
            bar.widthAnchor.constraint(equalToConstant: 2).isActive = true
            bar.heightAnchor.constraint(equalToConstant: h).isActive = true
            bar.bottomAnchor.constraint(equalTo: listeningWave.bottomAnchor).isActive = true
            waveBars.append(bar)
        }
        for (i, bar) in waveBars.enumerated() {
            if i == 0 {
                bar.leadingAnchor.constraint(equalTo: listeningWave.leadingAnchor).isActive = true
            } else {
                bar.leadingAnchor.constraint(equalTo: waveBars[i - 1].trailingAnchor, constant: 2).isActive = true
            }
        }
        waveBars.last?.trailingAnchor.constraint(equalTo: listeningWave.trailingAnchor).isActive = true

        brandLabel.translatesAutoresizingMaskIntoConstraints = false
        brandLabel.font = Tok.font(13, .semibold)
        brandLabel.textColor = Tok.tx1
        useCenteredSingleLineCell(brandLabel)
        brandLabel.stringValue = "Bluey"

        viaLabel.translatesAutoresizingMaskIntoConstraints = false
        viaLabel.font = Tok.font(11, .regular)
        viaLabel.textColor = Tok.tx3
        useCenteredSingleLineCell(viaLabel)
        viaLabel.stringValue = "· managed"

        configureSegmented()

        styleHeaderClose(closeButton)
        closeButton.target = self
        closeButton.action = #selector(closeClicked)

        headerBar.addSubview(statusDot)
        headerBar.addSubview(listeningWave)
        headerBar.addSubview(brandLabel)
        headerBar.addSubview(viaLabel)
        headerBar.addSubview(segContainer)
        headerBar.addSubview(closeButton)
        headerBar.addSubview(hairline)

        NSLayoutConstraint.activate([
            headerBar.heightAnchor.constraint(equalToConstant: 48),

            statusDot.leadingAnchor.constraint(equalTo: headerBar.leadingAnchor, constant: 14),
            statusDot.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            statusDot.widthAnchor.constraint(equalToConstant: 7),
            statusDot.heightAnchor.constraint(equalToConstant: 7),

            listeningWave.leadingAnchor.constraint(equalTo: headerBar.leadingAnchor, constant: 14),
            listeningWave.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            listeningWave.heightAnchor.constraint(equalToConstant: 11),

            brandLabel.leadingAnchor.constraint(equalTo: statusDot.trailingAnchor, constant: 10),
            brandLabel.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),

            viaLabel.leadingAnchor.constraint(equalTo: brandLabel.trailingAnchor, constant: 5),
            viaLabel.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            viaLabel.trailingAnchor.constraint(lessThanOrEqualTo: segContainer.leadingAnchor, constant: -8),

            segContainer.trailingAnchor.constraint(equalTo: closeButton.leadingAnchor, constant: -8),
            segContainer.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),

            closeButton.trailingAnchor.constraint(equalTo: headerBar.trailingAnchor, constant: -14),
            closeButton.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            closeButton.widthAnchor.constraint(equalToConstant: 26),
            closeButton.heightAnchor.constraint(equalToConstant: 26),

            hairline.leadingAnchor.constraint(equalTo: headerBar.leadingAnchor),
            hairline.trailingAnchor.constraint(equalTo: headerBar.trailingAnchor),
            hairline.bottomAnchor.constraint(equalTo: headerBar.bottomAnchor),
            hairline.heightAnchor.constraint(equalToConstant: 1),
        ])
    }

    /// Segmented switch (`.seg`): container glass-hi + hairline, radius 10, pad 2.
    private func configureSegmented() {
        segContainer.translatesAutoresizingMaskIntoConstraints = false
        segContainer.wantsLayer = true
        segContainer.layer?.backgroundColor = Tok.glassHi.cgColor
        segContainer.layer?.cornerRadius = 10
        segContainer.layer?.borderWidth = 1
        segContainer.layer?.borderColor = Tok.hairline.cgColor

        let titles = ["Ask", "History", "Agents"]
        var prev: NSButton?
        for (i, title) in titles.enumerated() {
            let button = NSButton(title: title, target: self, action: #selector(segClicked(_:)))
            button.translatesAutoresizingMaskIntoConstraints = false
            button.isBordered = false
            button.tag = i
            button.wantsLayer = true
            button.layer?.cornerRadius = 8
            button.setButtonType(.momentaryChange)
            segContainer.addSubview(button)
            segButtons.append(button)
            NSLayoutConstraint.activate([
                button.topAnchor.constraint(equalTo: segContainer.topAnchor, constant: 2),
                button.bottomAnchor.constraint(equalTo: segContainer.bottomAnchor, constant: -2),
                button.heightAnchor.constraint(equalToConstant: 22),
                button.widthAnchor.constraint(greaterThanOrEqualToConstant: title == "History" ? 56 : 46),
            ])
            if let prev {
                button.leadingAnchor.constraint(equalTo: prev.trailingAnchor, constant: 0).isActive = true
            } else {
                button.leadingAnchor.constraint(equalTo: segContainer.leadingAnchor, constant: 2).isActive = true
            }
            prev = button
        }
        prev?.trailingAnchor.constraint(equalTo: segContainer.trailingAnchor, constant: -2).isActive = true
        styleSegButtons()
    }

    private func styleSegButtons() {
        for (i, button) in segButtons.enumerated() {
            let selected = i == currentTab.rawValue
            button.layer?.backgroundColor = selected ? Tok.accentBg.cgColor : NSColor.clear.cgColor
            button.attributedTitle = NSAttributedString(
                string: button.title,
                attributes: [
                    .font: Tok.font(11.5, .semibold),
                    .foregroundColor: selected ? Tok.accentTx : Tok.tx3,
                ])
        }
    }

    private func styleHeaderClose(_ button: NSButton) {
        button.translatesAutoresizingMaskIntoConstraints = false
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 8
        button.contentTintColor = Tok.tx3
        button.toolTip = "Hide Bluey"
        if let image = symbolImage("minus") {
            image.isTemplate = true
            button.image = image
            button.imagePosition = .imageOnly
            button.imageScaling = .scaleProportionallyDown
        } else {
            button.title = "–"
        }
    }

    @objc private func segClicked(_ sender: NSButton) {
        guard let tab = BodyTab(rawValue: sender.tag) else { return }
        setBodyTab(tab)
    }

    @objc private func closeClicked() {
        // The ✕/– in the header HIDES (collapse to pill); turn-off is the
        // explicit pill End action / its own confirm.
        onClose?()
    }

    // MARK: Body tab switching

    private func setBodyTab(_ tab: BodyTab) {
        currentTab = tab
        workspace.isHidden = tab != .ask
        attachmentStrip.isHidden = tab != .ask || attachmentStrip.arrangedSubviews.isEmpty
        historyContainer.isHidden = tab != .history
        agentsScroll.isHidden = tab != .agents
        // The composer + footer show on every tab (the mockup keeps them on all
        // three): Ask follows up, History continues a session, Agents asks the
        // attached agent.
        composerBar.isHidden = false
        footerBar.isHidden = false
        styleSegButtons()
        updateFooter()
        updateContextBar()

        switch tab {
        case .ask:
            composer.placeholder = recordingActive ? "Ask while Bluey listens…" : "Ask a follow-up…"
        case .history:
            composer.placeholder = "Ask Bluey…  or pick a session to continue"
            if !sessionsLoaded { requestSessions(reset: true) }
        case .agents:
            composer.placeholder = "Ask your agent…"
            if !agentListLoaded { emitAgentListRequested() }
            renderAgents()
        }
    }

    // MARK: Phase 4 — Ask body (timeline feed + optional canvas split)

    private func configureAskBody() {
        workspace.translatesAutoresizingMaskIntoConstraints = false
        feed.translatesAutoresizingMaskIntoConstraints = false
        canvasPane.translatesAutoresizingMaskIntoConstraints = false
        canvasPane.isHidden = true
        workspace.addSubview(feed)
        workspace.addSubview(canvasPane)

        let canvasWidth = canvasPane.widthAnchor.constraint(equalToConstant: 0)
        canvasWidthConstraint = canvasWidth
        NSLayoutConstraint.activate([
            feed.topAnchor.constraint(equalTo: workspace.topAnchor),
            feed.leadingAnchor.constraint(equalTo: workspace.leadingAnchor),
            feed.bottomAnchor.constraint(equalTo: workspace.bottomAnchor),

            canvasPane.topAnchor.constraint(equalTo: workspace.topAnchor),
            canvasPane.trailingAnchor.constraint(equalTo: workspace.trailingAnchor),
            canvasPane.bottomAnchor.constraint(equalTo: workspace.bottomAnchor),
            canvasPane.leadingAnchor.constraint(equalTo: feed.trailingAnchor),
            canvasWidth,
        ])
    }

    // MARK: Phase 3 — Composer (`.comp`) + footer (`.foot`)

    private func configureComposer() {
        composerBar.translatesAutoresizingMaskIntoConstraints = false
        composerSurface.translatesAutoresizingMaskIntoConstraints = false
        composerSurface.wantsLayer = true
        composerSurface.layer?.backgroundColor = Tok.glassHi.cgColor
        composerSurface.layer?.cornerRadius = Tok.rLg
        composerSurface.layer?.borderWidth = 1
        composerSurface.layer?.borderColor = Tok.hairline.cgColor

        // "+" button (32px, radius 10).
        plusButton.translatesAutoresizingMaskIntoConstraints = false
        plusButton.isBordered = false
        plusButton.wantsLayer = true
        plusButton.layer?.cornerRadius = 10
        plusButton.layer?.backgroundColor = Tok.glassHi.cgColor
        plusButton.layer?.borderWidth = 1
        plusButton.layer?.borderColor = Tok.hairline.cgColor
        plusButton.contentTintColor = Tok.tx2
        plusButton.target = self
        plusButton.action = #selector(togglePlusMenu)
        plusButton.toolTip = "Add to context · session actions"
        if let image = symbolImage("plus") {
            image.isTemplate = true
            plusButton.image = image
            plusButton.imagePosition = .imageOnly
            plusButton.imageScaling = .scaleProportionallyDown
        } else { plusButton.title = "+" }

        // Field.
        composer.translatesAutoresizingMaskIntoConstraints = false
        composer.font = Tok.font(13, .regular)
        composer.textColor = Tok.tx1
        composer.insertionPointColor = Tok.accent

        // Listen toggle (waveform/mic + label).
        listenButton.translatesAutoresizingMaskIntoConstraints = false
        listenButton.isBordered = false
        listenButton.target = self
        listenButton.action = #selector(recordingClicked)
        styleListenButton(listening: false)

        // Send (32px, accent, radius 10).
        sendButton.translatesAutoresizingMaskIntoConstraints = false
        sendButton.isBordered = false
        sendButton.wantsLayer = true
        sendButton.layer?.cornerRadius = 10
        sendButton.layer?.backgroundColor = Tok.accent.cgColor
        sendButton.contentTintColor = .white
        sendButton.target = self
        sendButton.action = #selector(askClicked)
        sendButton.toolTip = "Send (⌘↵)"
        if let image = symbolImage("arrow.up") {
            image.isTemplate = true
            sendButton.image = image
            sendButton.imagePosition = .imageOnly
            sendButton.imageScaling = .scaleProportionallyDown
        } else { sendButton.title = "↑" }

        composerBar.addSubview(composerSurface)
        composerSurface.addSubview(plusButton)
        composerSurface.addSubview(composer)
        composerSurface.addSubview(listenButton)
        composerSurface.addSubview(sendButton)

        let textHeight = composer.heightAnchor.constraint(equalToConstant: 22)
        composerTextHeightConstraint = textHeight
        NSLayoutConstraint.activate([
            composerSurface.topAnchor.constraint(equalTo: composerBar.topAnchor),
            composerSurface.bottomAnchor.constraint(equalTo: composerBar.bottomAnchor),
            composerSurface.leadingAnchor.constraint(equalTo: composerBar.leadingAnchor, constant: 14),
            composerSurface.trailingAnchor.constraint(equalTo: composerBar.trailingAnchor, constant: -14),
            composerSurface.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),

            plusButton.leadingAnchor.constraint(equalTo: composerSurface.leadingAnchor, constant: 8),
            plusButton.centerYAnchor.constraint(equalTo: composerSurface.centerYAnchor),
            plusButton.widthAnchor.constraint(equalToConstant: 32),
            plusButton.heightAnchor.constraint(equalToConstant: 32),

            composer.leadingAnchor.constraint(equalTo: plusButton.trailingAnchor, constant: 9),
            composer.centerYAnchor.constraint(equalTo: composerSurface.centerYAnchor),
            composer.trailingAnchor.constraint(equalTo: listenButton.leadingAnchor, constant: -8),
            textHeight,

            listenButton.trailingAnchor.constraint(equalTo: sendButton.leadingAnchor, constant: -6),
            listenButton.centerYAnchor.constraint(equalTo: composerSurface.centerYAnchor),
            listenButton.heightAnchor.constraint(equalToConstant: 28),

            sendButton.trailingAnchor.constraint(equalTo: composerSurface.trailingAnchor, constant: -7),
            sendButton.centerYAnchor.constraint(equalTo: composerSurface.centerYAnchor),
            sendButton.widthAnchor.constraint(equalToConstant: 32),
            sendButton.heightAnchor.constraint(equalToConstant: 32),
        ])
    }

    private func styleListenButton(listening: Bool) {
        listenButton.wantsLayer = true
        let color = listening ? Tok.ok : Tok.tx3
        listenButton.contentTintColor = color
        let title = listening ? "Listening" : "Listen"
        let symbol = listening ? "waveform" : "mic"
        if let image = symbolImage(symbol) {
            image.isTemplate = true
            listenButton.image = image
            listenButton.imagePosition = .imageLeading
            listenButton.imageHugsTitle = true
            listenButton.imageScaling = .scaleProportionallyDown
        }
        listenButton.attributedTitle = NSAttributedString(
            string: title,
            attributes: [.font: Tok.font(11.5, .semibold), .foregroundColor: color])
        listenButton.toolTip = listening ? "Stop listening" : "Listen (mic + system audio)"
    }

    private func configureFooter() {
        footerBar.translatesAutoresizingMaskIntoConstraints = false
        footerModelLabel.translatesAutoresizingMaskIntoConstraints = false
        footerModelLabel.font = Tok.font(10.5, .regular)
        footerModelLabel.textColor = Tok.tx3
        footerConnectorsLabel.translatesAutoresizingMaskIntoConstraints = false
        footerConnectorsLabel.font = Tok.font(10.5, .regular)
        footerConnectorsLabel.textColor = Tok.tx3
        footerKeysLabel.translatesAutoresizingMaskIntoConstraints = false
        footerKeysLabel.font = Tok.mono(10.5, .regular)
        footerKeysLabel.textColor = Tok.tx4
        footerKeysLabel.alignment = .right

        footerBar.addSubview(footerModelLabel)
        footerBar.addSubview(footerConnectorsLabel)
        footerBar.addSubview(footerKeysLabel)
        NSLayoutConstraint.activate([
            footerBar.heightAnchor.constraint(equalToConstant: 26),
            footerModelLabel.leadingAnchor.constraint(equalTo: footerBar.leadingAnchor, constant: 16),
            footerModelLabel.centerYAnchor.constraint(equalTo: footerBar.centerYAnchor),
            footerConnectorsLabel.leadingAnchor.constraint(equalTo: footerModelLabel.trailingAnchor, constant: 11),
            footerConnectorsLabel.centerYAnchor.constraint(equalTo: footerBar.centerYAnchor),
            footerKeysLabel.trailingAnchor.constraint(equalTo: footerBar.trailingAnchor, constant: -16),
            footerKeysLabel.centerYAnchor.constraint(equalTo: footerBar.centerYAnchor),
            footerKeysLabel.leadingAnchor.constraint(greaterThanOrEqualTo: footerConnectorsLabel.trailingAnchor, constant: 8),
        ])
    }

    private func updateFooter() {
        switch currentTab {
        case .ask:
            footerModelLabel.stringValue = attachedAgentKind != nil ? "runs on your machine" : "opus-4.8"
            footerConnectorsLabel.stringValue = attachedAgentKind != nil ? "" : "perplexity · github"
            footerKeysLabel.stringValue = "⌘↵ ask · ⌥ hide"
        case .history:
            footerModelLabel.stringValue = "\(sessionTotal) session\(sessionTotal == 1 ? "" : "s")"
            footerConnectorsLabel.stringValue = ""
            footerKeysLabel.stringValue = "↵ continue · ⌘F search"
        case .agents:
            footerModelLabel.stringValue = attachedAgentKind.map { agentShortLabel($0).lowercased() } ?? "your agents"
            footerConnectorsLabel.stringValue = ""
            footerKeysLabel.stringValue = "answers run on your machine"
        }
    }

    // MARK: Context bar (`.ctxbar`) + attachment chips

    private func configureContextBar() {
        contextBar.translatesAutoresizingMaskIntoConstraints = false
        contextBar.wantsLayer = true
        contextBar.layer?.backgroundColor = NSColor(red: 0.231, green: 0.510, blue: 0.965, alpha: 0.07).cgColor
        contextBar.layer?.cornerRadius = 9
        contextBar.layer?.borderWidth = 1
        contextBar.layer?.borderColor = NSColor(red: 0.231, green: 0.510, blue: 0.965, alpha: 0.14).cgColor
        contextBar.isHidden = true

        let icon = NSImageView()
        icon.translatesAutoresizingMaskIntoConstraints = false
        if let image = symbolImage("checkmark.circle") {
            image.isTemplate = true
            icon.image = image
        }
        icon.contentTintColor = Tok.accentTx

        contextLabel.translatesAutoresizingMaskIntoConstraints = false
        contextLabel.font = Tok.font(11, .regular)
        contextLabel.textColor = Tok.tx2
        contextLabel.lineBreakMode = .byTruncatingTail

        contextBar.addSubview(icon)
        contextBar.addSubview(contextLabel)
        let ctxHeight = contextBar.heightAnchor.constraint(equalToConstant: 0)
        contextBarHeightConstraint = ctxHeight
        NSLayoutConstraint.activate([
            ctxHeight,
            icon.leadingAnchor.constraint(equalTo: contextBar.leadingAnchor, constant: 10),
            icon.centerYAnchor.constraint(equalTo: contextBar.centerYAnchor),
            icon.widthAnchor.constraint(equalToConstant: 13),
            icon.heightAnchor.constraint(equalToConstant: 13),
            contextLabel.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: 8),
            contextLabel.centerYAnchor.constraint(equalTo: contextBar.centerYAnchor),
            contextLabel.trailingAnchor.constraint(equalTo: contextBar.trailingAnchor, constant: -10),
        ])

        // Attachment chip strip (contextual, only when files attached).
        attachmentStrip.translatesAutoresizingMaskIntoConstraints = false
        attachmentStrip.orientation = .horizontal
        attachmentStrip.alignment = .centerY
        attachmentStrip.spacing = 7
        attachmentStrip.isHidden = true
    }

    private func updateContextBar() {
        guard currentTab == .ask else { setContextBarVisible(false); return }
        let screenCount = feed.screenTurnCount
        let turns = feed.conversationTurnCount
        let hasTranscript = feed.transcriptTurnCount > 0
        guard feed.hasCards, hasTranscript || screenCount > 0 || turns > 0 else {
            setContextBarVisible(false)
            return
        }
        var parts: [String] = []
        if hasTranscript { parts.append("transcript") }
        if screenCount > 0 { parts.append("\(screenCount) screen") }
        parts.append("\(turns) turn\(turns == 1 ? "" : "s")")
        contextLabel.stringValue = "In context: " + parts.joined(separator: " · ")
        setContextBarVisible(true)
    }

    private func setContextBarVisible(_ visible: Bool) {
        contextBar.isHidden = !visible
        contextBarHeightConstraint?.constant = visible ? 30 : 0
    }

    // MARK: Phase 8 — The "+" menu (secondary actions, decluttered)

    private func configurePlusMenu() {
        plusMenu.translatesAutoresizingMaskIntoConstraints = false
        plusMenu.wantsLayer = true
        plusMenu.layer?.backgroundColor = Tok.modalFill.cgColor
        plusMenu.layer?.cornerRadius = Tok.rLg
        plusMenu.layer?.borderWidth = 1
        plusMenu.layer?.borderColor = Tok.hairlineStrong.cgColor
        plusMenu.layer?.masksToBounds = true
        plusMenu.isHidden = true

        let stack = NSStackView()
        stack.translatesAutoresizingMaskIntoConstraints = false
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 0
        plusMenu.addSubview(stack)
        NSLayoutConstraint.activate([
            plusMenu.widthAnchor.constraint(equalToConstant: 214),
            stack.topAnchor.constraint(equalTo: plusMenu.topAnchor, constant: 6),
            stack.leadingAnchor.constraint(equalTo: plusMenu.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: plusMenu.trailingAnchor),
            stack.bottomAnchor.constraint(equalTo: plusMenu.bottomAnchor, constant: -6),
        ])

        stack.addArrangedSubview(plusMenuHeader("ADD TO CONTEXT"))
        stack.addArrangedSubview(plusMenuItem("Attach files", symbol: "paperclip", action: #selector(menuAttachClicked)))
        stack.addArrangedSubview(plusMenuItem("Capture browser page", symbol: "globe", action: #selector(menuCapturePageClicked)))
        stack.addArrangedSubview(plusMenuHeader("SESSION"))
        stack.addArrangedSubview(plusMenuItem("New session", symbol: "plus", action: #selector(menuNewSessionClicked)))
        stack.addArrangedSubview(plusMenuItem("Recap", symbol: "list.bullet", action: #selector(menuRecapClicked)))
        stack.addArrangedSubview(plusMenuItem("Answer style", symbol: "pencil", action: #selector(menuAnswerStyleClicked)))
        stack.addArrangedSubview(plusMenuItem("Opacity & settings", symbol: "gearshape", action: #selector(menuOpacityClicked)))
        for item in stack.arrangedSubviews {
            item.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        }
    }

    private func plusMenuHeader(_ text: String) -> NSView {
        let label = trackedLabel(text, size: 9.5, weight: .heavy, color: Tok.tx4, tracking: 0.7)
        let wrap = NSView()
        wrap.translatesAutoresizingMaskIntoConstraints = false
        wrap.addSubview(label)
        NSLayoutConstraint.activate([
            wrap.heightAnchor.constraint(equalToConstant: 26),
            label.leadingAnchor.constraint(equalTo: wrap.leadingAnchor, constant: 13),
            label.bottomAnchor.constraint(equalTo: wrap.bottomAnchor, constant: -4),
        ])
        return wrap
    }

    private func plusMenuItem(_ title: String, symbol: String, action: Selector) -> NSView {
        let button = NSButton(title: "", target: self, action: action)
        button.translatesAutoresizingMaskIntoConstraints = false
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.backgroundColor = NSColor.clear.cgColor

        let icon = NSImageView()
        icon.translatesAutoresizingMaskIntoConstraints = false
        if let image = symbolImage(symbol) {
            image.isTemplate = true
            icon.image = image
        }
        icon.contentTintColor = Tok.tx3
        let label = NSTextField(labelWithString: title)
        label.translatesAutoresizingMaskIntoConstraints = false
        label.font = Tok.font(12.5, .regular)
        label.textColor = Tok.tx1
        button.addSubview(icon)
        button.addSubview(label)
        NSLayoutConstraint.activate([
            button.heightAnchor.constraint(equalToConstant: 36),
            icon.leadingAnchor.constraint(equalTo: button.leadingAnchor, constant: 13),
            icon.centerYAnchor.constraint(equalTo: button.centerYAnchor),
            icon.widthAnchor.constraint(equalToConstant: 15),
            icon.heightAnchor.constraint(equalToConstant: 15),
            label.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: 11),
            label.centerYAnchor.constraint(equalTo: button.centerYAnchor),
            label.trailingAnchor.constraint(lessThanOrEqualTo: button.trailingAnchor, constant: -13),
        ])
        return button
    }

    @objc private func togglePlusMenu() {
        plusMenu.isHidden.toggle()
        if !plusMenu.isHidden {
            plusButton.layer?.backgroundColor = Tok.accentBg.cgColor
            plusButton.layer?.borderColor = Tok.accentBgStrong.cgColor
            plusButton.contentTintColor = Tok.accentTx
        } else {
            plusButton.layer?.backgroundColor = Tok.glassHi.cgColor
            plusButton.layer?.borderColor = Tok.hairline.cgColor
            plusButton.contentTintColor = Tok.tx2
        }
    }

    private func closePlusMenu() {
        guard !plusMenu.isHidden else { return }
        togglePlusMenu()
    }

    @objc private func menuAttachClicked() { closePlusMenu(); emitSimple("attach_requested") }
    @objc private func menuCapturePageClicked() { closePlusMenu(); emitActivePageCaptureRequested() }
    @objc private func menuNewSessionClicked() { closePlusMenu(); emitSimple("session_new_requested") }
    @objc private func menuRecapClicked() { closePlusMenu(); emitRecapRequested() }
    @objc private func menuAnswerStyleClicked() { closePlusMenu(); emitInstructionsRequested() }
    @objc private func menuOpacityClicked() { closePlusMenu(); showOpacityPopover() }

    // MARK: Phase 5 — History body (`.hx`): search · groups · rows · show-more

    private func configureHistoryBody() {
        historyContainer.translatesAutoresizingMaskIntoConstraints = false
        historyContainer.isHidden = true

        // Search field (`.srch`).
        let srch = NSView()
        srch.translatesAutoresizingMaskIntoConstraints = false
        srch.wantsLayer = true
        srch.layer?.backgroundColor = Tok.glassHi.cgColor
        srch.layer?.cornerRadius = Tok.rMd
        srch.layer?.borderWidth = 1
        srch.layer?.borderColor = Tok.hairline.cgColor

        let magnifier = NSImageView()
        magnifier.translatesAutoresizingMaskIntoConstraints = false
        if let image = symbolImage("magnifyingglass") {
            image.isTemplate = true
            magnifier.image = image
        }
        magnifier.contentTintColor = Tok.tx3

        searchField.translatesAutoresizingMaskIntoConstraints = false
        searchField.font = Tok.font(13, .regular)
        searchField.textColor = Tok.tx1
        searchField.isBezeled = false
        searchField.drawsBackground = false
        searchField.focusRingType = .none
        searchField.placeholderString = "Search sessions…"
        searchField.delegate = self
        searchField.target = self
        searchField.action = #selector(searchSubmitted)

        srch.addSubview(magnifier)
        srch.addSubview(searchField)

        historyScroll.translatesAutoresizingMaskIntoConstraints = false
        historyScroll.hasVerticalScroller = true
        historyScroll.drawsBackground = false
        historyStack.translatesAutoresizingMaskIntoConstraints = false
        historyStack.orientation = .vertical
        historyStack.alignment = .leading
        historyStack.spacing = 0
        historyStack.edgeInsets = NSEdgeInsets(top: 0, left: 8, bottom: 10, right: 8)
        historyScroll.documentView = historyStack

        historyContainer.addSubview(srch)
        historyContainer.addSubview(historyScroll)
        NSLayoutConstraint.activate([
            srch.topAnchor.constraint(equalTo: historyContainer.topAnchor, constant: 12),
            srch.leadingAnchor.constraint(equalTo: historyContainer.leadingAnchor, constant: 14),
            srch.trailingAnchor.constraint(equalTo: historyContainer.trailingAnchor, constant: -14),
            srch.heightAnchor.constraint(equalToConstant: 36),
            magnifier.leadingAnchor.constraint(equalTo: srch.leadingAnchor, constant: 12),
            magnifier.centerYAnchor.constraint(equalTo: srch.centerYAnchor),
            magnifier.widthAnchor.constraint(equalToConstant: 14),
            magnifier.heightAnchor.constraint(equalToConstant: 14),
            searchField.leadingAnchor.constraint(equalTo: magnifier.trailingAnchor, constant: 9),
            searchField.centerYAnchor.constraint(equalTo: srch.centerYAnchor),
            searchField.trailingAnchor.constraint(equalTo: srch.trailingAnchor, constant: -12),

            historyScroll.topAnchor.constraint(equalTo: srch.bottomAnchor, constant: 8),
            historyScroll.leadingAnchor.constraint(equalTo: historyContainer.leadingAnchor, constant: 6),
            historyScroll.trailingAnchor.constraint(equalTo: historyContainer.trailingAnchor, constant: -6),
            historyScroll.bottomAnchor.constraint(equalTo: historyContainer.bottomAnchor),
            historyStack.widthAnchor.constraint(equalTo: historyScroll.widthAnchor),
        ])
    }

    // MARK: Phase 6 — Agents body (`.ax`): agent cards + capability badges

    private func configureAgentsBody() {
        agentsScroll.translatesAutoresizingMaskIntoConstraints = false
        agentsScroll.hasVerticalScroller = true
        agentsScroll.drawsBackground = false
        agentsScroll.isHidden = true
        agentsStack.translatesAutoresizingMaskIntoConstraints = false
        agentsStack.orientation = .vertical
        agentsStack.alignment = .leading
        agentsStack.spacing = 10
        agentsStack.edgeInsets = NSEdgeInsets(top: 14, left: 14, bottom: 8, right: 14)
        agentsScroll.documentView = agentsStack
        NSLayoutConstraint.activate([
            agentsStack.widthAnchor.constraint(equalTo: agentsScroll.widthAnchor),
        ])
    }

    // MARK: Root assembly

    private func assembleLayout() {
        for v in [workspace, historyContainer, agentsScroll, contextBar, attachmentStrip,
                  composerBar, footerBar, headerBar, plusMenu,
                  connectorSheetOverlay, billingOverlay, closeConfirmOverlay, toastView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            addSubview(v)
        }

        // Body region fills between header and the composer/context block.
        NSLayoutConstraint.activate([
            headerBar.topAnchor.constraint(equalTo: topAnchor),
            headerBar.leadingAnchor.constraint(equalTo: leadingAnchor),
            headerBar.trailingAnchor.constraint(equalTo: trailingAnchor),

            // Footer pinned bottom (`.foot` pad 0/16/13).
            footerBar.leadingAnchor.constraint(equalTo: leadingAnchor),
            footerBar.trailingAnchor.constraint(equalTo: trailingAnchor),
            footerBar.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -13),

            // Composer (`.comp` margin 8×14) above the footer.
            composerBar.leadingAnchor.constraint(equalTo: leadingAnchor),
            composerBar.trailingAnchor.constraint(equalTo: trailingAnchor),
            composerBar.bottomAnchor.constraint(equalTo: footerBar.topAnchor, constant: -8),

            // Attachment strip (`.attach`) above composer (collapses when empty).
            attachmentStrip.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 14),
            attachmentStrip.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -14),
            attachmentStrip.bottomAnchor.constraint(equalTo: composerBar.topAnchor, constant: -2),

            // Context bar (`.ctxbar` margin 0×16) above composer.
            contextBar.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            contextBar.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            contextBar.bottomAnchor.constraint(equalTo: attachmentStrip.topAnchor, constant: -2),

            // Body region.
            workspace.topAnchor.constraint(equalTo: headerBar.bottomAnchor),
            workspace.leadingAnchor.constraint(equalTo: leadingAnchor),
            workspace.trailingAnchor.constraint(equalTo: trailingAnchor),
            workspace.bottomAnchor.constraint(equalTo: contextBar.topAnchor, constant: -4),

            historyContainer.topAnchor.constraint(equalTo: headerBar.bottomAnchor),
            historyContainer.leadingAnchor.constraint(equalTo: leadingAnchor),
            historyContainer.trailingAnchor.constraint(equalTo: trailingAnchor),
            historyContainer.bottomAnchor.constraint(equalTo: composerBar.topAnchor, constant: -4),

            agentsScroll.topAnchor.constraint(equalTo: headerBar.bottomAnchor),
            agentsScroll.leadingAnchor.constraint(equalTo: leadingAnchor),
            agentsScroll.trailingAnchor.constraint(equalTo: trailingAnchor),
            agentsScroll.bottomAnchor.constraint(equalTo: composerBar.topAnchor, constant: -4),
        ])

        // The "+" menu floats above the composer's plus button.
        NSLayoutConstraint.activate([
            plusMenu.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 18),
            plusMenu.bottomAnchor.constraint(equalTo: composerBar.topAnchor, constant: 4),
        ])

        // Full-bleed modal overlays.
        for overlay in [connectorSheetOverlay, billingOverlay, closeConfirmOverlay] {
            NSLayoutConstraint.activate([
                overlay.topAnchor.constraint(equalTo: topAnchor),
                overlay.leadingAnchor.constraint(equalTo: leadingAnchor),
                overlay.trailingAnchor.constraint(equalTo: trailingAnchor),
                overlay.bottomAnchor.constraint(equalTo: bottomAnchor),
            ])
        }

        // System toast at the top of the body. (The toast's own subviews —
        // title/body labels — are added inside configureSystemToast(); here we
        // only position the toast container within self.)
        NSLayoutConstraint.activate([
            toastView.topAnchor.constraint(equalTo: headerBar.bottomAnchor, constant: 12),
            toastView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            toastView.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
        ])

        // Z-order: header + composer + footer above the body; toast + menu + modals on top.
        headerBar.layer?.zPosition = 50
        composerBar.layer?.zPosition = 50
        footerBar.layer?.zPosition = 50
        contextBar.layer?.zPosition = 45
        attachmentStrip.layer?.zPosition = 45
        toastView.layer?.zPosition = 70
        plusMenu.layer?.zPosition = 80
        connectorSheetOverlay.layer?.zPosition = 90
        billingOverlay.layer?.zPosition = 90
        closeConfirmOverlay.layer?.zPosition = 90
    }

    // MARK: System toast (transient cards)

    private func configureSystemToast() {
        toastView.wantsLayer = true
        toastView.layer?.backgroundColor = Tok.modalFill.cgColor
        toastView.layer?.cornerRadius = Tok.rLg
        toastView.layer?.borderWidth = 1
        toastView.layer?.borderColor = Tok.hairlineStrong.cgColor
        toastView.isHidden = true
        toastTitleLabel.translatesAutoresizingMaskIntoConstraints = false
        toastTitleLabel.font = Tok.font(12, .semibold)
        toastTitleLabel.textColor = Tok.tx1
        toastBodyLabel.translatesAutoresizingMaskIntoConstraints = false
        toastBodyLabel.font = Tok.font(11.5, .regular)
        toastBodyLabel.textColor = Tok.tx2
        toastBodyLabel.maximumNumberOfLines = 3
        toastBodyLabel.preferredMaxLayoutWidth = 460
        toastView.addSubview(toastTitleLabel)
        toastView.addSubview(toastBodyLabel)
        NSLayoutConstraint.activate([
            toastTitleLabel.topAnchor.constraint(equalTo: toastView.topAnchor, constant: 11),
            toastTitleLabel.leadingAnchor.constraint(equalTo: toastView.leadingAnchor, constant: 13),
            toastTitleLabel.trailingAnchor.constraint(equalTo: toastView.trailingAnchor, constant: -13),
            toastBodyLabel.topAnchor.constraint(equalTo: toastTitleLabel.bottomAnchor, constant: 4),
            toastBodyLabel.leadingAnchor.constraint(equalTo: toastView.leadingAnchor, constant: 13),
            toastBodyLabel.trailingAnchor.constraint(equalTo: toastView.trailingAnchor, constant: -13),
            toastBodyLabel.bottomAnchor.constraint(equalTo: toastView.bottomAnchor, constant: -11),
        ])
    }

    // MARK: Phase 7 — Modals (dark, readable, over a dim backdrop)

    private func styleModalOverlay(_ overlay: ModalBlockerView) {
        overlay.translatesAutoresizingMaskIntoConstraints = false
        overlay.wantsLayer = true
        overlay.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.42).cgColor
        overlay.isHidden = true
    }

    private func styleModalPanel(_ panel: NSView, width: CGFloat) {
        panel.translatesAutoresizingMaskIntoConstraints = false
        panel.wantsLayer = true
        panel.layer?.backgroundColor = Tok.modalFill.cgColor
        panel.layer?.cornerRadius = Tok.rXl
        panel.layer?.borderWidth = 1
        panel.layer?.borderColor = Tok.hairlineStrong.cgColor
        panel.layer?.masksToBounds = true
        panel.widthAnchor.constraint(equalToConstant: width).isActive = true
    }

    private func makeModalButton(_ title: String, primary: Bool, danger: Bool = false, action: Selector) -> NSButton {
        let button = NSButton(title: title, target: self, action: action)
        button.translatesAutoresizingMaskIntoConstraints = false
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 9
        let fill: NSColor = danger ? Tok.danger : (primary ? Tok.accent : NSColor.clear)
        button.layer?.backgroundColor = fill.cgColor
        if !primary && !danger {
            button.layer?.borderWidth = 1
            button.layer?.borderColor = Tok.hairline.cgColor
        }
        let titleColor: NSColor = (primary || danger) ? .white : Tok.tx2
        button.attributedTitle = NSAttributedString(
            string: title,
            attributes: [.font: Tok.font(12, .semibold), .foregroundColor: titleColor])
        button.heightAnchor.constraint(equalToConstant: 30).isActive = true
        button.widthAnchor.constraint(greaterThanOrEqualToConstant: 84).isActive = true
        return button
    }

    // --- Connector sheet (auth-tier rows; expired → quiet "/mcp" status) ---

    private func configureConnectorSheet() {
        styleModalOverlay(connectorSheetOverlay)
        styleModalPanel(connectorSheetPanel, width: 380)

        connectorSheetTitle.translatesAutoresizingMaskIntoConstraints = false
        connectorSheetTitle.font = Tok.font(14.5, .semibold)
        connectorSheetTitle.textColor = Tok.tx1
        connectorSheetSummary.translatesAutoresizingMaskIntoConstraints = false
        connectorSheetSummary.font = Tok.font(12, .regular)
        connectorSheetSummary.textColor = Tok.tx3
        connectorSheetSummary.stringValue = "shape & readiness only, never secrets."
        connectorSheetSummary.maximumNumberOfLines = 2

        connectorSheetScroll.translatesAutoresizingMaskIntoConstraints = false
        connectorSheetScroll.hasVerticalScroller = true
        connectorSheetScroll.drawsBackground = false
        connectorSheetStack.translatesAutoresizingMaskIntoConstraints = false
        connectorSheetStack.orientation = .vertical
        connectorSheetStack.alignment = .leading
        connectorSheetStack.spacing = 0
        connectorSheetScroll.documentView = connectorSheetStack

        connectorSheetReauthLabel.translatesAutoresizingMaskIntoConstraints = false
        connectorSheetReauthLabel.font = Tok.font(11, .regular)
        connectorSheetReauthLabel.textColor = Tok.tx3
        connectorSheetReauthLabel.maximumNumberOfLines = 2
        connectorSheetReauthLabel.isHidden = true

        connectorSheetCancelButton.translatesAutoresizingMaskIntoConstraints = false
        let cancel = makeModalButton("Cancel", primary: false, action: #selector(connectorSheetCancelClicked))
        let attach = makeModalButton("Attach", primary: true, action: #selector(connectorSheetAttachClicked))

        connectorSheetOverlay.addSubview(connectorSheetPanel)
        connectorSheetPanel.addSubview(connectorSheetTitle)
        connectorSheetPanel.addSubview(connectorSheetSummary)
        connectorSheetPanel.addSubview(connectorSheetScroll)
        connectorSheetPanel.addSubview(connectorSheetReauthLabel)
        let footer = NSStackView(views: [cancel, attach])
        footer.translatesAutoresizingMaskIntoConstraints = false
        footer.orientation = .horizontal
        footer.spacing = 9
        connectorSheetPanel.addSubview(footer)

        NSLayoutConstraint.activate([
            connectorSheetPanel.centerXAnchor.constraint(equalTo: connectorSheetOverlay.centerXAnchor),
            connectorSheetPanel.centerYAnchor.constraint(equalTo: connectorSheetOverlay.centerYAnchor),

            connectorSheetTitle.topAnchor.constraint(equalTo: connectorSheetPanel.topAnchor, constant: 15),
            connectorSheetTitle.leadingAnchor.constraint(equalTo: connectorSheetPanel.leadingAnchor, constant: 17),
            connectorSheetTitle.trailingAnchor.constraint(equalTo: connectorSheetPanel.trailingAnchor, constant: -17),
            connectorSheetSummary.topAnchor.constraint(equalTo: connectorSheetTitle.bottomAnchor, constant: 4),
            connectorSheetSummary.leadingAnchor.constraint(equalTo: connectorSheetPanel.leadingAnchor, constant: 17),
            connectorSheetSummary.trailingAnchor.constraint(equalTo: connectorSheetPanel.trailingAnchor, constant: -17),

            connectorSheetScroll.topAnchor.constraint(equalTo: connectorSheetSummary.bottomAnchor, constant: 10),
            connectorSheetScroll.leadingAnchor.constraint(equalTo: connectorSheetPanel.leadingAnchor, constant: 17),
            connectorSheetScroll.trailingAnchor.constraint(equalTo: connectorSheetPanel.trailingAnchor, constant: -17),
            connectorSheetScroll.heightAnchor.constraint(lessThanOrEqualToConstant: 180),
            connectorSheetStack.widthAnchor.constraint(equalTo: connectorSheetScroll.widthAnchor),

            connectorSheetReauthLabel.topAnchor.constraint(equalTo: connectorSheetScroll.bottomAnchor, constant: 8),
            connectorSheetReauthLabel.leadingAnchor.constraint(equalTo: connectorSheetPanel.leadingAnchor, constant: 17),
            connectorSheetReauthLabel.trailingAnchor.constraint(equalTo: connectorSheetPanel.trailingAnchor, constant: -17),

            footer.topAnchor.constraint(equalTo: connectorSheetReauthLabel.bottomAnchor, constant: 12),
            footer.trailingAnchor.constraint(equalTo: connectorSheetPanel.trailingAnchor, constant: -17),
            footer.bottomAnchor.constraint(equalTo: connectorSheetPanel.bottomAnchor, constant: -15),
        ])
    }

    // --- BYOT billing disclosure (the G4 fix) ---

    private func configureBillingModal() {
        styleModalOverlay(billingOverlay)
        styleModalPanel(billingPanel, width: 380)

        billingTitle.translatesAutoresizingMaskIntoConstraints = false
        billingTitle.font = Tok.font(14.5, .semibold)
        billingTitle.textColor = Tok.tx1
        billingTitle.maximumNumberOfLines = 2
        billingSubtitle.translatesAutoresizingMaskIntoConstraints = false
        billingSubtitle.font = Tok.font(12, .regular)
        billingSubtitle.textColor = Tok.tx3

        let discBox = NSView()
        discBox.translatesAutoresizingMaskIntoConstraints = false
        discBox.wantsLayer = true
        discBox.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.22).cgColor
        discBox.layer?.cornerRadius = 9
        discBox.layer?.borderWidth = 1
        discBox.layer?.borderColor = Tok.hairline.cgColor
        billingDiscLabel.translatesAutoresizingMaskIntoConstraints = false
        billingDiscLabel.font = Tok.font(11.5, .regular)
        billingDiscLabel.textColor = Tok.tx2
        billingDiscLabel.maximumNumberOfLines = 0
        billingDiscLabel.preferredMaxLayoutWidth = 320
        discBox.addSubview(billingDiscLabel)

        let decline = makeModalButton("Decline", primary: false, action: #selector(billingDeclineClicked))
        let accept = makeModalButton("Accept & attach", primary: true, action: #selector(billingAcceptClicked))

        billingOverlay.addSubview(billingPanel)
        billingPanel.addSubview(billingTitle)
        billingPanel.addSubview(billingSubtitle)
        billingPanel.addSubview(discBox)
        let footer = NSStackView(views: [decline, accept])
        footer.translatesAutoresizingMaskIntoConstraints = false
        footer.orientation = .horizontal
        footer.spacing = 9
        billingPanel.addSubview(footer)

        NSLayoutConstraint.activate([
            billingPanel.centerXAnchor.constraint(equalTo: billingOverlay.centerXAnchor),
            billingPanel.centerYAnchor.constraint(equalTo: billingOverlay.centerYAnchor),

            billingTitle.topAnchor.constraint(equalTo: billingPanel.topAnchor, constant: 15),
            billingTitle.leadingAnchor.constraint(equalTo: billingPanel.leadingAnchor, constant: 17),
            billingTitle.trailingAnchor.constraint(equalTo: billingPanel.trailingAnchor, constant: -17),
            billingSubtitle.topAnchor.constraint(equalTo: billingTitle.bottomAnchor, constant: 4),
            billingSubtitle.leadingAnchor.constraint(equalTo: billingPanel.leadingAnchor, constant: 17),
            billingSubtitle.trailingAnchor.constraint(equalTo: billingPanel.trailingAnchor, constant: -17),

            discBox.topAnchor.constraint(equalTo: billingSubtitle.bottomAnchor, constant: 13),
            discBox.leadingAnchor.constraint(equalTo: billingPanel.leadingAnchor, constant: 17),
            discBox.trailingAnchor.constraint(equalTo: billingPanel.trailingAnchor, constant: -17),
            billingDiscLabel.topAnchor.constraint(equalTo: discBox.topAnchor, constant: 11),
            billingDiscLabel.leadingAnchor.constraint(equalTo: discBox.leadingAnchor, constant: 13),
            billingDiscLabel.trailingAnchor.constraint(equalTo: discBox.trailingAnchor, constant: -13),
            billingDiscLabel.bottomAnchor.constraint(equalTo: discBox.bottomAnchor, constant: -11),

            footer.topAnchor.constraint(equalTo: discBox.bottomAnchor, constant: 13),
            footer.trailingAnchor.constraint(equalTo: billingPanel.trailingAnchor, constant: -17),
            footer.bottomAnchor.constraint(equalTo: billingPanel.bottomAnchor, constant: -15),
        ])
    }

    // --- Turn-off confirm ---

    private func configureCloseConfirm() {
        styleModalOverlay(closeConfirmOverlay)
        styleModalPanel(closeConfirmPanel, width: 360)

        closeConfirmTitle.translatesAutoresizingMaskIntoConstraints = false
        closeConfirmTitle.font = Tok.font(14.5, .semibold)
        closeConfirmTitle.textColor = Tok.tx1
        closeConfirmBody.translatesAutoresizingMaskIntoConstraints = false
        closeConfirmBody.font = Tok.font(12, .regular)
        closeConfirmBody.textColor = Tok.tx3
        closeConfirmBody.maximumNumberOfLines = 0
        closeConfirmBody.preferredMaxLayoutWidth = 326

        let cancel = makeModalButton("Cancel", primary: false, action: #selector(cancelCloseConfirmClicked))
        closeConfirmCancelButton.translatesAutoresizingMaskIntoConstraints = false
        let turnOff = makeModalButton("Turn off", primary: false, danger: true, action: #selector(confirmTurnOffClicked))

        closeConfirmOverlay.addSubview(closeConfirmPanel)
        closeConfirmPanel.addSubview(closeConfirmTitle)
        closeConfirmPanel.addSubview(closeConfirmBody)
        let footer = NSStackView(views: [cancel, turnOff])
        footer.translatesAutoresizingMaskIntoConstraints = false
        footer.orientation = .horizontal
        footer.spacing = 9
        closeConfirmPanel.addSubview(footer)
        // Keep references so confirmDelete can repurpose the danger button.
        closeConfirmTurnOffButtonRef = turnOff

        NSLayoutConstraint.activate([
            closeConfirmPanel.centerXAnchor.constraint(equalTo: closeConfirmOverlay.centerXAnchor),
            closeConfirmPanel.centerYAnchor.constraint(equalTo: closeConfirmOverlay.centerYAnchor),
            closeConfirmTitle.topAnchor.constraint(equalTo: closeConfirmPanel.topAnchor, constant: 15),
            closeConfirmTitle.leadingAnchor.constraint(equalTo: closeConfirmPanel.leadingAnchor, constant: 17),
            closeConfirmTitle.trailingAnchor.constraint(equalTo: closeConfirmPanel.trailingAnchor, constant: -17),
            closeConfirmBody.topAnchor.constraint(equalTo: closeConfirmTitle.bottomAnchor, constant: 4),
            closeConfirmBody.leadingAnchor.constraint(equalTo: closeConfirmPanel.leadingAnchor, constant: 17),
            closeConfirmBody.trailingAnchor.constraint(equalTo: closeConfirmPanel.trailingAnchor, constant: -17),
            footer.topAnchor.constraint(equalTo: closeConfirmBody.bottomAnchor, constant: 13),
            footer.trailingAnchor.constraint(equalTo: closeConfirmPanel.trailingAnchor, constant: -17),
            footer.bottomAnchor.constraint(equalTo: closeConfirmPanel.bottomAnchor, constant: -15),
        ])
    }
    private var closeConfirmTurnOffButtonRef: NSButton?

    // MARK: Opacity popover (reuses the +menu surface look, small slider)

    private let opacityPopover = NSView()
    private let opacitySlider = NSSlider()
    private var opacityPopoverBuilt = false

    private func showOpacityPopover() {
        if !opacityPopoverBuilt { buildOpacityPopover() }
        opacityPopover.isHidden.toggle()
    }

    private func buildOpacityPopover() {
        opacityPopoverBuilt = true
        opacityPopover.translatesAutoresizingMaskIntoConstraints = false
        opacityPopover.wantsLayer = true
        opacityPopover.layer?.backgroundColor = Tok.modalFill.cgColor
        opacityPopover.layer?.cornerRadius = Tok.rLg
        opacityPopover.layer?.borderWidth = 1
        opacityPopover.layer?.borderColor = Tok.hairlineStrong.cgColor
        opacityPopover.isHidden = true
        let title = NSTextField(labelWithString: "Opacity")
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = Tok.font(11, .regular)
        title.textColor = Tok.tx3
        opacitySlider.translatesAutoresizingMaskIntoConstraints = false
        opacitySlider.minValue = 0.50
        opacitySlider.maxValue = 1.0
        opacitySlider.doubleValue = Double(backgroundOpacity)
        opacitySlider.target = self
        opacitySlider.action = #selector(opacityChanged)
        opacityPopover.addSubview(title)
        opacityPopover.addSubview(opacitySlider)
        addSubview(opacityPopover)
        opacityPopover.layer?.zPosition = 80
        NSLayoutConstraint.activate([
            opacityPopover.widthAnchor.constraint(equalToConstant: 210),
            opacityPopover.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 18),
            opacityPopover.bottomAnchor.constraint(equalTo: composerBar.topAnchor, constant: 4),
            title.topAnchor.constraint(equalTo: opacityPopover.topAnchor, constant: 12),
            title.leadingAnchor.constraint(equalTo: opacityPopover.leadingAnchor, constant: 13),
            opacitySlider.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 8),
            opacitySlider.leadingAnchor.constraint(equalTo: opacityPopover.leadingAnchor, constant: 13),
            opacitySlider.trailingAnchor.constraint(equalTo: opacityPopover.trailingAnchor, constant: -13),
            opacitySlider.bottomAnchor.constraint(equalTo: opacityPopover.bottomAnchor, constant: -13),
        ])
    }

    @objc private func opacityChanged() {
        applyOpacity(opacitySlider.doubleValue)
    }

    // MARK: Modal actions

    func showTurnOffConfirmation() {
        closePlusMenu()
        pendingDeleteSessionId = nil
        closeConfirmTitle.stringValue = "Turn Bluey off?"
        closeConfirmBody.stringValue = "This closes Bluey completely. To start again, run: bluey on"
        closeConfirmTurnOffButtonRef?.attributedTitle = NSAttributedString(
            string: "Turn off",
            attributes: [.font: Tok.font(12, .semibold), .foregroundColor: NSColor.white])
        closeConfirmTurnOffButtonRef?.action = #selector(confirmTurnOffClicked)
        presentOverlay(closeConfirmOverlay)
    }

    @objc private func cancelCloseConfirmClicked() { dismissCloseConfirm(animated: true) }

    private func dismissCloseConfirm(animated: Bool) {
        dismissOverlay(closeConfirmOverlay, animated: animated)
        pendingDeleteSessionId = nil
    }

    @objc private func confirmTurnOffClicked() {
        emitSimple("close_requested")
        dismissCloseConfirm(animated: false)
    }

    @objc private func confirmDeleteSessionClicked() {
        if let id = pendingDeleteSessionId { emitSessionDelete(id: id) }
        dismissCloseConfirm(animated: true)
    }

    // Billing.

    func presentBillingDisclosure(_ disclosure: BillingDisclosure) {
        pendingBilling = disclosure
        billingTitle.stringValue = "Before attaching \(disclosure.vendorDisplayName)"
        billingDiscLabel.stringValue = disclosure.disclosure.isEmpty
            ? "This uses your own API key. Usage is billed to your account by the vendor, not Bluey."
            : disclosure.disclosure
        presentOverlay(billingOverlay)
    }

    @objc private func billingDeclineClicked() { declineBilling() }

    private func declineBilling() {
        if let d = pendingBilling {
            emitBillingDisclosureResponded(
                vendorShort: d.vendorShort, accepted: false,
                pendingKind: d.pendingKind, pendingSessionId: d.pendingSessionId)
        }
        pendingBilling = nil
        dismissOverlay(billingOverlay, animated: true)
    }

    @objc private func billingAcceptClicked() {
        if let d = pendingBilling {
            emitBillingDisclosureResponded(
                vendorShort: d.vendorShort, accepted: true,
                pendingKind: d.pendingKind, pendingSessionId: d.pendingSessionId)
        }
        pendingBilling = nil
        dismissOverlay(billingOverlay, animated: true)
    }

    // Overlay show/hide.

    private func presentOverlay(_ overlay: ModalBlockerView) {
        overlay.isHidden = false
        overlay.alphaValue = 0
        window?.makeFirstResponder(overlay)
        NSAnimationContext.runAnimationGroup { ctx in
            ctx.duration = 0.12
            overlay.animator().alphaValue = 1
        }
    }

    private func dismissOverlay(_ overlay: ModalBlockerView, animated: Bool) {
        guard !overlay.isHidden else { return }
        guard animated else { overlay.isHidden = true; overlay.alphaValue = 1; return }
        NSAnimationContext.runAnimationGroup({ ctx in
            ctx.duration = 0.10
            overlay.animator().alphaValue = 0
        }, completionHandler: { overlay.isHidden = true; overlay.alphaValue = 1 })
    }

    // MARK: History — sessions IPC + grouped rendering

    private func requestSessions(reset: Bool) {
        if reset { sessionItems = []; sessionTotal = 0; sessionHasMore = false }
        emitSessionsRequested(offset: reset ? 0 : sessionItems.count, limit: 40, search: sessionQuery)
    }

    func setSessions(_ sessions: [OverlaySessionItem]) {
        // Legacy one-shot path: treat as a full page.
        sessionItems = sessions
        sessionTotal = sessions.count
        sessionHasMore = false
        sessionsLoaded = true
        renderHistory()
        updateFooter()
    }

    func setSessionsPage(sessions: [OverlaySessionItem], total: Int, offset: Int, hasMore: Bool, query: String) {
        // Ignore a stale reply that no longer matches what's typed.
        guard query == sessionQuery else { return }
        if offset == 0 {
            sessionItems = sessions
        } else {
            // Append, de-duping by id.
            let existing = Set(sessionItems.map { $0.id })
            sessionItems.append(contentsOf: sessions.filter { !existing.contains($0.id) })
        }
        sessionTotal = total
        sessionHasMore = hasMore
        sessionsLoaded = true
        renderHistory()
        updateFooter()
    }

    @objc private func searchSubmitted() { /* handled live in controlTextDidChange */ }

    func controlTextDidChange(_ obj: Notification) {
        guard let field = obj.object as? NSTextField, field === searchField else { return }
        sessionQuery = field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        requestSessions(reset: true)
    }

    private func clearHistoryStack() {
        for v in historyStack.arrangedSubviews {
            historyStack.removeArrangedSubview(v)
            v.removeFromSuperview()
        }
    }

    private func renderHistory() {
        clearHistoryStack()
        searchField.placeholderString = "Search \(sessionTotal) session\(sessionTotal == 1 ? "" : "s")…"

        // Bucket: Pinned, Today, Yesterday, Earlier, Older.
        var pinned: [OverlaySessionItem] = []
        var today: [OverlaySessionItem] = []
        var yesterday: [OverlaySessionItem] = []
        var earlier: [OverlaySessionItem] = []
        var older: [OverlaySessionItem] = []
        for s in sessionItems {
            if s.pinned { pinned.append(s); continue }
            switch dateBucket(s.updatedAt) {
            case .today: today.append(s)
            case .yesterday: yesterday.append(s)
            case .earlier: earlier.append(s)
            case .older: older.append(s)
            }
        }

        func addGroup(_ title: String, _ rows: [OverlaySessionItem]) {
            guard !rows.isEmpty else { return }
            addHistoryRow(makeGroupHeader(title))
            for s in rows { addHistoryRow(makeSessionRow(s)) }
        }
        if sessionItems.isEmpty {
            addHistoryRow(makeHistoryEmpty())
        } else {
            addGroup("Pinned", pinned)
            addGroup("Today", today)
            addGroup("Yesterday", yesterday)
            addGroup("Earlier", earlier)
            addGroup("Older", older)
            if sessionHasMore {
                let remaining = max(0, sessionTotal - sessionItems.count)
                addHistoryRow(makeShowMoreRow(remaining))
            }
        }
    }

    private func addHistoryRow(_ view: NSView) {
        historyStack.addArrangedSubview(view)
        view.widthAnchor.constraint(equalTo: historyStack.widthAnchor, constant: -16).isActive = true
    }

    private enum DateBucket { case today, yesterday, earlier, older }

    private func dateBucket(_ updatedAt: String) -> DateBucket {
        guard let date = parseTimestamp(updatedAt) else { return .older }
        let cal = Calendar.current
        if cal.isDateInToday(date) { return .today }
        if cal.isDateInYesterday(date) { return .yesterday }
        if let days = cal.dateComponents([.day], from: date, to: Date()).day, days <= 7 { return .earlier }
        return .older
    }

    private func parseTimestamp(_ raw: String) -> Date? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        if let epoch = Double(trimmed) {
            // Heuristic: ms vs s.
            return Date(timeIntervalSince1970: epoch > 1_000_000_000_000 ? epoch / 1000 : epoch)
        }
        let iso = ISO8601DateFormatter()
        return iso.date(from: trimmed)
    }

    private func relativeTime(_ raw: String) -> String {
        guard let date = parseTimestamp(raw) else { return "" }
        let cal = Calendar.current
        if cal.isDateInToday(date) {
            let f = DateFormatter(); f.dateFormat = "HH:mm"; return f.string(from: date)
        }
        if cal.isDateInYesterday(date) { return "Yest" }
        if let days = cal.dateComponents([.day], from: date, to: Date()).day, days <= 7 { return "\(days)d" }
        let f = DateFormatter(); f.dateFormat = "MMM d"; return f.string(from: date)
    }

    private func makeGroupHeader(_ title: String) -> NSView {
        let wrap = NSView()
        wrap.translatesAutoresizingMaskIntoConstraints = false
        let label = trackedLabel(title.uppercased(), size: 10.5, weight: .heavy, color: Tok.tx3, tracking: 0.6)
        let line = NSView()
        line.translatesAutoresizingMaskIntoConstraints = false
        line.wantsLayer = true
        line.layer?.backgroundColor = Tok.hairline.cgColor
        wrap.addSubview(label)
        wrap.addSubview(line)
        NSLayoutConstraint.activate([
            wrap.heightAnchor.constraint(equalToConstant: 30),
            label.leadingAnchor.constraint(equalTo: wrap.leadingAnchor, constant: 8),
            label.bottomAnchor.constraint(equalTo: wrap.bottomAnchor, constant: -5),
            line.leadingAnchor.constraint(equalTo: label.trailingAnchor, constant: 9),
            line.trailingAnchor.constraint(equalTo: wrap.trailingAnchor, constant: -8),
            line.centerYAnchor.constraint(equalTo: label.centerYAnchor),
            line.heightAnchor.constraint(equalToConstant: 1),
        ])
        return wrap
    }

    private func makeHistoryEmpty() -> NSView {
        let wrap = NSView()
        wrap.translatesAutoresizingMaskIntoConstraints = false
        let label = NSTextField(labelWithString: sessionQuery.isEmpty ? "No sessions yet." : "No matches.")
        label.translatesAutoresizingMaskIntoConstraints = false
        label.font = Tok.font(12.5, .regular)
        label.textColor = Tok.tx3
        wrap.addSubview(label)
        NSLayoutConstraint.activate([
            wrap.heightAnchor.constraint(equalToConstant: 60),
            label.centerXAnchor.constraint(equalTo: wrap.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: wrap.centerYAnchor),
        ])
        return wrap
    }

    private func makeShowMoreRow(_ remaining: Int) -> NSView {
        let button = NSButton(title: "", target: self, action: #selector(showMoreClicked))
        button.translatesAutoresizingMaskIntoConstraints = false
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = Tok.rMd
        button.layer?.borderWidth = 1
        button.layer?.borderColor = Tok.hairline.cgColor
        button.attributedTitle = NSAttributedString(
            string: "Show \(remaining) more",
            attributes: [.font: Tok.font(12, .semibold), .foregroundColor: Tok.accentTx])
        button.heightAnchor.constraint(equalToConstant: 38).isActive = true
        return button
    }

    @objc private func showMoreClicked() { requestSessions(reset: false) }

    private func makeSessionRow(_ session: OverlaySessionItem) -> NSView {
        if editingSessionId == session.id { return makeRenameRow(session) }
        let row = SessionRowView()
        row.sessionId = session.id
        row.rowMenuTarget = self
        row.renameAction = #selector(renameSessionMenuClicked(_:))
        row.deleteAction = #selector(deleteSessionMenuClicked(_:))
        row.translatesAutoresizingMaskIntoConstraints = false
        row.wantsLayer = true
        row.layer?.cornerRadius = Tok.rMd
        let selected = session.isActive
        row.layer?.backgroundColor = selected ? Tok.accentBg.cgColor : NSColor.clear.cgColor
        if selected {
            row.layer?.borderWidth = 1
            row.layer?.borderColor = Tok.accentBgStrong.cgColor
        }

        let openButton = NSButton(title: "", target: self, action: #selector(sessionRowClicked(_:)))
        openButton.translatesAutoresizingMaskIntoConstraints = false
        openButton.isBordered = false
        openButton.tag = sessionIndex(session.id)

        // 28px rail icon.
        let icon = NSView()
        icon.translatesAutoresizingMaskIntoConstraints = false
        icon.wantsLayer = true
        icon.layer?.cornerRadius = 8
        icon.layer?.backgroundColor = selected ? Tok.accentBgStrong.cgColor : Tok.glassHi.cgColor
        let iconImage = NSImageView()
        iconImage.translatesAutoresizingMaskIntoConstraints = false
        if let image = symbolImage("bubble.left.and.bubble.right") {
            image.isTemplate = true
            iconImage.image = image
        }
        iconImage.contentTintColor = selected ? Tok.accentTx : Tok.tx3
        icon.addSubview(iconImage)

        let title = NSTextField(labelWithString: session.title)
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = Tok.font(13, .regular)
        title.textColor = Tok.tx1
        title.lineBreakMode = .byTruncatingTail

        let subText = sessionSubtitle(session)
        let sub = NSTextField(labelWithString: subText)
        sub.translatesAutoresizingMaskIntoConstraints = false
        sub.font = Tok.font(11, .regular)
        sub.textColor = Tok.tx3
        sub.lineBreakMode = .byTruncatingTail

        // Pin toggle.
        let pin = NSButton(title: "", target: self, action: #selector(togglePinClicked(_:)))
        pin.translatesAutoresizingMaskIntoConstraints = false
        pin.isBordered = false
        pin.tag = sessionIndex(session.id)
        pin.contentTintColor = session.pinned ? Tok.accentTx : Tok.tx4
        pin.toolTip = session.pinned ? "Unpin" : "Pin to top"
        if let image = symbolImage(session.pinned ? "pin.fill" : "pin") {
            image.isTemplate = true
            pin.image = image
            pin.imagePosition = .imageOnly
            pin.imageScaling = .scaleProportionallyDown
        }

        let time = trackedLabel(relativeTime(session.updatedAt), size: 10, weight: .regular, color: Tok.tx4, tracking: 0)

        row.addSubview(openButton)
        row.addSubview(icon)
        row.addSubview(title)
        row.addSubview(sub)
        row.addSubview(pin)
        row.addSubview(time)
        NSLayoutConstraint.activate([
            row.heightAnchor.constraint(equalToConstant: 48),
            openButton.topAnchor.constraint(equalTo: row.topAnchor),
            openButton.leadingAnchor.constraint(equalTo: row.leadingAnchor),
            openButton.bottomAnchor.constraint(equalTo: row.bottomAnchor),
            openButton.trailingAnchor.constraint(equalTo: pin.leadingAnchor),

            icon.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 10),
            icon.centerYAnchor.constraint(equalTo: row.centerYAnchor),
            icon.widthAnchor.constraint(equalToConstant: 28),
            icon.heightAnchor.constraint(equalToConstant: 28),
            iconImage.centerXAnchor.constraint(equalTo: icon.centerXAnchor),
            iconImage.centerYAnchor.constraint(equalTo: icon.centerYAnchor),
            iconImage.widthAnchor.constraint(equalToConstant: 14),
            iconImage.heightAnchor.constraint(equalToConstant: 14),

            title.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: 11),
            title.topAnchor.constraint(equalTo: row.topAnchor, constant: 8),
            title.trailingAnchor.constraint(lessThanOrEqualTo: pin.leadingAnchor, constant: -8),
            sub.leadingAnchor.constraint(equalTo: title.leadingAnchor),
            sub.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 1),
            sub.trailingAnchor.constraint(lessThanOrEqualTo: pin.leadingAnchor, constant: -8),

            pin.trailingAnchor.constraint(equalTo: time.leadingAnchor, constant: -6),
            pin.centerYAnchor.constraint(equalTo: row.centerYAnchor),
            pin.widthAnchor.constraint(equalToConstant: 22),
            pin.heightAnchor.constraint(equalToConstant: 22),
            time.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -10),
            time.centerYAnchor.constraint(equalTo: row.centerYAnchor),
        ])
        return row
    }

    private func sessionSubtitle(_ session: OverlaySessionItem) -> String {
        var parts: [String] = []
        if let project = session.project, !project.isEmpty {
            parts.append((project as NSString).lastPathComponent)
        }
        if let turns = session.turnCount {
            parts.append("\(turns) turn\(turns == 1 ? "" : "s")")
        } else if !session.subtitle.isEmpty {
            parts.append(session.subtitle)
        }
        if session.isActive { parts.append("continuing") }
        return parts.joined(separator: " · ")
    }

    private func makeRenameRow(_ session: OverlaySessionItem) -> NSView {
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false
        row.wantsLayer = true
        row.layer?.cornerRadius = Tok.rMd
        row.layer?.backgroundColor = Tok.glassHi.cgColor
        let field = NSTextField()
        field.translatesAutoresizingMaskIntoConstraints = false
        field.stringValue = session.title
        field.font = Tok.font(13, .regular)
        field.textColor = Tok.tx1
        field.isBezeled = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.target = self
        field.action = #selector(saveInlineRenameClicked(_:))
        field.tag = sessionIndex(session.id)
        renameField = field
        let save = NSButton(title: "Save", target: self, action: #selector(saveInlineRenameClicked(_:)))
        save.translatesAutoresizingMaskIntoConstraints = false
        save.isBordered = false
        save.tag = sessionIndex(session.id)
        save.contentTintColor = Tok.accentTx
        row.addSubview(field)
        row.addSubview(save)
        NSLayoutConstraint.activate([
            row.heightAnchor.constraint(equalToConstant: 44),
            field.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 12),
            field.centerYAnchor.constraint(equalTo: row.centerYAnchor),
            field.trailingAnchor.constraint(equalTo: save.leadingAnchor, constant: -8),
            save.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -10),
            save.centerYAnchor.constraint(equalTo: row.centerYAnchor),
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
        emitSessionOpen(id: session.id)
        setBodyTab(.ask)
    }

    @objc private func togglePinClicked(_ sender: NSButton) {
        guard sender.tag >= 0, sender.tag < sessionItems.count else { return }
        let session = sessionItems[sender.tag]
        if session.pinned { emitSessionUnpinRequested(id: session.id) }
        else { emitSessionPinRequested(id: session.id) }
    }

    // Right-click row actions (Rename / Delete) — must-survive session CRUD,
    // kept off the clean row face per the mockup.
    @objc private func renameSessionMenuClicked(_ sender: NSMenuItem) {
        guard let id = sender.representedObject as? String else { return }
        editingSessionId = id
        renderHistory()
    }

    @objc private func deleteSessionMenuClicked(_ sender: NSMenuItem) {
        guard let id = sender.representedObject as? String,
              let session = sessionItems.first(where: { $0.id == id }) else { return }
        pendingDeleteSessionId = id
        closeConfirmTitle.stringValue = "Delete session?"
        closeConfirmBody.stringValue = "Remove “\(session.title)” from this device. This cannot be undone."
        closeConfirmTurnOffButtonRef?.attributedTitle = NSAttributedString(
            string: "Delete",
            attributes: [.font: Tok.font(12, .semibold), .foregroundColor: NSColor.white])
        closeConfirmTurnOffButtonRef?.action = #selector(confirmDeleteSessionClicked)
        presentOverlay(closeConfirmOverlay)
    }

    @objc private func saveInlineRenameClicked(_ sender: NSControl) {
        guard sender.tag >= 0, sender.tag < sessionItems.count else { return }
        let session = sessionItems[sender.tag]
        let title = (renameField?.stringValue ?? session.title).trimmingCharacters(in: .whitespacesAndNewlines)
        editingSessionId = nil
        if !title.isEmpty, title != session.title {
            emitSessionRename(id: session.id, title: title)
        }
        renderHistory()
    }

    // MARK: Agents — IPC + card rendering

    func setAgents(_ agents: [AgentSummary]) {
        agentSummaries = agents
        agentListLoaded = true
        attachedAgentKind = agents.first(where: { $0.attached })?.kind
        onAgentAttachmentChanged?(attachedAgentKind != nil)
        updateViaLabel()
        if currentTab == .agents { renderAgents() }
        updateFooter()
    }

    func setAgentSessions(kind: String, sessions: [AgentSessionSummary]) {
        agentSessions = sessions
        agentSessionsLoaded = true
        if case .sessions(let k, _) = agentStage, k == kind { renderAgents() }
    }

    private func clearAgentsStack() {
        for v in agentsStack.arrangedSubviews {
            agentsStack.removeArrangedSubview(v)
            v.removeFromSuperview()
        }
    }

    private func addAgentsRow(_ view: NSView) {
        agentsStack.addArrangedSubview(view)
        view.widthAnchor.constraint(equalTo: agentsStack.widthAnchor, constant: -28).isActive = true
    }

    private func renderAgents() {
        clearAgentsStack()
        switch agentStage {
        case .picker:
            if !agentListLoaded {
                addAgentsRow(makeAgentMessage("Discovering your coding agents…", dim: true))
            } else if agentSummaries.isEmpty {
                addAgentsRow(makeAgentMessage("No coding agents found on this machine.", dim: true))
            } else {
                for agent in agentSummaries { addAgentsRow(makeAgentCard(agent)) }
            }
        case .sessions(let kind, let displayName):
            addAgentsRow(makeAgentSessionsHeader(displayName))
            if !agentSessionsLoaded {
                addAgentsRow(makeAgentMessage("Loading \(displayName) sessions…", dim: true))
            } else if agentSessions.isEmpty {
                addAgentsRow(makeAgentMessage("No prior sessions, or history is off. Enable agent history in Settings.", dim: true))
            } else {
                for session in agentSessions {
                    addAgentsRow(makeAgentSessionRow(kind: kind, session: session))
                }
            }
        }
    }

    private func makeAgentMessage(_ text: String, dim: Bool) -> NSView {
        let wrap = NSView()
        wrap.translatesAutoresizingMaskIntoConstraints = false
        let label = NSTextField(wrappingLabelWithString: text)
        label.translatesAutoresizingMaskIntoConstraints = false
        label.font = Tok.font(12.5, .regular)
        label.textColor = dim ? Tok.tx3 : Tok.tx1
        label.preferredMaxLayoutWidth = 460
        wrap.addSubview(label)
        NSLayoutConstraint.activate([
            wrap.heightAnchor.constraint(greaterThanOrEqualToConstant: 36),
            label.topAnchor.constraint(equalTo: wrap.topAnchor, constant: 8),
            label.leadingAnchor.constraint(equalTo: wrap.leadingAnchor),
            label.trailingAnchor.constraint(equalTo: wrap.trailingAnchor),
            label.bottomAnchor.constraint(equalTo: wrap.bottomAnchor, constant: -8),
        ])
        return wrap
    }

    /// `.acard` — agent card with capability badge + connector readiness + actions.
    private func makeAgentCard(_ agent: AgentSummary) -> NSView {
        let card = NSView()
        card.translatesAutoresizingMaskIntoConstraints = false
        card.wantsLayer = true
        card.layer?.cornerRadius = Tok.rLg
        card.layer?.borderWidth = 1
        if agent.attached {
            card.layer?.backgroundColor = Tok.accentBg.cgColor
            card.layer?.borderColor = Tok.accentBgStrong.cgColor
        } else {
            card.layer?.backgroundColor = Tok.glassHi.cgColor
            card.layer?.borderColor = Tok.hairline.cgColor
        }

        let icon = NSImageView()
        icon.translatesAutoresizingMaskIntoConstraints = false
        if let image = symbolImage(agent.attached ? "sparkle" : "cube") {
            image.isTemplate = true
            icon.image = image
        }
        icon.contentTintColor = agent.attached ? Tok.accentTx : Tok.tx2

        let name = NSTextField(labelWithString: agent.displayName)
        name.translatesAutoresizingMaskIntoConstraints = false
        name.font = Tok.font(13.5, .semibold)
        name.textColor = Tok.tx1

        let badge = makeCapabilityBadge(agent)

        let meta = NSTextField(labelWithString: agentMetaText(agent))
        meta.translatesAutoresizingMaskIntoConstraints = false
        meta.font = Tok.font(11, .regular)
        meta.textColor = Tok.tx3

        let actions = NSStackView()
        actions.translatesAutoresizingMaskIntoConstraints = false
        actions.orientation = .horizontal
        actions.spacing = 8
        let cap = agent.capability
        let canUse = cap != "cloud_blocked"
        if agent.attached {
            actions.addArrangedSubview(makeAgentActionButton("Stop using", primary: false, kind: agent.kind, action: #selector(agentDetachClicked)))
            actions.addArrangedSubview(makeAgentActionButton("View sessions", primary: false, kind: agent.kind, action: #selector(agentViewSessionsClicked(_:))))
        } else if canUse {
            actions.addArrangedSubview(makeAgentActionButton("Use this agent", primary: true, kind: agent.kind, action: #selector(agentUseClicked(_:))))
            actions.addArrangedSubview(makeAgentActionButton("View sessions", primary: false, kind: agent.kind, action: #selector(agentViewSessionsClicked(_:))))
        }

        card.addSubview(icon)
        card.addSubview(name)
        card.addSubview(badge)
        card.addSubview(meta)
        let hasActions = !actions.arrangedSubviews.isEmpty
        if hasActions { card.addSubview(actions) }

        var constraints: [NSLayoutConstraint] = [
            icon.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 14),
            icon.topAnchor.constraint(equalTo: card.topAnchor, constant: 14),
            icon.widthAnchor.constraint(equalToConstant: 15),
            icon.heightAnchor.constraint(equalToConstant: 15),
            name.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: 9),
            name.centerYAnchor.constraint(equalTo: icon.centerYAnchor),
            badge.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -14),
            badge.centerYAnchor.constraint(equalTo: icon.centerYAnchor),
            badge.leadingAnchor.constraint(greaterThanOrEqualTo: name.trailingAnchor, constant: 8),
            meta.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 14),
            meta.topAnchor.constraint(equalTo: icon.bottomAnchor, constant: 6),
            meta.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -14),
        ]
        if hasActions {
            constraints.append(contentsOf: [
                actions.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 14),
                actions.topAnchor.constraint(equalTo: meta.bottomAnchor, constant: 11),
                actions.bottomAnchor.constraint(equalTo: card.bottomAnchor, constant: -13),
            ])
        } else {
            constraints.append(meta.bottomAnchor.constraint(equalTo: card.bottomAnchor, constant: -13))
        }
        NSLayoutConstraint.activate(constraints)
        return card
    }

    private func makeCapabilityBadge(_ agent: AgentSummary) -> NSView {
        let (text, fill, fg): (String, NSColor, NSColor)
        switch agent.capability {
        case "drive":
            text = agent.attached ? "Active" : "Ready"
            fill = Tok.accentBg; fg = Tok.accentTx
        case "read_only":
            text = "Read-only"; fill = Tok.glassHi; fg = Tok.tx3
        case "needs_reauth", "needs_trust":
            text = "Needs re-auth"; fill = Tok.warn.withAlphaComponent(0.14); fg = Tok.warn
        case "cloud_blocked":
            text = "Cloud blocked"; fill = Tok.danger.withAlphaComponent(0.14); fg = Tok.danger
        default:
            text = agent.capability.replacingOccurrences(of: "_", with: " ").capitalized
            fill = Tok.glassHi; fg = Tok.tx3
        }
        let label = NSTextField(labelWithString: text)
        label.translatesAutoresizingMaskIntoConstraints = false
        label.font = Tok.font(10, .semibold)
        label.textColor = fg
        label.wantsLayer = true
        label.layer?.backgroundColor = fill.cgColor
        label.layer?.cornerRadius = 9
        useCenteredSingleLineCell(label)
        label.textColor = fg
        let wrap = NSView()
        wrap.translatesAutoresizingMaskIntoConstraints = false
        wrap.wantsLayer = true
        wrap.layer?.backgroundColor = fill.cgColor
        wrap.layer?.cornerRadius = 9
        wrap.addSubview(label)
        NSLayoutConstraint.activate([
            wrap.heightAnchor.constraint(equalToConstant: 18),
            label.topAnchor.constraint(equalTo: wrap.topAnchor),
            label.bottomAnchor.constraint(equalTo: wrap.bottomAnchor),
            label.leadingAnchor.constraint(equalTo: wrap.leadingAnchor, constant: 8),
            label.trailingAnchor.constraint(equalTo: wrap.trailingAnchor, constant: -8),
        ])
        label.layer?.backgroundColor = NSColor.clear.cgColor
        return wrap
    }

    private func agentMetaText(_ agent: AgentSummary) -> String {
        var s = "\(agent.readyConnectorCount)/\(agent.connectorCount) connectors ready"
        if let count = agent.sessionCount { s += " · \(count) session\(count == 1 ? "" : "s")" }
        return s
    }

    private func makeAgentActionButton(_ title: String, primary: Bool, kind: String, action: Selector) -> NSButton {
        let button = NSButton(title: title, target: self, action: action)
        button.translatesAutoresizingMaskIntoConstraints = false
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 9
        button.identifier = NSUserInterfaceItemIdentifier(kind)
        if primary {
            button.layer?.backgroundColor = Tok.accent.cgColor
        } else {
            button.layer?.backgroundColor = NSColor.clear.cgColor
            button.layer?.borderWidth = 1
            button.layer?.borderColor = Tok.hairline.cgColor
        }
        button.attributedTitle = NSAttributedString(
            string: title,
            attributes: [.font: Tok.font(12, .semibold), .foregroundColor: primary ? NSColor.white : Tok.tx2])
        button.heightAnchor.constraint(equalToConstant: 28).isActive = true
        button.widthAnchor.constraint(greaterThanOrEqualToConstant: 96).isActive = true
        return button
    }

    @objc private func agentUseClicked(_ sender: NSButton) {
        guard let kind = sender.identifier?.rawValue else { return }
        beginAttachFlow(kind: kind, sessionId: nil)
    }

    @objc private func agentDetachClicked() {
        emitAgentDetachRequested()
        attachedAgentKind = nil
        onAgentAttachmentChanged?(false)
        updateViaLabel()
        updateFooter()
    }

    @objc private func agentViewSessionsClicked(_ sender: NSButton) {
        guard let kind = sender.identifier?.rawValue,
              let agent = agentSummaries.first(where: { $0.kind == kind }) else { return }
        agentStage = .sessions(kind: kind, displayName: agent.displayName)
        agentSessions = []
        agentSessionsLoaded = false
        renderAgents()
        emitAgentSessionsRequested(kind: kind, offset: 0, limit: 40, search: "")
    }

    private func makeAgentSessionsHeader(_ displayName: String) -> NSView {
        let wrap = NSView()
        wrap.translatesAutoresizingMaskIntoConstraints = false
        let back = NSButton(title: "", target: self, action: #selector(agentBackClicked))
        back.translatesAutoresizingMaskIntoConstraints = false
        back.isBordered = false
        back.contentTintColor = Tok.tx2
        if let image = symbolImage("chevron.left") {
            image.isTemplate = true
            back.image = image
            back.imagePosition = .imageOnly
        } else { back.title = "‹" }
        let label = NSTextField(labelWithString: "\(displayName) · sessions")
        label.translatesAutoresizingMaskIntoConstraints = false
        label.font = Tok.font(13, .semibold)
        label.textColor = Tok.tx1
        wrap.addSubview(back)
        wrap.addSubview(label)
        NSLayoutConstraint.activate([
            wrap.heightAnchor.constraint(equalToConstant: 30),
            back.leadingAnchor.constraint(equalTo: wrap.leadingAnchor),
            back.centerYAnchor.constraint(equalTo: wrap.centerYAnchor),
            back.widthAnchor.constraint(equalToConstant: 22),
            back.heightAnchor.constraint(equalToConstant: 22),
            label.leadingAnchor.constraint(equalTo: back.trailingAnchor, constant: 6),
            label.centerYAnchor.constraint(equalTo: wrap.centerYAnchor),
        ])
        return wrap
    }

    @objc private func agentBackClicked() {
        agentStage = .picker
        renderAgents()
    }

    private func makeAgentSessionRow(kind: String, session: AgentSessionSummary) -> NSView {
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false
        row.wantsLayer = true
        row.layer?.cornerRadius = Tok.rMd
        row.layer?.backgroundColor = Tok.glassHi.cgColor
        row.layer?.borderWidth = 1
        row.layer?.borderColor = Tok.hairline.cgColor

        let button = NSButton(title: "", target: self, action: #selector(agentSessionRowClicked(_:)))
        button.translatesAutoresizingMaskIntoConstraints = false
        button.isBordered = false
        button.identifier = NSUserInterfaceItemIdentifier("\(kind)\u{1F}\(session.id)")

        let title = NSTextField(labelWithString: session.title ?? "Session \(session.id.prefix(8))")
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = Tok.font(13, .regular)
        title.textColor = Tok.tx1
        title.lineBreakMode = .byTruncatingTail

        let subParts = [session.project.map { ($0 as NSString).lastPathComponent }, relativeTime(session.updatedAt)].compactMap { $0 }.filter { !$0.isEmpty }
        let sub = NSTextField(labelWithString: subParts.joined(separator: " · "))
        sub.translatesAutoresizingMaskIntoConstraints = false
        sub.font = Tok.font(11, .regular)
        sub.textColor = Tok.tx3

        row.addSubview(button)
        row.addSubview(title)
        row.addSubview(sub)
        NSLayoutConstraint.activate([
            row.heightAnchor.constraint(equalToConstant: 46),
            button.topAnchor.constraint(equalTo: row.topAnchor),
            button.leadingAnchor.constraint(equalTo: row.leadingAnchor),
            button.trailingAnchor.constraint(equalTo: row.trailingAnchor),
            button.bottomAnchor.constraint(equalTo: row.bottomAnchor),
            title.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 12),
            title.topAnchor.constraint(equalTo: row.topAnchor, constant: 7),
            title.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -12),
            sub.leadingAnchor.constraint(equalTo: title.leadingAnchor),
            sub.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 1),
            sub.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -12),
        ])
        return row
    }

    @objc private func agentSessionRowClicked(_ sender: NSButton) {
        guard let raw = sender.identifier?.rawValue else { return }
        let parts = raw.components(separatedBy: "\u{1F}")
        guard parts.count == 2 else { return }
        beginAttachFlow(kind: parts[0], sessionId: parts[1])
    }

    private func updateViaLabel() {
        if let kind = attachedAgentKind, let agent = agentSummaries.first(where: { $0.kind == kind }) {
            brandLabel.stringValue = agent.displayName
            viaLabel.stringValue = "· your agent"
            statusDot.layer?.backgroundColor = Tok.accent.cgColor
        } else {
            brandLabel.stringValue = "Bluey"
            viaLabel.stringValue = "· managed"
            statusDot.layer?.backgroundColor = Tok.ok.cgColor
        }
    }

    // MARK: Connector sheet — IPC + rendering + attach flow

    private func beginAttachFlow(kind: String, sessionId: String?) {
        pendingConnectorKind = kind
        pendingConnectorSessionId = sessionId
        pendingConnectorInfos = []
        pendingConnectorsLoaded = false
        renderConnectorSheet()
        presentOverlay(connectorSheetOverlay)
        emitAgentConnectorsRequested(kind: kind)
    }

    func setAgentConnectors(kind: String, connectors: [AgentConnectorInfo]) {
        guard pendingConnectorKind == kind else { return }
        pendingConnectorInfos = connectors
        pendingConnectorsLoaded = true
        renderConnectorSheet()
    }

    private func renderConnectorSheet() {
        for v in connectorSheetStack.arrangedSubviews {
            connectorSheetStack.removeArrangedSubview(v)
            v.removeFromSuperview()
        }
        let name = pendingConnectorKind.map { kind in
            agentSummaries.first(where: { $0.kind == kind })?.displayName ?? agentShortLabel(kind).capitalized
        } ?? "agent"
        connectorSheetSummary.stringValue = "From \(name) · shape & readiness only, never secrets."
        var hasExpired = false
        if !pendingConnectorsLoaded {
            connectorSheetStack.addArrangedSubview(makeAgentMessage("Reading inherited connectors…", dim: true))
        } else if pendingConnectorInfos.isEmpty {
            connectorSheetStack.addArrangedSubview(makeAgentMessage("No inherited connectors.", dim: true))
        } else {
            for c in pendingConnectorInfos {
                if !c.ready { hasExpired = true }
                let row = makeConnectorRow(c)
                connectorSheetStack.addArrangedSubview(row)
                row.widthAnchor.constraint(equalTo: connectorSheetStack.widthAnchor).isActive = true
            }
        }
        // Quiet re-auth guidance (G3 — NO reauth button/event).
        if hasExpired, let kind = pendingConnectorKind {
            let agentName = agentSummaries.first(where: { $0.kind == kind })?.displayName ?? agentShortLabel(kind).capitalized
            connectorSheetReauthLabel.stringValue = "A connector login expired — run /mcp in \(agentName) to reconnect."
            connectorSheetReauthLabel.isHidden = false
        } else {
            connectorSheetReauthLabel.isHidden = true
        }
    }

    private func makeConnectorRow(_ connector: AgentConnectorInfo) -> NSView {
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false
        let name = NSTextField(labelWithString: connector.name)
        name.translatesAutoresizingMaskIntoConstraints = false
        name.font = Tok.font(12.5, .regular)
        name.textColor = Tok.tx1
        let tier = NSTextField(labelWithString: connector.authTier.replacingOccurrences(of: "_", with: "-"))
        tier.translatesAutoresizingMaskIntoConstraints = false
        tier.font = Tok.font(10, .regular)
        tier.textColor = Tok.tx3
        tier.wantsLayer = true
        tier.layer?.borderWidth = 1
        tier.layer?.borderColor = Tok.hairline.cgColor
        tier.layer?.cornerRadius = 9
        useCenteredSingleLineCell(tier)
        tier.textColor = Tok.tx3
        let status = NSTextField(labelWithString: connector.ready ? "ready" : "login expired")
        status.translatesAutoresizingMaskIntoConstraints = false
        status.font = Tok.font(11, .regular)
        status.textColor = connector.ready ? Tok.ok : Tok.warn
        let line = NSView()
        line.translatesAutoresizingMaskIntoConstraints = false
        line.wantsLayer = true
        line.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.05).cgColor
        row.addSubview(name)
        row.addSubview(tier)
        row.addSubview(status)
        row.addSubview(line)
        NSLayoutConstraint.activate([
            row.heightAnchor.constraint(equalToConstant: 38),
            name.leadingAnchor.constraint(equalTo: row.leadingAnchor),
            name.centerYAnchor.constraint(equalTo: row.centerYAnchor, constant: -1),
            tier.trailingAnchor.constraint(equalTo: status.leadingAnchor, constant: -8),
            tier.centerYAnchor.constraint(equalTo: name.centerYAnchor),
            tier.heightAnchor.constraint(equalToConstant: 17),
            status.trailingAnchor.constraint(equalTo: row.trailingAnchor),
            status.centerYAnchor.constraint(equalTo: name.centerYAnchor),
            line.leadingAnchor.constraint(equalTo: row.leadingAnchor),
            line.trailingAnchor.constraint(equalTo: row.trailingAnchor),
            line.bottomAnchor.constraint(equalTo: row.bottomAnchor),
            line.heightAnchor.constraint(equalToConstant: 1),
        ])
        return row
    }

    @objc private func connectorSheetCancelClicked() {
        pendingConnectorKind = nil
        dismissConnectorSheet()
    }

    @objc private func connectorSheetAttachClicked() {
        if let kind = pendingConnectorKind {
            emitAgentAttachRequested(kind: kind, sessionId: pendingConnectorSessionId)
            attachedAgentKind = kind
            onAgentAttachmentChanged?(true)
            updateViaLabel()
            updateFooter()
        }
        dismissConnectorSheet()
        agentStage = .picker
    }

    private func dismissConnectorSheet() {
        dismissOverlay(connectorSheetOverlay, animated: true)
    }

    // MARK: External API — cards / transcript / state

    func pushCard(_ card: RenderedCard) {
        if shouldRenderAsToast(card) {
            showSystemToast(for: card)
            return
        }
        feed.push(card)
        routeCanvasIfNeeded(card)
        if currentTab != .ask { setBodyTab(.ask) }
        updateContextBar()
    }

    func updateCard(id: String, body: String, done: Bool, costLabel: String?, artifact: OverlayArtifact?) {
        if let updated = feed.update(id: id, body: body, done: done, costLabel: costLabel, artifact: artifact) {
            routeCanvasIfNeeded(updated)
        }
        updateContextBar()
    }

    func pushFixProposal(_ proposal: FixProposal) {
        let card = RenderedCard(
            id: proposal.proposalId, kind: "fix_proposal", title: "Proposed fix",
            body: proposal.diagnosis, done: true, costLabel: nil, artifact: nil,
            source: nil, fixProposal: proposal, fixState: .pending)
        feed.push(card)
        if currentTab != .ask { setBodyTab(.ask) }
        updateContextBar()
    }

    func appendLiveTranscript(source: String, text: String, final: Bool) {
        let body = displayTranscriptText(text)
        markListening()
        guard !body.isEmpty else { return }
        // Stream into a single rolling HEARD turn until finalized; on `final`,
        // commit it and start a fresh one for the next utterance.
        if let id = transcriptCardId, !final {
            feed.update(id: id, body: text, done: false, costLabel: nil, artifact: nil)
        } else if let id = transcriptCardId, final {
            feed.update(id: id, body: text, done: true, costLabel: nil, artifact: nil)
            transcriptCardId = nil
        } else {
            let id = UUID().uuidString
            transcriptCardId = final ? nil : id
            let card = RenderedCard(
                id: id, kind: "transcript", title: source, body: text,
                done: final, costLabel: nil, artifact: nil, source: source)
            feed.push(card)
        }
        if currentTab != .ask { setBodyTab(.ask) }
        updateContextBar()
    }

    private func markListening() {
        if !recordingActive {
            recordingActive = true
            styleListenButton(listening: true)
            showListeningWave(true)
        }
    }

    func setListeningState(_ state: PillRunState) {
        switch state {
        case .listening:
            recordingActive = true
            styleListenButton(listening: true)
            showListeningWave(true)
        case .connecting:
            recordingActive = false
            styleListenButton(listening: false)
            showListeningWave(false)
        case .paused, .failed, .ready:
            recordingActive = false
            styleListenButton(listening: false)
            showListeningWave(false)
            transcriptCardId = nil
        }
    }

    private func showListeningWave(_ on: Bool) {
        listeningWave.isHidden = !on
        statusDot.isHidden = on
        if on {
            statusDot.layer?.backgroundColor = Tok.ok.cgColor
        }
    }

    // MARK: External API — context / sessions / balance / reset

    func setContextItems(_ items: [OverlayContextItem]) {
        for v in attachmentStrip.arrangedSubviews {
            attachmentStrip.removeArrangedSubview(v)
            v.removeFromSuperview()
        }
        for item in items { attachmentStrip.addArrangedSubview(makeAttachmentChip(item)) }
        attachmentStrip.isHidden = items.isEmpty || currentTab != .ask
    }

    private func makeAttachmentChip(_ item: OverlayContextItem) -> NSView {
        let chip = NSView()
        chip.translatesAutoresizingMaskIntoConstraints = false
        chip.wantsLayer = true
        chip.layer?.backgroundColor = Tok.glassHi.cgColor
        chip.layer?.cornerRadius = Tok.rSm
        chip.layer?.borderWidth = 1
        chip.layer?.borderColor = Tok.hairline.cgColor
        let icon = NSImageView()
        icon.translatesAutoresizingMaskIntoConstraints = false
        if let image = symbolImage("doc") {
            image.isTemplate = true
            icon.image = image
        }
        icon.contentTintColor = Tok.tx3
        let title = NSTextField(labelWithString: item.title)
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = Tok.font(11, .regular)
        title.textColor = Tok.tx2
        title.lineBreakMode = .byTruncatingTail
        let remove = RemoveAttachmentButton(title: "", target: self, action: #selector(removeAttachmentClicked(_:)))
        remove.translatesAutoresizingMaskIntoConstraints = false
        remove.isBordered = false
        remove.contextId = item.id
        remove.contentTintColor = Tok.tx4
        if let image = symbolImage("xmark") {
            image.isTemplate = true
            remove.image = image
            remove.imagePosition = .imageOnly
            remove.imageScaling = .scaleProportionallyDown
        } else { remove.title = "×" }
        chip.addSubview(icon)
        chip.addSubview(title)
        chip.addSubview(remove)
        NSLayoutConstraint.activate([
            chip.heightAnchor.constraint(equalToConstant: 26),
            chip.widthAnchor.constraint(lessThanOrEqualToConstant: 210),
            icon.leadingAnchor.constraint(equalTo: chip.leadingAnchor, constant: 8),
            icon.centerYAnchor.constraint(equalTo: chip.centerYAnchor),
            icon.widthAnchor.constraint(equalToConstant: 12),
            icon.heightAnchor.constraint(equalToConstant: 12),
            title.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: 6),
            title.centerYAnchor.constraint(equalTo: chip.centerYAnchor),
            title.trailingAnchor.constraint(equalTo: remove.leadingAnchor, constant: -4),
            remove.trailingAnchor.constraint(equalTo: chip.trailingAnchor, constant: -6),
            remove.centerYAnchor.constraint(equalTo: chip.centerYAnchor),
            remove.widthAnchor.constraint(equalToConstant: 16),
            remove.heightAnchor.constraint(equalToConstant: 16),
        ])
        return chip
    }

    @objc private func removeAttachmentClicked(_ sender: RemoveAttachmentButton) {
        let id = sender.contextId.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !id.isEmpty else { return }
        emitRemoveContext(id: id)
    }

    func setBalanceLabel(_ label: String) {
        // Managed-mode balance lives in the footer chip (off the header face).
        guard attachedAgentKind == nil else { return }
        let trimmed = label.replacingOccurrences(of: "Balance", with: "").trimmingCharacters(in: .whitespaces)
        if currentTab == .ask, !trimmed.isEmpty, trimmed != "--" {
            footerConnectorsLabel.stringValue = "perplexity · github · \(trimmed)"
        }
    }

    func resetSessionSurface() {
        feed.clear()
        transcriptCardId = nil
        updateContextBar()
    }

    func focusComposerForQuestion() {
        closePlusMenu()
        if currentTab != .ask { setBodyTab(.ask) }
        composer.placeholder = recordingActive ? "Ask while Bluey listens…" : "Ask a follow-up…"
        window?.makeFirstResponder(composer)
    }

    func showSignedOutLogin(url: URL?) {
        statusDot.layer?.backgroundColor = Tok.warn.cgColor
    }

    func showSignedInReady() {
        statusDot.layer?.backgroundColor = Tok.ok.cgColor
    }

    // MARK: Composer actions

    @objc private func askClicked() {
        closePlusMenu()
        let raw = composer.string.trimmingCharacters(in: .whitespacesAndNewlines)
        let q = raw.isEmpty
            ? "Answer the latest clear question or useful context from this Bluey session."
            : raw
        composer.clearText()
        // Auto route: the daemon picks the lane (provider/model/mode nil).
        emitAsk(question: q, provider: nil, model: nil, mode: nil)
        if currentTab != .ask { setBodyTab(.ask) }
        window?.makeFirstResponder(composer)
    }

    @objc private func recordingClicked() {
        if recordingActive {
            emitSimple("recording_stop_requested")
            recordingActive = false
            onListeningStateChanged?(.paused)
            styleListenButton(listening: false)
            showListeningWave(false)
        } else {
            emitSimple("recording_start_requested")
            onListeningStateChanged?(.connecting)
            styleListenButton(listening: true)
            showListeningWave(true)
        }
    }

    private func setComposerTextHeight(_ rawHeight: CGFloat) {
        let clamped = min(max(22, rawHeight - 14), 110)
        composerTextHeightConstraint?.constant = clamped
    }

    // MARK: System toast (system cards that aren't login)

    private func shouldRenderAsToast(_ card: RenderedCard) -> Bool {
        guard normalizedCardKind(card.kind) == "system" else { return false }
        // Login/sign-in system cards render in the feed; other system notices
        // (ready, indexing, errors) flash as a toast.
        return actionableLoginURL(from: card) == nil
    }

    private func actionableLoginURL(from card: RenderedCard) -> URL? {
        guard normalizedCardKind(card.kind) == "system" else { return nil }
        for line in card.body.components(separatedBy: .newlines) {
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            let candidate: String
            if trimmed.hasPrefix("login_url:") {
                candidate = trimmed.replacingOccurrences(of: "login_url:", with: "").trimmingCharacters(in: .whitespacesAndNewlines)
            } else if trimmed.hasPrefix("https://") || trimmed.hasPrefix("http://") {
                candidate = trimmed
            } else { continue }
            if let url = URL(string: candidate), let scheme = url.scheme?.lowercased(), ["http", "https"].contains(scheme) {
                return url
            }
        }
        return nil
    }

    private func showSystemToast(for card: RenderedCard) {
        toastTitleLabel.stringValue = card.title.isEmpty ? "Bluey" : card.title
        toastBodyLabel.stringValue = systemToastBody(card.body)
        toastView.isHidden = false
        toastView.alphaValue = 1
        toastHideWorkItem?.cancel()
        let work = DispatchWorkItem { [weak self] in self?.hideSystemToast() }
        toastHideWorkItem = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 4.0, execute: work)
    }

    private func hideSystemToast() {
        guard !toastView.isHidden else { return }
        NSAnimationContext.runAnimationGroup({ ctx in
            ctx.duration = 0.18
            toastView.animator().alphaValue = 0
        }, completionHandler: { [weak self] in self?.toastView.isHidden = true })
    }

    private func systemToastBody(_ body: String) -> String {
        var lines: [String] = []
        for raw in body.components(separatedBy: .newlines) {
            let line = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !line.isEmpty, !line.hasPrefix("login_url:") else { continue }
            lines.append(line)
        }
        let joined = lines.joined(separator: " · ").replacingOccurrences(of: "knowledge base", with: "documents")
        if joined.count <= 190 { return joined }
        let end = joined.index(joined.startIndex, offsetBy: 187)
        return String(joined[..<end]) + "…"
    }

    // MARK: NSTextFieldDelegate (answer-style box cancel handled upstream)

    func control(_ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        false
    }

    // MARK: Phase 9 — Click gate (transparent overlay; modals short-circuit)

    func isInteractiveAtScreenPoint(_ screenPoint: NSPoint) -> Bool {
        guard let window else { return false }
        let windowPoint = window.convertPoint(fromScreen: screenPoint)
        let localPoint = convert(windowPoint, from: nil)
        guard bounds.contains(localPoint) else { return false }

        // Modals capture the whole panel while open.
        if !closeConfirmOverlay.isHidden { return true }
        if !connectorSheetOverlay.isHidden { return true }
        if !billingOverlay.isHidden { return true }
        // The "+" menu and opacity popover capture clicks while open.
        if !plusMenu.isHidden, plusMenu.frame.contains(localPoint) { return true }
        if opacityPopoverBuilt, !opacityPopover.isHidden, opacityPopover.frame.contains(localPoint) { return true }

        // Resize grip (bottom-right).
        if !resizeEdges(at: localPoint).isEmpty { return true }

        // Explicit chrome (header drag region, composer, tabs, footer keys).
        if hitsExplicitInteractiveChrome(at: localPoint) { return true }

        // Card affordances inside the otherwise click-through feed.
        let feedPoint = feed.convert(windowPoint, from: nil)
        if feed.hasInteractiveControl(at: feedPoint) { return true }

        return hasInteractiveView(at: localPoint)
            || feed.hasCopyControl(atScreenPoint: screenPoint)
    }

    private func hitsExplicitInteractiveChrome(at localPoint: NSPoint) -> Bool {
        // Whole bands that should always take clicks. NOTE: `workspace` (the Ask
        // feed) is deliberately NOT here — the feed stays click-through to the
        // desktop except over card affordances (handled via the feed's own
        // hit-test + the generic control walk), preserving the transparent
        // overlay. History/Agents ARE interactive surfaces and are hidden when
        // not the active tab, so the isHidden guard gates them by tab.
        let bands: [NSView] = [headerBar, composerBar, footerBar, contextBar,
                               attachmentStrip, historyContainer, agentsScroll, plusMenu]
        return bands.contains { view in
            guard !view.isHidden, view.alphaValue > 0.01 else { return false }
            let rect = view.convert(view.bounds, to: self)
            return rect.contains(localPoint)
        }
    }

    private func hasInteractiveView(at localPoint: NSPoint) -> Bool {
        var hit: NSView? = hitTest(localPoint)
        while let view = hit {
            if view === self || view === workspace || view === headerBar || view === composerBar {
                hit = view.superview
                continue
            }
            if view is NSButton || view is NSPopUpButton || view is NSSlider
                || view is NSScroller || view is NSTextView {
                return true
            }
            if let textField = view as? NSTextField, textField.isEditable {
                return true
            }
            hit = view.superview
        }
        return false
    }

    // MARK: Resize (bottom-right grip; header drags via HeaderDragView)

    private func resizeEdges(at point: NSPoint) -> ResizeEdges {
        guard bounds.contains(point),
              closeConfirmOverlay.isHidden, connectorSheetOverlay.isHidden, billingOverlay.isHidden,
              !headerBar.frame.insetBy(dx: -4, dy: -4).contains(point),
              !composerBar.frame.insetBy(dx: -4, dy: -4).contains(point)
        else { return [] }
        var edges: ResizeEdges = []
        if point.x >= bounds.width - resizeHitSize { edges.insert(.right) }
        if point.y <= resizeHitSize { edges.insert(.bottom) }
        return edges
    }

    override func mouseDown(with event: NSEvent) {
        let local = convert(event.locationInWindow, from: nil)
        // Clicking outside the open +menu/opacity popover dismisses it.
        if !plusMenu.isHidden, !plusMenu.frame.contains(local) { closePlusMenu() }
        if opacityPopoverBuilt, !opacityPopover.isHidden, !opacityPopover.frame.contains(local) { opacityPopover.isHidden = true }
        let edges = resizeEdges(at: local)
        if !edges.isEmpty {
            activeResizeEdges = edges
            resizeStartMouse = NSEvent.mouseLocation
            resizeStartFrame = window?.frame ?? .zero
            return
        }
        super.mouseDown(with: event)
    }

    override func mouseDragged(with event: NSEvent) {
        guard !activeResizeEdges.isEmpty, let window else {
            super.mouseDragged(with: event)
            return
        }
        let mouse = NSEvent.mouseLocation
        let dx = mouse.x - resizeStartMouse.x
        let dy = mouse.y - resizeStartMouse.y
        var frame = resizeStartFrame
        if activeResizeEdges.contains(.right) {
            frame.size.width = max(ExpandedPanelMetrics.minCompactWidth, resizeStartFrame.width + dx)
        }
        if activeResizeEdges.contains(.bottom) {
            let newHeight = max(ExpandedPanelMetrics.minHeight, resizeStartFrame.height - dy)
            frame.origin.y = resizeStartFrame.maxY - newHeight
            frame.size.height = newHeight
        }
        window.setFrame(frame, display: true)
    }

    override func mouseUp(with event: NSEvent) {
        if !activeResizeEdges.isEmpty { activeResizeEdges = []; return }
        super.mouseUp(with: event)
    }

    // MARK: Canvas routing (carried; auto-open + close via the pane button)

    private func routeCanvasIfNeeded(_ card: RenderedCard) {
        guard let artifact = makeCanvasArtifact(from: card) else { return }
        latestCanvas = artifact
        canvasPane.render(artifact)
        if shouldAutoOpenCanvas(for: card, artifact: artifact) {
            setCanvasOpen(true)
        }
    }

    private func shouldAutoOpenCanvas(for card: RenderedCard, artifact: CanvasArtifact) -> Bool {
        if card.artifact != nil { return true }
        guard card.kind == "answer" else { return false }
        switch artifact.kind {
        case .code, .systemDesign, .screen: return true
        case .document, .structured: return false
        }
    }

    private func setCanvasOpen(_ open: Bool) {
        if !open, canvasFullWindow { restoreCanvasWindow() }
        canvasOpen = open
        canvasPane.isHidden = !open
        canvasWidthConstraint?.constant = open ? canvasWidth() : 0
        NSAnimationContext.runAnimationGroup { ctx in
            ctx.duration = 0.16
            self.layoutSubtreeIfNeeded()
        }
    }

    private func canvasWidth() -> CGFloat {
        if canvasFullWindow { return min(max(380, bounds.width * 0.44), 560) }
        return min(max(300, bounds.width * 0.42), 360)
    }

    private func toggleCanvasFullWindow() {
        guard let window else { return }
        if canvasFullWindow {
            restoreCanvasWindow()
        } else {
            preCanvasFullWindowFrame = window.frame
            canvasFullWindow = true
            canvasPane.setFullWindow(true)
            let screen = window.screen?.visibleFrame ?? NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
            let maxW = max(ExpandedPanelMetrics.minCompactWidth, screen.width - ExpandedPanelMetrics.screenInset * 2)
            let maxH = max(ExpandedPanelMetrics.minHeight, screen.height - ExpandedPanelMetrics.screenInset * 2)
            var frame = NSRect(x: screen.midX - maxW / 2, y: screen.midY - maxH / 2, width: maxW, height: maxH)
            frame = ExpandedPanelMetrics.fitExpandedFrameToVisibleScreen(frame, visibleFrame: screen)
            canvasWidthConstraint?.constant = canvasWidth()
            NSAnimationContext.runAnimationGroup { ctx in
                ctx.duration = 0.16
                window.animator().setFrame(frame, display: true)
                self.layoutSubtreeIfNeeded()
            }
        }
    }

    private func restoreCanvasWindow() {
        guard let window else { return }
        canvasFullWindow = false
        canvasPane.setFullWindow(false)
        let target = preCanvasFullWindowFrame
        preCanvasFullWindowFrame = nil
        canvasWidthConstraint?.constant = canvasOpen ? canvasWidth() : 0
        if let target {
            NSAnimationContext.runAnimationGroup { ctx in
                ctx.duration = 0.16
                window.animator().setFrame(target, display: true)
                self.layoutSubtreeIfNeeded()
            }
        }
    }

    // MARK: Canvas artifact detection (carried verbatim from the prior build)

    private func makeCanvasArtifact(from card: RenderedCard) -> CanvasArtifact? {
        guard card.kind == "answer" || card.kind == "context" || card.kind == "system" else { return nil }
        let body = card.body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !body.isEmpty else { return nil }
        if let artifact = card.artifact {
            let kind = CanvasKind.fromArtifactType(artifact.artifactType)
            let confidence = artifact.confidence.map { "Confidence \(Int(($0 * 100).rounded()))%" }
            return CanvasArtifact(
                kind: kind,
                title: artifact.title.isEmpty ? kind.title : artifact.title,
                subtitle: confidence ?? kind.subtitle,
                content: artifact.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? body : artifact.body,
                sourceCardId: card.id)
        }
        let lower = body.lowercased()
        let codeBlocks = extractCodeBlocks(from: body)
        if !codeBlocks.isEmpty || looksLikeCode(lower) {
            return CanvasArtifact(kind: .code, title: "Code canvas", subtitle: "Code, tests, complexity",
                content: formatCodeCanvas(body: body, codeBlocks: codeBlocks), sourceCardId: card.id)
        }
        if looksLikeSystemDesign(lower) {
            return CanvasArtifact(kind: .systemDesign, title: "System design canvas", subtitle: "Architecture, tradeoffs, scale",
                content: formatStructuredCanvas(body, fallbackHeading: "System Design"), sourceCardId: card.id)
        }
        if looksLikeScreenAnalysis(lower) {
            return CanvasArtifact(kind: .screen, title: "Screen analysis", subtitle: "Detected context and answer",
                content: formatStructuredCanvas(body, fallbackHeading: "Screen Context"), sourceCardId: card.id)
        }
        if card.kind == "context" || looksLikeDocumentWork(lower) {
            return CanvasArtifact(kind: .document, title: "Document notes", subtitle: "Attached context distilled",
                content: formatStructuredCanvas(body, fallbackHeading: "Document Context"), sourceCardId: card.id)
        }
        if body.count > 950 && hasStructuredShapePanel(body) {
            return CanvasArtifact(kind: .structured, title: "Workspace", subtitle: "Structured workspace",
                content: formatStructuredCanvas(body, fallbackHeading: "Notes"), sourceCardId: card.id)
        }
        return nil
    }

    private func extractCodeBlocks(from text: String) -> [String] {
        var blocks: [String] = []
        var current: [String] = []
        var inFence = false
        for line in text.components(separatedBy: .newlines) {
            if line.trimmingCharacters(in: .whitespaces).hasPrefix("```") {
                if inFence { blocks.append(current.joined(separator: "\n")); current = [] }
                inFence.toggle()
                continue
            }
            if inFence { current.append(line) }
        }
        if !current.isEmpty { blocks.append(current.joined(separator: "\n")) }
        return blocks.filter { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    }

    private func looksLikeCode(_ lower: String) -> Bool {
        let signals = ["function ", "const ", "let ", "var ", "def ", "class ", "import ", "return ", "=>", "{", "}", "();"]
        return signals.filter { lower.contains($0) }.count >= 3
    }

    private func looksLikeSystemDesign(_ lower: String) -> Bool {
        let signals = ["throughput", "latency", "tradeoff", "shard", "load balancer", "microservice", "event-driven"]
        return signals.filter { lower.contains($0) }.count >= 3
    }

    private func looksLikeScreenAnalysis(_ lower: String) -> Bool {
        lower.contains("screenshot") || lower.contains("screen context")
            || lower.contains("analyse screen") || lower.contains("analyze screen") || lower.contains("image shows")
    }

    private func looksLikeDocumentWork(_ lower: String) -> Bool {
        lower.contains("attached document") || lower.contains("pdf") || lower.contains("resume")
            || lower.contains("document context") || lower.contains("source:")
    }

    private func hasStructuredShapePanel(_ text: String) -> Bool {
        let lines = text.components(separatedBy: .newlines)
        let structured = lines.filter { line in
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            return trimmed.hasPrefix("- ") || trimmed.hasPrefix("* ") || trimmed.hasPrefix("#")
                || trimmed.range(of: #"^\d+[\.\)]\s"#, options: .regularExpression) != nil
        }
        return structured.count >= 3
    }

    private func formatCodeCanvas(body: String, codeBlocks: [String]) -> String {
        if !codeBlocks.isEmpty { return codeBlocks.joined(separator: "\n\n") }
        return body
    }

    private func formatStructuredCanvas(_ body: String, fallbackHeading: String) -> String {
        body
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
        view.onAgentAttachmentChanged = { [weak self] attached in
            self?.pillView?.agentAttached = attached
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
        case .setSessionsPage(let sessions, let total, let offset, let hasMore, let query):
            ensureExpandedWindow()
            expandedView?.setSessionsPage(
                sessions: sessions, total: total, offset: offset,
                hasMore: hasMore, query: query)
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
                artifact: card.artifact, source: card.source))
        case .updateCard(let id, let body, let done, let costLabel, let artifact):
            ensureExpandedWindow()
            expandedView?.updateCard(id: id, body: body, done: done, costLabel: costLabel, artifact: artifact)
        case .setAgents(let agents):
            ensureExpandedWindow()
            expandedView?.setAgents(agents)
        case .setAgentSessions(let kind, let sessions):
            expandedView?.setAgentSessions(kind: kind, sessions: sessions)
        case .setAgentConnectors(let kind, let connectors):
            expandedView?.setAgentConnectors(kind: kind, connectors: connectors)
        case .pushFixProposal(let proposal):
            ensureExpandedWindow()
            expandedView?.pushFixProposal(proposal)
        case .pushBillingDisclosure(let disclosure):
            ensureExpandedWindow()
            expandedView?.presentBillingDisclosure(disclosure)
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
            artifact: nil,
            source: nil)
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
