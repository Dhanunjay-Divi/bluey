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
import ApplicationServices
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

private let privateInstructionRefusal = "I can’t share Bluey’s private instructions, prompts, guardrails, tokens, or internal configuration. Ask me what you want to do, and I’ll help with the answer itself."

private func sanitizeOverlayOutput(kind: String, body: String) -> String {
    let normalizedKind = normalizedCardKind(kind)
    guard normalizedKind != "question", normalizedKind != "transcript" else { return body }
    guard !looksLikePrivateInstructionDisclosure(body) else { return privateInstructionRefusal }
    return formatOverlayAnswerText(body)
}

private func formatOverlayAnswerText(_ body: String) -> String {
    stripPlainTextMarkdownDecoration(body)
        .replacingOccurrences(
            of: #"(?<=[\:\.\!\?\*\)])\s*(-\s+[A-Z])"#,
            with: "\n$1",
            options: .regularExpression)
        .replacingOccurrences(
            of: #"\.\s+(Recommended ratings:|Rationale:|Approach|Patch|Explanation|Complexity|Edge cases)"#,
            with: ".\n\n$1",
            options: .regularExpression)
        .replacingOccurrences(of: #"\n{3,}"#, with: "\n\n", options: .regularExpression)
}

private func stripPlainTextMarkdownDecoration(_ text: String) -> String {
    var output: [String] = []
    var inFence = false
    for rawLine in text.components(separatedBy: .newlines) {
        let trimmed = rawLine.trimmingCharacters(in: .whitespaces)
        if trimmed.hasPrefix("```") {
            inFence.toggle()
            output.append(rawLine)
            continue
        }
        guard !inFence else {
            output.append(rawLine)
            continue
        }

        let withoutHeading = rawLine.replacingOccurrences(
            of: #"^\s{0,3}#{1,6}\s+"#,
            with: "",
            options: .regularExpression)
        output.append(
            withoutHeading
                .replacingOccurrences(of: "**", with: "")
                .replacingOccurrences(of: "__", with: ""))
    }
    return output.joined(separator: "\n")
}

private func sanitizeOverlayArtifact(_ artifact: OverlayArtifact?) -> OverlayArtifact? {
    guard let artifact else { return nil }
    if artifact.artifactType != "code" && artifact.artifactType != "system_design" {
        return nil
    }
    if looksLikePrivateInstructionDisclosure(artifact.body)
        || looksLikePrivateInstructionDisclosure(artifact.title)
    {
        return nil
    }
    return artifact
}

private func looksLikePrivateInstructionDisclosure(_ text: String) -> Bool {
    let normalized = normalizePrivateInstructionText(text)
    guard !normalized.isEmpty else { return false }
    let directLeaks = [
        "the prompts that define how i work",
        "embedded in my system instructions",
        "plain summary of the key rules i follow",
        "identity and scope",
        "talk track rule",
        "question type detection",
        "voice and person",
        "depth matching",
        "canvas and workbench split",
        "style restrictions",
        "output shape",
        "human speak contract",
        "answer rules"
    ]
    if directLeaks.contains(where: { normalized.contains($0) }) {
        return true
    }
    return normalized.contains("system instructions")
        && (normalized.contains("i follow")
            || normalized.contains("how i work")
            || normalized.contains("bluey"))
}

private func normalizePrivateInstructionText(_ text: String) -> String {
    var output = ""
    var lastWasSpace = false
    for scalar in text.lowercased().unicodeScalars {
        if CharacterSet.alphanumerics.contains(scalar) {
            output.unicodeScalars.append(scalar)
            lastWasSpace = false
        } else if !lastWasSpace {
            output.append(" ")
            lastWasSpace = true
        }
    }
    return output.trimmingCharacters(in: .whitespacesAndNewlines)
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

private enum BlueyLightTheme {
    static let accent = NSColor(red: 0.000, green: 0.385, blue: 0.600, alpha: 1.0)
    static let accentBorder = NSColor(red: 0.000, green: 0.465, blue: 0.700, alpha: 1.0)
    static let accentSoft = NSColor(red: 0.690, green: 0.885, blue: 0.965, alpha: 1.0)
    static let panel = NSColor(red: 0.565, green: 0.610, blue: 0.638, alpha: 1.0)
    static let content = NSColor(red: 0.720, green: 0.748, blue: 0.765, alpha: 1.0)
    static let contentHigh = NSColor(red: 0.790, green: 0.810, blue: 0.822, alpha: 1.0)
    static let contentLow = NSColor(red: 0.632, green: 0.678, blue: 0.708, alpha: 1.0)
    static let bar = NSColor(red: 0.405, green: 0.455, blue: 0.488, alpha: 1.0)
    static let barChrome = NSColor(red: 0.625, green: 0.672, blue: 0.700, alpha: 1.0)
    static let chrome = NSColor(red: 0.618, green: 0.662, blue: 0.688, alpha: 1.0)
    static let chromeGuard = NSColor(red: 0.390, green: 0.448, blue: 0.486, alpha: 1.0)
    static let surface = NSColor(red: 0.812, green: 0.832, blue: 0.842, alpha: 1.0)
    static let surfaceRaised = NSColor(red: 0.875, green: 0.890, blue: 0.898, alpha: 1.0)
    static let text = NSColor(red: 0.050, green: 0.064, blue: 0.078, alpha: 1.0)
    static let textDim = NSColor(red: 0.205, green: 0.262, blue: 0.305, alpha: 1.0)
    static let border = NSColor(red: 0.030, green: 0.055, blue: 0.075, alpha: 0.34)
    static let shadow = NSColor.black.withAlphaComponent(0.28)
}

private let minimumOverlayBackgroundOpacity: CGFloat = 0.18
private let supportedDropFormatsMessage =
    "Supported: PDF, DOC/DOCX, Excel/ODS, CSV/TSV, text, Markdown, code/data files, and PNG/JPEG/WebP/GIF/HEIC/BMP/TIFF images."
private let supportedDropExtensions: Set<String> = [
    "md", "markdown", "txt", "log", "csv", "tsv", "rst", "adoc",
    "rs", "swift", "c", "h", "cpp", "hpp", "js", "jsx", "ts", "tsx",
    "py", "go", "java", "kt", "kts", "cs", "rb", "php", "sql", "sh",
    "ps1", "toml", "yaml", "yml", "json", "html", "css", "scss",
    "pdf", "doc", "docx", "rtf", "xls", "xlsx", "xlsm", "xlsb", "ods",
    "png", "jpg", "jpeg", "gif", "webp", "heic", "heif", "bmp", "tiff", "tif",
]
private let overlayLightThemeDefaultsKey = "bluey.overlay.lightTheme"
private let overlayAutoSendStopModeDefaultsKey = "bluey.overlay.autoSendStopMode"
private let overlayAutoSendStopModeVersionDefaultsKey = "bluey.overlay.autoSendStopModeVersion"
private let overlayAutoSendStopModeCurrentVersion = 2

private func blueyMaterialAlpha(_ base: CGFloat, opacity: CGFloat, floor: CGFloat = 0.02) -> CGFloat {
    min(1.0, max(floor, base * min(max(opacity, minimumOverlayBackgroundOpacity), 1.0)))
}

private func blueyLightMaterialAlpha(_ base: CGFloat, opacity: CGFloat, floor: CGFloat = 0.10) -> CGFloat {
    let material = min(max(opacity, 0.02), 1.0)
    let shaped = 0.12 + material * 0.88
    return min(0.96, max(floor, base * shaped))
}

private enum OverlayPlacementStore {
    private static let pillFrameKey = "bluey.overlay.pill.frame.v2"
    private static let expandedFrameKey = "bluey.overlay.expanded.frame.v2"

    static func loadPillFrame(in visibleFrame: NSRect) -> NSRect? {
        loadFrame(key: pillFrameKey).map { clampedPillFrame($0, in: visibleFrame) }
    }

    static func savePillFrame(_ frame: NSRect, in visibleFrame: NSRect) {
        saveFrame(clampedPillFrame(frame, in: visibleFrame), key: pillFrameKey)
    }

    static func clearPillFrame() {
        UserDefaults.standard.removeObject(forKey: pillFrameKey)
    }

    static func loadExpandedFrame(in visibleFrame: NSRect) -> NSRect? {
        loadFrame(key: expandedFrameKey).map {
            ExpandedPanelMetrics.fitExpandedFrameToVisibleScreen($0, visibleFrame: visibleFrame)
        }
    }

    static func saveExpandedFrame(_ frame: NSRect, in visibleFrame: NSRect) {
        saveFrame(
            ExpandedPanelMetrics.fitExpandedFrameToVisibleScreen(frame, visibleFrame: visibleFrame),
            key: expandedFrameKey)
    }

    private static func loadFrame(key: String) -> NSRect? {
        guard let raw = UserDefaults.standard.string(forKey: key), !raw.isEmpty else {
            return nil
        }
        let frame = NSRectFromString(raw)
        guard frame.width > 20, frame.height > 20 else {
            return nil
        }
        return frame
    }

    private static func saveFrame(_ frame: NSRect, key: String) {
        UserDefaults.standard.set(NSStringFromRect(frame), forKey: key)
    }

    private static func clampedPillFrame(_ frame: NSRect, in visibleFrame: NSRect) -> NSRect {
        let inset: CGFloat = 8
        var clamped = frame
        clamped.size = PillMetrics.size
        clamped.origin.x = min(
            max(visibleFrame.minX + inset, clamped.origin.x),
            visibleFrame.maxX - clamped.size.width - inset)
        clamped.origin.y = min(
            max(visibleFrame.minY + inset, clamped.origin.y),
            visibleFrame.maxY - clamped.size.height - inset)
        return clamped
    }
}

private func drawBlueyGlassPanel(
    in rect: NSRect,
    radius: CGFloat,
    opacity: CGFloat
) {
    guard rect.width > 1, rect.height > 1 else { return }
    let material = min(max(opacity, minimumOverlayBackgroundOpacity), 1.0)
    let path = NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius)

    NSGraphicsContext.saveGraphicsState()
    let shadow = NSShadow()
    shadow.shadowColor = NSColor.black.withAlphaComponent(blueyMaterialAlpha(0.34, opacity: material, floor: 0.10))
    shadow.shadowBlurRadius = 22
    shadow.shadowOffset = .zero
    shadow.set()

    NSGradient(colors: [
        NSColor(red: 0.007, green: 0.011, blue: 0.017, alpha: blueyMaterialAlpha(0.98, opacity: material)),
        NSColor(red: 0.014, green: 0.026, blue: 0.032, alpha: blueyMaterialAlpha(0.95, opacity: material)),
        NSColor(red: 0.007, green: 0.010, blue: 0.015, alpha: blueyMaterialAlpha(0.99, opacity: material)),
    ])?.draw(in: path, angle: -18)
    NSGraphicsContext.restoreGraphicsState()

    BlueyTheme.cyan.withAlphaComponent(blueyMaterialAlpha(0.38, opacity: material)).setStroke()
    path.lineWidth = 1.0
    path.stroke()

    let inner = rect.insetBy(dx: 1.5, dy: 1.5)
    let innerPath = NSBezierPath(roundedRect: inner, xRadius: max(0, radius - 1.5), yRadius: max(0, radius - 1.5))
    NSColor.white.withAlphaComponent(blueyMaterialAlpha(0.050, opacity: material)).setStroke()
    innerPath.lineWidth = 0.8
    innerPath.stroke()

    let glossPath = NSBezierPath(roundedRect: rect.insetBy(dx: 2.0, dy: 2.0), xRadius: max(0, radius - 2.0), yRadius: max(0, radius - 2.0))
    NSGradient(colors: [
        NSColor.white.withAlphaComponent(blueyMaterialAlpha(0.08, opacity: material)),
        NSColor.white.withAlphaComponent(0.0),
    ])?.draw(in: glossPath, angle: 90)
}

private func drawBlueyLightPanel(
    in rect: NSRect,
    radius: CGFloat,
    opacity: CGFloat
) {
    guard rect.width > 1, rect.height > 1 else { return }
    let path = NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius)
    let fillAlpha = blueyLightMaterialAlpha(0.98, opacity: opacity, floor: 0.18)

    NSGraphicsContext.saveGraphicsState()
    let shadow = NSShadow()
    shadow.shadowColor = BlueyLightTheme.shadow
        .withAlphaComponent(blueyLightMaterialAlpha(0.30, opacity: opacity, floor: 0.08))
    shadow.shadowBlurRadius = 26
    shadow.shadowOffset = .zero
    shadow.set()

    NSGradient(colors: [
        BlueyLightTheme.barChrome.withAlphaComponent(fillAlpha),
        BlueyLightTheme.panel.withAlphaComponent(fillAlpha),
        BlueyLightTheme.chromeGuard.withAlphaComponent(fillAlpha),
    ])?.draw(in: path, angle: -16)
    NSGraphicsContext.restoreGraphicsState()

    BlueyLightTheme.accentBorder
        .withAlphaComponent(blueyLightMaterialAlpha(0.64, opacity: opacity, floor: 0.30))
        .setStroke()
    path.lineWidth = 1.35
    path.stroke()

    let inner = rect.insetBy(dx: 1.5, dy: 1.5)
    let innerPath = NSBezierPath(roundedRect: inner, xRadius: max(0, radius - 1.5), yRadius: max(0, radius - 1.5))
    NSColor.white.withAlphaComponent(blueyLightMaterialAlpha(0.22, opacity: opacity, floor: 0.06)).setStroke()
    innerPath.lineWidth = 0.8
    innerPath.stroke()

    let gloss = rect.insetBy(dx: 2.0, dy: 2.0)
    let glossPath = NSBezierPath(roundedRect: gloss, xRadius: max(0, radius - 2.0), yRadius: max(0, radius - 2.0))
    NSGradient(colors: [
        NSColor.white.withAlphaComponent(blueyLightMaterialAlpha(0.12, opacity: opacity, floor: 0.02)),
        NSColor.white.withAlphaComponent(0.02),
    ])?.draw(in: glossPath, angle: 92)
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

private final class OpacityScrubberView: NSControl {
    var value: Double = 0.94 {
        didSet { needsDisplay = true }
    }
    var onChange: ((Double) -> Void)?

    override var acceptsFirstResponder: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard !isHidden, alphaValue > 0.01, bounds.contains(point) else { return nil }
        return self
    }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .pointingHand)
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        updateValue(from: event)
    }

    override func mouseDragged(with event: NSEvent) {
        updateValue(from: event)
    }

    func updateValue(fromWindowEvent event: NSEvent) {
        updateValue(from: event)
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard bounds.width > 78, bounds.height > 10 else { return }
        let track = trackRect()
        NSColor.white.withAlphaComponent(0.10).setFill()
        NSBezierPath(roundedRect: track, xRadius: track.height / 2, yRadius: track.height / 2).fill()

        let progress = CGFloat((value - Double(minimumOverlayBackgroundOpacity)) / (1.0 - Double(minimumOverlayBackgroundOpacity)))
        let clampedProgress = min(max(progress, 0), 1)
        let fillWidth = max(track.height, track.width * clampedProgress)
        let fill = NSRect(x: track.minX, y: track.minY, width: fillWidth, height: track.height)
        NSColor(red: 0.20, green: 0.54, blue: 1.0, alpha: 0.95).setFill()
        NSBezierPath(roundedRect: fill, xRadius: fill.height / 2, yRadius: fill.height / 2).fill()

        let knobCenter = NSPoint(x: track.minX + track.width * clampedProgress, y: track.midY)
        let knobRect = NSRect(x: knobCenter.x - 7, y: knobCenter.y - 7, width: 14, height: 14)
        NSColor.white.withAlphaComponent(0.96).setFill()
        NSBezierPath(ovalIn: knobRect).fill()
        NSColor.black.withAlphaComponent(0.18).setStroke()
        let outline = NSBezierPath(ovalIn: knobRect.insetBy(dx: 0.5, dy: 0.5))
        outline.lineWidth = 1
        outline.stroke()
    }

    private func updateValue(from event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        let track = trackRect()
        let progress = min(max((point.x - track.minX) / max(track.width, 1), 0), 1)
        let minimum = Double(minimumOverlayBackgroundOpacity)
        onChange?(minimum + Double(progress) * (1.0 - minimum))
    }

    private func trackRect() -> NSRect {
        let start = min(bounds.width - 36, CGFloat(50))
        let end = max(start + 1, bounds.width - 31)
        return NSRect(x: start, y: (bounds.height - 4) / 2, width: end - start, height: 4)
    }
}

private enum ExpandedPanelMetrics {
    static let maxCompactWidth: CGFloat = 960
    static let minCompactWidth: CGFloat = 760
    static let minResizeWidth: CGFloat = 620
    static let maxCanvasWidth: CGFloat = 1120
    static let maxFocusWidth: CGFloat = 1040
    static let focusHeight: CGFloat = 620
    static let height: CGFloat = 640
    static let minHeight: CGFloat = 500
    static let screenInset: CGFloat = 12
    static let focusInset: CGFloat = 48
    static let cameraSafeTopInset: CGFloat = 64
    static let cornerRadius: CGFloat = 28

    static func fittingWidth(for screen: NSRect, preferred: CGFloat) -> CGFloat {
        let available = max(360, screen.width - screenInset * 2)
        return min(preferred, available)
    }

    static func fittingHeight(for screen: NSRect, preferred: CGFloat = height) -> CGFloat {
        let available = max(minHeight, screen.height - screenInset * 2)
        return min(preferred, available)
    }

    static func fittingFocusWidth(for screen: NSRect, preferred: CGFloat) -> CGFloat {
        let available = max(360, screen.width - focusInset * 2)
        return min(preferred, available)
    }

    static func fittingFocusHeight(for screen: NSRect, preferred: CGFloat = focusHeight) -> CGFloat {
        let available = max(minHeight, screen.height - focusInset * 2)
        return min(preferred, available)
    }

    static func fittingMinimumWidth(for screen: NSRect, targetWidth: CGFloat) -> CGFloat {
        let available = max(360, screen.width - screenInset * 2)
        return min(minResizeWidth, targetWidth, available)
    }

    static func fittingMaximumWidth(for screen: NSRect) -> CGFloat {
        max(360, screen.width - screenInset * 2)
    }

    static func fittingMaximumHeight(for screen: NSRect) -> CGFloat {
        max(minHeight, screen.height - screenInset * 2)
    }

    static func compactFrame(in visibleFrame: NSRect) -> NSRect {
        let width = fittingWidth(for: visibleFrame, preferred: minCompactWidth)
        let height = fittingHeight(for: visibleFrame, preferred: minHeight)
        return fitExpandedFrameToVisibleScreen(
            NSRect(
                x: visibleFrame.midX - width / 2,
                y: visibleFrame.maxY - height - cameraSafeTopInset,
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

    static func focusFrame(in visibleFrame: NSRect, preferredWidth: CGFloat, preferredHeight: CGFloat = focusHeight) -> NSRect {
        let width = fittingFocusWidth(for: visibleFrame, preferred: preferredWidth)
        let height = fittingFocusHeight(for: visibleFrame, preferred: preferredHeight)
        return fitExpandedFrameToVisibleScreen(
            NSRect(
                x: visibleFrame.midX - width / 2,
                y: visibleFrame.midY - height / 2,
                width: width,
                height: height),
            visibleFrame: visibleFrame)
    }

    static func fillVisibleScreenFrame(_ frame: NSRect, visibleFrame: NSRect) -> NSRect {
        var fitted = frame
        fitted.size.width = min(max(360, fitted.size.width), visibleFrame.width)
        fitted.size.height = min(max(minHeight, fitted.size.height), visibleFrame.height)
        fitted.origin.x = min(
            max(visibleFrame.minX, fitted.origin.x),
            visibleFrame.maxX - fitted.size.width)
        fitted.origin.y = min(
            max(visibleFrame.minY, fitted.origin.y),
            visibleFrame.maxY - fitted.size.height)
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
    private let ownedTextStorage: NSTextStorage?
    private let ownedLayoutManager: NSLayoutManager?

    var placeholder = "Ask anything..." {
        didSet { needsDisplay = true }
    }
    var placeholderColor = BlueyTheme.textDim.withAlphaComponent(0.78) {
        didSet { needsDisplay = true }
    }
    var onSubmit: (() -> Void)?
    var onMeasuredHeight: ((CGFloat) -> Void)?
    var onFocusChanged: ((Bool) -> Void)?

    private var inputFocused = false
    private var customCaretVisible = true
    private var customCaretTimer: Timer?

    override var acceptsFirstResponder: Bool { true }

    override init(frame frameRect: NSRect, textContainer container: NSTextContainer?) {
        if let container {
            ownedTextStorage = nil
            ownedLayoutManager = nil
            super.init(frame: frameRect, textContainer: container)
        } else {
            let storage = NSTextStorage()
            let manager = NSLayoutManager()
            let container = NSTextContainer(size: NSSize(
                width: max(frameRect.width, 80),
                height: CGFloat.greatestFiniteMagnitude))
            container.lineFragmentPadding = 0
            container.widthTracksTextView = true
            container.heightTracksTextView = false
            storage.addLayoutManager(manager)
            manager.addTextContainer(container)
            ownedTextStorage = storage
            ownedLayoutManager = manager
            super.init(frame: frameRect, textContainer: container)
        }
        drawsBackground = false
        isRichText = false
        isAutomaticQuoteSubstitutionEnabled = false
        isAutomaticDashSubstitutionEnabled = false
        isAutomaticTextReplacementEnabled = false
        isContinuousSpellCheckingEnabled = false
        textColor = BlueyTheme.text
        insertionPointColor = BlueyTheme.cyan
        font = NSFont.systemFont(ofSize: 14.5, weight: .medium)
        typingAttributes = composerTextAttributes
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

    deinit {
        customCaretTimer?.invalidate()
    }

    override func becomeFirstResponder() -> Bool {
        let accepted = super.becomeFirstResponder()
        if accepted {
            setInputFocused(true)
            armTypingCaret()
        }
        return accepted
    }

    override func resignFirstResponder() -> Bool {
        let accepted = super.resignFirstResponder()
        if accepted {
            setInputFocused(false)
        }
        return accepted
    }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .arrow)
    }

    override func cursorUpdate(with event: NSEvent) {
        NSCursor.arrow.set()
    }

    override func mouseMoved(with event: NSEvent) {
        super.mouseMoved(with: event)
        NSCursor.arrow.set()
    }

    override func mouseDragged(with event: NSEvent) {
        super.mouseDragged(with: event)
        NSCursor.arrow.set()
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard string.isEmpty else { return }
        let attributes: [NSAttributedString.Key: Any] = [
            .font: font ?? NSFont.systemFont(ofSize: 14.5, weight: .medium),
            .foregroundColor: placeholderColor,
        ]
        let caretOffset: CGFloat = inputFocused ? 13 : 0
        let rect = NSRect(
            x: caretOffset,
            y: textContainerInset.height + 1,
            width: max(0, bounds.width - caretOffset),
            height: 22)
        placeholder.draw(in: rect, withAttributes: attributes)
        if inputFocused, customCaretVisible {
            let caretRect = NSRect(
                x: 2,
                y: textContainerInset.height + 1,
                width: 2,
                height: max(18, min(24, bounds.height - textContainerInset.height * 2)))
            insertionPointColor.setFill()
            NSBezierPath(roundedRect: caretRect, xRadius: 1, yRadius: 1).fill()
        }
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
        if handleEditingShortcut(event) {
            return
        }
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

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if handleEditingShortcut(event) {
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    override func mouseDown(with event: NSEvent) {
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
        window?.makeFirstResponder(self)
        handleMouseGesture(atWindowPoint: event.locationInWindow, clickCount: event.clickCount)
        armTypingCaret()
    }

    func clearText() {
        string = ""
        selectedRange = NSRange(location: 0, length: 0)
        armTypingCaret()
        needsDisplay = true
        notifyMeasuredHeight()
    }

    func armTypingCaret() {
        let length = (string as NSString).length
        let range = selectedRange()
        setSelectedRange(NSRange(location: min(range.location, length), length: 0))
        customCaretVisible = true
        needsDisplay = true
        displayIfNeeded()
    }

    func insertPlainText(_ text: String) {
        guard !text.isEmpty else { return }
        replacePlainText(in: selectedRange(), with: text)
    }

    func deleteBackwardPlainText() {
        let range = selectedRange()
        if range.length > 0 {
            replacePlainText(in: range, with: "")
            return
        }
        guard range.location > 0 else { return }
        replacePlainText(in: NSRange(location: range.location - 1, length: 1), with: "")
    }

    func handleMouseGesture(atWindowPoint windowPoint: NSPoint, clickCount: Int) {
        window?.makeFirstResponder(self)
        let index = insertionIndex(atWindowPoint: windowPoint)
        let length = (string as NSString).length
        guard length > 0 else {
            setSelectedRange(NSRange(location: 0, length: 0))
            armTypingCaret()
            needsDisplay = true
            return
        }

        switch clickCount {
        case 3...:
            setSelectedRange(sentenceRange(containing: index))
        case 2:
            setSelectedRange(wordRange(containing: index))
        default:
            setSelectedRange(NSRange(location: min(index, length), length: 0))
        }
        scrollRangeToVisible(selectedRange())
        needsDisplay = true
    }

    private func setInputFocused(_ focused: Bool) {
        guard inputFocused != focused else {
            needsDisplay = true
            return
        }
        inputFocused = focused
        onFocusChanged?(focused)
        if focused {
            customCaretVisible = true
            customCaretTimer?.invalidate()
            let timer = Timer(timeInterval: 0.52, repeats: true) { [weak self] _ in
                guard let self else { return }
                self.customCaretVisible.toggle()
                self.needsDisplay = true
            }
            customCaretTimer = timer
            RunLoop.main.add(timer, forMode: .common)
        } else {
            customCaretTimer?.invalidate()
            customCaretTimer = nil
            customCaretVisible = false
        }
        needsDisplay = true
    }

    private func replacePlainText(in range: NSRange, with replacement: String) {
        let currentLength = (string as NSString).length
        let location = min(max(range.location, 0), currentLength)
        let maxLength = max(0, currentLength - location)
        let clampedRange = NSRange(location: location, length: min(max(range.length, 0), maxLength))
        guard shouldChangeText(in: clampedRange, replacementString: replacement) else { return }
        let attributed = NSAttributedString(string: replacement, attributes: composerTextAttributes)
        textStorage?.replaceCharacters(in: clampedRange, with: attributed)
        let newLocation = location + (replacement as NSString).length
        setSelectedRange(NSRange(location: newLocation, length: 0))
        didChangeText()
        scrollRangeToVisible(selectedRange())
    }

    private var composerTextAttributes: [NSAttributedString.Key: Any] {
        [
            .font: font ?? NSFont.systemFont(ofSize: 14.5, weight: .medium),
            .foregroundColor: textColor ?? BlueyTheme.text,
        ]
    }

    private func insertionIndex(atWindowPoint windowPoint: NSPoint) -> Int {
        let localPoint = convert(windowPoint, from: nil)
        guard let manager = layoutManager, let container = textContainer else {
            return min(max(0, string.count), (string as NSString).length)
        }
        container.containerSize = NSSize(width: max(80, bounds.width), height: CGFloat.greatestFiniteMagnitude)
        manager.ensureLayout(for: container)
        let containerPoint = NSPoint(
            x: localPoint.x - textContainerOrigin.x,
            y: localPoint.y - textContainerOrigin.y)
        let index = manager.characterIndex(
            for: containerPoint,
            in: container,
            fractionOfDistanceBetweenInsertionPoints: nil)
        return min(max(index, 0), (string as NSString).length)
    }

    private func wordRange(containing index: Int) -> NSRange {
        let nsString = string as NSString
        let length = nsString.length
        guard length > 0 else { return NSRange(location: 0, length: 0) }
        let seed = min(max(index, 0), length - 1)
        let wordCharacters = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "_'"))
        if !character(at: seed, in: nsString, isIn: wordCharacters) {
            return NSRange(location: seed, length: min(1, length - seed))
        }

        var start = seed
        while start > 0, character(at: start - 1, in: nsString, isIn: wordCharacters) {
            start -= 1
        }
        var end = seed + 1
        while end < length, character(at: end, in: nsString, isIn: wordCharacters) {
            end += 1
        }
        return NSRange(location: start, length: end - start)
    }

    private func sentenceRange(containing index: Int) -> NSRange {
        let nsString = string as NSString
        let length = nsString.length
        guard length > 0 else { return NSRange(location: 0, length: 0) }
        let seed = min(max(index, 0), length - 1)
        let terminators = CharacterSet(charactersIn: ".!?\n")

        var start = seed
        while start > 0, !character(at: start - 1, in: nsString, isIn: terminators) {
            start -= 1
        }
        while start < length, character(at: start, in: nsString, isIn: .whitespacesAndNewlines) {
            start += 1
        }

        var end = seed
        while end < length, !character(at: end, in: nsString, isIn: terminators) {
            end += 1
        }
        if end < length, character(at: end, in: nsString, isIn: terminators) {
            end += 1
        }
        return NSRange(location: min(start, length), length: max(0, end - start))
    }

    private func character(at index: Int, in string: NSString, isIn set: CharacterSet) -> Bool {
        guard index >= 0, index < string.length,
              let scalar = UnicodeScalar(string.character(at: index))
        else {
            return false
        }
        return set.contains(scalar)
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

    private func handleEditingShortcut(_ event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        guard flags.contains(.command),
              !flags.contains(.control),
              !flags.contains(.option)
        else {
            return false
        }

        switch (event.charactersIgnoringModifiers ?? "").lowercased() {
        case "a":
            selectAll(nil)
            return true
        case "c":
            copy(nil)
            return true
        case "v":
            paste(nil)
            return true
        case "x":
            cut(nil)
            return true
        default:
            return false
        }
    }
}

private final class ArrowCursorTextView: NSTextView {
    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .arrow)
    }

    override func cursorUpdate(with event: NSEvent) {
        NSCursor.arrow.set()
    }

    override func mouseMoved(with event: NSEvent) {
        super.mouseMoved(with: event)
        NSCursor.arrow.set()
    }

    override func mouseDragged(with event: NSEvent) {
        super.mouseDragged(with: event)
        NSCursor.arrow.set()
    }
}

private final class ArrowCursorTextField: NSTextField {
    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .arrow)
    }

    override func cursorUpdate(with event: NSEvent) {
        NSCursor.arrow.set()
    }

    override func mouseMoved(with event: NSEvent) {
        super.mouseMoved(with: event)
        NSCursor.arrow.set()
    }

    override func mouseDragged(with event: NSEvent) {
        super.mouseDragged(with: event)
        NSCursor.arrow.set()
    }
}

private final class ComposerSurfaceView: NSView {
    weak var composer: ComposerTextView?
    private var inputFocused = false
    private var restingBorderColor = NSColor.white.withAlphaComponent(0.105)
    private var focusedBorderColor = BlueyTheme.cyan.withAlphaComponent(0.62)

    override var acceptsFirstResponder: Bool { true }

    override func mouseDown(with event: NSEvent) {
        if let composer {
            NSApp.activate(ignoringOtherApps: true)
            window?.makeKeyAndOrderFront(nil)
            window?.makeFirstResponder(composer)
            composer.armTypingCaret()
        }
        super.mouseDown(with: event)
    }

    func setInputFocused(_ focused: Bool) {
        guard inputFocused != focused else { return }
        inputFocused = focused
        refreshFocusChrome()
    }

    func updateBorderColors(resting: NSColor, focused: NSColor) {
        restingBorderColor = resting
        focusedBorderColor = focused
        refreshFocusChrome()
    }

    private func refreshFocusChrome() {
        guard let layer else { return }
        layer.borderColor = (inputFocused ? focusedBorderColor : restingBorderColor).cgColor
        layer.shadowColor = focusedBorderColor.cgColor
        layer.shadowOpacity = inputFocused ? 0.18 : 0
        layer.shadowRadius = inputFocused ? 10 : 0
        layer.shadowOffset = .zero
    }
}

private final class SessionDrawerView: NSView {
    weak var scrollView: NSScrollView?

    override func scrollWheel(with event: NSEvent) {
        guard let scrollView else {
            super.scrollWheel(with: event)
            return
        }
        scrollView.scrollWheel(with: event)
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
    let attachments: [OverlayContextItem]?

    enum CodingKeys: String, CodingKey {
        case id, kind, title, body
        case createdAt = "created_at"
        case source
        case costLabel = "cost_label"
        case artifact
        case attachments
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

private struct OverlayContextItem: Decodable {
    let id: String
    let title: String
    let kind: String
    let path: String?
}

private struct OverlaySessionItem {
    let id: String
    let title: String
    let subtitle: String
    let contextCount: Int
    let imageCount: Int
    let isActive: Bool
}

// Matches the trusted remote-control event source marker used by the host
// remote-input bridge. Bluey should not consume those injected clicks.
private let blueyTrustedRemoteInputEventSourceUserData: Int64 = 0x70696e6b797231
private let remoteInputPassthroughDefaultMs = 900
private let remoteInputPassthroughHeuristicMs = 1_200
private let remoteControlAppNeedles = [
    "anydesk",
    "chrome remote desktop",
    "jump desktop",
    "logmein",
    "microsoft remote desktop",
    "parsec",
    "realvnc",
    "remote desktop",
    "remotix",
    "rustdesk",
    "screen sharing",
    "screensharing",
    "splashtop",
    "teamviewer",
    "vnc viewer",
]

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
    case setPassthrough(enabled: Bool, durationMs: Int?)
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
                contextCount: item["context_count"] as? Int ?? 0,
                imageCount: item["image_count"] as? Int ?? 0,
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
    case "set_passthrough", "input_passthrough":
        let durationMs = (obj["duration_ms"] as? Int) ?? (obj["durationMs"] as? Int)
        return .setPassthrough(
            enabled: obj["enabled"] as? Bool ?? true,
            durationMs: durationMs)
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
    let localAllowed = argumentFlag("--bluey-local-visible-overlay")
        || envFlag("BLUEY_LOCAL_VISIBLE_OVERLAY")
        || envFlag("BLUEY_ALLOW_CAPTURE_VISIBLE_LOCAL")
    let captureRequested = argumentFlag("--bluey-overlay-capture-visible")
        || envFlag("BLUEY_OVERLAY_CAPTURE_VISIBLE")
        || envFlag("BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE")
    return devEnabled && localAllowed && captureRequested
#else
    return false
#endif
}()

private let trustedRemoteInputTapEnabled: Bool = {
#if DEBUG
    let devEnabled = argumentFlag("--bluey-dev-overlay") || envFlag("BLUEY_DEV_OVERLAY")
    let tapRequested = argumentFlag("--bluey-trusted-remote-input-tap")
        || envFlag("BLUEY_TRUSTED_REMOTE_INPUT_TAP")
    return devEnabled && tapRequested
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

private func emitAsk(question: String, provider: String?, model: String?, mode: String?, visibleContextIds: [String] = []) {
    var p: [String: Any] = ["type": "ask_requested", "question": question]
    if let provider = provider { p["provider"] = provider }
    if let model = model       { p["model"]    = model }
    if let mode = mode         { p["mode"]     = mode }
    if !visibleContextIds.isEmpty {
        p["visible_context_ids"] = visibleContextIds
    }
    emitEvent(p)
}

private func emitAnalyzeScreen(question: String?) {
    var payload: [String: Any] = ["type": "analyze_screen_requested"]
    if let question = question?.trimmingCharacters(in: .whitespacesAndNewlines), !question.isEmpty {
        payload["question"] = question
    }
    emitEvent(payload)
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

private func emitOpacityUpdated(_ opacity: Double) {
    emitEvent(["type": "opacity_updated", "opacity": opacity])
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
    var fillsVisibleFrame = false
    var contentCornerRadius: CGFloat? {
        didSet { applyContentCornerMask() }
    }
    private weak var pendingManualButton: NSButton?
    private weak var pendingManualScrubber: OpacityScrubberView?

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
        self.acceptsMouseMovedEvents = true
        // Production keeps the overlay out of screen capture. Capture-visible
        // QA is dev-gated and must never be enabled in customer launch paths.
        self.sharingType = captureVisibleForDebug ? .readOnly : .none
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }

    override func sendEvent(_ event: NSEvent) {
        let shouldApplyOverlayCursorAfterDispatch: Bool
        switch event.type {
        case .leftMouseDown:
            shouldApplyOverlayCursorAfterDispatch = true
            if let scrubber = manualOpacityScrubber(atWindowPoint: event.locationInWindow) {
                pendingManualScrubber = scrubber
                scrubber.updateValue(fromWindowEvent: event)
                return
            }
            if let button = manualButton(atWindowPoint: event.locationInWindow) {
                pendingManualButton = button
                button.highlight(true)
                return
            }
        case .leftMouseUp:
            shouldApplyOverlayCursorAfterDispatch = true
            if let scrubber = pendingManualScrubber {
                scrubber.updateValue(fromWindowEvent: event)
                pendingManualScrubber = nil
                return
            }
            if let button = pendingManualButton {
                button.highlight(false)
                let releaseButton = manualButton(atWindowPoint: event.locationInWindow)
                if releaseButton === button || button.frame.contains(button.superview?.convert(event.locationInWindow, from: nil) ?? .zero) {
                    button.performClick(nil)
                }
                pendingManualButton = nil
                return
            }
        case .leftMouseDragged:
            shouldApplyOverlayCursorAfterDispatch = true
            if let scrubber = pendingManualScrubber {
                scrubber.updateValue(fromWindowEvent: event)
                return
            }
            if let button = pendingManualButton {
                button.highlight(false)
                pendingManualButton = nil
            }
        case .mouseMoved, .cursorUpdate:
            shouldApplyOverlayCursorAfterDispatch = true
        case .keyDown:
            shouldApplyOverlayCursorAfterDispatch = false
            if let panel = contentView as? ExpandedPanelView,
               panel.routeKeyDownToComposer(event) {
                return
            }
        default:
            shouldApplyOverlayCursorAfterDispatch = false
            break
        }
        super.sendEvent(event)
        if shouldApplyOverlayCursorAfterDispatch,
           let panel = contentView as? ExpandedPanelView {
            panel.applyOverlayCursorPolicy(atWindowPoint: event.locationInWindow)
        }
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if let panel = contentView as? ExpandedPanelView,
           panel.routeKeyDownToComposer(event) {
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

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

    private func manualButton(atWindowPoint point: NSPoint) -> NSButton? {
        (contentView as? ExpandedPanelView)?.manualButton(atWindowPoint: point)
    }

    private func manualOpacityScrubber(atWindowPoint point: NSPoint) -> OpacityScrubberView? {
        (contentView as? ExpandedPanelView)?.manualOpacityScrubber(atWindowPoint: point)
    }

    override func setFrame(_ frameRect: NSRect, display displayFlag: Bool) {
        super.setFrame(clampedFrame(frameRect), display: displayFlag)
        syncContentViewFrame()
    }

    override func setFrame(_ frameRect: NSRect, display displayFlag: Bool, animate animateFlag: Bool) {
        super.setFrame(clampedFrame(frameRect), display: displayFlag, animate: animateFlag)
        syncContentViewFrame()
        if animateFlag {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.18) { [weak self] in
                self?.syncContentViewFrame()
            }
        }
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
            syncContentViewFrame()
        } else {
            super.setContentSize(size)
            syncContentViewFrame()
        }
    }

    func syncContentViewFrame() {
        guard let contentView else { return }
        let contentSize = contentRect(forFrameRect: frame).size
        let expected = NSRect(origin: .zero, size: contentSize)
        if abs(contentView.frame.width - expected.width) > 0.5
            || abs(contentView.frame.height - expected.height) > 0.5
            || abs(contentView.frame.minX) > 0.5
            || abs(contentView.frame.minY) > 0.5
        {
            contentView.frame = expected
        }
        contentView.needsLayout = true
        contentView.layoutSubtreeIfNeeded()
        (contentView as? ExpandedPanelView)?.synchronizeWindowGeometry()
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
        if fillsVisibleFrame {
            let fullScreenFrame = screen?.frame
                ?? NSScreen.main?.frame
                ?? visibleFrame
            return ExpandedPanelMetrics.fillVisibleScreenFrame(clamped, visibleFrame: fullScreenFrame)
        }
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
    private var dragStartedInHeader = false
    var onDragStateChanged: ((Bool) -> Void)?
    var onWindowFrameChanged: ((NSRect) -> Void)?

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func resetCursorRects() {
        super.resetCursorRects()
        addCursorRect(bounds, cursor: .openHand)
    }

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
            dragStartedInHeader = false
            onDragStateChanged?(false)
            super.mouseDown(with: event)
            return
        }
        dragStartedInHeader = true
        onDragStateChanged?(true)
        window?.makeKey()
        defer {
            dragStartedInHeader = false
            onDragStateChanged?(false)
        }
        window?.performDrag(with: event)
        if let window {
            onWindowFrameChanged?(window.frame)
        }
    }

    override func mouseDragged(with event: NSEvent) {
        guard dragStartedInHeader else {
            super.mouseDragged(with: event)
            return
        }
    }

    override func mouseUp(with event: NSEvent) {
        dragStartedInHeader = false
        onDragStateChanged?(false)
        super.mouseUp(with: event)
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
                || candidate is ClickableHeaderBadge
            {
                return true
            }
            if let textField = candidate as? NSTextField,
               textField.isEditable || textField.isSelectable {
                return true
            }
            current = candidate.superview
        }
        return false
    }
}

private final class HeaderShieldView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    override var acceptsFirstResponder: Bool { false }
}

private final class ClickableHeaderBadge: NSTextField {
    var onClick: (() -> Void)?

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard !isHidden, alphaValue > 0.01, bounds.contains(point) else { return nil }
        return self
    }

    override func resetCursorRects() {
        super.resetCursorRects()
        addCursorRect(bounds, cursor: .pointingHand)
    }

    override func mouseDown(with event: NSEvent) {
        onClick?()
    }
}

private final class CopyCardButton: NSButton {
    var copyText = ""
    var resetWorkItem: DispatchWorkItem?
}

private final class CanvasCardButton: NSButton {
    var cardId = ""
}

private final class RemoveAttachmentButton: NSButton {
    var contextId = ""
}

private final class AttachmentOpenButton: NSButton {
    var filePath: String?
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

private enum PillHealthState: Equatable {
    case unknown
    case ready
    case needsAttention
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
        let clean = label.trimmingCharacters(in: .whitespacesAndNewlines)
        balanceText = clean
        let lower = clean.lowercased()
        if lower.contains("login") || lower.contains("sign in") || lower.contains("low") {
            setHealthState(.needsAttention)
        } else if clean.hasPrefix("$") && !lower.contains("--") {
            setHealthState(.ready)
        }
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
    var onMoved: ((NSRect) -> Void)?
    private var runState: PillRunState = .ready
    private var healthState: PillHealthState = .unknown
    private var mouseDownLocation: NSPoint?
    private var mouseDownScreenLocation: NSPoint?
    private var dragStartWindowFrame: NSRect?
    private var didDragFromMouseDown = false

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

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override var acceptsFirstResponder: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard bounds.contains(point), !isHidden, alphaValue > 0.01 else { return nil }
        let railPoint = convert(point, to: controlRail)
        if controlRail.bounds.contains(railPoint) {
            for button in [styleButton, runButton, endButton].reversed() {
                let buttonPoint = controlRail.convert(railPoint, to: button)
                if button.bounds.contains(buttonPoint), !button.isHidden, button.alphaValue > 0.01 {
                    return button
                }
            }
        }
        return self
    }

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
        dotColor = resolvedDotColor
        updateRunStateDisplay()
    }

    func setHealthState(_ state: PillHealthState) {
        healthState = state
        dotColor = resolvedDotColor
        updateRunStateDisplay()
    }

    private var resolvedDotColor: NSColor {
        if runState == .failed || healthState == .needsAttention {
            return BlueyTheme.danger
        }
        if runState == .connecting {
            return BlueyTheme.warning
        }
        if runState == .listening || healthState == .ready {
            return BlueyTheme.green
        }
        return BlueyTheme.textDim.withAlphaComponent(0.72)
    }

    private var resolvedAccessibilityLabel: String {
        if runState == .failed || healthState == .needsAttention {
            return "Bluey needs attention"
        }
        if runState == .connecting || runState == .listening {
            return runState.accessibilityLabel
        }
        if healthState == .ready {
            return "Bluey ready"
        }
        return "Bluey starting"
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
        setAccessibilityLabel(resolvedAccessibilityLabel)
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

    private func lightMaterialAlpha(_ base: CGFloat, floor: CGFloat = 0.24) -> CGFloat {
        blueyLightMaterialAlpha(base, opacity: backgroundOpacity, floor: floor)
    }

    func applyBackgroundOpacity(_ opacity: Double) {
        backgroundOpacity = min(max(CGFloat(opacity), minimumOverlayBackgroundOpacity), 1.0)
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
        let outer = bounds.insetBy(dx: 0.85, dy: 0.85)
        let radius = outer.height / 2
        drawBlueyGlassPanel(in: outer, radius: radius, opacity: backgroundOpacity)
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeKey()
        mouseDownLocation = event.locationInWindow
        mouseDownScreenLocation = NSEvent.mouseLocation
        dragStartWindowFrame = window?.frame
        didDragFromMouseDown = false
    }

    override func mouseDragged(with event: NSEvent) {
        guard
            mouseDownLocation != nil,
            let startScreen = mouseDownScreenLocation,
            let startFrame = dragStartWindowFrame,
            let window
        else {
            super.mouseDragged(with: event)
            return
        }

        let currentScreen = NSEvent.mouseLocation
        let dx = currentScreen.x - startScreen.x
        let dy = currentScreen.y - startScreen.y
        guard didDragFromMouseDown || abs(dx) > 4 || abs(dy) > 4 else { return }

        didDragFromMouseDown = true
        var nextFrame = startFrame
        nextFrame.origin.x += dx
        nextFrame.origin.y += dy
        nextFrame = clampedPillDragFrame(nextFrame)
        window.setFrame(nextFrame, display: true)
    }

    override func mouseUp(with event: NSEvent) {
        let dragged = didDragFromMouseDown
        if dragged, let window {
            onMoved?(window.frame)
        }
        defer {
            mouseDownLocation = nil
            mouseDownScreenLocation = nil
            dragStartWindowFrame = nil
            didDragFromMouseDown = false
        }
        if !dragged {
            onClick?()
        }
    }

    private func clampedPillDragFrame(_ frame: NSRect) -> NSRect {
        let visibleFrame = OverlayScreenPlacement.activeVisibleFrame()
        let inset: CGFloat = 8
        var clamped = frame
        clamped.size = PillMetrics.size
        clamped.origin.x = min(
            max(visibleFrame.minX + inset, clamped.origin.x),
            visibleFrame.maxX - clamped.size.width - inset)
        clamped.origin.y = min(
            max(visibleFrame.minY + inset, clamped.origin.y),
            visibleFrame.maxY - clamped.size.height - inset)
        return clamped
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
    var attachments: [OverlayContextItem]
}

private enum CanvasKind: Equatable {
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

    var shortTitle: String {
        switch self {
        case .code: return "Coding"
        case .systemDesign: return "System Design"
        case .screen: return "Screen"
        case .document: return "Docs"
        case .structured: return "Workspace"
        }
    }

    var subtitle: String {
        switch self {
        case .code: return "Code and complexity"
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
    var title: String
    var subtitle: String
    var content: String
    let sourceCardId: String
    var followupCount: Int = 0
    var sourceQuestion: String? = nil
}

private struct AnswerStreamStats {
    let startedAt: CFTimeInterval
    var firstUpdateAt: CFTimeInterval?
    var lastBodyChars: Int = 0
}

private final class FlippedStackView: NSStackView {
    override var isFlipped: Bool { true }
}

private final class FeedView: NSView {
    private var cards: [RenderedCard] = []
    private let stack = FlippedStackView()
    private let scroll = NSScrollView()
    private let emptyState = NSView()
    private var userScrolledAwayFromLatest = false
    private let autoScrollTolerance: CGFloat = 44
    private var lightThemeEnabled = false
    private var currentOpacity: CGFloat = 0.94
    var onTranscript: ((RenderedCard) -> Void)?
    var onOpenURL: ((URL) -> Void)?
    var onOpenCanvasForCard: ((String) -> Void)?

    private var panelColor: NSColor {
        lightThemeEnabled
            ? BlueyLightTheme.content
            : BlueyTheme.panel
    }

    private var surfaceColor: NSColor {
        lightThemeEnabled
            ? BlueyLightTheme.surface
            : BlueyTheme.surface
    }

    private var textColor: NSColor {
        lightThemeEnabled
            ? BlueyLightTheme.text
            : BlueyTheme.text
    }

    private var dimTextColor: NSColor {
        lightThemeEnabled
            ? BlueyLightTheme.textDim
            : BlueyTheme.textDim
    }

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
        stack.edgeInsets = NSEdgeInsets(top: 28, left: 0, bottom: 16, right: 0)
        stack.translatesAutoresizingMaskIntoConstraints = false

        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        scroll.verticalScrollElasticity = .allowed
        scroll.borderType = .noBorder
        scroll.drawsBackground = false
        scroll.contentInsets = NSEdgeInsets(top: 8, left: 0, bottom: 10, right: 0)
        scroll.documentView = stack
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.wantsLayer = true
        scroll.layer?.masksToBounds = true
        scroll.contentView.wantsLayer = true
        scroll.contentView.layer?.masksToBounds = true
        scroll.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(scrollContentBoundsDidChange(_:)),
            name: NSView.boundsDidChangeNotification,
            object: scroll.contentView)
        addSubview(scroll)
        configureEmptyState()
        NSLayoutConstraint.activate([
            scroll.topAnchor.constraint(equalTo: topAnchor),
            scroll.leadingAnchor.constraint(equalTo: leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: scroll.contentView.leadingAnchor),
            stack.topAnchor.constraint(equalTo: scroll.contentView.topAnchor),
            stack.trailingAnchor.constraint(equalTo: scroll.contentView.trailingAnchor),
            stack.bottomAnchor.constraint(greaterThanOrEqualTo: scroll.contentView.bottomAnchor),
            stack.widthAnchor.constraint(equalTo: scroll.contentView.widthAnchor),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        let rect = bounds.insetBy(dx: 0.5, dy: 0.5)
        let path = NSBezierPath(roundedRect: rect, xRadius: 16, yRadius: 16)

        guard lightThemeEnabled else {
            BlueyTheme.panel
                .withAlphaComponent(blueyMaterialAlpha(0.95, opacity: currentOpacity))
                .setFill()
            path.fill()
            BlueyTheme.hairline.setStroke()
            path.lineWidth = 0.8
            path.stroke()
            return
        }

        let alpha = blueyLightMaterialAlpha(0.98, opacity: currentOpacity, floor: 0.16)
        NSGraphicsContext.saveGraphicsState()
        path.addClip()
        NSGradient(colors: [
            BlueyLightTheme.contentHigh.withAlphaComponent(alpha),
            BlueyLightTheme.content.withAlphaComponent(alpha),
            BlueyLightTheme.contentLow.withAlphaComponent(alpha),
        ])?.draw(in: path, angle: 88)
        NSGraphicsContext.restoreGraphicsState()

        BlueyLightTheme.border.withAlphaComponent(0.62).setStroke()
        path.lineWidth = 0.8
        path.stroke()

        let highlight = NSBezierPath(roundedRect: rect.insetBy(dx: 1.2, dy: 1.2), xRadius: 14.8, yRadius: 14.8)
        NSColor.white.withAlphaComponent(blueyLightMaterialAlpha(0.16, opacity: currentOpacity, floor: 0.025)).setStroke()
        highlight.lineWidth = 0.8
        highlight.stroke()
    }

    func push(_ card: RenderedCard) {
        var card = card
        card.body = sanitizeOverlayOutput(kind: card.kind, body: card.body)
        card.artifact = sanitizeOverlayArtifact(card.artifact)
        if normalizedCardKind(card.kind) == "transcript" {
            onTranscript?(card)
            emitCardRendered(id: card.id)
            return
        }
        if loginURL(from: card) != nil {
            removeAllCards()
        }
        let shouldAutoScroll = shouldFollowIncomingContent()
        cards.append(card)
        emptyState.isHidden = true
        let view = makeCardView(card)
        stack.addArrangedSubview(view)
        view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        scrollToBottomIfNeeded(shouldAutoScroll)
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
            artifact: nil,
            attachments: [])
        pushTranscriptCard(card)
    }

    @discardableResult
    func update(id: String, body: String, done: Bool, costLabel: String?, artifact: OverlayArtifact?) -> RenderedCard? {
        guard let idx = cards.firstIndex(where: { $0.id == id }) else { return nil }
        let shouldAutoScroll = shouldFollowIncomingContent()
        cards[idx].body = sanitizeOverlayOutput(kind: cards[idx].kind, body: body)
        cards[idx].done = done
        if let costLabel {
            cards[idx].costLabel = costLabel
        }
        if artifact != nil {
            cards[idx].artifact = sanitizeOverlayArtifact(artifact)
        }
        replaceCardView(at: idx)
        scrollToBottomIfNeeded(shouldAutoScroll)
        return cards[idx]
    }

    func clear() {
        removeAllCards()
        emptyState.isHidden = false
    }

    func forwardScrollWheel(_ event: NSEvent) {
        scroll.scrollWheel(with: event)
        updateScrollPinAfterUserInput()
    }

    func latestCopyableCardText() -> String? {
        for card in cards.reversed() {
            let kind = normalizedCardKind(card.kind)
            guard kind == "answer" || kind == "context" || kind == "warning" else { continue }
            let text = chatBody(for: card, rawBody: card.body)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            guard !text.isEmpty, text != "Thinking..." else { continue }
            return text
        }
        return nil
    }

    override func scrollWheel(with event: NSEvent) {
        scroll.scrollWheel(with: event)
        updateScrollPinAfterUserInput()
    }

    @objc private func scrollContentBoundsDidChange(_ notification: Notification) {
        userScrolledAwayFromLatest = !isScrolledNearLatest()
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
            if view is CopyCardButton || view is CanvasCardButton {
                return true
            }
            hit = view.superview
        }
        return false
    }

    func nearestQuestionBody(beforeCardId id: String) -> String? {
        guard let idx = cards.firstIndex(where: { $0.id == id }), idx > 0 else { return nil }
        for card in cards[..<idx].reversed() {
            guard normalizedCardKind(card.kind) == "question" else { continue }
            let body = card.body.trimmingCharacters(in: .whitespacesAndNewlines)
            return body.isEmpty ? nil : body
        }
        return nil
    }

    var hasVisibleCards: Bool {
        !cards.isEmpty
    }

    func setLightTheme(_ enabled: Bool, opacity: CGFloat) {
        let changed = lightThemeEnabled != enabled
        lightThemeEnabled = enabled
        applyBackgroundOpacity(opacity)
        refreshEmptyStateTheme()
        needsDisplay = true
        scroll.needsDisplay = true
        scroll.contentView.needsDisplay = true
        emptyState.needsDisplay = true
        guard changed else { return }
        let shouldAutoScroll = shouldFollowIncomingContent()
        for view in stack.arrangedSubviews {
            stack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        for card in cards {
            let view = makeCardView(card)
            stack.addArrangedSubview(view)
            view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        }
        scrollToBottomIfNeeded(shouldAutoScroll)
    }

    func applyBackgroundOpacity(_ opacity: CGFloat) {
        currentOpacity = opacity
        if lightThemeEnabled {
            layer?.backgroundColor = NSColor.clear.cgColor
            layer?.borderColor = BlueyLightTheme.border.cgColor
            needsDisplay = true
            return
        }
        layer?.contents = nil
        layer?.backgroundColor = panelColor
            .withAlphaComponent(blueyMaterialAlpha(0.95, opacity: opacity))
            .cgColor
        layer?.borderColor = BlueyTheme.hairline.cgColor
        needsDisplay = true
    }

    private func removeAllCards() {
        userScrolledAwayFromLatest = false
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
            artifact: input.artifact,
            attachments: input.attachments)

        emptyState.isHidden = true
        let shouldAutoScroll = shouldFollowIncomingContent()
        if let lastIndex = cards.indices.last,
           normalizedCardKind(cards[lastIndex].kind) == "transcript",
           transcriptCardSource(cards[lastIndex]) == source {
            let merged = mergedTranscriptBody(cards[lastIndex].body, card.body)
            guard merged != cards[lastIndex].body else {
                scrollToBottomIfNeeded(shouldAutoScroll)
                return
            }
            cards[lastIndex].body = merged
            replaceCardView(at: lastIndex)
            scrollToBottomIfNeeded(shouldAutoScroll)
            return
        }

        cards.append(card)
        let view = makeCardView(card)
        stack.addArrangedSubview(view)
        view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        scrollToBottomIfNeeded(shouldAutoScroll)
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

        let title = NSTextField(labelWithString: "Ready when you are")
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = NSFont.systemFont(ofSize: 19, weight: .bold)
        title.textColor = BlueyTheme.text
        title.alignment = .center

        let subtitle = NSTextField(labelWithString: "Ask a question, attach what you have, or capture the screen.")
        subtitle.translatesAutoresizingMaskIntoConstraints = false
        subtitle.font = NSFont.systemFont(ofSize: 11.5, weight: .medium)
        subtitle.textColor = BlueyTheme.textDim
        subtitle.alignment = .center
        subtitle.maximumNumberOfLines = 2
        subtitle.lineBreakMode = .byWordWrapping

        let dropTarget = NSView()
        dropTarget.translatesAutoresizingMaskIntoConstraints = false
        dropTarget.wantsLayer = true
        dropTarget.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.035).cgColor
        dropTarget.layer?.cornerRadius = 13
        dropTarget.layer?.borderWidth = 1
        dropTarget.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.24).cgColor

        let dropTitle = NSTextField(labelWithString: "Drop files for this conversation")
        dropTitle.translatesAutoresizingMaskIntoConstraints = false
        dropTitle.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
        dropTitle.textColor = BlueyTheme.text
        dropTitle.alignment = .center

        let dropHint = NSTextField(labelWithString: "Docs, images, code, notes")
        dropHint.translatesAutoresizingMaskIntoConstraints = false
        dropHint.font = NSFont.systemFont(ofSize: 10, weight: .medium)
        dropHint.textColor = BlueyTheme.textDim
        dropHint.alignment = .center

        emptyState.addSubview(title)
        emptyState.addSubview(subtitle)
        emptyState.addSubview(dropTarget)
        dropTarget.addSubview(dropTitle)
        dropTarget.addSubview(dropHint)

        NSLayoutConstraint.activate([
            emptyState.centerXAnchor.constraint(equalTo: centerXAnchor),
            emptyState.centerYAnchor.constraint(equalTo: centerYAnchor, constant: -12),
            emptyState.widthAnchor.constraint(lessThanOrEqualTo: widthAnchor, multiplier: 0.76),

            title.topAnchor.constraint(equalTo: emptyState.topAnchor),
            title.leadingAnchor.constraint(equalTo: emptyState.leadingAnchor),
            title.trailingAnchor.constraint(equalTo: emptyState.trailingAnchor),

            subtitle.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 6),
            subtitle.leadingAnchor.constraint(equalTo: emptyState.leadingAnchor),
            subtitle.trailingAnchor.constraint(equalTo: emptyState.trailingAnchor),

            dropTarget.topAnchor.constraint(equalTo: subtitle.bottomAnchor, constant: 12),
            dropTarget.centerXAnchor.constraint(equalTo: emptyState.centerXAnchor),
            dropTarget.widthAnchor.constraint(lessThanOrEqualTo: emptyState.widthAnchor),
            dropTarget.widthAnchor.constraint(greaterThanOrEqualToConstant: 252),
            dropTarget.heightAnchor.constraint(equalToConstant: 50),
            dropTarget.bottomAnchor.constraint(equalTo: emptyState.bottomAnchor),

            dropTitle.topAnchor.constraint(equalTo: dropTarget.topAnchor, constant: 8),
            dropTitle.leadingAnchor.constraint(equalTo: dropTarget.leadingAnchor, constant: 18),
            dropTitle.trailingAnchor.constraint(equalTo: dropTarget.trailingAnchor, constant: -18),

            dropHint.topAnchor.constraint(equalTo: dropTitle.bottomAnchor, constant: 2),
            dropHint.leadingAnchor.constraint(equalTo: dropTarget.leadingAnchor, constant: 18),
            dropHint.trailingAnchor.constraint(equalTo: dropTarget.trailingAnchor, constant: -18),
        ])
        refreshEmptyStateTheme()
    }

    private func refreshEmptyStateTheme() {
        func visit(_ view: NSView) {
            if let label = view as? NSTextField {
                let size = label.font?.pointSize ?? 0
                label.textColor = size >= 12 ? textColor : dimTextColor
            } else if view !== emptyState, view.subviews.contains(where: { $0 is NSTextField }) {
                view.wantsLayer = true
                if lightThemeEnabled {
                    view.layer?.backgroundColor = BlueyLightTheme.surfaceRaised
                        .withAlphaComponent(blueyLightMaterialAlpha(0.92, opacity: currentOpacity, floor: 0.18))
                        .cgColor
                    view.layer?.borderColor = BlueyLightTheme.accentBorder.withAlphaComponent(0.56).cgColor
                    view.layer?.shadowColor = NSColor.black.cgColor
                    view.layer?.shadowOpacity = 0.12
                    view.layer?.shadowRadius = 16
                    view.layer?.shadowOffset = NSSize(width: 0, height: -5)
                } else {
                    view.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.035).cgColor
                    view.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.24).cgColor
                    view.layer?.shadowOpacity = 0
                }
            }
            for child in view.subviews {
                visit(child)
            }
        }
        visit(emptyState)
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
        if rightAligned {
            bubble.layer?.backgroundColor = (lightThemeEnabled
                ? BlueyLightTheme.surfaceRaised.withAlphaComponent(blueyLightMaterialAlpha(0.90, opacity: currentOpacity, floor: 0.20))
                : NSColor(red: 0.90, green: 0.93, blue: 0.95, alpha: 0.96)).cgColor
        } else {
            bubble.layer?.backgroundColor = (answerLike ? NSColor.clear : surfaceColor).cgColor
        }
        bubble.layer?.cornerRadius = rightAligned ? 16 : 12
        bubble.layer?.borderWidth = answerLike ? 0 : 1
        bubble.layer?.borderColor = rightAligned
            ? (lightThemeEnabled ? BlueyLightTheme.border : NSColor.white.withAlphaComponent(0.20)).cgColor
            : accent.withAlphaComponent(answerLike ? 0.24 : 0.14).cgColor
        bubble.layer?.shadowColor = NSColor.black.cgColor
        bubble.layer?.shadowOpacity = answerLike ? 0 : (lightThemeEnabled ? 0.07 : 0.14)
        bubble.layer?.shadowRadius = lightThemeEnabled ? 8 : 10
        bubble.layer?.shadowOffset = NSSize(width: 0, height: -4)
        bubble.translatesAutoresizingMaskIntoConstraints = false

        let metaLabel = NSTextField(labelWithString: kindLabel(card))
        metaLabel.font = NSFont.systemFont(ofSize: 11, weight: .bold)
        metaLabel.textColor = metaLabelColor(for: card, rightAligned: rightAligned, accent: accent)
        metaLabel.translatesAutoresizingMaskIntoConstraints = false

        let titleText = displayTitle(for: card)
        let titleLabel = NSTextField(labelWithString: titleText)
        titleLabel.font = NSFont.systemFont(ofSize: 12.5, weight: .semibold)
        titleLabel.textColor = rightAligned ? NSColor.black.withAlphaComponent(0.74) : textColor
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.lineBreakMode = .byTruncatingTail

        let signInURL = signInLike ? loginURL(from: card) : nil
        let rawBody = card.body.isEmpty && !card.done ? "Thinking..." : card.body
        let bodyText = signInURL == nil
            ? chatBody(for: card, rawBody: rawBody)
            : signInBody(from: rawBody)
        let bodyLabel = ArrowCursorTextField(wrappingLabelWithString: bodyText)
        bodyLabel.font = bodyFont(for: card)
        bodyLabel.textColor = rightAligned ? NSColor.black : textColor
        bodyLabel.alignment = signInURL == nil ? .left : .center
        bodyLabel.translatesAutoresizingMaskIntoConstraints = false
        bodyLabel.preferredMaxLayoutWidth = signInURL == nil ? (rightAligned ? 360 : 480) : 430
        bodyLabel.isSelectable = signInURL == nil
        bodyLabel.allowsEditingTextAttributes = false
        if let attributedBody = attributedChatBody(for: card, text: bodyText, rightAligned: rightAligned) {
            bodyLabel.attributedStringValue = attributedBody
        }

        let statusLabel = NSTextField(labelWithString: statusText(for: card))
        statusLabel.font = NSFont.monospacedSystemFont(ofSize: 9.5, weight: .semibold)
        statusLabel.textColor = rightAligned ? NSColor.black.withAlphaComponent(0.46) : dimTextColor
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        let copyButton = shouldShowCopyButton(for: card, rightAligned: rightAligned, signInURL: signInURL)
            ? makeCopyCardButton(text: rawBody.isEmpty ? bodyText : rawBody, rightAligned: rightAligned)
            : nil
        let canvasButton = shouldShowCanvasButton(for: card, rightAligned: rightAligned, signInURL: signInURL)
            ? makeCanvasCardButton(cardId: card.id, artifactType: card.artifact?.artifactType, rightAligned: rightAligned)
            : nil
        let actionButtons = [copyButton, canvasButton].compactMap { $0 }

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
        let sentAttachmentStrip = (!signInLike && rightAligned && !card.attachments.isEmpty)
            ? makeSentAttachmentStrip(card.attachments)
            : nil

        row.addSubview(bubble)
        bubble.addSubview(metaLabel)
        bubble.addSubview(titleLabel)
        bubble.addSubview(bodyLabel)
        bubble.addSubview(statusLabel)
        if let sentAttachmentStrip {
            bubble.addSubview(sentAttachmentStrip)
        }
        if let copyButton {
            bubble.addSubview(copyButton)
        }
        if let canvasButton {
            bubble.addSubview(canvasButton)
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
            if let firstButton = actionButtons.first {
                constraints.append(statusLabel.trailingAnchor.constraint(lessThanOrEqualTo: firstButton.leadingAnchor, constant: -6))
                for (index, button) in actionButtons.enumerated() {
                    constraints.append(contentsOf: [
                        button.centerYAnchor.constraint(equalTo: metaLabel.centerYAnchor),
                        button.widthAnchor.constraint(equalToConstant: actionButtonWidth(button)),
                        button.heightAnchor.constraint(equalToConstant: 22),
                    ])
                    if index + 1 < actionButtons.count {
                        constraints.append(button.trailingAnchor.constraint(equalTo: actionButtons[index + 1].leadingAnchor, constant: -6))
                    } else {
                        constraints.append(button.trailingAnchor.constraint(equalTo: bubble.trailingAnchor, constant: -10))
                    }
                }
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
        } else if let sentAttachmentStrip {
            constraints.append(contentsOf: [
                bodyLabel.bottomAnchor.constraint(equalTo: sentAttachmentStrip.topAnchor, constant: -8),
                sentAttachmentStrip.leadingAnchor.constraint(equalTo: bodyLabel.leadingAnchor),
                sentAttachmentStrip.trailingAnchor.constraint(equalTo: bodyLabel.trailingAnchor),
                sentAttachmentStrip.bottomAnchor.constraint(equalTo: bubble.bottomAnchor, constant: -12),
                sentAttachmentStrip.heightAnchor.constraint(equalToConstant: 32),
            ])
        } else {
            constraints.append(bodyLabel.bottomAnchor.constraint(equalTo: bubble.bottomAnchor, constant: -12))
        }
        NSLayoutConstraint.activate(constraints)
        return row
    }

    private func makeSentAttachmentStrip(_ items: [OverlayContextItem]) -> NSScrollView {
        let scroll = NSScrollView()
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.drawsBackground = false
        scroll.borderType = .noBorder
        scroll.hasVerticalScroller = false
        scroll.hasHorizontalScroller = true
        scroll.autohidesScrollers = false
        scroll.scrollerStyle = .overlay
        scroll.horizontalScrollElasticity = .allowed
        scroll.usesPredominantAxisScrolling = false

        let stack = NSStackView()
        stack.translatesAutoresizingMaskIntoConstraints = false
        stack.orientation = .horizontal
        stack.alignment = .centerY
        stack.spacing = 6
        stack.edgeInsets = NSEdgeInsets(top: 1, left: 0, bottom: 1, right: 0)
        for item in items {
            stack.addArrangedSubview(makeSentAttachmentChip(item))
        }

        scroll.documentView = stack
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: scroll.contentView.leadingAnchor),
            stack.topAnchor.constraint(equalTo: scroll.contentView.topAnchor),
            stack.bottomAnchor.constraint(equalTo: scroll.contentView.bottomAnchor),
            stack.heightAnchor.constraint(equalTo: scroll.contentView.heightAnchor),
            stack.trailingAnchor.constraint(greaterThanOrEqualTo: scroll.contentView.trailingAnchor),
        ])
        return scroll
    }

    private func makeSentAttachmentChip(_ item: OverlayContextItem) -> NSView {
        let isImage = feedAttachmentIsImage(item.kind)
        let chip = NSView()
        chip.translatesAutoresizingMaskIntoConstraints = false
        chip.wantsLayer = true
        chip.layer?.backgroundColor = isImage
            ? NSColor(red: 0.78, green: 0.90, blue: 0.98, alpha: 0.34).cgColor
            : NSColor.black.withAlphaComponent(0.045).cgColor
        chip.layer?.cornerRadius = 12
        chip.layer?.borderWidth = 1
        chip.layer?.borderColor = isImage
            ? BlueyTheme.cyan.withAlphaComponent(0.34).cgColor
            : NSColor.black.withAlphaComponent(0.10).cgColor
        let titleText = item.title.isEmpty ? (isImage ? "Screen" : "Attached file") : item.title
        chip.toolTip = (["Sent with this question", titleText, item.kind.uppercased(), item.path]
            .compactMap { value -> String? in
                guard let value, !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                    return nil
                }
                return value
            }
            .joined(separator: "\n"))

        let icon = NSImageView()
        icon.translatesAutoresizingMaskIntoConstraints = false
        icon.imageScaling = isImage ? .scaleProportionallyUpOrDown : .scaleProportionallyDown
        icon.wantsLayer = true
        icon.layer?.cornerRadius = isImage ? 5 : 0
        icon.layer?.masksToBounds = isImage
        icon.layer?.borderWidth = isImage ? 1 : 0
        icon.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.20).cgColor
        if isImage, let path = item.path, let image = NSImage(contentsOfFile: path) {
            image.isTemplate = false
            icon.image = image
            icon.contentTintColor = nil
        } else if let image = symbolImage(feedAttachmentSymbol(for: item.kind)) {
            image.isTemplate = true
            icon.image = image
            icon.contentTintColor = feedAttachmentAccent(for: item.kind)
        }

        let title = NSTextField(labelWithString: titleText)
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = NSFont.systemFont(ofSize: 10.5, weight: .semibold)
        title.textColor = NSColor.black.withAlphaComponent(0.72)
        title.lineBreakMode = .byTruncatingMiddle
        title.maximumNumberOfLines = 1
        title.toolTip = chip.toolTip

        let open = AttachmentOpenButton(title: "", target: self, action: #selector(openSentAttachmentClicked(_:)))
        open.translatesAutoresizingMaskIntoConstraints = false
        open.filePath = item.path
        open.isBordered = false
        open.toolTip = item.path == nil ? chip.toolTip : "Open \(titleText)"

        chip.addSubview(icon)
        chip.addSubview(title)
        chip.addSubview(open)
        NSLayoutConstraint.activate([
            chip.heightAnchor.constraint(equalToConstant: 28),
            chip.widthAnchor.constraint(greaterThanOrEqualToConstant: isImage ? 104 : 96),
            chip.widthAnchor.constraint(lessThanOrEqualToConstant: isImage ? 170 : 184),

            icon.leadingAnchor.constraint(equalTo: chip.leadingAnchor, constant: 7),
            icon.centerYAnchor.constraint(equalTo: chip.centerYAnchor),
            icon.widthAnchor.constraint(equalToConstant: isImage ? 22 : 14),
            icon.heightAnchor.constraint(equalToConstant: isImage ? 22 : 14),

            title.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: isImage ? 6 : 5),
            title.centerYAnchor.constraint(equalTo: chip.centerYAnchor),
            title.trailingAnchor.constraint(equalTo: chip.trailingAnchor, constant: -9),

            open.topAnchor.constraint(equalTo: chip.topAnchor),
            open.leadingAnchor.constraint(equalTo: chip.leadingAnchor),
            open.bottomAnchor.constraint(equalTo: chip.bottomAnchor),
            open.trailingAnchor.constraint(equalTo: chip.trailingAnchor),
        ])
        return chip
    }

    @objc private func openSentAttachmentClicked(_ sender: AttachmentOpenButton) {
        guard let path = sender.filePath?.trimmingCharacters(in: .whitespacesAndNewlines),
              !path.isEmpty else { return }
        onOpenURL?(URL(fileURLWithPath: path))
    }

    private func feedAttachmentSymbol(for kind: String) -> String {
        switch kind {
        case "image", "diagram", "screen", "screenshot": return "photo"
        case "code": return "curlybraces"
        case "text": return "doc.plaintext"
        case "document": return "doc.text"
        default: return "doc"
        }
    }

    private func feedAttachmentIsImage(_ kind: String) -> Bool {
        kind == "image" || kind == "diagram" || kind == "screen" || kind == "screenshot"
    }

    private func feedAttachmentAccent(for kind: String) -> NSColor {
        switch kind {
        case "image", "diagram", "screen", "screenshot":
            return NSColor(red: 0.58, green: 0.74, blue: 1.0, alpha: 1.0)
        case "code":
            return NSColor(red: 0.58, green: 1.0, blue: 0.74, alpha: 1.0)
        case "text":
            return NSColor(red: 1.0, green: 0.82, blue: 0.42, alpha: 1.0)
        case "document":
            return NSColor(red: 1.0, green: 0.43, blue: 0.34, alpha: 1.0)
        default:
            return BlueyTheme.cyan
        }
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

    private func makeCanvasCardButton(cardId: String, artifactType: String?, rightAligned: Bool) -> CanvasCardButton {
        let button = CanvasCardButton(title: "", target: self, action: #selector(openCanvasCardClicked(_:)))
        button.translatesAutoresizingMaskIntoConstraints = false
        button.cardId = cardId
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 10
        button.layer?.backgroundColor = rightAligned
            ? NSColor.black.withAlphaComponent(0.06).cgColor
            : BlueyTheme.cyan.withAlphaComponent(0.10).cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = rightAligned
            ? NSColor.black.withAlphaComponent(0.10).cgColor
            : BlueyTheme.cyan.withAlphaComponent(0.28).cgColor
        button.contentTintColor = rightAligned
            ? NSColor.black.withAlphaComponent(0.58)
            : BlueyTheme.cyan.withAlphaComponent(0.96)
        let kind = CanvasKind.fromArtifactType(artifactType ?? "")
        if let image = symbolImage(kind.icon) ?? symbolImage("sidebar.right") {
            image.isTemplate = true
            button.image = image
            button.imagePosition = .imageOnly
            button.imageScaling = .scaleProportionallyDown
        } else {
            button.title = "Canvas"
        }
        button.toolTip = "Open \(kind.title)"
        return button
    }

    private func actionButtonWidth(_ button: NSButton) -> CGFloat {
        button is CopyCardButton ? 22 : 24
    }

    private func shouldShowCopyButton(for card: RenderedCard, rightAligned: Bool, signInURL: URL?) -> Bool {
        guard signInURL == nil, !rightAligned else { return false }
        let body = card.body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !body.isEmpty, body != "Thinking..." else { return false }
        let kind = normalizedCardKind(card.kind)
        return kind == "answer" || kind == "context" || kind == "warning" || card.artifact != nil
    }

    private func shouldShowCanvasButton(for card: RenderedCard, rightAligned: Bool, signInURL: URL?) -> Bool {
        guard signInURL == nil, !rightAligned else { return false }
        guard normalizedCardKind(card.kind) == "answer" else { return false }
        return card.artifact != nil
    }

    @objc private func copyCardClicked(_ sender: CopyCardButton) {
        let text = sender.copyText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        flashCopySuccess(sender)
    }

    @objc private func openCanvasCardClicked(_ sender: CanvasCardButton) {
        onOpenCanvasForCard?(sender.cardId)
    }

    private func flashCopySuccess(_ button: CopyCardButton) {
        button.resetWorkItem?.cancel()
        let originalImage = button.image
        let originalTitle = button.title
        let originalTint = button.contentTintColor
        if let image = symbolImage("checkmark") {
            image.isTemplate = true
            button.image = image
            button.imagePosition = .imageOnly
            button.title = ""
        } else {
            button.image = nil
            button.title = "✓"
        }
        button.contentTintColor = BlueyTheme.green
        button.toolTip = "Copied"

        let reset = DispatchWorkItem { [weak button] in
            guard let button else { return }
            button.image = originalImage
            button.title = originalTitle
            button.contentTintColor = originalTint
            button.toolTip = "Copy this message"
            button.resetWorkItem = nil
        }
        button.resetWorkItem = reset
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.1, execute: reset)
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
        case "answer":      return "Bluey"
        case "question":    return "Question"
        case "action_item": return "ACTION"
        case "decision":    return "DECISION"
        case "context":     return "CONTEXT"
        case "transcript":
            let source = card.title.trimmingCharacters(in: .whitespacesAndNewlines)
            return source.isEmpty ? "Audio" : source
        case "warning":     return "WARNING"
        case "system":      return "SYSTEM"
        default:            return "Bluey"
        }
    }

    private func metaLabelColor(for card: RenderedCard, rightAligned: Bool, accent: NSColor) -> NSColor {
        let kind = normalizedCardKind(card.kind)
        if kind == "transcript" {
            return sourceMarkerColor(rightAligned: rightAligned)
        }
        if kind == "question" {
            return rightAligned ? NSColor.black.withAlphaComponent(0.54) : BlueyTheme.green
        }
        return rightAligned ? NSColor.black.withAlphaComponent(0.58) : accent
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

    private func attributedChatBody(for card: RenderedCard, text: String, rightAligned: Bool) -> NSAttributedString? {
        let kind = normalizedCardKind(card.kind)
        guard kind == "question" || kind == "transcript" else { return nil }
        let font = bodyFont(for: card)
        let attributed = NSMutableAttributedString(
            string: text,
            attributes: [
                .font: font,
                .foregroundColor: rightAligned ? NSColor.black : textColor,
            ])
        let sourceFont = NSFont.systemFont(ofSize: font.pointSize, weight: .bold)
        let sourceColor = sourceMarkerColor(rightAligned: rightAligned)
        let fullRange = NSRange(location: 0, length: (text as NSString).length)
        guard let regex = try? NSRegularExpression(pattern: #"(?m)^(System|Mic|Audio)(:|\s·)"#) else {
            return attributed
        }
        regex.enumerateMatches(in: text, range: fullRange) { match, _, _ in
            guard let match, match.numberOfRanges > 1 else { return }
            let labelRange = match.range(at: 1)
            attributed.addAttributes(
                [
                    .font: sourceFont,
                    .foregroundColor: sourceColor,
                ],
                range: labelRange)
        }
        return attributed
    }

    private func sourceMarkerColor(rightAligned: Bool) -> NSColor {
        rightAligned
            ? NSColor(red: 0.00, green: 0.42, blue: 0.20, alpha: 1.0)
            : BlueyTheme.green
    }

    private func chatBody(for card: RenderedCard, rawBody: String) -> String {
        let kind = normalizedCardKind(card.kind)
        if kind == "question", !card.attachments.isEmpty {
            return stripVisibleAttachmentFallback(from: rawBody)
        }
        guard kind == "answer" else { return rawBody }

        if let artifact = card.artifact {
            let base: String
            if artifact.artifactType == "code" {
                base = stripFencedCode(from: rawBody)
            } else {
                base = rawBody
            }
            return canvasBackedChatBody(
                from: base,
                fallback: artifactFallbackLine(for: artifact.artifactType),
                suffix: artifactChatSuffix(for: artifact.artifactType))
        }

        if rawBody.contains("```") {
            let notes = stripFencedCode(from: rawBody)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            if notes.isEmpty {
                return ""
            }
            return canvasBackedChatBody(
                from: notes,
                fallback: "",
                suffix: "")
        }

        return stripInlineCodeMarkers(rawBody)
    }

    private func stripVisibleAttachmentFallback(from text: String) -> String {
        let marker = "\n\nAttached to this answer:"
        if let range = text.range(of: marker) {
            return text[..<range.lowerBound].trimmingCharacters(in: .whitespacesAndNewlines)
        }
        return text
    }

    private func canvasBackedChatBody(from text: String, fallback: String, suffix: String) -> String {
        let body = stripInlineCodeMarkers(cleanedCanvasChatBody(from: text))
        if body.isEmpty {
            return fallback
        }
        return body
    }

    private func stripInlineCodeMarkers(_ text: String) -> String {
        text.replacingOccurrences(of: "`", with: "")
    }

    private func artifactFallbackLine(for artifactType: String) -> String {
        switch artifactType {
        case "code":
            return ""
        case "system_design":
            return ""
        case "screen":
            return ""
        case "document":
            return ""
        default:
            return ""
        }
    }

    private func artifactChatSuffix(for artifactType: String) -> String {
        artifactFallbackLine(for: artifactType)
    }

    private func cleanedCanvasChatBody(from text: String) -> String {
        stripFencedCode(from: text)
            .components(separatedBy: .newlines)
            .map { line -> String in
                let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
                if trimmed.hasPrefix("#") {
                    return trimmed.trimmingCharacters(in: CharacterSet(charactersIn: "# "))
                }
                if trimmed == "---" || trimmed == "----" {
                    return ""
                }
                return trimmed
            }
            .filter { !$0.isEmpty }
            .joined(separator: "\n")
            .trimmingCharacters(in: .whitespacesAndNewlines)
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

    private func shouldFollowIncomingContent() -> Bool {
        !userScrolledAwayFromLatest || isScrolledNearLatest()
    }

    private func scrollToBottomIfNeeded(_ shouldScroll: Bool) {
        guard shouldScroll else { return }
        scrollToBottom()
    }

    private func updateScrollPinAfterUserInput() {
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.userScrolledAwayFromLatest = !self.isScrolledNearLatest()
        }
    }

    private func isScrolledNearLatest() -> Bool {
        layoutSubtreeIfNeeded()
        stack.layoutSubtreeIfNeeded()
        scroll.layoutSubtreeIfNeeded()
        let maxOffset = max(0, stack.bounds.height - scroll.contentView.bounds.height)
        let latestY = stack.isFlipped ? maxOffset : 0
        return abs(scroll.contentView.bounds.origin.y - latestY) <= autoScrollTolerance
    }

    private func scrollToBottom() {
        DispatchQueue.main.async { [weak self] in
            guard let s = self else { return }
            let maxOffset = max(0, s.stack.bounds.height - s.scroll.contentView.bounds.height)
            let targetY = s.stack.isFlipped ? maxOffset : 0
            let bottom = NSPoint(x: 0, y: targetY)
            s.scroll.contentView.scroll(to: bottom)
            s.scroll.reflectScrolledClipView(s.scroll.contentView)
            s.userScrolledAwayFromLatest = false
        }
    }
}

// MARK: - Canvas pane

private final class CanvasPaneView: NSView {
    private let header = NSView()
    private let iconView = NSImageView()
    private let titleLabel = NSTextField(labelWithString: "Workspace")
    private let subtitleLabel = NSTextField(labelWithString: "Structured output appears here")
    private let previousButton = NSButton(title: "", target: nil, action: nil)
    private let nextButton = NSButton(title: "", target: nil, action: nil)
    private let positionLabel = NSTextField(labelWithString: "")
    private let fullWindowButton = NSButton(title: "", target: nil, action: nil)
    private let copyButton = NSButton(title: "", target: nil, action: nil)
    private let closeButton = NSButton(title: "", target: nil, action: nil)
    private let scroll = NSScrollView()
    private let textView = ArrowCursorTextView()
    private var currentText = ""
    private var fullWindow = false

    var onCollapse: (() -> Void)?
    var onToggleFullWindow: (() -> Void)?
    var onPrevious: (() -> Void)?
    var onNext: (() -> Void)?

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
        previousButton.translatesAutoresizingMaskIntoConstraints = false
        nextButton.translatesAutoresizingMaskIntoConstraints = false
        positionLabel.translatesAutoresizingMaskIntoConstraints = false
        fullWindowButton.translatesAutoresizingMaskIntoConstraints = false
        copyButton.translatesAutoresizingMaskIntoConstraints = false
        closeButton.translatesAutoresizingMaskIntoConstraints = false
        scroll.translatesAutoresizingMaskIntoConstraints = false

        addSubview(header)
        header.addSubview(iconView)
        header.addSubview(titleLabel)
        header.addSubview(subtitleLabel)
        header.addSubview(previousButton)
        header.addSubview(positionLabel)
        header.addSubview(nextButton)
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
        positionLabel.font = NSFont.monospacedDigitSystemFont(ofSize: 10.5, weight: .semibold)
        positionLabel.textColor = BlueyTheme.textDim
        positionLabel.alignment = .center
        positionLabel.lineBreakMode = .byClipping

        styleCanvasHeaderButton(previousButton, symbol: "chevron.left", fallback: "<")
        previousButton.toolTip = "Previous canvas"
        previousButton.target = self
        previousButton.action = #selector(previousClicked)

        styleCanvasHeaderButton(nextButton, symbol: "chevron.right", fallback: ">")
        nextButton.toolTip = "Next canvas"
        nextButton.target = self
        nextButton.action = #selector(nextClicked)

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
        textView.isHorizontallyResizable = false
        textView.isVerticallyResizable = true
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = true
        textView.textContainer?.containerSize = NSSize(
            width: 1,
            height: CGFloat.greatestFiniteMagnitude)

        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = false
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

            nextButton.trailingAnchor.constraint(equalTo: fullWindowButton.leadingAnchor, constant: -6),
            nextButton.topAnchor.constraint(equalTo: header.topAnchor),
            nextButton.widthAnchor.constraint(equalToConstant: 26),
            nextButton.heightAnchor.constraint(equalToConstant: 26),

            positionLabel.trailingAnchor.constraint(equalTo: nextButton.leadingAnchor, constant: -5),
            positionLabel.centerYAnchor.constraint(equalTo: nextButton.centerYAnchor),
            positionLabel.widthAnchor.constraint(equalToConstant: 34),

            previousButton.trailingAnchor.constraint(equalTo: positionLabel.leadingAnchor, constant: -5),
            previousButton.topAnchor.constraint(equalTo: header.topAnchor),
            previousButton.widthAnchor.constraint(equalToConstant: 26),
            previousButton.heightAnchor.constraint(equalToConstant: 26),

            titleLabel.leadingAnchor.constraint(equalTo: iconView.trailingAnchor, constant: 8),
            titleLabel.topAnchor.constraint(equalTo: header.topAnchor),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: previousButton.leadingAnchor, constant: -8),

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

    override func layout() {
        super.layout()
        updateTextWrapping()
    }

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
        textView.textStorage?.setAttributedString(attributedCanvasText(artifact.content))
        updateTextWrapping()
        textView.scrollRangeToVisible(NSRange(location: 0, length: 0))
    }

    private func attributedCanvasText(_ text: String) -> NSAttributedString {
        let baseFont = NSFont.monospacedSystemFont(ofSize: 12.2, weight: .regular)
        let headerFont = NSFont.monospacedSystemFont(ofSize: 12.2, weight: .semibold)
        let output = NSMutableAttributedString(
            string: text,
            attributes: [
                .font: baseFont,
                .foregroundColor: BlueyTheme.text,
            ])
        var location = 0
        var inChangedSection = false

        for rawLine in text.components(separatedBy: "\n") {
            let lineLength = (rawLine as NSString).length
            let range = NSRange(location: location, length: lineLength)
            let trimmed = rawLine.trimmingCharacters(in: .whitespacesAndNewlines)
            let header = trimmed.uppercased()
            let isDivider = !trimmed.isEmpty && trimmed.allSatisfy { $0 == "-" || $0 == "=" }

            if let role = canvasHeaderRole(header) {
                inChangedSection = role == "change"
                output.addAttributes(
                    [
                        .font: headerFont,
                        .foregroundColor: role == "change" ? BlueyTheme.green : BlueyTheme.cyan,
                    ],
                    range: range)
            } else if inChangedSection, !isDivider, range.length > 0 {
                output.addAttribute(
                    .backgroundColor,
                    value: BlueyTheme.green.withAlphaComponent(0.08),
                    range: range)
                if trimmed.hasPrefix("+") {
                    output.addAttribute(.foregroundColor, value: BlueyTheme.green, range: range)
                } else if trimmed.hasPrefix("-") {
                    output.addAttribute(.foregroundColor, value: BlueyTheme.danger, range: range)
                } else if trimmed.hasPrefix("@@") || trimmed.hasPrefix("diff --git") {
                    output.addAttribute(.foregroundColor, value: BlueyTheme.cyan, range: range)
                }
            } else if isDivider, range.length > 0 {
                output.addAttribute(
                    .foregroundColor,
                    value: BlueyTheme.textDim.withAlphaComponent(0.65),
                    range: range)
            }

            location += lineLength + 1
        }

        return output
    }

    private func canvasHeaderRole(_ header: String) -> String? {
        if header == "PATCH"
            || header.hasPrefix("PATCH ")
            || header == "DIFF"
            || header.hasPrefix("DIFF ")
            || header == "CHANGED BLOCK"
            || header.hasPrefix("CHANGED BLOCK ")
            || header == "CHANGED LINES"
            || header.hasPrefix("CHANGED LINES ")
            || header == "FOLLOW-UP"
            || header.hasPrefix("FOLLOW-UP ")
        {
            return "change"
        }
        if [
            "CODE",
            "COMPLEXITY",
            "TIME",
            "SPACE",
            "NOTES",
            "EXPLANATION",
            "APPROACH",
        ].contains(header) {
            return "section"
        }
        return nil
    }

    func setNavigation(index: Int, total: Int) {
        let hasMultiple = total > 1
        previousButton.isHidden = !hasMultiple
        nextButton.isHidden = !hasMultiple
        positionLabel.isHidden = !hasMultiple
        previousButton.isEnabled = hasMultiple && index > 0
        nextButton.isEnabled = hasMultiple && index < total - 1
        positionLabel.stringValue = hasMultiple ? "\(index + 1)/\(total)" : ""
        previousButton.contentTintColor = previousButton.isEnabled ? BlueyTheme.textDim : BlueyTheme.textDim.withAlphaComponent(0.35)
        nextButton.contentTintColor = nextButton.isEnabled ? BlueyTheme.textDim : BlueyTheme.textDim.withAlphaComponent(0.35)
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

    private func updateTextWrapping() {
        let width = max(1, scroll.contentSize.width)
        textView.textContainer?.containerSize = NSSize(
            width: width,
            height: CGFloat.greatestFiniteMagnitude)
    }

    @objc private func copyClicked() {
        let text = currentText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        flashCanvasCopySuccess()
    }

    private func flashCanvasCopySuccess() {
        let originalImage = copyButton.image
        let originalTitle = copyButton.title
        let originalTint = copyButton.contentTintColor
        if let image = symbolImage("checkmark") {
            image.isTemplate = true
            copyButton.image = image
            copyButton.imagePosition = .imageOnly
            copyButton.title = ""
        } else {
            copyButton.image = nil
            copyButton.title = "✓"
        }
        copyButton.contentTintColor = BlueyTheme.green
        copyButton.toolTip = "Copied"
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.1) { [weak self] in
            guard let self else { return }
            self.copyButton.image = originalImage
            self.copyButton.title = originalTitle
            self.copyButton.contentTintColor = originalTint
            self.copyButton.toolTip = "Copy canvas"
        }
    }

    @objc private func fullWindowClicked() {
        onToggleFullWindow?()
    }

    @objc private func previousClicked() {
        onPrevious?()
    }

    @objc private func nextClicked() {
        onNext?()
    }

    @objc private func collapseClicked() {
        onCollapse?()
    }
}

// MARK: - Expanded panel (feed + composer)

private final class ExpandedPanelView: NSView, NSTextFieldDelegate {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    private enum AutoSendStopMode: Int, CaseIterable {
        case off = 0
        case mic = 1
        case system = 2
        case micAndSystem = 3

        var title: String {
            switch self {
            case .off: return "Don't auto-send"
            case .mic: return "Auto-send when mic stops"
            case .system: return "Auto-send when system stops"
            case .micAndSystem: return "Auto-send when mic or system stops"
            }
        }

        var menuTitle: String {
            switch self {
            case .off: return "Don't auto-send"
            case .mic: return "Auto-send when mic stops"
            case .system: return "Auto-send when system stops"
            case .micAndSystem: return "Auto-send when mic or system stops"
            }
        }

        var compactTitle: String {
            isEnabled ? "✓ Auto-send" : "Auto-send"
        }

        var tooltip: String {
            switch self {
            case .off:
                return "Stop only pauses listening"
            case .mic:
                return "When Stop is clicked, send only if mic captions are ready"
            case .system:
                return "When Stop is clicked, send only if system audio captions are ready"
            case .micAndSystem:
                return "When Stop is clicked, send if mic or system captions are ready"
            }
        }

        var isEnabled: Bool { self != .off }
    }

    private enum ChromeMetrics {
        static let headerGuardHeight: CGFloat = 64
        static let headerBarHeight: CGFloat = 42
        static let headerHorizontalInset: CGFloat = 10
        static let headerTopInset: CGFloat = 14
        static let composerInputHeight: CGFloat = 26
        static let composerBaseHeight: CGFloat = 64
        static let composerExtraChromeHeight: CGFloat = 38
        static let transcriptStripHeight: CGFloat = 20
        static let transcriptRailDisplayChars: Int = 520
        static let transcriptPreviewMemoryChars: Int = 1_400
    }

    let feed: FeedView
    let workspace: NSView
    let canvasPane: CanvasPaneView
    let toastView: NSView
    let toastTitleLabel: NSTextField
    let toastBodyLabel: NSTextField
    let headerBar: HeaderDragView
    let headerChrome: HeaderShieldView
    let headerStack: NSStackView
    let brandStack: NSStackView
    let headerLogo: BlueyLogoView
    let headerWordmark: BlueyWordmarkView
    let headerSpacer: NSView
    let statusLabel: NSTextField
    let modelMenu: NSPopUpButton
    let routeBadge: NSTextField
    let knowledgeBadge: ClickableHeaderBadge
    let themeButton: NSButton
    let balanceLabel: NSTextField
    let fullSizeButton: NSButton
    let interactionModeButton: NSButton
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
    let transcriptClearButton: NSButton
    let attachmentStrip: NSScrollView
    let attachmentStack: NSStackView
    let composerBar: NSView
    let composerSurface: NSView
    let composerScroll: NSScrollView
    let composer: ComposerTextView
    let recordingButton: NSButton
    let askButton: NSButton
    let autoSendModeMenu: NSPopUpButton
    let analyzeButton: NSButton
    let attachButton: NSButton
    let instructionsButton: NSButton
    let opacityControl: OpacityScrubberView
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
    var onWindowFrameChanged: ((NSRect) -> Void)?
    var onInteractionModeChanged: (() -> Void)?
    private var recordingActive = false
    private var recordingDesiredActive = false
    private var recordingTransitionInFlight = false
    private var lastRecordingToggleAt = Date.distantPast
    private var transcriptSnippets: [String] = []
    private var latestLiveTranscriptLine: String?
    private var latestLiveTranscriptLinesBySource: [String: String] = [:]
    private var autoSendListenCaptureActive = false
    private var autoSendTranscriptLinesBySource: [String: String] = [:]
    private var liveTranscriptPreviewBodies: [String: String] = [:]
    private var consumedTranscriptFingerprints: [String] = []
    private var lastTranscriptStripSource: String?
    private var transcriptStripShouldFollowTail = false
    private var sessionItems: [OverlaySessionItem] = []
    private var sessionsHaveLoaded = false
    private var editingSessionId: String?
    private var pendingDeleteSessionId: String?
    private var renameField: NSTextField?
    private var canvasWidthConstraint: NSLayoutConstraint?
    private var composerBarHeightConstraint: NSLayoutConstraint?
    private var composerTextHeightConstraint: NSLayoutConstraint?
    private var composerDocumentHeightConstraint: NSLayoutConstraint?
    private var composerMeasuredHeight: CGFloat = ChromeMetrics.composerInputHeight
    private var attachmentStripHeightConstraint: NSLayoutConstraint?
    private var sessionDrawerTopConstraint: NSLayoutConstraint?
    private var sessionDrawerLeadingConstraint: NSLayoutConstraint?
    private var sessionDrawerWidthConstraint: NSLayoutConstraint?
    private var sessionDrawerHeightConstraint: NSLayoutConstraint?
    private var toastHideWorkItem: DispatchWorkItem?
    private var knowledgeIndexTimer: Timer?
    private var knowledgeIndexSafetyWorkItem: DispatchWorkItem?
    private var knowledgeIndexFrame = 0
    private var knowledgeBadgeContentVisible = false
    private var contextItems: [OverlayContextItem] = []
    private var pendingContextItemIds: Set<String> = []
    private var showingSavedContextItems = false
    private var contextMutationExpectedUntil: Date?
    private var hasVisibleContextAttachments = false
    private var screenContextReadyForAnswer = false
    private var autoSendStopMode: AutoSendStopMode = {
        let version = UserDefaults.standard.integer(forKey: overlayAutoSendStopModeVersionDefaultsKey)
        guard version >= overlayAutoSendStopModeCurrentVersion else {
            return .off
        }
        guard let stored = UserDefaults.standard.object(forKey: overlayAutoSendStopModeDefaultsKey) as? Int,
              let mode = AutoSendStopMode(rawValue: stored) else {
            return .off
        }
        return mode
    }()
    private var autoSendAfterStopWorkItem: DispatchWorkItem?
    private var lastSubmittedAskFingerprint: String?
    private var lastSubmittedAskAt: CFTimeInterval = 0
    private var answerStreamStats: [String: AnswerStreamStats] = [:]
    private var audioPulseTimer: Timer?
    private var audioPulseFrame = 0
    private var attachPickerPending = false
    private var attachPickerResetWorkItem: DispatchWorkItem?
    private let knowledgeIndexFrames = [
        "Indexing · ● 101",
        "Indexing · ● 010",
        "Indexing · ● 111",
        "Indexing · ● 001",
    ]
    private var canvases: [CanvasArtifact] = []
    private var activeCanvasIndex: Int?
    private var canvasCardAssignments: [String: Int] = [:]
    private var canvasOpen = false
    private var canvasFullWindow = false
    private var lastSessionToggleAt: TimeInterval = 0
    private var lastCanvasToggleAt: TimeInterval = 0
    private var suppressCanvasFullWindowUntil: TimeInterval = 0
    private var preCanvasFullWindowFrame: NSRect?
    private var windowFullSize = false
    private var preWindowFullSizeFrame: NSRect?
    private var passThroughMode = true
    private var headerDragInProgress = false
    private struct ResizeEdges: OptionSet {
        let rawValue: Int
        static let left = ResizeEdges(rawValue: 1 << 0)
        static let right = ResizeEdges(rawValue: 1 << 1)
        static let top = ResizeEdges(rawValue: 1 << 2)
        static let bottom = ResizeEdges(rawValue: 1 << 3)
    }
    private static let resizeNorthwestSoutheastCursor = makeDiagonalResizeCursor(northwestSoutheast: true)
    private static let resizeNortheastSouthwestCursor = makeDiagonalResizeCursor(northwestSoutheast: false)
    private var activeResizeEdges: ResizeEdges = []
    private var resizeStartMouse = NSPoint.zero
    private var resizeStartFrame = NSRect.zero
    private var resizeCursorActive = false
    private var composerInputArmedUntil: TimeInterval = 0
    private let resizeHitSize: CGFloat = 14
    private var backgroundOpacity: CGFloat = 0.94
    private var dropHighlightActive = false
    private var lightThemeEnabled = UserDefaults.standard.bool(forKey: overlayLightThemeDefaultsKey)

    override init(frame frameRect: NSRect) {
        feed = FeedView(frame: .zero)
        workspace = NSView()
        canvasPane = CanvasPaneView(frame: .zero)
        toastView = NSView()
        toastTitleLabel = NSTextField(labelWithString: "")
        toastBodyLabel = NSTextField(wrappingLabelWithString: "")
        headerBar = HeaderDragView()
        headerChrome = HeaderShieldView()
        headerStack = NSStackView()
        brandStack = NSStackView()
        headerLogo = BlueyLogoView()
        headerWordmark = BlueyWordmarkView()
        headerSpacer = NSView()
        statusLabel = NSTextField(labelWithString: "")
        modelMenu = NSPopUpButton(frame: .zero, pullsDown: false)
        routeBadge = NSTextField(labelWithString: "● Ready")
        knowledgeBadge = ClickableHeaderBadge(labelWithString: "")
        themeButton = NSButton(title: "", target: nil, action: nil)
        balanceLabel = NSTextField(labelWithString: "Balance --")
        fullSizeButton = NSButton(title: "", target: nil, action: nil)
        interactionModeButton = NSButton(title: "", target: nil, action: nil)
        canvasToggleButton = NSButton(title: "", target: nil, action: nil)
        navButton = NSButton(title: "", target: nil, action: nil)
        newSessionButton = NSButton(title: "", target: nil, action: nil)
        sessionDrawer = SessionDrawerView()
        drawerTitleLabel = NSTextField(labelWithString: "History")
        drawerSubtitleLabel = NSTextField(labelWithString: "Local recordings on this device.")
        drawerCloseButton = NSButton(title: "", target: nil, action: nil)
        latestSessionButton = NSButton(title: "Continue latest", target: nil, action: nil)
        sessionScroll = NSScrollView()
        sessionStack = FlippedStackView()
        answerStyleOverlay = ModalBlockerView()
        answerStylePanel = NSView()
        answerStyleLabel = NSTextField(labelWithString: "How Bluey should answer")
        answerStyleBox = ArrowCursorTextField()
        answerStyleSaveButton = NSButton(title: "Save", target: nil, action: nil)
        transcriptStrip = NSView()
        transcriptActivityDot = NSView()
        transcriptStateLabel = NSTextField(labelWithString: "IDLE")
        transcriptScroll = NSScrollView()
        transcriptLabel = NSTextField(labelWithString: "Live captions preview")
        transcriptClearButton = NSButton(title: "", target: nil, action: nil)
        attachmentStrip = NSScrollView()
        attachmentStack = NSStackView()
        composerBar = NSView()
        composerSurface = ComposerSurfaceView()
        composerScroll = NSScrollView()
        composer = ComposerTextView(frame: .zero, textContainer: nil)
        recordingButton = NSButton(title: "Listen", target: nil, action: nil)
        askButton = NSButton(title: "Answer", target: nil, action: nil)
        autoSendModeMenu = NSPopUpButton(frame: .zero, pullsDown: true)
        analyzeButton = NSButton(title: "Screen", target: nil, action: nil)
        attachButton = NSButton(title: "", target: nil, action: nil)
        instructionsButton = NSButton(title: "Tone", target: nil, action: nil)
        opacityControl = OpacityScrubberView()
        opacityLabel = NSTextField(labelWithString: "Opacity")
        opacitySlider = NSSlider(value: 0.94, minValue: Double(minimumOverlayBackgroundOpacity), maxValue: 1.0, target: nil, action: nil)
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

        (sessionDrawer as? SessionDrawerView)?.scrollView = sessionScroll

        migrateAutoSendStopModeDefaultsIfNeeded()
        registerForDraggedTypes([.fileURL])

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
        feed.onOpenCanvasForCard = { [weak self] cardId in
            self?.openCanvasForCardId(cardId)
        }

        for view in [
            headerBar,
            headerChrome,
            headerStack,
            brandStack,
            headerLogo,
            headerWordmark,
            headerSpacer,
            statusLabel,
            modelMenu,
            routeBadge,
            knowledgeBadge,
            themeButton,
            balanceLabel,
            fullSizeButton,
            interactionModeButton,
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
            transcriptClearButton,
            attachmentStrip,
            attachmentStack,
            composerBar,
            composerSurface,
            composerScroll,
            composer,
            recordingButton,
            askButton,
            autoSendModeMenu,
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
        // The header bar owns its controls. Earlier builds kept the controls
        // as root-level siblings positioned over a constrained header, which
        // made z-order and hit testing fragile after resize/fullscreen passes.
        headerBar.translatesAutoresizingMaskIntoConstraints = false
        headerChrome.translatesAutoresizingMaskIntoConstraints = false
        headerStack.translatesAutoresizingMaskIntoConstraints = true
        headerStack.isHidden = true
        // These rows are manually pinned in keepFixedChromeInBounds(). Keeping
        // Auto Layout in charge of the same frames caused visible chrome and
        // mouse hit regions to drift after fullscreen/restore passes.
        for manualFrameView in [
            headerChrome,
            headerBar,
            workspace,
            transcriptStrip,
            attachmentStrip,
            composerBar,
        ] {
            manualFrameView.translatesAutoresizingMaskIntoConstraints = true
        }

        brandStack.addArrangedSubview(headerWordmark)
        brandStack.addArrangedSubview(statusLabel)
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
        transcriptStrip.addSubview(transcriptClearButton)
        transcriptScroll.documentView = transcriptLabel
        transcriptLabel.translatesAutoresizingMaskIntoConstraints = true
        addSubview(attachmentStrip)
        addSubview(composerBar)
        composerBar.addSubview(composerSurface)
        composerSurface.addSubview(composerScroll)
        composerScroll.documentView = composer
        composerSurface.addSubview(recordingButton)
        composerSurface.addSubview(askButton)
        composerBar.addSubview(attachButton)
        composerBar.addSubview(instructionsButton)
        composerBar.addSubview(opacityControl)
        opacityControl.addSubview(opacityLabel)
        opacityControl.addSubview(opacitySlider)
        opacityControl.addSubview(opacityValueLabel)
        composerBar.addSubview(modelMenu)
        composerBar.addSubview(autoSendModeMenu)
        composerBar.addSubview(analyzeButton)
        // Add a non-content top shield below the interactive header. It masks
        // any scroll-view overshoot during resize/restore while keeping the
        // actual Bluey header controls fully clickable.
        addSubview(headerChrome)
        addSubview(headerBar)
        for view in [
            navButton,
            newSessionButton,
            headerLogo,
            brandStack,
            routeBadge,
            knowledgeBadge,
            canvasToggleButton,
            themeButton,
            balanceLabel,
            fullSizeButton,
            interactionModeButton,
            hideButton,
            closeButton,
        ] {
            headerBar.addSubview(view)
            view.translatesAutoresizingMaskIntoConstraints = true
        }
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
        // even when AppKit re-lays out the dense center workspace. The shield
        // sits below the real header and never receives mouse events.
        headerChrome.layer?.zPosition = 44
        headerBar.layer?.zPosition = 45
        transcriptStrip.layer?.zPosition = 40
        attachmentStrip.layer?.zPosition = 40
        composerBar.layer?.zPosition = 50
        toastView.layer?.zPosition = 70

        let canvasWidth = canvasPane.widthAnchor.constraint(equalToConstant: 0)
        canvasWidthConstraint = canvasWidth
        let composerTextHeight = composerSurface.heightAnchor.constraint(equalToConstant: ChromeMetrics.composerInputHeight)
        let composerDocumentHeight = composer.heightAnchor.constraint(equalToConstant: ChromeMetrics.composerInputHeight)
        let composerBarHeight = composerBar.heightAnchor.constraint(equalToConstant: ChromeMetrics.composerBaseHeight)
        let attachmentStripHeight = attachmentStrip.heightAnchor.constraint(equalToConstant: 0)
        let sessionDrawerTop = sessionDrawer.topAnchor.constraint(equalTo: topAnchor, constant: 72)
        let sessionDrawerLeading = sessionDrawer.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 18)
        let sessionDrawerWidth = sessionDrawer.widthAnchor.constraint(equalToConstant: 360)
        let sessionDrawerHeight = sessionDrawer.heightAnchor.constraint(equalToConstant: 420)
        composerTextHeightConstraint = composerTextHeight
        composerDocumentHeightConstraint = composerDocumentHeight
        composerBarHeightConstraint = composerBarHeight
        attachmentStripHeightConstraint = attachmentStripHeight
        sessionDrawerTopConstraint = sessionDrawerTop
        sessionDrawerLeadingConstraint = sessionDrawerLeading
        sessionDrawerWidthConstraint = sessionDrawerWidth
        sessionDrawerHeightConstraint = sessionDrawerHeight

        NSLayoutConstraint.activate([
            headerWordmark.widthAnchor.constraint(equalToConstant: 62),
            headerWordmark.heightAnchor.constraint(equalToConstant: 22),

            feed.topAnchor.constraint(equalTo: workspace.topAnchor),
            feed.leadingAnchor.constraint(equalTo: workspace.leadingAnchor),
            feed.bottomAnchor.constraint(equalTo: workspace.bottomAnchor),
            feed.widthAnchor.constraint(greaterThanOrEqualToConstant: 260),

            canvasPane.topAnchor.constraint(equalTo: workspace.topAnchor),
            canvasPane.leadingAnchor.constraint(equalTo: feed.trailingAnchor, constant: 8),
            canvasPane.trailingAnchor.constraint(equalTo: workspace.trailingAnchor),
            canvasPane.bottomAnchor.constraint(equalTo: workspace.bottomAnchor),
            canvasWidth,

            toastView.topAnchor.constraint(equalTo: workspace.topAnchor, constant: 18),
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

            sessionDrawerTop,
            sessionDrawerLeading,
            sessionDrawerWidth,
            sessionDrawerHeight,
            sessionDrawer.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -18),
            sessionDrawer.bottomAnchor.constraint(lessThanOrEqualTo: bottomAnchor, constant: -104),

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
            sessionStack.bottomAnchor.constraint(greaterThanOrEqualTo: sessionScroll.contentView.bottomAnchor),
            sessionStack.widthAnchor.constraint(equalTo: sessionScroll.contentView.widthAnchor),

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

            transcriptActivityDot.leadingAnchor.constraint(equalTo: transcriptStrip.leadingAnchor, constant: 10),
            transcriptActivityDot.centerYAnchor.constraint(equalTo: transcriptStrip.centerYAnchor),
            transcriptActivityDot.widthAnchor.constraint(equalToConstant: 6),
            transcriptActivityDot.heightAnchor.constraint(equalToConstant: 6),

            transcriptStateLabel.leadingAnchor.constraint(equalTo: transcriptActivityDot.trailingAnchor, constant: 6),
            transcriptStateLabel.centerYAnchor.constraint(equalTo: transcriptStrip.centerYAnchor),
            transcriptStateLabel.widthAnchor.constraint(equalToConstant: 72),

            transcriptScroll.topAnchor.constraint(equalTo: transcriptStrip.topAnchor, constant: 2),
            transcriptScroll.leadingAnchor.constraint(equalTo: transcriptStateLabel.trailingAnchor, constant: 8),
            transcriptScroll.trailingAnchor.constraint(equalTo: transcriptClearButton.leadingAnchor, constant: -6),
            transcriptScroll.bottomAnchor.constraint(equalTo: transcriptStrip.bottomAnchor, constant: -2),

            transcriptClearButton.trailingAnchor.constraint(equalTo: transcriptStrip.trailingAnchor, constant: -6),
            transcriptClearButton.centerYAnchor.constraint(equalTo: transcriptStrip.centerYAnchor),
            transcriptClearButton.widthAnchor.constraint(equalToConstant: 18),
            transcriptClearButton.heightAnchor.constraint(equalToConstant: 18),

            attachmentStack.leadingAnchor.constraint(equalTo: attachmentStrip.contentView.leadingAnchor),
            attachmentStack.topAnchor.constraint(equalTo: attachmentStrip.contentView.topAnchor),
            attachmentStack.bottomAnchor.constraint(equalTo: attachmentStrip.contentView.bottomAnchor),
            attachmentStack.heightAnchor.constraint(equalTo: attachmentStrip.heightAnchor),

            composerSurface.topAnchor.constraint(equalTo: composerBar.topAnchor, constant: 6),
            composerSurface.leadingAnchor.constraint(equalTo: composerBar.leadingAnchor, constant: 8),
            composerSurface.trailingAnchor.constraint(equalTo: composerBar.trailingAnchor, constant: -8),
            composerTextHeight,

            composerScroll.topAnchor.constraint(equalTo: composerSurface.topAnchor, constant: 1),
            composerScroll.leadingAnchor.constraint(equalTo: composerSurface.leadingAnchor, constant: 9),
            composerScroll.trailingAnchor.constraint(equalTo: recordingButton.leadingAnchor, constant: -6),
            composerScroll.bottomAnchor.constraint(equalTo: composerSurface.bottomAnchor, constant: -1),

            composer.leadingAnchor.constraint(equalTo: composerScroll.contentView.leadingAnchor),
            composer.topAnchor.constraint(equalTo: composerScroll.contentView.topAnchor),
            composer.widthAnchor.constraint(equalTo: composerScroll.contentView.widthAnchor),
            composer.heightAnchor.constraint(greaterThanOrEqualTo: composerScroll.contentView.heightAnchor),
            composerDocumentHeight,

            recordingButton.trailingAnchor.constraint(equalTo: askButton.leadingAnchor, constant: -4),
            recordingButton.centerYAnchor.constraint(equalTo: composerSurface.centerYAnchor),
            recordingButton.widthAnchor.constraint(equalToConstant: 60),
            recordingButton.heightAnchor.constraint(equalToConstant: 22),

            askButton.trailingAnchor.constraint(equalTo: composerSurface.trailingAnchor, constant: -5),
            askButton.centerYAnchor.constraint(equalTo: composerSurface.centerYAnchor),
            askButton.widthAnchor.constraint(equalToConstant: 72),
            askButton.heightAnchor.constraint(equalToConstant: 24),

            attachButton.leadingAnchor.constraint(equalTo: composerBar.leadingAnchor, constant: 8),
            attachButton.bottomAnchor.constraint(equalTo: composerBar.bottomAnchor, constant: -6),
            attachButton.widthAnchor.constraint(equalToConstant: 22),
            attachButton.heightAnchor.constraint(equalToConstant: 22),

            instructionsButton.leadingAnchor.constraint(equalTo: attachButton.trailingAnchor, constant: 6),
            instructionsButton.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            instructionsButton.widthAnchor.constraint(equalToConstant: 58),
            instructionsButton.heightAnchor.constraint(equalToConstant: 22),

            opacityControl.leadingAnchor.constraint(equalTo: instructionsButton.trailingAnchor, constant: 6),
            opacityControl.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            opacityControl.widthAnchor.constraint(equalToConstant: 126),
            opacityControl.heightAnchor.constraint(equalToConstant: 22),

            opacityLabel.leadingAnchor.constraint(equalTo: opacityControl.leadingAnchor, constant: 7),
            opacityLabel.centerYAnchor.constraint(equalTo: opacityControl.centerYAnchor),
            opacityLabel.widthAnchor.constraint(equalToConstant: 38),

            opacitySlider.leadingAnchor.constraint(equalTo: opacityLabel.trailingAnchor, constant: 5),
            opacitySlider.centerYAnchor.constraint(equalTo: opacityControl.centerYAnchor),
            opacitySlider.trailingAnchor.constraint(equalTo: opacityValueLabel.leadingAnchor, constant: -5),
            opacitySlider.heightAnchor.constraint(equalToConstant: 14),

            opacityValueLabel.trailingAnchor.constraint(equalTo: opacityControl.trailingAnchor, constant: -6),
            opacityValueLabel.centerYAnchor.constraint(equalTo: opacityControl.centerYAnchor),
            opacityValueLabel.widthAnchor.constraint(equalToConstant: 24),

            analyzeButton.trailingAnchor.constraint(equalTo: composerBar.trailingAnchor, constant: -10),
            analyzeButton.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            analyzeButton.widthAnchor.constraint(equalToConstant: 64),
            analyzeButton.heightAnchor.constraint(equalToConstant: 22),

            modelMenu.trailingAnchor.constraint(equalTo: analyzeButton.leadingAnchor, constant: -6),
            modelMenu.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            modelMenu.widthAnchor.constraint(greaterThanOrEqualToConstant: 86),
            modelMenu.widthAnchor.constraint(lessThanOrEqualToConstant: 118),
            modelMenu.heightAnchor.constraint(equalToConstant: 22),

            autoSendModeMenu.leadingAnchor.constraint(equalTo: opacityControl.trailingAnchor, constant: 6),
            autoSendModeMenu.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            autoSendModeMenu.widthAnchor.constraint(equalToConstant: 118),
            autoSendModeMenu.heightAnchor.constraint(equalToConstant: 22),

            modelMenu.leadingAnchor.constraint(greaterThanOrEqualTo: autoSendModeMenu.trailingAnchor, constant: 6),

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
        interactionModeButton.target = self
        interactionModeButton.action = #selector(interactionModeClicked)
        themeButton.target = self
        themeButton.action = #selector(themeClicked)
        closeButton.target = self
        closeButton.action = #selector(closeClicked)
        closeConfirmCancelButton.target = self
        closeConfirmCancelButton.action = #selector(cancelCloseConfirmClicked)
        closeConfirmTurnOffButton.target = self
        closeConfirmTurnOffButton.action = #selector(confirmTurnOffClicked)
        opacitySlider.target = self
        opacitySlider.action = #selector(opacityChanged)
        opacityControl.onChange = { [weak self] value in
            self?.applyOpacity(value)
        }
        composer.onSubmit = { [weak self] in self?.askClicked() }
        composer.onMeasuredHeight = { [weak self] height in self?.setComposerTextHeight(height) }
        composer.onFocusChanged = { [weak self] focused in
            (self?.composerSurface as? ComposerSurfaceView)?.setInputFocused(focused)
        }
        (composerSurface as? ComposerSurfaceView)?.composer = composer
        headerBar.onDragStateChanged = { [weak self] active in
            self?.headerDragInProgress = active
        }
        headerBar.onWindowFrameChanged = { [weak self] frame in
            self?.onWindowFrameChanged?(frame)
        }
        recordingButton.target = self
        recordingButton.action = #selector(recordingClicked)
        askButton.target = self
        askButton.action = #selector(askClicked)
        askButton.keyEquivalent = "\r"
        askButton.keyEquivalentModifierMask = [.command]
        autoSendModeMenu.target = self
        autoSendModeMenu.action = #selector(autoSendModeChanged)
        analyzeButton.target = self
        analyzeButton.action = #selector(analyzeClicked)
        attachButton.target = self
        attachButton.action = #selector(attachClicked)
        instructionsButton.target = self
        instructionsButton.action = #selector(instructionsClicked)
        transcriptClearButton.target = self
        transcriptClearButton.action = #selector(clearTranscriptClicked)

        sessionDrawer.isHidden = true
        answerStyleOverlay.isHidden = true
        canvasPane.isHidden = true
        canvasToggleButton.isHidden = true
        canvasPane.onCollapse = { [weak self] in self?.setCanvasOpen(false) }
        canvasPane.onToggleFullWindow = { [weak self] in self?.toggleCanvasFullWindow() }
        canvasPane.onPrevious = { [weak self] in self?.showPreviousCanvas() }
        canvasPane.onNext = { [weak self] in self?.showNextCanvas() }
        canvasPane.setFullWindow(false)
        canvasPane.setNavigation(index: 0, total: 0)
        styleHeaderIconButton(navButton, symbol: "sidebar.left", fallback: "[]")
        styleHeaderIconButton(drawerCloseButton, symbol: "xmark", fallback: "x")
        styleHeaderIconButton(canvasToggleButton, symbol: "sidebar.right", fallback: "|")
        styleHeaderIconButton(newSessionButton, symbol: "square.and.pencil", fallback: "+")
        styleControlButton(latestSessionButton, symbol: "clock.arrow.circlepath", accent: false)
        styleControlButton(answerStyleSaveButton, symbol: "checkmark", accent: true)
        styleControlButton(recordingButton, symbol: "waveform", accent: false)
        styleControlButton(instructionsButton, symbol: "text.bubble", accent: false)
        styleIconButton(transcriptClearButton, symbol: "xmark", fallback: "x")
        styleIconButton(attachButton, symbol: "plus", fallback: "+")
        styleControlButton(analyzeButton, symbol: "sparkle.magnifyingglass", accent: false)
        styleControlButton(askButton, symbol: "arrow.up", accent: true)
        styleHeaderIconButton(hideButton, symbol: "eye.slash", fallback: "-")
        styleHeaderIconButton(closeButton, symbol: "xmark", fallback: "x")
        updateFullSizeButtonChrome()
        updateInteractionModeChrome(showToast: false)
        configureTooltips()
        updateTranscriptClearButtonVisibility()
        setContextItems([])
        setTranscriptState("READY", active: false)
        applyOpacity(opacitySlider.doubleValue)
    }
    required init?(coder: NSCoder) { fatalError() }

    deinit {
        audioPulseTimer?.invalidate()
        knowledgeIndexTimer?.invalidate()
        toastHideWorkItem?.cancel()
        autoSendAfterStopWorkItem?.cancel()
    }

    override func layout() {
        super.layout()
        applyShellChrome()
        keepFixedChromeInBounds()
        if canvasOpen {
            updateCanvasWidth()
        }
        layoutTranscriptRailForCurrentText()
    }

    override func resetCursorRects() {
        super.resetCursorRects()
        addCursorRect(bounds, cursor: .arrow)
        guard closeConfirmOverlay.isHidden, answerStyleOverlay.isHidden, !windowFullSize else { return }
        addCursorRect(NSRect(x: 0, y: 0, width: resizeHitSize, height: resizeHitSize), cursor: Self.resizeNortheastSouthwestCursor)
        addCursorRect(NSRect(x: bounds.width - resizeHitSize, y: bounds.height - resizeHitSize, width: resizeHitSize, height: resizeHitSize), cursor: Self.resizeNortheastSouthwestCursor)
        addCursorRect(NSRect(x: 0, y: bounds.height - resizeHitSize, width: resizeHitSize, height: resizeHitSize), cursor: Self.resizeNorthwestSoutheastCursor)
        addCursorRect(NSRect(x: bounds.width - resizeHitSize, y: 0, width: resizeHitSize, height: resizeHitSize), cursor: Self.resizeNorthwestSoutheastCursor)
        addCursorRect(NSRect(x: 0, y: resizeHitSize, width: resizeHitSize, height: max(0, bounds.height - (resizeHitSize * 2))), cursor: .resizeLeftRight)
        addCursorRect(NSRect(x: bounds.width - resizeHitSize, y: resizeHitSize, width: resizeHitSize, height: max(0, bounds.height - (resizeHitSize * 2))), cursor: .resizeLeftRight)
        addCursorRect(NSRect(x: resizeHitSize, y: bounds.height - resizeHitSize, width: max(0, bounds.width - (resizeHitSize * 2)), height: resizeHitSize), cursor: .resizeUpDown)
        addCursorRect(NSRect(x: resizeHitSize, y: 0, width: max(0, bounds.width - (resizeHitSize * 2)), height: resizeHitSize), cursor: .resizeUpDown)
    }

    private func applyShellChrome() {
        wantsLayer = true
        layer?.cornerRadius = ExpandedPanelMetrics.cornerRadius
        layer?.masksToBounds = true
        layer?.backgroundColor = NSColor.clear.cgColor
        layer?.borderWidth = 1
        layer?.borderColor = (lightThemeEnabled
            ? BlueyLightTheme.accentBorder.withAlphaComponent(0.70)
            : BlueyTheme.cyan.withAlphaComponent(materialAlpha(0.24, floor: 0.06))).cgColor
        layer?.shadowColor = NSColor.black.cgColor
        layer?.shadowOpacity = lightThemeEnabled ? Float(lightMaterialAlpha(0.20, floor: 0.06)) : Float(materialAlpha(0.30, floor: 0.10))
        layer?.shadowRadius = lightThemeEnabled ? 22 : 28
        layer?.shadowOffset = .zero
        if #available(macOS 10.15, *) {
            layer?.cornerCurve = .continuous
        }
    }

    private func materialAlpha(_ base: CGFloat, floor: CGFloat = 0.0) -> CGFloat {
        blueyMaterialAlpha(base, opacity: backgroundOpacity, floor: floor)
    }

    private func lightMaterialAlpha(_ base: CGFloat, floor: CGFloat = 0.24) -> CGFloat {
        blueyLightMaterialAlpha(base, opacity: backgroundOpacity, floor: floor)
    }

    private var themedTextColor: NSColor {
        lightThemeEnabled
            ? BlueyLightTheme.text
            : BlueyTheme.text
    }

    private var themedDimTextColor: NSColor {
        lightThemeEnabled
            ? BlueyLightTheme.textDim
            : BlueyTheme.textDim
    }

    private var themedHeaderColor: NSColor {
        lightThemeEnabled
            ? BlueyLightTheme.bar.withAlphaComponent(lightMaterialAlpha(0.92, floor: 0.18))
            : NSColor(red: 0.018, green: 0.022, blue: 0.030, alpha: materialAlpha(0.58, floor: 0.20))
    }

    private var themedHeaderChromeColor: NSColor {
        lightThemeEnabled
            ? BlueyLightTheme.barChrome.withAlphaComponent(lightMaterialAlpha(0.88, floor: 0.20))
            : NSColor(red: 0.008, green: 0.011, blue: 0.016, alpha: materialAlpha(0.46, floor: 0.16))
    }

    private var themedComposerColor: NSColor {
        lightThemeEnabled
            ? BlueyLightTheme.bar.withAlphaComponent(lightMaterialAlpha(0.92, floor: 0.18))
            : NSColor(red: 0.014, green: 0.016, blue: 0.022, alpha: materialAlpha(0.94))
    }

    private var themedSurfaceColor: NSColor {
        lightThemeEnabled
            ? BlueyLightTheme.surfaceRaised.withAlphaComponent(lightMaterialAlpha(0.88, floor: 0.18))
            : NSColor.white.withAlphaComponent(materialAlpha(0.045))
    }

    private var themedAccentColor: NSColor {
        lightThemeEnabled ? BlueyLightTheme.accent : BlueyTheme.cyan
    }

    private var themedAccentBorderColor: NSColor {
        lightThemeEnabled ? BlueyLightTheme.accentBorder : BlueyTheme.cyan
    }

    private var themedAccentFillColor: NSColor {
        lightThemeEnabled ? BlueyLightTheme.accentSoft : BlueyTheme.cyan
    }

    private func refreshBackgroundChrome() {
        layer?.backgroundColor = NSColor.clear.cgColor
        layer?.borderColor = (lightThemeEnabled
            ? BlueyLightTheme.accentBorder.withAlphaComponent(0.70)
            : BlueyTheme.cyan.withAlphaComponent(materialAlpha(0.26, floor: 0.06))).cgColor
        layer?.shadowOpacity = lightThemeEnabled ? Float(lightMaterialAlpha(0.20, floor: 0.06)) : Float(materialAlpha(0.30, floor: 0.10))
        headerBar.layer?.backgroundColor = themedHeaderColor.cgColor
        headerBar.layer?.borderColor = (lightThemeEnabled
            ? BlueyLightTheme.accentBorder.withAlphaComponent(0.62)
            : BlueyTheme.cyan.withAlphaComponent(0.18)).cgColor
        headerBar.layer?.shadowOpacity = lightThemeEnabled ? 0.12 : 0.18
        headerChrome.layer?.backgroundColor = themedHeaderChromeColor.cgColor
        modelMenu.layer?.backgroundColor = themedSurfaceColor.cgColor
        modelMenu.layer?.borderColor = (lightThemeEnabled
            ? BlueyLightTheme.border
            : NSColor.white.withAlphaComponent(materialAlpha(0.12))).cgColor
        modelMenu.contentTintColor = themedTextColor
        updateAutoSendModeChrome()
        refreshControlChromeForTheme()
        updateThemeButtonChrome()
        statusLabel.textColor = themedDimTextColor
        balanceLabel.textColor = themedTextColor
        transcriptStateLabel.textColor = transcriptStateLabel.stringValue == "LIVE" ? BlueyTheme.green : themedDimTextColor
        transcriptLabel.textColor = themedTextColor
        opacityLabel.textColor = themedDimTextColor
        opacityValueLabel.textColor = themedDimTextColor
        composer.textColor = themedTextColor
        composer.insertionPointColor = themedAccentColor
        composer.placeholderColor = themedDimTextColor.withAlphaComponent(lightThemeEnabled ? 0.82 : 0.78)
        composer.typingAttributes = [
            .font: composer.font ?? NSFont.systemFont(ofSize: 13.5, weight: .semibold),
            .foregroundColor: themedTextColor,
        ]
        balanceLabel.layer?.backgroundColor = NSColor.clear.cgColor
        balanceLabel.layer?.borderColor = NSColor.clear.cgColor
        toastView.layer?.backgroundColor = NSColor(
            red: lightThemeEnabled ? 0.805 : 0.018,
            green: lightThemeEnabled ? 0.842 : 0.023,
            blue: lightThemeEnabled ? 0.862 : 0.030,
            alpha: lightThemeEnabled ? lightMaterialAlpha(0.88, floor: 0.18) : materialAlpha(0.96)
        ).cgColor
        toastView.layer?.borderColor = (lightThemeEnabled
            ? BlueyLightTheme.accentBorder.withAlphaComponent(0.60)
            : BlueyTheme.cyan.withAlphaComponent(0.28)).cgColor
        toastTitleLabel.textColor = themedTextColor
        toastBodyLabel.textColor = themedDimTextColor
        transcriptStrip.layer?.backgroundColor = (lightThemeEnabled
            ? BlueyLightTheme.surface.withAlphaComponent(lightMaterialAlpha(0.80, floor: 0.16))
            : NSColor.black.withAlphaComponent(materialAlpha(0.16))).cgColor
        transcriptStrip.layer?.borderColor = (lightThemeEnabled
            ? BlueyLightTheme.border
            : BlueyTheme.hairline).cgColor
        transcriptLabel.attributedStringValue = attributedTranscriptStripText(transcriptLabel.stringValue)
        sessionDrawer.layer?.backgroundColor = NSColor(
            red: lightThemeEnabled ? 0.750 : 0.035,
            green: lightThemeEnabled ? 0.790 : 0.040,
            blue: lightThemeEnabled ? 0.812 : 0.050,
            alpha: lightThemeEnabled ? lightMaterialAlpha(0.88, floor: 0.18) : materialAlpha(0.98)
        ).cgColor
        sessionDrawer.layer?.borderColor = (lightThemeEnabled
            ? BlueyLightTheme.border
            : BlueyTheme.hairline).cgColor
        drawerTitleLabel.textColor = themedTextColor
        drawerSubtitleLabel.textColor = themedDimTextColor
        answerStylePanel.layer?.backgroundColor = BlueyTheme.panelDeep
            .withAlphaComponent(materialAlpha(0.97))
            .cgColor
        composerBar.layer?.backgroundColor = themedComposerColor.cgColor
        composerBar.layer?.borderColor = (lightThemeEnabled
            ? BlueyLightTheme.accentBorder.withAlphaComponent(0.68)
            : BlueyTheme.cyan.withAlphaComponent(0.22)).cgColor
        composerBar.layer?.shadowOpacity = lightThemeEnabled ? 0.10 : 0.18
        composerSurface.layer?.backgroundColor = themedSurfaceColor.cgColor
        let composerSurfaceBorder = lightThemeEnabled
            ? BlueyLightTheme.border
            : NSColor.white.withAlphaComponent(materialAlpha(0.105))
        composerSurface.layer?.borderColor = composerSurfaceBorder.cgColor
        (composerSurface as? ComposerSurfaceView)?.updateBorderColors(
            resting: composerSurfaceBorder,
            focused: themedAccentBorderColor.withAlphaComponent(lightThemeEnabled ? 0.82 : 0.68))
        opacityControl.layer?.backgroundColor = NSColor.clear.cgColor
        opacityControl.layer?.borderColor = NSColor.clear.cgColor
        closeConfirmPanel.layer?.backgroundColor = BlueyTheme.panelDeep
            .withAlphaComponent(materialAlpha(0.97))
            .cgColor
        feed.setLightTheme(lightThemeEnabled, opacity: backgroundOpacity)
        canvasPane.applyBackgroundOpacity(backgroundOpacity)
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        let outer = bounds.insetBy(dx: 0.75, dy: 0.75)
        let radius = windowFullSize ? 0 : ExpandedPanelMetrics.cornerRadius
        if lightThemeEnabled {
            drawBlueyLightPanel(in: outer, radius: radius, opacity: backgroundOpacity)
        } else {
            drawBlueyGlassPanel(
                in: outer,
                radius: radius,
                opacity: backgroundOpacity)
        }
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53, dismissActiveOverlay() {
            return
        }
        super.keyDown(with: event)
    }

    override func scrollWheel(with event: NSEvent) {
        let localPoint = convert(event.locationInWindow, from: nil)
        if !sessionDrawer.isHidden, rectForView(sessionDrawer).contains(localPoint) {
            sessionScroll.scrollWheel(with: event)
            return
        }
        if rectForView(transcriptScroll).contains(localPoint) {
            transcriptScroll.scrollWheel(with: event)
            return
        }
        if rectForView(composerSurface).contains(localPoint) {
            composerScroll.scrollWheel(with: event)
            return
        }
        if passThroughMode {
            if rectForView(feed).contains(localPoint) {
                feed.forwardScrollWheel(event)
                return
            }
            if rectForView(canvasPane).contains(localPoint) {
                canvasPane.scrollWheel(with: event)
                return
            }
            super.scrollWheel(with: event)
            return
        }
        if rectForView(feed).contains(localPoint) {
            feed.forwardScrollWheel(event)
            return
        }
        if rectForView(canvasPane).contains(localPoint) {
            canvasPane.scrollWheel(with: event)
            return
        }
        super.scrollWheel(with: event)
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard !isHidden, alphaValue > 0.01, bounds.contains(point) else {
            return nil
        }
        if !closeConfirmOverlay.isHidden || !answerStyleOverlay.isHidden {
            return super.hitTest(point) ?? self
        }
        if headerDragInProgress {
            return super.hitTest(point) ?? self
        }
        if isKnowledgeBadgeHit(at: point) {
            return self
        }
        if passThroughMode {
            if let hit = super.hitTest(point), isExplicitInteractiveHit(hit) {
                return hit
            }
            if hasManualInteractiveControl(at: point) {
                return self
            }
            if !resizeEdges(at: point).isEmpty {
                return self
            }
            return self
        }
        if let hit = super.hitTest(point), isExplicitInteractiveHit(hit) {
            return hit
        }
        return self
    }

    override func mouseDown(with event: NSEvent) {
        let localPoint = convert(event.locationInWindow, from: nil)
        if isKnowledgeBadgeHit(at: localPoint) {
            toggleSavedContextItems()
            return
        }
        if rectForView(composerBar).insetBy(dx: -8, dy: -8).contains(localPoint) {
            if let hit = super.hitTest(localPoint),
               isView(hit, inside: composer) || isView(hit, inside: composerScroll) {
                focusComposerForInput()
                composer.handleMouseGesture(
                    atWindowPoint: event.locationInWindow,
                    clickCount: event.clickCount)
                return
            }
        }
        if passThroughMode {
            let edges = resizeEdges(at: localPoint)
            if !edges.isEmpty, let window {
                beginResize(edges: edges, window: window)
                return
            }
            if isHeaderMoveHandleHit(at: localPoint) {
                beginHeaderDrag(with: event)
                return
            }
            if hasInteractiveView(at: localPoint) {
                super.mouseDown(with: event)
                return
            }
            beginHeaderDrag(with: event)
            return
        }
        let edges = resizeEdges(at: localPoint)
        if !passThroughMode, !edges.isEmpty, let window {
            beginResize(edges: edges, window: window)
            return
        }
        if !passThroughMode, shouldStartHeaderDrag(at: localPoint) {
            beginHeaderDrag(with: event)
            return
        }
        if !hasInteractiveView(at: localPoint) {
            beginHeaderDrag(with: event)
            return
        }
        super.mouseDown(with: event)
    }

    override func mouseMoved(with event: NSEvent) {
        super.mouseMoved(with: event)
        applyOverlayCursorPolicy(atWindowPoint: event.locationInWindow)
    }

    override func mouseExited(with event: NSEvent) {
        clearResizeCursorIfNeeded()
        super.mouseExited(with: event)
    }

    func applyOverlayCursorPolicy(atWindowPoint windowPoint: NSPoint) {
        let localPoint = convert(windowPoint, from: nil)
        guard bounds.contains(localPoint) else {
            clearResizeCursorIfNeeded()
            return
        }
        guard shouldReceiveMouseEvents(atWindowPoint: windowPoint) else {
            return
        }
        let edges = resizeEdges(at: localPoint)
        if !edges.isEmpty {
            setResizeCursor(for: edges)
            return
        }
        clearResizeCursorIfNeeded()
        NSCursor.arrow.set()
    }

    func routeKeyDownToComposer(_ event: NSEvent) -> Bool {
        guard closeConfirmOverlay.isHidden, answerStyleOverlay.isHidden else {
            return false
        }
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let key = (event.charactersIgnoringModifiers ?? "").lowercased()
        if let firstResponder = window?.firstResponder, firstResponder !== composer {
            if routeTextResponderShortcut(key: key, flags: flags, firstResponder: firstResponder) {
                return true
            }
            let renameEditor = renameField?.currentEditor()
            if let renameEditor, firstResponder === renameEditor {
                return false
            }
            if let renameField, firstResponder === renameField {
                return false
            }
            if firstResponder is NSTextView {
                return false
            }
            if let textField = firstResponder as? NSTextField,
               textField.isEditable || textField.isSelectable {
                return false
            }
        }
        if flags.contains(.command),
           !["a", "c", "v", "x"].contains(key) {
            return false
        }
        if flags.contains(.control) || flags.contains(.option) {
            return false
        }
        focusComposerForInput()
        if flags.contains(.command) {
            switch key {
            case "a":
                composer.selectAll(nil)
            case "c":
                if composer.selectedRange().length > 0 {
                    composer.copy(nil)
                } else if let text = feed.latestCopyableCardText() {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(text, forType: .string)
                    showSystemToast(title: "Copied", body: "Latest Bluey answer copied.", duration: 1.2)
                } else {
                    composer.copy(nil)
                }
            case "v":
                if let text = NSPasteboard.general.string(forType: .string), !text.isEmpty {
                    composer.insertPlainText(text)
                }
            case "x":
                composer.cut(nil)
            default:
                return false
            }
            return true
        }

        if event.keyCode == 51 {
            composer.deleteBackwardPlainText()
            return true
        }

        if event.keyCode == 36 || event.keyCode == 76 {
            composer.keyDown(with: event)
            return true
        }

        if let characters = event.characters, !characters.isEmpty {
            composer.insertPlainText(characters)
            return true
        }

        composer.keyDown(with: event)
        return true
    }

    private func routeTextResponderShortcut(
        key: String,
        flags: NSEvent.ModifierFlags,
        firstResponder: NSResponder
    ) -> Bool {
        guard flags.contains(.command),
              !flags.contains(.control),
              !flags.contains(.option),
              ["a", "c", "v", "x"].contains(key)
        else {
            return false
        }

        if let textView = firstResponder as? NSTextView, textView !== composer {
            return routeTextViewShortcut(key: key, textView: textView)
        }

        if let textField = firstResponder as? NSTextField,
           textField.isEditable || textField.isSelectable,
           let editor = textField.currentEditor() as? NSTextView,
           editor !== composer {
            return routeTextViewShortcut(key: key, textView: editor)
        }

        if let editor = window?.fieldEditor(false, for: nil) as? NSTextView,
           editor !== composer,
           editor.selectedRange().length > 0 {
            return routeTextViewShortcut(key: key, textView: editor)
        }

        return false
    }

    private func routeTextViewShortcut(key: String, textView: NSTextView) -> Bool {
        switch key {
        case "a":
            textView.selectAll(nil)
            return true
        case "c":
            guard textView.selectedRange().length > 0 else { return false }
            textView.copy(nil)
            return true
        case "v":
            guard textView.isEditable else { return false }
            textView.paste(nil)
            return true
        case "x":
            guard textView.isEditable else { return false }
            textView.cut(nil)
            return true
        default:
            return false
        }
    }

    private func focusComposerForInput() {
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
        window?.makeFirstResponder(composer)
        composer.armTypingCaret()
        composerInputArmedUntil = CACurrentMediaTime() + 1.5
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
        frame = snappedResizeFrame(frame, for: window)
        guard !framesNearlyEqual(frame, window.frame) else {
            return
        }

        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0
            context.allowsImplicitAnimation = false
            window.setFrame(frame, display: true, animate: false)
        }
    }

    override func mouseUp(with event: NSEvent) {
        if !activeResizeEdges.isEmpty {
            (window as? OverlayWindow)?.lockedFrameHeight = nil
            if let window {
                onWindowFrameChanged?(window.frame)
            }
            activeResizeEdges = []
            updateResizeCursor(at: convert(event.locationInWindow, from: nil))
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
        return shouldReceiveMouseEvents(atWindowPoint: windowPoint)
    }

    func shouldReceiveMouseEvents(at screenPoint: NSPoint) -> Bool {
        guard let window else { return false }
        let windowPoint = window.convertPoint(fromScreen: screenPoint)
        return shouldReceiveMouseEvents(atWindowPoint: windowPoint)
    }

    func shouldReceiveMouseEvents(atWindowPoint windowPoint: NSPoint) -> Bool {
        if dropHighlightActive {
            return true
        }
        let localPoint = convert(windowPoint, from: nil)
        guard bounds.contains(localPoint) else {
            clearResizeCursorIfNeeded()
            return false
        }
        guard passThroughMode else {
            return true
        }
        return isInteractiveAtLocalPoint(localPoint, windowPoint: windowPoint)
    }

    private func isInteractiveAtLocalPoint(_ localPoint: NSPoint, windowPoint _: NSPoint) -> Bool {
        if headerDragInProgress {
            if NSEvent.pressedMouseButtons != 0 {
                return true
            }
            headerDragInProgress = false
        }
        guard bounds.contains(localPoint) else {
            clearResizeCursorIfNeeded()
            return false
        }

        if !closeConfirmOverlay.isHidden {
            return true
        }
        if !answerStyleOverlay.isHidden {
            return true
        }
        if hasManualInteractiveControl(at: localPoint) {
            return true
        }
        if isKnowledgeBadgeHit(at: localPoint) {
            return true
        }
        if !sessionDrawer.isHidden, rectForView(sessionDrawer).contains(localPoint) {
            return true
        }
        if canvasOpen, rectForView(canvasPane).contains(localPoint) {
            return true
        }
        if isHeaderMoveHandleHit(at: localPoint) {
            return true
        }
        if !resizeEdges(at: localPoint).isEmpty {
            return true
        }
        if hasInteractiveView(at: localPoint) {
            return true
        }
        clearResizeCursorIfNeeded()
        return true
    }

    func manualButton(atWindowPoint point: NSPoint) -> NSButton? {
        let localPoint = convert(point, from: nil)
        guard bounds.contains(localPoint) else { return nil }
        if !closeConfirmOverlay.isHidden,
           let button = manualButton(in: closeConfirmOverlay, atRootPoint: localPoint) {
            return button
        }
        if !answerStyleOverlay.isHidden,
           let button = manualButton(in: answerStyleOverlay, atRootPoint: localPoint) {
            return button
        }
        return manualButton(in: self, atRootPoint: localPoint)
    }

    func manualOpacityScrubber(atWindowPoint point: NSPoint) -> OpacityScrubberView? {
        let localPoint = convert(point, from: nil)
        return manualOpacityScrubber(atRootPoint: localPoint)
    }

    private func hasManualInteractiveControl(at localPoint: NSPoint) -> Bool {
        guard bounds.contains(localPoint) else { return false }
        return manualButton(in: self, atRootPoint: localPoint) != nil
            || manualOpacityScrubber(atRootPoint: localPoint) != nil
            || transcriptClearHit(at: localPoint)
    }

    private func manualOpacityScrubber(atRootPoint localPoint: NSPoint) -> OpacityScrubberView? {
        guard bounds.contains(localPoint),
              closeConfirmOverlay.isHidden,
              answerStyleOverlay.isHidden,
              !opacityControl.isHidden,
              opacityControl.alphaValue > 0.01,
              opacityControl.isEnabled
        else {
            return nil
        }
        let rect = opacityControl.convert(opacityControl.bounds, to: self)
            .insetBy(dx: -12, dy: -10)
        return rect.contains(localPoint) ? opacityControl : nil
    }

    private func manualButton(in view: NSView, atRootPoint rootPoint: NSPoint) -> NSButton? {
        for subview in view.subviews.reversed() {
            guard !subview.isHidden, subview.alphaValue > 0.01 else { continue }
            let modalPanelButton = view === answerStylePanel || view === closeConfirmPanel
            let hitPadding: CGFloat = subview is NSButton
                ? (modalPanelButton ? 8 : 7)
                : 8
            let rect = subview.convert(subview.bounds, to: self)
                .insetBy(dx: -hitPadding, dy: -hitPadding)
            guard rect.contains(rootPoint) else { continue }

            if let button = subview as? NSButton, button.isEnabled {
                return button
            }
            if let nested = manualButton(in: subview, atRootPoint: rootPoint) {
                return nested
            }
        }
        return nil
    }

    private func hitsExplicitInteractiveChrome(at localPoint: NSPoint) -> Bool {
        let controls: [NSView] = [
            knowledgeBadge,
            themeButton,
            navButton,
            newSessionButton,
            canvasToggleButton,
            fullSizeButton,
            interactionModeButton,
            hideButton,
            closeButton,
            composerScroll,
            composer,
            transcriptClearButton,
            recordingButton,
            askButton,
            attachButton,
            instructionsButton,
            opacityControl,
            opacityLabel,
            opacitySlider,
            opacityValueLabel,
            modelMenu,
            autoSendModeMenu,
            analyzeButton,
        ]
        return controls.contains { view in
            guard !view.isHidden, view.alphaValue > 0.01 else { return false }
            let padding: CGFloat = view is NSTextView ? 8 : 10
            let rect = view.convert(view.bounds, to: self).insetBy(dx: -padding, dy: -padding)
            return rect.contains(localPoint)
        }
    }

    private func rectForView(_ view: NSView) -> NSRect {
        view.convert(view.bounds, to: self)
    }

    private func isTranscriptStripPoint(_ localPoint: NSPoint) -> Bool {
        guard !transcriptStrip.isHidden, transcriptStrip.alphaValue > 0.01 else { return false }
        return rectForView(transcriptStrip).insetBy(dx: -4, dy: -4).contains(localPoint)
    }

    private func transcriptClearHit(at localPoint: NSPoint) -> Bool {
        guard !transcriptClearButton.isHidden,
              transcriptClearButton.isEnabled,
              transcriptClearButton.alphaValue > 0.01
        else {
            return false
        }
        return rectForView(transcriptClearButton).insetBy(dx: -10, dy: -10).contains(localPoint)
    }

    private func isKnowledgeBadgeHit(at localPoint: NSPoint) -> Bool {
        guard !knowledgeBadge.isHidden,
              knowledgeBadge.alphaValue > 0.01,
              knowledgeBadgeContentVisible,
              !contextItems.isEmpty
        else {
            return false
        }
        return rectForView(knowledgeBadge).insetBy(dx: -12, dy: -10).contains(localPoint)
    }

    private func isHeaderMoveHandleHit(at localPoint: NSPoint) -> Bool {
        guard !headerBar.isHidden, headerBar.alphaValue > 0.01 else { return false }
        let handles = [headerLogo, brandStack]
        return handles.contains { view in
            guard !view.isHidden, view.alphaValue > 0.01 else { return false }
            return rectForView(view).insetBy(dx: -8, dy: -8).contains(localPoint)
        }
    }

    private func headerHitView(at localPoint: NSPoint) -> NSView? {
        let headerPoint = headerBar.convert(localPoint, from: self)
        guard headerBar.bounds.insetBy(dx: -8, dy: -8).contains(headerPoint) else { return nil }
        return headerBar.hitTest(headerPoint)
    }

    private func shouldStartHeaderDrag(at localPoint: NSPoint) -> Bool {
        guard rectForView(headerBar).insetBy(dx: -8, dy: -8).contains(localPoint),
              !resizeShouldWinOverHeaderDrag(at: localPoint)
        else {
            return false
        }
        guard let hit = headerHitView(at: localPoint) else {
            return true
        }
        return !isExplicitInteractiveHit(hit)
    }

    private func resizeShouldWinOverHeaderDrag(at point: NSPoint) -> Bool {
        point.x <= resizeHitSize || point.x >= bounds.width - resizeHitSize || point.y <= resizeHitSize
    }

    private func beginHeaderDrag(with event: NSEvent) {
        headerDragInProgress = true
        window?.makeKey()
        defer {
            headerDragInProgress = false
        }
        window?.performDrag(with: event)
        if let window {
            onWindowFrameChanged?(window.frame)
        }
    }

    private func beginResize(edges: ResizeEdges, window: NSWindow) {
        activeResizeEdges = edges
        setResizeCursor(for: edges)
        resizeStartMouse = NSEvent.mouseLocation
        resizeStartFrame = window.frame
        window.makeKey()
    }

    private func hasInteractiveView(at localPoint: NSPoint) -> Bool {
        var hit: NSView? = super.hitTest(localPoint)
        while let view = hit {
            if isExplicitInteractiveHit(view) {
                return true
            }
            hit = view.superview
        }
        return false
    }

    private func isExplicitInteractiveHit(_ hitView: NSView) -> Bool {
        var hit: NSView? = hitView
        while let view = hit {
            if view === self || view === workspace || view === headerBar || view === headerChrome || view === composerBar {
                hit = view.superview
                continue
            }
            if view is NSButton
                || view is NSPopUpButton
                || view is NSSlider
                || view is OpacityScrubberView
                || view is ClickableHeaderBadge
            {
                return true
            }
            if view is NSScroller {
                return isView(view, inside: composerScroll)
                    || isView(view, inside: sessionScroll)
                    || isView(view, inside: canvasPane)
            }
            if view is NSTextView {
                return isView(view, inside: composer)
                    || isView(view, inside: composerScroll)
                    || isView(view, inside: sessionDrawer)
                    || isView(view, inside: canvasPane)
            }
            if view is NSSecureTextField {
                return true
            }
            if let textField = view as? NSTextField,
               textField.isEditable || textField.isSelectable {
                return true
            }
            hit = view.superview
        }
        return false
    }

    private func isView(_ view: NSView, inside ancestor: NSView) -> Bool {
        var current: NSView? = view
        while let candidate = current {
            if candidate === ancestor {
                return true
            }
            current = candidate.superview
        }
        return false
    }

    private func resizeEdges(at point: NSPoint) -> ResizeEdges {
        guard bounds.contains(point),
              closeConfirmOverlay.isHidden,
              answerStyleOverlay.isHidden,
              !windowFullSize,
              canStartResize(at: point)
        else {
            return []
        }
        var edges: ResizeEdges = []
        if point.x <= resizeHitSize {
            edges.insert(.left)
        }
        if point.x >= bounds.width - resizeHitSize {
            edges.insert(.right)
        }
        if point.y >= bounds.height - resizeHitSize {
            edges.insert(.top)
        }
        if point.y <= resizeHitSize {
            edges.insert(.bottom)
        }
        return edges
    }

    private func canStartResize(at point: NSPoint) -> Bool {
        return point.x <= resizeHitSize
            || point.x >= bounds.width - resizeHitSize
            || point.y >= bounds.height - resizeHitSize
            || point.y <= resizeHitSize
    }

    private func updateResizeCursor(at localPoint: NSPoint) {
        let edges = resizeEdges(at: localPoint)
        guard !edges.isEmpty else {
            clearResizeCursorIfNeeded()
            return
        }
        setResizeCursor(for: edges)
    }

    private func setResizeCursor(for edges: ResizeEdges) {
        resizeCursor(for: edges)?.set()
        resizeCursorActive = true
    }

    private func clearResizeCursorIfNeeded() {
        if resizeCursorActive {
            NSCursor.arrow.set()
            resizeCursorActive = false
        }
    }

    private func resizeCursor(for edges: ResizeEdges) -> NSCursor? {
        let horizontal = edges.contains(.left) || edges.contains(.right)
        let vertical = edges.contains(.top) || edges.contains(.bottom)
        if horizontal && vertical {
            if (edges.contains(.left) && edges.contains(.top))
                || (edges.contains(.right) && edges.contains(.bottom)) {
                return Self.resizeNorthwestSoutheastCursor
            }
            return Self.resizeNortheastSouthwestCursor
        }
        if horizontal {
            return .resizeLeftRight
        }
        if vertical {
            return .resizeUpDown
        }
        return nil
    }

    private static func makeDiagonalResizeCursor(northwestSoutheast: Bool) -> NSCursor {
        let size = NSSize(width: 24, height: 24)
        let image = NSImage(size: size)
        image.lockFocus()
        NSColor.clear.setFill()
        NSRect(origin: .zero, size: size).fill()

        let start = northwestSoutheast ? NSPoint(x: 5, y: 19) : NSPoint(x: 5, y: 5)
        let end = northwestSoutheast ? NSPoint(x: 19, y: 5) : NSPoint(x: 19, y: 19)
        drawResizeCursorLine(from: start, to: end, color: NSColor.black.withAlphaComponent(0.55), width: 4.2)
        drawResizeCursorLine(from: start, to: end, color: NSColor.white, width: 2.2)
        drawResizeCursorArrowHead(at: start, toward: end, color: NSColor.black.withAlphaComponent(0.55), width: 4.2)
        drawResizeCursorArrowHead(at: end, toward: start, color: NSColor.black.withAlphaComponent(0.55), width: 4.2)
        drawResizeCursorArrowHead(at: start, toward: end, color: NSColor.white, width: 2.2)
        drawResizeCursorArrowHead(at: end, toward: start, color: NSColor.white, width: 2.2)

        image.unlockFocus()
        return NSCursor(image: image, hotSpot: NSPoint(x: 12, y: 12))
    }

    private static func drawResizeCursorLine(from start: NSPoint, to end: NSPoint, color: NSColor, width: CGFloat) {
        let path = NSBezierPath()
        path.move(to: start)
        path.line(to: end)
        path.lineWidth = width
        path.lineCapStyle = .round
        color.setStroke()
        path.stroke()
    }

    private static func drawResizeCursorArrowHead(at point: NSPoint, toward target: NSPoint, color: NSColor, width: CGFloat) {
        let dx = target.x - point.x
        let dy = target.y - point.y
        let length = max(1, hypot(dx, dy))
        let ux = dx / length
        let uy = dy / length
        let px = -uy
        let py = ux
        let base = NSPoint(x: point.x + ux * 5.2, y: point.y + uy * 5.2)
        let wing: CGFloat = 4.2

        let path = NSBezierPath()
        path.move(to: NSPoint(x: base.x + px * wing, y: base.y + py * wing))
        path.line(to: point)
        path.line(to: NSPoint(x: base.x - px * wing, y: base.y - py * wing))
        path.lineWidth = width
        path.lineCapStyle = .round
        path.lineJoinStyle = .round
        color.setStroke()
        path.stroke()
    }

    private func clampedResizeFrame(
        _ proposed: NSRect,
        from start: NSRect,
        edges: ResizeEdges,
        window: NSWindow
    ) -> NSRect {
        let visibleFrame = window.screen?.visibleFrame
            ?? NSScreen.main?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let inset = ExpandedPanelMetrics.screenInset
        let minX = visibleFrame.minX + inset
        let maxX = visibleFrame.maxX - inset
        let minY = visibleFrame.minY + inset
        let maxY = visibleFrame.maxY - inset
        let availableWidth = max(360, maxX - minX)
        let availableHeight = max(ExpandedPanelMetrics.minHeight, maxY - minY)
        let minWidth = min(
            max(window.minSize.width, ExpandedPanelMetrics.minResizeWidth),
            availableWidth)
        let minHeight = min(
            max(window.minSize.height, ExpandedPanelMetrics.minHeight),
            availableHeight)
        let configuredMaxWidth = window.maxSize.width > 0
            ? window.maxSize.width
            : CGFloat.greatestFiniteMagnitude
        let configuredMaxHeight = window.maxSize.height > 0
            ? window.maxSize.height
            : CGFloat.greatestFiniteMagnitude
        let maxWidth = max(minWidth, min(configuredMaxWidth, availableWidth))
        let maxHeight = max(minHeight, min(configuredMaxHeight, availableHeight))

        var left = start.minX
        var right = start.maxX
        var bottom = start.minY
        var top = start.maxY

        if edges.contains(.left), !edges.contains(.right) {
            let lower = max(minX, right - maxWidth)
            let upper = min(right - minWidth, maxX - minWidth)
            left = clamp(proposed.minX, lower, upper)
        } else if edges.contains(.right), !edges.contains(.left) {
            let lower = max(left + minWidth, minX + minWidth)
            let upper = min(maxX, left + maxWidth)
            right = clamp(proposed.maxX, lower, upper)
        } else {
            let width = clamp(proposed.width, minWidth, maxWidth)
            left = clamp(proposed.minX, minX, maxX - width)
            right = left + width
        }

        if edges.contains(.bottom), !edges.contains(.top) {
            let lower = max(minY, top - maxHeight)
            let upper = min(top - minHeight, maxY - minHeight)
            bottom = clamp(proposed.minY, lower, upper)
        } else if edges.contains(.top), !edges.contains(.bottom) {
            let lower = max(bottom + minHeight, minY + minHeight)
            let upper = min(maxY, bottom + maxHeight)
            top = clamp(proposed.maxY, lower, upper)
        } else {
            let height = clamp(proposed.height, minHeight, maxHeight)
            bottom = clamp(proposed.minY, minY, maxY - height)
            top = bottom + height
        }

        return NSRect(
            x: left,
            y: bottom,
            width: max(minWidth, right - left),
            height: max(minHeight, top - bottom))
    }

    private func clamp(_ value: CGFloat, _ lower: CGFloat, _ upper: CGFloat) -> CGFloat {
        guard lower <= upper else { return lower }
        return min(max(value, lower), upper)
    }

    private func snappedResizeFrame(_ frame: NSRect, for window: NSWindow) -> NSRect {
        let scale = max(1, window.screen?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 1)
        func snap(_ value: CGFloat) -> CGFloat {
            (value * scale).rounded() / scale
        }
        return NSRect(
            x: snap(frame.origin.x),
            y: snap(frame.origin.y),
            width: snap(frame.size.width),
            height: snap(frame.size.height))
    }

    private func framesNearlyEqual(_ lhs: NSRect, _ rhs: NSRect) -> Bool {
        abs(lhs.origin.x - rhs.origin.x) < 0.5
            && abs(lhs.origin.y - rhs.origin.y) < 0.5
            && abs(lhs.size.width - rhs.size.width) < 0.5
            && abs(lhs.size.height - rhs.size.height) < 0.5
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

    func synchronizeWindowGeometry() {
        layoutSubtreeIfNeeded()
        keepFixedChromeInBounds()
    }

    private func keepFixedChromeInBounds() {
        // The outer rows are constrained fixed chrome; do not manually
        // resize the content here. We do pin the header frame explicitly
        // because this borderless transparent window can otherwise leave the
        // fixed header unpainted after fullscreen/restore or AppKit fitting
        // passes. The workspace still sizes from the constraints below it.
        let windowContentSize = window.map { $0.contentRect(forFrameRect: $0.frame).size }
        let layoutWidth = max(0, min(bounds.width, windowContentSize?.width ?? bounds.width))
        let layoutHeight = max(0, min(bounds.height, windowContentSize?.height ?? bounds.height))
        guard layoutWidth > 120, layoutHeight >= ExpandedPanelMetrics.minHeight else { return }
        // Some AppKit scroll/document views repaint above ordinary layer
        // zPosition when the borderless overlay is resized/restored. Reassert
        // actual sibling order so transcript/feed content can never cover the
        // Bluey header or make the controls unclickable.
        let composerHeight = composerBarHeightConstraint?.constant ?? ChromeMetrics.composerBaseHeight
        let attachmentHeight = attachmentStripHeightConstraint?.constant ?? 0
        let transcriptHeight: CGFloat = ChromeMetrics.transcriptStripHeight
        let horizontalInset: CGFloat = windowFullSize ? 0 : 8
        let bottomInset: CGFloat = windowFullSize ? 0 : 8
        let chromeGap: CGFloat = 5
        let workspaceGap: CGFloat = 6

        headerChrome.frame = NSRect(
            x: 0,
            y: layoutHeight - ChromeMetrics.headerGuardHeight,
            width: layoutWidth,
            height: ChromeMetrics.headerGuardHeight)
        headerBar.frame = NSRect(
            x: ChromeMetrics.headerHorizontalInset,
            y: layoutHeight - ChromeMetrics.headerTopInset - ChromeMetrics.headerBarHeight,
            width: max(0, layoutWidth - ChromeMetrics.headerHorizontalInset * 2),
            height: ChromeMetrics.headerBarHeight)

        composerBar.frame = NSRect(
            x: horizontalInset,
            y: bottomInset,
            width: max(0, layoutWidth - horizontalInset * 2),
            height: composerHeight)
        transcriptStrip.frame = NSRect(
            x: horizontalInset,
            y: composerBar.frame.maxY + chromeGap,
            width: max(0, layoutWidth - horizontalInset * 2),
            height: transcriptHeight)
        let attachmentGap: CGFloat = attachmentHeight > 0 ? chromeGap : 0
        attachmentStrip.frame = NSRect(
            x: horizontalInset,
            y: transcriptStrip.frame.maxY + attachmentGap,
            width: max(0, layoutWidth - horizontalInset * 2),
            height: attachmentHeight)
        if attachmentHeight > 0 {
            updateAttachmentStripDocumentWidth()
        }
        let workspaceBottom = (attachmentHeight > 0 ? attachmentStrip.frame.maxY : transcriptStrip.frame.maxY) + workspaceGap
        let workspaceTop = headerChrome.frame.minY - workspaceGap
        workspace.frame = NSRect(
            x: horizontalInset,
            y: workspaceBottom,
            width: max(0, layoutWidth - horizontalInset * 2),
            height: max(0, workspaceTop - workspaceBottom))

        raiseFixedChromeToFront()
        layoutHeaderChromeControls()
        headerBar.isHidden = false
        headerChrome.isHidden = false
        headerBar.alphaValue = 1
        headerChrome.alphaValue = 1
        headerChrome.layer?.zPosition = 3_990
        headerBar.layer?.zPosition = 4_000
        for view in [
            navButton,
            newSessionButton,
            headerLogo,
            brandStack,
            routeBadge,
            knowledgeBadge,
            canvasToggleButton,
            themeButton,
            balanceLabel,
            fullSizeButton,
            interactionModeButton,
            hideButton,
            closeButton,
        ] {
            view.wantsLayer = true
            view.layer?.zPosition = 4_010
            view.alphaValue = 1
            if view.superview !== headerBar {
                view.removeFromSuperview()
                headerBar.addSubview(view, positioned: .above, relativeTo: nil)
            }
        }
        workspace.layer?.zPosition = 1
        feed.layer?.zPosition = 1
        canvasPane.layer?.zPosition = 1
        sessionDrawer.layer?.zPosition = 3_900
        updateSessionDrawerGeometry(layoutWidth: layoutWidth, layoutHeight: layoutHeight)
        answerStyleOverlay.layer?.zPosition = 4_200
        closeConfirmOverlay.layer?.zPosition = 4_300
        toastView.layer?.zPosition = 4_100
        transcriptStrip.layer?.zPosition = 3_000
        attachmentStrip.layer?.zPosition = 3_000
        composerBar.layer?.zPosition = 4_000

        canvasToggleButton.isHidden = canvases.isEmpty

        workspace.layer?.masksToBounds = true
        feed.layer?.masksToBounds = true
        canvasPane.layer?.masksToBounds = true
        transcriptStrip.layer?.masksToBounds = true
        attachmentStrip.layer?.masksToBounds = true
        composerBar.layer?.masksToBounds = false
        headerBar.layer?.masksToBounds = false
        headerChrome.layer?.masksToBounds = true

        workspace.needsLayout = true
        workspace.layoutSubtreeIfNeeded()
        headerBar.needsDisplay = true
        headerChrome.needsDisplay = true
        brandStack.needsLayout = true
        brandStack.layoutSubtreeIfNeeded()
        feed.needsLayout = true
        canvasPane.needsLayout = true
        composerBar.needsLayout = true
        transcriptStrip.needsLayout = true
    }

    private func raiseFixedChromeToFront() {
        if headerChrome.superview === self {
            let topSibling = subviews.last { $0 !== headerChrome }
            addSubview(headerChrome, positioned: .above, relativeTo: topSibling)
        }
        if headerBar.superview === self {
            let topSibling = subviews.last { $0 !== headerBar }
            addSubview(headerBar, positioned: .above, relativeTo: topSibling)
        }
        for overlay in [sessionDrawer, answerStyleOverlay, closeConfirmOverlay, toastView] {
            if overlay.superview === self, !overlay.isHidden {
                let topSibling = subviews.last { $0 !== overlay }
                addSubview(overlay, positioned: .above, relativeTo: topSibling)
            }
        }
    }

    private func layoutHeaderChromeControls() {
        let frame = headerBar.bounds
        guard frame.width > 140 else { return }

        let buttonSize: CGFloat = 28
        let iconSize: CGFloat = 26
        let yButton = (frame.height - buttonSize) / 2
        let yIcon = (frame.height - iconSize) / 2
        let yBadge = (frame.height - 22) / 2

        var left = CGFloat(10)
        let showHistoryLabel = frame.width >= 760
        let historyWidth: CGFloat = showHistoryLabel ? 82 : buttonSize
        styleHistoryHeaderButton(navButton, compact: !showHistoryLabel)
        navButton.frame = NSRect(x: left, y: yButton, width: historyWidth, height: buttonSize)
        left += historyWidth + 6
        newSessionButton.frame = NSRect(x: left, y: yButton, width: buttonSize, height: buttonSize)
        left += buttonSize + 8
        headerLogo.frame = NSRect(x: left, y: yIcon, width: iconSize, height: iconSize)
        left += iconSize + 5

        let brandWidth = min(108, max(84, frame.width * 0.10))
        let brandHeight: CGFloat = statusLabel.isHidden ? 23 : 34
        brandStack.frame = NSRect(x: left, y: (frame.height - brandHeight) / 2, width: brandWidth, height: brandHeight)
        left += brandWidth + 8

        var right = frame.width - 12
        closeButton.frame = NSRect(x: right - 26, y: yButton + 1, width: 26, height: 26)
        right -= 33
        hideButton.frame = NSRect(x: right - 26, y: yButton + 1, width: 26, height: 26)
        right -= 33
        interactionModeButton.frame = NSRect(x: right - 26, y: yButton + 1, width: 26, height: 26)
        right -= 33
        fullSizeButton.frame = NSRect(x: right - 26, y: yButton + 1, width: 26, height: 26)
        right -= 37

        let balanceWidth = min(86, max(66, frame.width * 0.11))
        balanceLabel.frame = NSRect(x: right - balanceWidth, y: yBadge, width: balanceWidth, height: 22)
        right -= balanceWidth + 8

        themeButton.frame = NSRect(x: right - 26, y: yButton + 1, width: 26, height: 26)
        right -= 33

        if !canvases.isEmpty {
            canvasToggleButton.isHidden = false
            canvasToggleButton.frame = NSRect(x: right - 26, y: yButton + 1, width: 26, height: 26)
            canvasToggleButton.toolTip = canvasOpen ? "Collapse canvas" : "Open canvas"
            right -= 33
        } else {
            canvasToggleButton.isHidden = true
            canvasToggleButton.frame = NSRect(x: right, y: yButton + 1, width: 0, height: 26)
        }

        let badgeGap: CGFloat = 6
        let middleWidth = max(0, right - left)
        let routeWidth = headerBadgeWidth(routeBadge, minimum: 74, maximum: 94)
        let preferredDocsWidth = headerBadgeWidth(knowledgeBadge, minimum: 92, maximum: 112)
        let canShowRoute = middleWidth >= routeWidth
        let docsWidth = min(preferredDocsWidth, max(0, middleWidth - (canShowRoute ? routeWidth + badgeGap : 0)))
        routeBadge.isHidden = !canShowRoute
        knowledgeBadge.isHidden = !knowledgeBadgeContentVisible || docsWidth < 82
        if !routeBadge.isHidden {
            routeBadge.frame = NSRect(x: left, y: yBadge, width: routeWidth, height: 22)
            left += routeWidth + badgeGap
        }
        if !knowledgeBadge.isHidden {
            knowledgeBadge.frame = NSRect(x: left, y: yBadge, width: docsWidth, height: 22)
        }
    }

    private func headerBadgeWidth(_ label: NSTextField, minimum: CGFloat, maximum: CGFloat) -> CGFloat {
        let measured = ceil(label.attributedStringValue.size().width) + 14
        return min(maximum, max(minimum, measured))
    }

    private func configureHeader() {
        headerBar.wantsLayer = true
        headerBar.layer?.backgroundColor = NSColor(red: 0.018, green: 0.022, blue: 0.030, alpha: 1.0).cgColor
        headerBar.layer?.cornerRadius = 21
        headerBar.layer?.borderWidth = 1
        headerBar.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.18).cgColor
        headerBar.layer?.shadowColor = NSColor.black.cgColor
        headerBar.layer?.shadowOpacity = 0.18
        headerBar.layer?.shadowRadius = 14
        headerBar.layer?.shadowOffset = NSSize(width: 0, height: -6)
        headerBar.alphaValue = 1

        headerChrome.wantsLayer = true
        headerChrome.layer?.backgroundColor = NSColor(red: 0.008, green: 0.011, blue: 0.016, alpha: 1.0).cgColor
        headerChrome.layer?.cornerRadius = 0
        headerChrome.layer?.borderWidth = 0
        headerChrome.isHidden = false

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
        statusLabel.isHidden = true
        statusLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        modelMenu.addItems(withTitles: ["Auto", "Instant", "Balanced", "Deep"])
        modelMenu.selectItem(at: 0)
        modelMenu.isBordered = false
        modelMenu.wantsLayer = true
        modelMenu.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.075).cgColor
        modelMenu.layer?.cornerRadius = 12
        modelMenu.layer?.borderWidth = 1
        modelMenu.layer?.borderColor = NSColor.white.withAlphaComponent(0.12).cgColor
        modelMenu.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        modelMenu.contentTintColor = BlueyTheme.text
        modelMenu.setContentHuggingPriority(.defaultLow, for: .horizontal)
        modelMenu.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        configureAutoSendModeMenuItems()
        autoSendModeMenu.isBordered = false
        autoSendModeMenu.wantsLayer = true
        autoSendModeMenu.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        autoSendModeMenu.contentTintColor = BlueyTheme.text
        autoSendModeMenu.setContentHuggingPriority(.defaultLow, for: .horizontal)
        autoSendModeMenu.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        updateAutoSendModeChrome()
        updateThemeButtonChrome()

        styleHeaderBadge(routeBadge, textColor: BlueyTheme.green)
        routeBadge.toolTip = "Auto Router classification and selected lane"

        styleHeaderBadge(knowledgeBadge, textColor: BlueyTheme.text)
        knowledgeBadge.toolTip = "Show or hide attached files"
        knowledgeBadge.isSelectable = false
        knowledgeBadge.focusRingType = .none
        knowledgeBadge.onClick = { [weak self] in
            self?.toggleSavedContextItems()
        }

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
        transcriptStrip.layer?.cornerRadius = 11
        transcriptStrip.layer?.borderWidth = 1
        transcriptStrip.layer?.borderColor = BlueyTheme.hairline.cgColor

        transcriptActivityDot.wantsLayer = true
        transcriptActivityDot.layer?.cornerRadius = 3
        transcriptActivityDot.layer?.backgroundColor = BlueyTheme.textDim.withAlphaComponent(0.55).cgColor
        transcriptActivityDot.layer?.shadowColor = BlueyTheme.cyan.cgColor
        transcriptActivityDot.layer?.shadowOpacity = 0
        transcriptActivityDot.layer?.shadowRadius = 7
        transcriptActivityDot.layer?.shadowOffset = .zero

        transcriptStateLabel.font = NSFont.monospacedSystemFont(ofSize: 9, weight: .bold)
        transcriptStateLabel.textColor = BlueyTheme.textDim
        transcriptStateLabel.alignment = .left
        transcriptStateLabel.lineBreakMode = .byTruncatingTail

        transcriptScroll.drawsBackground = false
        transcriptScroll.hasVerticalScroller = false
        transcriptScroll.hasHorizontalScroller = true
        transcriptScroll.autohidesScrollers = false
        transcriptScroll.borderType = .noBorder
        transcriptScroll.scrollerStyle = .overlay

        transcriptLabel.isBezeled = false
        transcriptLabel.drawsBackground = false
        transcriptLabel.font = NSFont.systemFont(ofSize: 11.25, weight: .semibold)
        transcriptLabel.textColor = BlueyTheme.text
        transcriptLabel.lineBreakMode = .byClipping
        transcriptLabel.maximumNumberOfLines = 1
        transcriptLabel.alignment = .left
        if let cell = transcriptLabel.cell as? NSTextFieldCell {
            cell.isScrollable = true
            cell.wraps = false
            cell.lineBreakMode = .byClipping
        }
        transcriptScroll.horizontalScrollElasticity = .allowed
        transcriptScroll.verticalScrollElasticity = .none
        transcriptScroll.usesPredominantAxisScrolling = false

        attachmentStack.orientation = .horizontal
        attachmentStack.alignment = .centerY
        attachmentStack.spacing = 6
        attachmentStack.edgeInsets = NSEdgeInsets(top: 3, left: 4, bottom: 3, right: 8)

        attachmentStrip.drawsBackground = false
        attachmentStrip.hasVerticalScroller = false
        attachmentStrip.hasHorizontalScroller = true
        attachmentStrip.autohidesScrollers = false
        attachmentStrip.horizontalScrollElasticity = .allowed
        attachmentStrip.usesPredominantAxisScrolling = false
        attachmentStrip.borderType = .noBorder
        attachmentStrip.documentView = attachmentStack
        attachmentStrip.scrollerStyle = .overlay
        attachmentStrip.isHidden = true
        attachmentStrip.wantsLayer = true
        attachmentStrip.layer?.cornerRadius = 13
        attachmentStrip.layer?.masksToBounds = true
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
        sessionScroll.verticalScrollElasticity = .allowed
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
        composerBar.layer?.cornerRadius = 16
        composerBar.layer?.borderWidth = 1
        composerBar.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.22).cgColor
        composerBar.layer?.shadowColor = NSColor.black.cgColor
        composerBar.layer?.shadowOpacity = 0.18
        composerBar.layer?.shadowRadius = 14
        composerBar.layer?.shadowOffset = .zero

        composerSurface.wantsLayer = true
        composerSurface.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.045).cgColor
        composerSurface.layer?.cornerRadius = 13
        composerSurface.layer?.borderWidth = 1
        composerSurface.layer?.borderColor = NSColor.white.withAlphaComponent(0.105).cgColor
        (composerSurface as? ComposerSurfaceView)?.updateBorderColors(
            resting: NSColor.white.withAlphaComponent(0.105),
            focused: BlueyTheme.cyan.withAlphaComponent(0.62))

        composerScroll.drawsBackground = false
        composerScroll.borderType = .noBorder
        composerScroll.hasHorizontalScroller = false
        composerScroll.hasVerticalScroller = true
        composerScroll.autohidesScrollers = true
        composerScroll.horizontalScrollElasticity = .none
        composerScroll.verticalScrollElasticity = .allowed
        composerScroll.contentView.drawsBackground = false
        composerScroll.contentView.postsBoundsChangedNotifications = true
        composerScroll.wantsLayer = true
        composerScroll.layer?.backgroundColor = NSColor.clear.cgColor

        opacityControl.wantsLayer = true
        opacityControl.layer?.backgroundColor = NSColor.clear.cgColor
        opacityControl.layer?.cornerRadius = 13
        opacityControl.layer?.borderWidth = 0
        opacityControl.layer?.borderColor = NSColor.clear.cgColor
        opacityControl.toolTip = "Overlay opacity"
        opacityLabel.stringValue = "Opacity"
        opacityLabel.font = NSFont.systemFont(ofSize: 9.8, weight: .semibold)
        opacityLabel.textColor = BlueyTheme.textDim
        opacityLabel.alignment = .left
        opacityValueLabel.font = NSFont.monospacedDigitSystemFont(ofSize: 9.8, weight: .semibold)
        opacityValueLabel.textColor = BlueyTheme.textDim
        opacityValueLabel.alignment = .right
        opacitySlider.controlSize = .small
        opacitySlider.wantsLayer = true
        opacitySlider.isHidden = true
        opacitySlider.toolTip = "Overlay opacity"
        opacityControl.value = opacitySlider.doubleValue

        composer.placeholder = "Ask anything..."
        composer.setContentHuggingPriority(.defaultLow, for: .horizontal)
        composer.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        for control in [
            recordingButton,
            autoSendModeMenu,
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
        headerBar.toolTip = "Drag this bar to move Bluey"
        headerChrome.toolTip = "Drag this bar to move Bluey"
        headerLogo.toolTip = "Bluey"
        headerWordmark.toolTip = "Bluey"
        navButton.toolTip = "Open or close conversation history"
        drawerCloseButton.toolTip = "Close conversation history"
        newSessionButton.toolTip = "Start a new recording"
        modelMenu.toolTip = "Choose Auto, Instant, Balanced, or Deep"
        autoSendModeMenu.toolTip = autoSendStopMode.tooltip
        canvasToggleButton.toolTip = "Open or collapse the canvas"
        themeButton.toolTip = lightThemeEnabled ? "Switch to dark theme" : "Switch to light theme"
        balanceLabel.toolTip = "Remaining Bluey credits"
        fullSizeButton.toolTip = windowFullSize ? "Restore compact Bluey" : "Fill this screen"
        interactionModeButton.toolTip = passThroughMode
            ? "Move-anywhere on: controls click normally, and blank Bluey space drags the panel."
            : "Interactive on: blank Bluey space moves/resizes the panel, and the whole panel receives clicks."
        hideButton.toolTip = "Minimize Bluey to the small pill"
        closeButton.toolTip = "Turn Bluey off"
        recordingButton.toolTip = "Start or stop listening"
        instructionsButton.toolTip = "Set how Bluey should answer"
        attachButton.toolTip = "Attach documents or images"
        analyzeButton.toolTip = "Capture the screen as context"
        askButton.toolTip = "Send the question. Enter answers; Shift+Enter adds a new line."
        composer.toolTip = "Type or paste a question for Bluey"
        transcriptStrip.toolTip = "Live captions preview"
        transcriptClearButton.toolTip = "Clear captions from the next answer. Already transcribed cloud audio may still count as used."
        attachmentStrip.toolTip = "Attached documents and images. Scroll horizontally to see more."
        opacityControl.toolTip = "Adjust Bluey opacity"
        opacitySlider.toolTip = "Adjust Bluey opacity"
        headerLogo.toolTip = "Drag Bluey"
        headerWordmark.toolTip = "Drag Bluey"
        brandStack.toolTip = "Drag Bluey"
        latestSessionButton.toolTip = "Continue the latest recording"
        answerStyleSaveButton.toolTip = "Save answer style for this session"
    }

    private func refreshControlChromeForTheme() {
        styleHeaderIconButton(drawerCloseButton, symbol: "xmark", fallback: "x")
        styleHeaderIconButton(canvasToggleButton, symbol: "sidebar.right", fallback: "|")
        styleHeaderIconButton(newSessionButton, symbol: "square.and.pencil", fallback: "+")
        styleHeaderIconButton(hideButton, symbol: "eye.slash", fallback: "-")
        styleHeaderIconButton(closeButton, symbol: "xmark", fallback: "x")
        styleIconButton(transcriptClearButton, symbol: "xmark", fallback: "x")
        styleIconButton(attachButton, symbol: "plus", fallback: "+")
        styleControlButton(latestSessionButton, symbol: "clock.arrow.circlepath", accent: false)
        styleControlButton(answerStyleSaveButton, symbol: "checkmark", accent: true)
        styleControlButton(instructionsButton, symbol: "text.bubble", accent: false)
        styleControlButton(analyzeButton, symbol: "sparkle.magnifyingglass", accent: false)
        styleControlButton(askButton, symbol: "arrow.up", accent: true)
        let recordingSymbol = recordingActive ? "stop.fill" : "waveform"
        styleControlButton(recordingButton, symbol: recordingSymbol, accent: recordingActive)
        updateFullSizeButtonChrome()
        updateInteractionModeChrome(showToast: false)
    }

    private func styleControlButton(_ button: NSButton, symbol: String, accent: Bool) {
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 11
        let titleColor = lightThemeEnabled ? BlueyLightTheme.text : BlueyTheme.text
        let accentColor = themedAccentColor
        if lightThemeEnabled {
            button.layer?.backgroundColor = (accent
                ? themedAccentFillColor.withAlphaComponent(lightMaterialAlpha(0.92, floor: 0.28))
                : BlueyLightTheme.surface.withAlphaComponent(lightMaterialAlpha(0.82, floor: 0.16))).cgColor
            button.layer?.borderColor = (accent
                ? themedAccentBorderColor.withAlphaComponent(0.78)
                : BlueyLightTheme.border).cgColor
        } else {
            button.layer?.backgroundColor = accent
                ? NSColor(red: 0.045, green: 0.145, blue: 0.190, alpha: 0.94).cgColor
                : NSColor.white.withAlphaComponent(0.042).cgColor
            button.layer?.borderColor = (accent ? BlueyTheme.cyan.withAlphaComponent(0.42) : NSColor.white.withAlphaComponent(0.075)).cgColor
        }
        button.layer?.borderWidth = accent ? 0.8 : 0.6
        button.font = NSFont.systemFont(ofSize: 10.6, weight: .bold)
        button.attributedTitle = NSAttributedString(
            string: button.title,
            attributes: [
                .font: button.font ?? NSFont.systemFont(ofSize: 10.6, weight: .bold),
                .foregroundColor: titleColor,
            ])
        button.contentTintColor = accent ? accentColor : (lightThemeEnabled ? BlueyLightTheme.textDim : BlueyTheme.cyan)
        button.image = nil
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
        button.contentTintColor = lightThemeEnabled ? BlueyLightTheme.text : BlueyTheme.text
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
                    .foregroundColor: lightThemeEnabled ? BlueyLightTheme.textDim : BlueyTheme.textDim,
                ])
        }
        button.imageHugsTitle = true
        button.alignment = .center
    }

    private func styleHistoryHeaderButton(_ button: NSButton, compact: Bool) {
        if compact {
            styleHeaderIconButton(button, symbol: "sidebar.left", fallback: "[]")
            return
        }

        button.title = "History"
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 14
        button.layer?.backgroundColor = (lightThemeEnabled
            ? BlueyLightTheme.surface.withAlphaComponent(lightMaterialAlpha(0.82, floor: 0.16))
            : NSColor.white.withAlphaComponent(0.045)).cgColor
        button.layer?.borderWidth = 0.7
        button.layer?.borderColor = themedAccentBorderColor.withAlphaComponent(lightThemeEnabled ? 0.48 : 0.18).cgColor
        button.font = NSFont.systemFont(ofSize: 10.5, weight: .bold)
        button.attributedTitle = NSAttributedString(
            string: "History",
            attributes: [
                .font: button.font ?? NSFont.systemFont(ofSize: 10.5, weight: .bold),
                .foregroundColor: lightThemeEnabled ? BlueyLightTheme.text : BlueyTheme.text,
            ])
        button.contentTintColor = themedAccentColor
        button.image = nil
        if let image = symbolImage("sidebar.left") {
            image.isTemplate = true
            button.image = image
        }
        button.imagePosition = .imageLeading
        button.imageHugsTitle = true
        button.imageScaling = .scaleProportionallyDown
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
        button.contentTintColor = accent ? (lightThemeEnabled ? BlueyLightTheme.text : BlueyTheme.text) : themedAccentColor
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
                    .foregroundColor: lightThemeEnabled ? BlueyLightTheme.text : BlueyTheme.text,
                ])
        }
        button.imageHugsTitle = true
        button.alignment = .center
    }

    @objc private func hideClicked() { onClose?() }

    @objc private func themeClicked() {
        lightThemeEnabled.toggle()
        UserDefaults.standard.set(lightThemeEnabled, forKey: overlayLightThemeDefaultsKey)
        refreshBackgroundChrome()
        configureTooltips()
        needsDisplay = true
    }

    @objc private func closeClicked() {
        showTurnOffConfirmation()
    }

    @objc private func fullSizeClicked() {
        toggleWindowFullSize()
    }

    @objc private func interactionModeClicked() {
        passThroughMode.toggle()
        updateInteractionModeChrome()
        onInteractionModeChanged?()
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
        setHeaderSubtitle()
        emitSessionDelete(id: id)
    }

    @objc private func opacityChanged() {
        applyOpacity(opacitySlider.doubleValue)
    }

    func applyOpacity(_ opacity: Double) {
        let value = min(max(opacity, Double(minimumOverlayBackgroundOpacity)), 1.0)
        backgroundOpacity = CGFloat(value)
        if abs(opacitySlider.doubleValue - value) > 0.001 {
            opacitySlider.doubleValue = value
        }
        opacityControl.value = value
        opacityValueLabel.stringValue = "\(Int((value * 100.0).rounded()))"
        refreshBackgroundChrome()
        onOpacityChanged?(value)
        emitOpacityUpdated(value)
    }

    private func setComposerTextHeight(_ rawHeight: CGFloat) {
        composerMeasuredHeight = max(rawHeight, ChromeMetrics.composerInputHeight)
        let textHeight = min(composerMeasuredHeight, 68)
        let previousTextHeight = composerTextHeightConstraint?.constant ?? 0
        let previousDocumentHeight = composerDocumentHeightConstraint?.constant ?? 0
        composerDocumentHeightConstraint?.constant = composerMeasuredHeight
        let textHeightChanged = abs(previousTextHeight - textHeight) > 0.5
        let documentHeightChanged = abs(previousDocumentHeight - composerMeasuredHeight) > 0.5
        guard textHeightChanged || documentHeightChanged else { return }
        composerTextHeightConstraint?.constant = textHeight
        composerBarHeightConstraint?.constant = textHeight + ChromeMetrics.composerExtraChromeHeight
        needsLayout = true
        layoutSubtreeIfNeeded()
        composer.scrollRangeToVisible(composer.selectedRange())
    }

    private func shouldIgnoreRapidToggle(lastActionAt: inout TimeInterval) -> Bool {
        let now = CACurrentMediaTime()
        let eventClickCount = NSApp.currentEvent?.clickCount ?? 1
        let isRapidRepeat = now - lastActionAt < 0.28
        lastActionAt = now
        return eventClickCount > 1 || isRapidRepeat
    }

    @objc private func toggleSessionsClicked() {
        guard !shouldIgnoreRapidToggle(lastActionAt: &lastSessionToggleAt) else { return }
        if !sessionDrawer.isHidden {
            sessionDrawer.isHidden = true
            setHeaderSubtitle()
            return
        }
        emitLifecycle(
            "session_drawer_opened",
            detail: "had_loaded=\(sessionsHaveLoaded) cached_sessions=\(sessionItems.count)"
        )
        if !sessionsHaveLoaded {
            renderSessionDrawerMessage("Loading...")
        }
        emitSimple("session_list_requested")
        updateSessionDrawerGeometry(layoutWidth: bounds.width, layoutHeight: bounds.height)
        sessionDrawer.isHidden = false
        setHeaderSubtitle()
    }

    @objc private func closeSessionsClicked() {
        sessionDrawer.isHidden = true
    }

    @objc private func toggleCanvasClicked() {
        guard !canvases.isEmpty else { return }
        guard !shouldIgnoreRapidToggle(lastActionAt: &lastCanvasToggleAt) else { return }
        if canvasOpen {
            setCanvasOpen(false)
        } else {
            suppressCanvasFullWindowUntil = CACurrentMediaTime() + 0.45
            renderActiveCanvas()
            setCanvasOpen(true)
        }
    }

    private func toggleCanvasFullWindow() {
        guard canvasOpen, window != nil else { return }
        guard CACurrentMediaTime() >= suppressCanvasFullWindowUntil else { return }
        if canvasFullWindow {
            restoreCanvasWindow()
        } else {
            expandCanvasWindow()
        }
    }

    @objc private func newSessionClicked() {
        guard !isCurrentSessionSurfaceEmpty else {
            showSystemToast(title: "Ready", body: "This recording is already empty.", duration: 1.8)
            return
        }
        let preserveFrame = !canvasOpen
        let previousFrame = preserveFrame ? window?.frame : nil
        resetSessionSurface()
        composer.clearText()
        setHeaderSubtitle()
        sessionDrawer.isHidden = true
        if let previousFrame {
            layoutSubtreeIfNeeded()
            window?.setFrame(previousFrame, display: true)
        }
        emitSimple("session_new_requested")
    }

    private var isCurrentSessionSurfaceEmpty: Bool {
        !feed.hasVisibleCards
            && transcriptSnippets.isEmpty
            && latestLiveTranscriptLine == nil
            && latestLiveTranscriptLinesBySource.isEmpty
            && liveTranscriptPreviewBodies.isEmpty
            && !hasVisibleContextAttachments
            && !screenContextReadyForAnswer
            && canvases.isEmpty
            && composer.string.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    @objc private func continueSessionClicked() {
        sessionDrawer.isHidden = true
        setHeaderSubtitle()
        emitSimple("session_continue_requested")
    }

    @objc private func saveAnswerStyleClicked() {
        let text = answerStyleBox.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        emitInstructions(text: text)
        setHeaderSubtitle()
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
        let now = Date()
        if now.timeIntervalSince(lastRecordingToggleAt) < 0.45 {
            return
        }
        lastRecordingToggleAt = now

        if recordingTransitionInFlight && !recordingDesiredActive {
            return
        }

        if recordingActive || recordingDesiredActive {
            let shouldScheduleAutoSend = recordingActive
            emitSimple("recording_stop_requested")
            recordingActive = false
            recordingDesiredActive = false
            recordingTransitionInFlight = true
            onListeningStateChanged?(.paused)
            recordingDesiredActive = false
            recordingTransitionInFlight = true
            recordingButton.title = "Listen"
            setHeaderSubtitle()
            composer.placeholder = "Ask anything..."
            updateAudioRouteBadge("● Ready", accent: BlueyTheme.green)
            styleControlButton(recordingButton, symbol: "waveform", accent: false)
            setTranscriptState("READY", active: false)
            if shouldScheduleAutoSend {
                scheduleAutoSendAfterExplicitStop()
            }
        } else {
            prepareAutoSendListenCapture()
            emitSimple("recording_start_requested")
            recordingActive = false
            recordingDesiredActive = true
            recordingTransitionInFlight = true
            onListeningStateChanged?(.connecting)
            recordingButton.title = "Starting"
            setHeaderSubtitle()
            composer.placeholder = "Starting audio..."
            styleControlButton(recordingButton, symbol: "waveform", accent: false)
            setTranscriptState("STARTING", active: true)
            seedTranscriptPreviewIfEmpty("Mic + System: starting audio...")
        }
    }

    @objc private func autoSendModeChanged() {
        let tag = autoSendModeMenu.selectedItem?.tag ?? -1
        if tag >= 0 {
            setAutoSendStopMode(AutoSendStopMode(rawValue: tag) ?? .off)
        }
    }

    @objc private func autoSendModeMenuItemSelected(_ sender: NSMenuItem) {
        guard let mode = AutoSendStopMode(rawValue: sender.tag) else {
            updateAutoSendModeChrome()
            return
        }
        setAutoSendStopMode(mode)
    }

    private func migrateAutoSendStopModeDefaultsIfNeeded() {
        let defaults = UserDefaults.standard
        guard defaults.integer(forKey: overlayAutoSendStopModeVersionDefaultsKey) < overlayAutoSendStopModeCurrentVersion else {
            return
        }
        autoSendStopMode = .off
        defaults.set(autoSendStopMode.rawValue, forKey: overlayAutoSendStopModeDefaultsKey)
        defaults.set(overlayAutoSendStopModeCurrentVersion, forKey: overlayAutoSendStopModeVersionDefaultsKey)
    }

    private func setAutoSendStopMode(_ mode: AutoSendStopMode) {
        autoSendStopMode = mode
        UserDefaults.standard.set(mode.rawValue, forKey: overlayAutoSendStopModeDefaultsKey)
        UserDefaults.standard.set(overlayAutoSendStopModeCurrentVersion, forKey: overlayAutoSendStopModeVersionDefaultsKey)
        autoSendAfterStopWorkItem?.cancel()
        autoSendAfterStopWorkItem = nil
        updateAutoSendModeChrome()
    }

    private func configureAutoSendModeMenuItems() {
        autoSendModeMenu.removeAllItems()
        autoSendModeMenu.addItem(withTitle: autoSendStopMode.compactTitle)
        autoSendModeMenu.item(at: 0)?.tag = -1
        autoSendModeMenu.menu?.minimumWidth = 270
        autoSendModeMenu.menu?.addItem(.separator())
        for mode in AutoSendStopMode.allCases {
            autoSendModeMenu.addItem(withTitle: mode.menuTitle)
            let item = autoSendModeMenu.item(at: autoSendModeMenu.numberOfItems - 1)
            item?.tag = mode.rawValue
            item?.state = mode == autoSendStopMode ? .on : .off
            item?.target = self
            item?.action = #selector(autoSendModeMenuItemSelected(_:))
        }
        autoSendModeMenu.selectItem(at: 0)
    }

    private func updateAutoSendModeChrome() {
        let enabled = autoSendStopMode.isEnabled
        configureAutoSendModeMenuItems()
        autoSendModeMenu.toolTip = autoSendStopMode.tooltip
        autoSendModeMenu.contentTintColor = enabled ? BlueyTheme.green : themedTextColor
        autoSendModeMenu.layer?.cornerRadius = 12
        autoSendModeMenu.layer?.borderWidth = 1
        autoSendModeMenu.layer?.backgroundColor = (enabled
            ? BlueyTheme.green.withAlphaComponent(materialAlpha(0.14, floor: 0.05))
            : themedSurfaceColor).cgColor
        autoSendModeMenu.layer?.borderColor = (enabled
            ? BlueyTheme.green.withAlphaComponent(materialAlpha(0.42, floor: 0.14))
            : (lightThemeEnabled
                ? NSColor.black.withAlphaComponent(0.10)
                : NSColor.white.withAlphaComponent(materialAlpha(0.12)))).cgColor
    }

    private func hasAutoSendStopContext() -> Bool {
        switch autoSendStopMode {
        case .off:
            return false
        case .mic:
            return hasAutoSendTranscriptContext(forSource: "Mic")
        case .system:
            return hasAutoSendTranscriptContext(forSource: "System")
        case .micAndSystem:
            return hasAutoSendTranscriptContext(forSource: "Mic") || hasAutoSendTranscriptContext(forSource: "System")
        }
    }

    private func hasAutoSendTranscriptContext(forSource source: String) -> Bool {
        autoSendTranscriptLinesBySource[source]?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
    }

    private func hasTranscriptQuestionContext() -> Bool {
        transcriptQuestionForAnswer()?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
            || liveTranscriptPreviewQuestionForAnswer()?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
    }

    private func shouldBlockSilentListenAnswer(raw: String) -> Bool {
        guard raw.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return false }
        guard autoSendListenCaptureActive || recordingActive else { return false }
        guard !hasTranscriptQuestionContext() else { return false }

        if !recordingActive {
            autoSendAfterStopWorkItem?.cancel()
            autoSendAfterStopWorkItem = nil
            autoSendListenCaptureActive = false
            autoSendTranscriptLinesBySource.removeAll()
        }
        showSystemToast(
            title: "No captions yet",
            body: recordingActive
                ? "Speak first, type a question, or stop Listen before using attached context."
                : "Nothing was transcribed from that Listen run.",
            duration: 2.8)
        return true
    }

    private func shouldSuppressDuplicateAsk(question: String, visibleContextIds: [String]) -> Bool {
        var normalizedQuestion = normalizeTranscriptMemoryLine(question)
        if let transcript = transcriptQuestionForAnswer() ?? liveTranscriptPreviewQuestionForAnswer() {
            let transcriptFingerprint = compactTranscriptMemoryLine(transcript)
            if !transcriptFingerprint.isEmpty {
                normalizedQuestion += "|transcript:"
                normalizedQuestion += String(transcriptFingerprint.suffix(640))
            }
        }
        let normalizedContext = visibleContextIds.sorted().joined(separator: ",")
        let fingerprint = "\(normalizedQuestion)|\(normalizedContext)"
        guard !fingerprint.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return false
        }
        let now = CACurrentMediaTime()
        if fingerprint == lastSubmittedAskFingerprint, now - lastSubmittedAskAt < 8.0 {
            return true
        }
        lastSubmittedAskFingerprint = fingerprint
        lastSubmittedAskAt = now
        return false
    }

    private func scheduleAutoSendAfterExplicitStop() {
        autoSendAfterStopWorkItem?.cancel()
        autoSendAfterStopWorkItem = nil
        guard autoSendStopMode.isEnabled else { return }
        scheduleAutoSendAfterStopAttempt(mode: autoSendStopMode, attempt: 0)
    }

    private func scheduleAutoSendAfterStopAttempt(mode: AutoSendStopMode, attempt: Int) {
        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.autoSendAfterStopWorkItem = nil
            guard self.autoSendStopMode == mode else { return }
            guard !self.recordingActive else { return }
            guard self.hasAutoSendStopContext() else {
                if attempt < 4 {
                    self.scheduleAutoSendAfterStopAttempt(mode: mode, attempt: attempt + 1)
                } else {
                    self.autoSendListenCaptureActive = false
                    self.autoSendTranscriptLinesBySource.removeAll()
                }
                return
            }
            self.sendAutoStopAnswer(mode: mode)
        }
        autoSendAfterStopWorkItem = work
        let delay = attempt == 0 ? 1.2 : 0.75
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
    }

    private func sendAutoStopAnswer(mode: AutoSendStopMode) {
        guard let q = autoSendQuestionForStopMode(mode) else {
            emitLifecycle(
                "autosend_answer_skipped",
                status: "empty",
                detail: "mode=\(mode.rawValue) sources=\(autoSendTranscriptLinesBySource.count)"
            )
            autoSendListenCaptureActive = false
            autoSendTranscriptLinesBySource.removeAll()
            return
        }
        consumeTranscriptBufferForAnswer()
        let route = selectedRoute()
        updateRouteBadge(for: q, selectedRoute: route)
        let sentContextIds = Array(pendingContextItemIds)
        guard !shouldSuppressDuplicateAsk(question: q, visibleContextIds: sentContextIds) else {
            emitLifecycle(
                "autosend_answer_skipped",
                status: "duplicate",
                detail: "mode=\(mode.rawValue) question_chars=\(q.count) context_ids=\(sentContextIds.count)"
            )
            autoSendListenCaptureActive = false
            autoSendTranscriptLinesBySource.removeAll()
            return
        }
        emitLifecycle(
            "autosend_answer_sent",
            detail: "mode=\(mode.rawValue) question_chars=\(q.count) generic_live_prompt=\(isLiveTranscriptAnswerPrompt(q)) context_ids=\(sentContextIds.count)"
        )
        emitAsk(
            question: q,
            provider: route.provider,
            model: route.model,
            mode: route.mode,
            visibleContextIds: sentContextIds
        )
        consumeSentPendingContextAttachments()
        screenContextReadyForAnswer = false
        window?.makeFirstResponder(composer)
    }

    private func autoSendQuestionForStopMode(_ mode: AutoSendStopMode) -> String? {
        let allowedSources: [String]
        switch mode {
        case .off:
            return nil
        case .mic:
            allowedSources = ["Mic"]
        case .system:
            allowedSources = ["System"]
        case .micAndSystem:
            allowedSources = ["Mic", "System"]
        }
        let lines = allowedSources.compactMap { source -> String? in
            guard let body = autoSendTranscriptLinesBySource[source]?.trimmingCharacters(in: .whitespacesAndNewlines),
                  !body.isEmpty else {
                return nil
            }
            return "\(source): \(body)"
        }
        let joined = compactTranscriptQuestionLines(lines)
            .joined(separator: "\n")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return liveTranscriptVisibleQuestion(from: joined)
            ?? (joined.isEmpty ? nil : boundedTranscriptTail(joined, maxChars: ChromeMetrics.transcriptPreviewMemoryChars))
    }

    func prepareAutoSendListenCapture() {
        autoSendAfterStopWorkItem?.cancel()
        autoSendAfterStopWorkItem = nil
        autoSendListenCaptureActive = true
        autoSendTranscriptLinesBySource.removeAll()
    }

    @objc private func askClicked() {
        autoSendAfterStopWorkItem?.cancel()
        autoSendAfterStopWorkItem = nil
        let raw = composer.string.trimmingCharacters(in: .whitespacesAndNewlines)
        let hadTranscriptContext = hasTranscriptQuestionContext()
        let hadPreviewTranscriptContext = liveTranscriptPreviewQuestionForAnswer() != nil
        guard !shouldBlockSilentListenAnswer(raw: raw) else {
            window?.makeFirstResponder(composer)
            return
        }
        guard let q = composedQuestionForAnswer(typed: raw) ?? fallbackQuestionForAttachedContext() else {
            showSystemToast(for: RenderedCard(
                id: "empty-answer-\(UUID().uuidString)",
                kind: "system",
                title: "Nothing to answer yet",
                body: "Ask a question, start Listen, attach what you have, or capture the screen first.",
                done: true,
                costLabel: nil,
                artifact: nil,
                attachments: []))
            window?.makeFirstResponder(composer)
            return
        }
        let sentContextIds = Array(pendingContextItemIds)
        guard !shouldSuppressDuplicateAsk(question: q, visibleContextIds: sentContextIds) else {
            emitLifecycle(
                "ask_answer_skipped",
                status: "duplicate_suppressed",
                detail: "typed_chars=\(raw.count) question_chars=\(q.count) transcript_context=\(hadTranscriptContext) preview_transcript_context=\(hadPreviewTranscriptContext) context_ids=\(sentContextIds.count)"
            )
            showSystemToast(title: "Already sent", body: "Bluey is already answering that request.", duration: 1.8)
            window?.makeFirstResponder(composer)
            return
        }
        composer.clearText()
        consumeTranscriptBufferForAnswer()
        let route = selectedRoute()
        updateRouteBadge(for: q, selectedRoute: route)
        emitLifecycle(
            "ask_answer_sent",
            detail: "typed_chars=\(raw.count) question_chars=\(q.count) transcript_context=\(hadTranscriptContext) preview_transcript_context=\(hadPreviewTranscriptContext) generic_live_prompt=\(isLiveTranscriptAnswerPrompt(q)) context_ids=\(sentContextIds.count)"
        )
        emitAsk(
            question: q,
            provider: route.provider,
            model: route.model,
            mode: route.mode,
            visibleContextIds: sentContextIds
        )
        consumeSentPendingContextAttachments()
        screenContextReadyForAnswer = false
        window?.makeFirstResponder(composer)
    }

    @objc private func analyzeClicked() {
        let raw = composer.string.trimmingCharacters(in: .whitespacesAndNewlines)
        let question = composedQuestionForAnswer(typed: raw)
        routeBadge.stringValue = "Screen · ready"
        routeBadge.textColor = themedAccentColor
        routeBadge.toolTip = "Screen capture is ready for the next answer"
        setHeaderSubtitle("Capturing screen")
        screenContextReadyForAnswer = true
        expectContextMutationForPendingSend()
        emitAnalyzeScreen(question: question)
        window?.makeFirstResponder(composer)
    }

    @objc private func attachClicked() {
        guard beginAttachPickerHandoff() else {
            showKnowledgePlaceholder("File picker is already opening...")
            return
        }
        setKnowledgeBadge("Docs loading", accent: BlueyTheme.warning)
        showKnowledgePlaceholder("Indexing selected files...")
        expectContextMutationForPendingSend()
        emitSimple("attach_requested")
    }

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        guard !draggedFilePaths(from: sender).isEmpty else { return [] }
        setDropHighlight(true)
        return .copy
    }

    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        draggedFilePaths(from: sender).isEmpty ? [] : .copy
    }

    override func draggingExited(_ sender: NSDraggingInfo?) {
        setDropHighlight(false)
    }

    override func prepareForDragOperation(_ sender: NSDraggingInfo) -> Bool {
        !draggedFilePaths(from: sender).isEmpty
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        let paths = draggedFilePaths(from: sender)
        setDropHighlight(false)
        guard !paths.isEmpty else { return false }
        endAttachPickerHandoff()
        let supportedPaths = paths.filter(isSupportedDropPath)
        let skippedCount = paths.count - supportedPaths.count
        if skippedCount > 0 {
            showUnsupportedDropMessage(skippedCount: skippedCount, totalCount: paths.count)
        }
        guard !supportedPaths.isEmpty else {
            setKnowledgeBadge("Unsupported file", accent: BlueyTheme.warning)
            showKnowledgePlaceholder(supportedDropFormatsMessage)
            return true
        }
        setKnowledgeBadge("Docs loading", accent: BlueyTheme.warning)
        showKnowledgePlaceholder(supportedPaths.count == 1 ? "Indexing dropped document..." : "Indexing dropped documents...")
        expectContextMutationForPendingSend()
        emitAttachFiles(paths: supportedPaths)
        return true
    }

    override func concludeDragOperation(_ sender: NSDraggingInfo?) {
        setDropHighlight(false)
    }

    private func draggedFilePaths(from sender: NSDraggingInfo) -> [String] {
        let objects = sender.draggingPasteboard.readObjects(
            forClasses: [NSURL.self],
            options: [.urlReadingFileURLsOnly: true])
        let urls = (objects as? [URL])
            ?? (objects as? [NSURL])?.compactMap { $0 as URL }
            ?? []
        return urls
            .map(\.path)
            .filter { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    }

    private func isSupportedDropPath(_ path: String) -> Bool {
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory),
              !isDirectory.boolValue
        else {
            return false
        }
        let ext = URL(fileURLWithPath: path).pathExtension.lowercased()
        return supportedDropExtensions.contains(ext)
    }

    private func showUnsupportedDropMessage(skippedCount: Int, totalCount: Int) {
        let title = skippedCount == totalCount ? "File type not supported" : "Some files were skipped"
        let skippedText = skippedCount == 1 ? "1 file" : "\(skippedCount) files"
        let intro = skippedCount == totalCount
            ? "Bluey cannot use that file type as context yet."
            : "Bluey skipped \(skippedText) that are not readable context."
        showSystemToast(for: RenderedCard(
            id: "unsupported-drop-\(UUID().uuidString)",
            kind: "warning",
            title: title,
            body: "\(intro) \(supportedDropFormatsMessage)",
            done: true,
            costLabel: nil,
            artifact: nil,
            attachments: []))
    }

    @discardableResult
    private func beginAttachPickerHandoff() -> Bool {
        guard !attachPickerPending else { return false }
        attachPickerPending = true
        attachButton.alphaValue = 0.6
        attachButton.toolTip = "Opening file picker..."

        attachPickerResetWorkItem?.cancel()
        let workItem = DispatchWorkItem { [weak self] in
            self?.endAttachPickerHandoff()
        }
        attachPickerResetWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.0, execute: workItem)
        return true
    }

    private func endAttachPickerHandoff() {
        attachPickerResetWorkItem?.cancel()
        attachPickerResetWorkItem = nil
        attachPickerPending = false
        attachButton.alphaValue = attachButton.isEnabled ? 1.0 : 0.45
        attachButton.toolTip = "Attach documents"
    }

    private func setDropHighlight(_ active: Bool) {
        guard dropHighlightActive != active else { return }
        dropHighlightActive = active
        feed.layer?.borderColor = active
            ? themedAccentBorderColor.withAlphaComponent(lightThemeEnabled ? 0.86 : 0.72).cgColor
            : themedAccentBorderColor.withAlphaComponent(lightThemeEnabled ? 0.34 : 0.16).cgColor
        feed.layer?.shadowColor = themedAccentBorderColor.cgColor
        feed.layer?.shadowOpacity = active ? 0.22 : 0
        feed.layer?.shadowRadius = active ? 18 : 0
        feed.layer?.shadowOffset = .zero
        if active {
            showSystemToast(for: RenderedCard(
                id: "drop-documents-\(UUID().uuidString)",
                kind: "system",
                title: "Drop documents",
                body: "Release to attach them to this Bluey session.",
                done: true,
                costLabel: nil,
                artifact: nil,
                attachments: []))
        }
    }

    @objc private func removeAttachmentClicked(_ sender: RemoveAttachmentButton) {
        let id = sender.contextId.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !id.isEmpty else { return }
        setKnowledgeBadge("Docs updating", accent: BlueyTheme.warning)
        emitRemoveContext(id: id)
    }

    @objc private func clearTranscriptClicked() {
        clearLocalTranscriptContext(replacementText: "Captions cleared")
        emitSimple("transcript_clear_requested")
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
        applyAccentInsertionPoint(to: answerStyleBox)
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.12
            answerStyleOverlay.animator().alphaValue = 1
        }
    }

    private func applyAccentInsertionPoint(to control: NSControl) {
        if let editor = control.currentEditor() as? NSTextView {
            editor.insertionPointColor = themedAccentColor
        }
    }

    func focusComposerForQuestion() {
        dismissAnswerStyleEditor(animated: false)
        dismissCloseConfirm(animated: false)
        composer.placeholder = recordingActive
            ? "Ask while Bluey listens..."
            : "Ask anything..."
        window?.makeFirstResponder(composer)
        composer.armTypingCaret()
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
            autoSendModeMenu,
            fullSizeButton,
            recordingButton,
            askButton,
            analyzeButton,
            transcriptClearButton,
            attachButton,
            instructionsButton,
            opacityControl,
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

    private func setHeaderSubtitle(_ text: String = "") {
        let clean = text.trimmingCharacters(in: .whitespacesAndNewlines)
        statusLabel.stringValue = clean
        statusLabel.isHidden = clean.isEmpty
        layoutHeaderChromeControls()
    }

    func showSignedOutLogin(url: URL?) {
        setHeaderSubtitle("Local ready")
        routeBadge.stringValue = "Sign in"
        routeBadge.textColor = BlueyTheme.warning
        routeBadge.toolTip = "Sign in to use managed answers and balance"
        routeBadge.layer?.borderColor = NSColor.clear.cgColor
        routeBadge.layer?.backgroundColor = NSColor.clear.cgColor
        balanceLabel.stringValue = "Login"
        setKnowledgeBadge("Docs locked", accent: BlueyTheme.textDim)
        composer.placeholder = url == nil ? "Sign in to use managed answers..." : "Sign in, then ask anything..."
        statusLabel.toolTip = "Cloud answers, balance, sync, and documents unlock after login"
    }

    func showSignedInReady() {
        setHeaderSubtitle()
        statusLabel.toolTip = nil
        routeBadge.stringValue = "● Ready"
        routeBadge.textColor = BlueyTheme.green
        routeBadge.toolTip = "Ready"
        routeBadge.layer?.borderColor = NSColor.clear.cgColor
        routeBadge.layer?.backgroundColor = NSColor.clear.cgColor
        if balanceLabel.stringValue == "Login" {
            balanceLabel.stringValue = "Balance --"
        }
        setKnowledgeBadge("Docs empty", accent: BlueyTheme.textDim)
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
        let clean = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let lower = clean.lowercased()
        if clean.isEmpty || lower.contains("empty") || lower.contains("locked") {
            knowledgeBadgeContentVisible = false
            knowledgeBadge.stringValue = ""
            knowledgeBadge.isHidden = true
            knowledgeBadge.toolTip = nil
            layoutHeaderChromeControls()
            return
        }
        knowledgeBadgeContentVisible = true
        knowledgeBadge.stringValue = clean
        knowledgeBadge.textColor = accent
        knowledgeBadge.layer?.borderColor = NSColor.clear.cgColor
        knowledgeBadge.layer?.backgroundColor = NSColor.clear.cgColor
        knowledgeBadge.toolTip = contextItems.isEmpty
            ? "Attached document status"
            : savedContextBadgeTooltip(for: contextItems.count, showing: showingSavedContextItems)
        layoutHeaderChromeControls()
    }

    private func startKnowledgeIndexing() {
        knowledgeIndexTimer?.invalidate()
        knowledgeIndexSafetyWorkItem?.cancel()
        knowledgeBadgeContentVisible = true
        knowledgeBadge.isHidden = false
        knowledgeIndexFrame = 0
        applyKnowledgeIndexFrame()
        knowledgeIndexTimer = Timer.scheduledTimer(withTimeInterval: 0.42, repeats: true) { [weak self] _ in
            guard let self else { return }
            self.knowledgeIndexFrame = (self.knowledgeIndexFrame + 1) % self.knowledgeIndexFrames.count
            self.applyKnowledgeIndexFrame()
        }
        let workItem = DispatchWorkItem { [weak self] in
            self?.finishKnowledgeIndexingIfStale()
        }
        knowledgeIndexSafetyWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + 4.0, execute: workItem)
    }

    private func applyKnowledgeIndexFrame() {
        let frame = knowledgeIndexFrames[knowledgeIndexFrame % knowledgeIndexFrames.count]
        knowledgeBadge.stringValue = frame
        knowledgeBadge.textColor = BlueyTheme.green
        knowledgeBadge.layer?.borderColor = NSColor.clear.cgColor
        knowledgeBadge.layer?.backgroundColor = NSColor.clear.cgColor
        knowledgeBadge.toolTip = "Indexing attached documents"
        knowledgeBadgeContentVisible = true
        layoutHeaderChromeControls()
    }

    private func stopKnowledgeIndexing() {
        knowledgeIndexTimer?.invalidate()
        knowledgeIndexTimer = nil
        knowledgeIndexSafetyWorkItem?.cancel()
        knowledgeIndexSafetyWorkItem = nil
        knowledgeBadge.toolTip = "Show or hide attached files"
    }

    private func finishKnowledgeIndexingIfStale() {
        guard knowledgeIndexTimer != nil else { return }
        knowledgeIndexTimer?.invalidate()
        knowledgeIndexTimer = nil
        knowledgeIndexSafetyWorkItem = nil

        if hasVisibleContextAttachments {
            knowledgeBadgeContentVisible = true
            knowledgeBadge.isHidden = false
            knowledgeBadge.stringValue = savedContextBadgeTitle(for: contextItems.count, showing: showingSavedContextItems)
            knowledgeBadge.textColor = BlueyTheme.green
            knowledgeBadge.toolTip = savedContextBadgeTooltip(for: contextItems.count, showing: showingSavedContextItems)
        } else {
            knowledgeBadgeContentVisible = false
            knowledgeBadge.stringValue = ""
            knowledgeBadge.isHidden = true
            knowledgeBadge.toolTip = nil
            renderAttachmentStrip([])
        }
        layoutHeaderChromeControls()
        layoutSubtreeIfNeeded()
    }

    private func showKnowledgePlaceholder(_ text: String) {
        for view in attachmentStack.arrangedSubviews {
            attachmentStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        attachmentStrip.isHidden = false
        attachmentStripHeightConstraint?.constant = 28

        let chip = NSTextField(labelWithString: text)
        chip.translatesAutoresizingMaskIntoConstraints = false
        chip.font = NSFont.systemFont(ofSize: 10.5, weight: .semibold)
        chip.textColor = BlueyTheme.textDim
        chip.alignment = .center
        chip.lineBreakMode = .byTruncatingTail
        chip.wantsLayer = true
        chip.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.035).cgColor
        chip.layer?.cornerRadius = 11
        chip.layer?.borderWidth = 1
        chip.layer?.borderColor = BlueyTheme.hairline.cgColor
        chip.toolTip = text
        attachmentStack.addArrangedSubview(chip)
        NSLayoutConstraint.activate([
            chip.heightAnchor.constraint(equalToConstant: 22),
            chip.widthAnchor.constraint(greaterThanOrEqualToConstant: 132),
            chip.widthAnchor.constraint(lessThanOrEqualToConstant: 220),
        ])
        layoutSubtreeIfNeeded()
    }

    private func setTranscriptState(_ text: String, active: Bool) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let upper = trimmed.uppercased()
        let display = upper == "TRANSCRIBING" ? "LIVE" : upper
        transcriptStateLabel.stringValue = display
        transcriptStateLabel.textColor = active ? BlueyTheme.green : themedDimTextColor
        transcriptActivityDot.layer?.backgroundColor = (active
            ? BlueyTheme.green
            : themedDimTextColor.withAlphaComponent(0.55)).cgColor
        transcriptActivityDot.layer?.shadowOpacity = active ? 0.45 : 0
        transcriptStrip.layer?.borderColor = (active
            ? BlueyTheme.green.withAlphaComponent(0.26)
            : (lightThemeEnabled ? BlueyLightTheme.border : BlueyTheme.hairline)).cgColor
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
            transcriptStrip.layer?.backgroundColor = (lightThemeEnabled
                ? BlueyLightTheme.surfaceRaised.withAlphaComponent(lightMaterialAlpha(0.78, floor: 0.16))
                : NSColor.black.withAlphaComponent(0.16)).cgColor
            transcriptStrip.layer?.borderColor = (lightThemeEnabled
                ? BlueyLightTheme.border
                : BlueyTheme.hairline).cgColor
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
            recordingDesiredActive = true
            recordingTransitionInFlight = true
            lastTranscriptStripSource = nil
            recordingButton.title = "Starting"
            setHeaderSubtitle()
            composer.placeholder = "Connecting audio..."
            updateAudioRouteBadge("● Starting", accent: BlueyTheme.green)
            styleControlButton(recordingButton, symbol: "waveform", accent: false)
            setTranscriptState("STARTING", active: true)
            seedTranscriptPreviewIfEmpty("Mic + System: starting audio...")
        case .listening:
            recordingActive = true
            recordingDesiredActive = true
            recordingTransitionInFlight = false
            lastTranscriptStripSource = nil
            recordingButton.title = "Stop"
            setHeaderSubtitle()
            composer.placeholder = "Listening... type a follow-up anytime"
            updateAudioRouteBadge("● Listening", accent: BlueyTheme.green)
            styleControlButton(recordingButton, symbol: "stop.fill", accent: true)
            setTranscriptState("LISTENING", active: true)
            seedTranscriptPreviewIfEmpty("Mic + System: captions appear here.")
        case .paused:
            recordingActive = false
            recordingDesiredActive = false
            recordingTransitionInFlight = false
            lastTranscriptStripSource = nil
            recordingButton.title = "Listen"
            setHeaderSubtitle()
            composer.placeholder = "Ask anything..."
            updateAudioRouteBadge("● Ready", accent: BlueyTheme.green)
            styleControlButton(recordingButton, symbol: "waveform", accent: false)
            setTranscriptState("READY", active: false)
            seedTranscriptPreviewIfEmpty("Live captions preview")
        case .failed:
            recordingActive = false
            recordingDesiredActive = false
            recordingTransitionInFlight = false
            autoSendAfterStopWorkItem?.cancel()
            autoSendAfterStopWorkItem = nil
            autoSendListenCaptureActive = false
            autoSendTranscriptLinesBySource.removeAll()
            lastTranscriptStripSource = nil
            recordingButton.title = "Listen"
            setHeaderSubtitle("Audio issue")
            composer.placeholder = "Ask anything..."
            updateAudioRouteBadge("● Audio", accent: BlueyTheme.warning)
            styleControlButton(recordingButton, symbol: "waveform", accent: false)
            setTranscriptState("FAILED", active: false)
        case .ready:
            recordingActive = false
            recordingDesiredActive = false
            recordingTransitionInFlight = false
            autoSendAfterStopWorkItem?.cancel()
            autoSendAfterStopWorkItem = nil
            autoSendListenCaptureActive = false
            autoSendTranscriptLinesBySource.removeAll()
            lastTranscriptStripSource = nil
            recordingButton.title = "Listen"
            setHeaderSubtitle()
            composer.placeholder = "Ask anything..."
            updateAudioRouteBadge("● Ready", accent: BlueyTheme.green)
            styleControlButton(recordingButton, symbol: "waveform", accent: false)
            setTranscriptState("READY", active: false)
            seedTranscriptPreviewIfEmpty("Live captions preview")
        }
    }

    func scheduleAutoSendAfterExternalStop() {
        scheduleAutoSendAfterExplicitStop()
    }

    private func updateAudioRouteBadge(_ text: String, accent: NSColor) {
        routeBadge.stringValue = text
        routeBadge.textColor = accent
        routeBadge.toolTip = text.replacingOccurrences(of: "● ", with: "")
        routeBadge.layer?.borderColor = NSColor.clear.cgColor
        routeBadge.layer?.backgroundColor = NSColor.clear.cgColor
    }

    private func updateRouteBadge(
        for question: String,
        selectedRoute: (provider: String?, model: String?, mode: String?)
    ) {
        let manual = (selectedRoute.provider ?? "auto").lowercased() != "auto"
        if manual {
            switch (selectedRoute.mode ?? selectedRoute.model ?? selectedRoute.provider ?? "Manual").lowercased() {
            case let value where value.contains("instant"):
                routeBadge.stringValue = "Instant"
            case let value where value.contains("deep"):
                routeBadge.stringValue = "Deep"
            case let value where value.contains("balanced"):
                routeBadge.stringValue = "Balanced"
            default:
                routeBadge.stringValue = "Manual"
            }
            routeBadge.textColor = BlueyTheme.text
            routeBadge.toolTip = "Selected answer lane"
            return
        }

        let lower = question.lowercased()
        let vision = lower.contains("screen") || lower.contains("screenshot") || lower.contains("image")
        if vision {
            routeBadge.stringValue = "Auto · Vision"
            routeBadge.textColor = BlueyTheme.warning
            routeBadge.toolTip = "Bluey is using the screen/image lane"
        } else {
            routeBadge.stringValue = "Auto · Balanced"
            routeBadge.textColor = themedAccentColor
            routeBadge.toolTip = "Bluey is answering with the balanced auto lane"
        }
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
        endAttachPickerHandoff()
        let previousIds = Set(contextItems.map(\.id))
        let currentIds = Set(items.map(\.id))
        let newlyAddedIds = currentIds.subtracting(previousIds)
        let mutationExpected = isExpectingContextMutationForPendingSend()
        contextItems = items
        hasVisibleContextAttachments = !items.isEmpty
        pendingContextItemIds.formIntersection(currentIds)
        if !newlyAddedIds.isEmpty {
            if mutationExpected {
                pendingContextItemIds.formUnion(newlyAddedIds)
            }
            showingSavedContextItems = true
        } else if pendingContextItemIds.isEmpty {
            showingSavedContextItems = true
        }
        guard !items.isEmpty else {
            pendingContextItemIds.removeAll()
            showingSavedContextItems = false
            setKnowledgeBadge("Docs empty", accent: BlueyTheme.textDim)
            renderAttachmentStrip([])
            refreshAttachmentStripLayout()
            return
        }

        setKnowledgeBadge(savedContextBadgeTitle(for: items.count, showing: showingSavedContextItems), accent: BlueyTheme.green)
        renderAttachmentStrip(itemsForVisibleAttachmentStrip())
        refreshAttachmentStripLayout()
    }

    private func expectContextMutationForPendingSend() {
        contextMutationExpectedUntil = Date().addingTimeInterval(45)
    }

    private func isExpectingContextMutationForPendingSend() -> Bool {
        guard let deadline = contextMutationExpectedUntil else { return false }
        if deadline >= Date() {
            return true
        }
        contextMutationExpectedUntil = nil
        return false
    }

    private func toggleSavedContextItems() {
        guard !contextItems.isEmpty else { return }
        showingSavedContextItems.toggle()
        setKnowledgeBadge(savedContextBadgeTitle(for: contextItems.count, showing: showingSavedContextItems), accent: BlueyTheme.green)
        renderAttachmentStrip(itemsForVisibleAttachmentStrip())
        refreshAttachmentStripLayout()
    }

    private func refreshAttachmentStripLayout() {
        needsLayout = true
        layoutSubtreeIfNeeded()
        keepFixedChromeInBounds()
        layoutSubtreeIfNeeded()
    }

    private func savedContextBadgeTitle(for count: Int, showing: Bool = false) -> String {
        let noun = count == 1 ? "file" : "files"
        return showing ? "Hide \(count) \(noun)" : "Show \(count) \(noun)"
    }

    private func savedContextBadgeTooltip(for count: Int, showing: Bool = false) -> String {
        let noun = count == 1 ? "file" : "files"
        return showing
            ? "Hide the \(count) \(noun) attached to this conversation"
            : "Show every file attached to this conversation"
    }

    private func itemsForVisibleAttachmentStrip() -> [OverlayContextItem] {
        if showingSavedContextItems {
            return contextItems
        }
        return contextItems.filter { pendingContextItemIds.contains($0.id) }
    }

    private func isScreenContextItem(_ item: OverlayContextItem) -> Bool {
        item.kind.caseInsensitiveCompare("image") == .orderedSame
            && item.title.localizedCaseInsensitiveContains("screen")
    }

    private func renderAttachmentStrip(_ items: [OverlayContextItem]) {
        for view in attachmentStack.arrangedSubviews {
            attachmentStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }

        guard !items.isEmpty else {
            attachmentStrip.isHidden = true
            attachmentStripHeightConstraint?.constant = 0
            attachmentStrip.toolTip = "Attached documents and images"
            return
        }

        attachmentStrip.isHidden = false
        attachmentStrip.toolTip = showingSavedContextItems
            ? "All files in this conversation. Scroll horizontally to see more."
            : "Files that will be sent with the next answer. Scroll horizontally to see more."
        attachmentStripHeightConstraint?.constant = 34
        for item in items {
            attachmentStack.addArrangedSubview(makeAttachmentChip(item))
        }
        updateAttachmentStripDocumentWidth()
        attachmentStrip.contentView.scroll(to: .zero)
        attachmentStrip.reflectScrolledClipView(attachmentStrip.contentView)
    }

    private func updateAttachmentStripDocumentWidth() {
        attachmentStack.layoutSubtreeIfNeeded()
        let fitting = attachmentStack.fittingSize
        let viewportWidth = max(0, attachmentStrip.contentView.bounds.width)
        attachmentStack.frame = NSRect(
            x: 0,
            y: 0,
            width: max(viewportWidth + 1, fitting.width),
            height: max(attachmentStrip.bounds.height, fitting.height)
        )
    }

    private func consumeSentPendingContextAttachments() {
        guard !pendingContextItemIds.isEmpty || showingSavedContextItems else { return }
        pendingContextItemIds.removeAll()
        showingSavedContextItems = false
        contextMutationExpectedUntil = nil
        if !contextItems.isEmpty {
            showingSavedContextItems = true
            setKnowledgeBadge(
                savedContextBadgeTitle(for: contextItems.count, showing: showingSavedContextItems),
                accent: BlueyTheme.green)
            renderAttachmentStrip(itemsForVisibleAttachmentStrip())
            refreshAttachmentStripLayout()
            return
        }
        renderAttachmentStrip([])
    }

    func setSessions(_ sessions: [OverlaySessionItem]) {
        sessionItems = sessions
        sessionsHaveLoaded = true
        renameField = nil
        updateSessionDrawerGeometry(layoutWidth: bounds.width, layoutHeight: bounds.height)
        for view in sessionStack.arrangedSubviews {
            sessionStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }

        if sessions.isEmpty {
            emitLifecycle("session_drawer_sessions_rendered", detail: "session_count=0 active_count=0 context_total=0 image_total=0")
            renderSessionDrawerMessage("No local saved recordings found. Synced sessions live on the web dashboard.")
            return
        }

        let activeCount = sessions.filter { $0.isActive }.count
        let contextTotal = sessions.reduce(0) { $0 + $1.contextCount }
        let imageTotal = sessions.reduce(0) { $0 + $1.imageCount }
        emitLifecycle(
            "session_drawer_sessions_rendered",
            detail: "session_count=\(sessions.count) active_count=\(activeCount) context_total=\(contextTotal) image_total=\(imageTotal)"
        )
        for session in sessions {
            let row = makeSessionRow(session)
            sessionStack.addArrangedSubview(row)
            row.widthAnchor.constraint(equalTo: sessionStack.widthAnchor, constant: -2).isActive = true
        }
        updateSessionDrawerGeometry(layoutWidth: bounds.width, layoutHeight: bounds.height)
    }

    private func renderSessionDrawerMessage(_ message: String) {
        renameField = nil
        for view in sessionStack.arrangedSubviews {
            sessionStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        let label = NSTextField(wrappingLabelWithString: message)
        label.font = NSFont.systemFont(ofSize: 11.5, weight: .medium)
        label.textColor = BlueyTheme.textDim
        label.alignment = .center
        label.translatesAutoresizingMaskIntoConstraints = false
        sessionStack.addArrangedSubview(label)
        label.widthAnchor.constraint(equalTo: sessionStack.widthAnchor, constant: -20).isActive = true
    }

    private func updateSessionDrawerGeometry(layoutWidth: CGFloat, layoutHeight: CGFloat) {
        guard layoutWidth > 0, layoutHeight > 0 else { return }
        let sideInset: CGFloat = 18
        let headerClearance = ChromeMetrics.headerTopInset + ChromeMetrics.headerBarHeight + 14
        let bottomClearance = (composerBarHeightConstraint?.constant ?? ChromeMetrics.composerBaseHeight)
            + ChromeMetrics.transcriptStripHeight
            + 44
        let availableWidth = max(260, layoutWidth - sideInset * 2)
        let availableHeight = max(190, layoutHeight - headerClearance - bottomClearance)
        let visibleRows = CGFloat(min(max(sessionItems.count, 1), 6))
        let desiredHeight = 104 + visibleRows * 58
        let drawerWidth = min(380, max(310, min(availableWidth, layoutWidth * 0.36)))
        let drawerHeight = min(availableHeight, max(220, desiredHeight))

        sessionDrawerTopConstraint?.constant = headerClearance
        sessionDrawerLeadingConstraint?.constant = sideInset
        sessionDrawerWidthConstraint?.constant = drawerWidth
        sessionDrawerHeightConstraint?.constant = drawerHeight
    }

    func resetSessionSurface() {
        autoSendAfterStopWorkItem?.cancel()
        autoSendAfterStopWorkItem = nil
        feed.clear()
        hideSystemToast(immediately: true)
        setContextItems([])
        transcriptSnippets.removeAll()
        latestLiveTranscriptLine = nil
        latestLiveTranscriptLinesBySource.removeAll()
        autoSendListenCaptureActive = false
        autoSendTranscriptLinesBySource.removeAll()
        autoSendAfterStopWorkItem?.cancel()
        autoSendAfterStopWorkItem = nil
        lastSubmittedAskFingerprint = nil
        lastSubmittedAskAt = 0
        liveTranscriptPreviewBodies.removeAll()
        consumedTranscriptFingerprints.removeAll()
        lastTranscriptStripSource = nil
        updateTranscriptStripText("Live captions preview", scrollToEnd: false)
        updateTranscriptClearButtonVisibility()
        setTranscriptState("READY", active: false)
        routeBadge.stringValue = "● Ready"
        routeBadge.textColor = BlueyTheme.green
        canvases.removeAll()
        activeCanvasIndex = nil
        canvasCardAssignments.removeAll()
        canvasPane.setNavigation(index: 0, total: 0)
        setCanvasOpen(false)
        canvasToggleButton.isHidden = true
    }

    func pushCard(_ card: RenderedCard) {
        if shouldRenderAsToast(card) {
            showSystemToast(for: card)
            emitCardRendered(id: card.id)
            return
        }
        trackAnswerStreamPush(card)
        feed.push(card)
        routeCanvasIfNeeded(card)
    }

    func updateCard(id: String, body: String, done: Bool, costLabel: String?, artifact: OverlayArtifact?) {
        guard let card = feed.update(id: id, body: body, done: done, costLabel: costLabel, artifact: artifact) else {
            return
        }
        trackAnswerStreamUpdate(card)
        if let artifact {
            let question = feed.nearestQuestionBody(beforeCardId: id)
            let artifactKind = CanvasKind.fromArtifactType(artifact.artifactType)
            if shouldPreserveCanvasForExplanatoryFollowup(question: question, artifactKind: artifactKind) {
                if !done {
                    setHeaderSubtitle("Answer streaming")
                } else {
                    routeBadge.stringValue = "● Ready"
                    routeBadge.textColor = BlueyTheme.green
                    routeBadge.toolTip = "Ready"
                }
            } else {
                routeBadge.stringValue = routeBadgeText(for: artifact)
                routeBadge.toolTip = "Answer opened a \(routeBadgeText(for: artifact).lowercased()) workbench"
            }
        } else if !done {
            setHeaderSubtitle("Answer streaming")
        } else {
            routeBadge.stringValue = "● Ready"
            routeBadge.textColor = BlueyTheme.green
            routeBadge.toolTip = "Ready"
            if card.kind == "answer" {
                let question = feed.nearestQuestionBody(beforeCardId: id)
                if shouldCloseCanvasForPlainAnswer(question: question) {
                    setCanvasOpen(false)
                }
            }
        }
        routeCanvasIfNeeded(card)
    }

    private func trackAnswerStreamPush(_ card: RenderedCard) {
        guard normalizedCardKind(card.kind) == "answer", !card.done else { return }
        answerStreamStats[card.id] = AnswerStreamStats(
            startedAt: CACurrentMediaTime(),
            firstUpdateAt: card.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : CACurrentMediaTime(),
            lastBodyChars: card.body.count)
    }

    private func trackAnswerStreamUpdate(_ card: RenderedCard) {
        guard normalizedCardKind(card.kind) == "answer" else { return }
        let now = CACurrentMediaTime()
        var stats = answerStreamStats[card.id] ?? AnswerStreamStats(startedAt: now)
        let bodyChars = card.body.count
        let artifactType = card.artifact?.artifactType ?? "none"
        if stats.firstUpdateAt == nil, bodyChars > 0 {
            stats.firstUpdateAt = now
            emitLifecycle(
                "answer_stream_first_update",
                detail: "card_id=\(card.id) first_update_ms=\(Int((now - stats.startedAt) * 1000)) body_chars=\(bodyChars) artifact=\(artifactType)"
            )
        }
        stats.lastBodyChars = bodyChars
        if card.done {
            let firstUpdateMs = stats.firstUpdateAt.map { Int(($0 - stats.startedAt) * 1000) } ?? -1
            emitLifecycle(
                "answer_stream_finished",
                detail: "card_id=\(card.id) first_update_ms=\(firstUpdateMs) total_ms=\(Int((now - stats.startedAt) * 1000)) body_chars=\(bodyChars) artifact=\(artifactType) cost_label=\(card.costLabel == nil ? "none" : "present")"
            )
            answerStreamStats.removeValue(forKey: card.id)
        } else {
            answerStreamStats[card.id] = stats
        }
    }

    private func shouldRenderAsToast(_ card: RenderedCard) -> Bool {
        let kind = normalizedCardKind(card.kind)
        guard (kind == "system" || kind == "warning" || kind == "context"), actionableLoginURL(from: card) == nil else {
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

    private func showSystemToast(title: String, body: String, duration: TimeInterval) {
        toastHideWorkItem?.cancel()
        toastTitleLabel.stringValue = title
        toastBodyLabel.stringValue = systemToastBody(body)
        setHeaderSubtitle()

        toastView.isHidden = false
        toastView.animator().alphaValue = 1

        let work = DispatchWorkItem { [weak self] in
            self?.hideSystemToast(immediately: false)
        }
        toastHideWorkItem = work
        DispatchQueue.main.asyncAfter(deadline: .now() + duration, execute: work)
    }

    private func showSystemToast(for card: RenderedCard) {
        toastHideWorkItem?.cancel()
        let title = card.title.trimmingCharacters(in: .whitespacesAndNewlines)
        let body = systemToastBody(card.body)
        toastTitleLabel.stringValue = title.isEmpty ? "Bluey" : title
        toastBodyLabel.stringValue = body
        setHeaderSubtitle()

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
        let question = feed.nearestQuestionBody(beforeCardId: card.id)
        guard var artifact = makeCanvasArtifact(from: card) else {
            if card.done, card.kind == "answer", canvasOpen, activeCanvasIndex != nil {
                let shouldClose = shouldCloseCanvasForPlainAnswer(question: question)
                emitLifecycle(
                    shouldClose ? "canvas_close_plain_answer" : "canvas_preserve_plain_answer",
                    detail: canvasLogDetail(card: card, question: question))
                if shouldClose {
                    setCanvasOpen(false)
                }
            }
            return
        }
        if !card.done && card.artifact == nil {
            emitLifecycle("canvas_waiting_for_artifact", detail: canvasLogDetail(card: card, question: question))
            return
        }
        if shouldPreserveCanvasForExplanatoryFollowup(question: question, artifact: artifact) {
            emitLifecycle(
                "canvas_preserve_followup",
                detail: canvasLogDetail(card: card, question: question, artifact: artifact))
            if canvasOpen {
                renderActiveCanvas()
            }
            return
        }
        registerCanvasArtifact(&artifact, question: question)
        canvasToggleButton.isHidden = false
        if shouldAutoOpenCanvas(for: card, artifact: artifact) {
            setCanvasOpen(true)
        } else if canvasOpen {
            renderActiveCanvas()
        }
    }

    private func canvasLogDetail(
        card: RenderedCard,
        question: String?,
        artifact: CanvasArtifact? = nil
    ) -> String {
        var parts = [
            "card=\(card.id)",
            "kind=\(card.kind)",
            "done=\(card.done)",
            "question_chars=\((question ?? "").count)",
            "question_words=\(wordCount(question))",
            "question_intent=\(questionIntentLabel(question))",
            "body_chars=\(card.body.count)",
            "body_lines=\(card.body.components(separatedBy: .newlines).count)",
        ]
        if let artifact {
            parts.append("artifact_kind=\(artifact.kind.shortTitle)")
            parts.append("artifact_title_chars=\(artifact.title.count)")
            parts.append("artifact_body_chars=\(artifact.content.count)")
        }
        return parts.joined(separator: " ")
    }

    private func wordCount(_ text: String?) -> Int {
        (text ?? "")
            .split(whereSeparator: { $0.isWhitespace })
            .count
    }

    private func questionIntentLabel(_ text: String?) -> String {
        let lower = (text ?? "").lowercased()
        let codeSignals = ["code", "build", "implement", "function", "class", "api", "algorithm", "cache", "sql", "bug", "error"]
        let explainSignals = ["explain", "logic", "why", "how does", "how it works", "walk me", "understand"]
        let designSignals = ["system design", "architecture", "scale", "design "]
        let hasCode = codeSignals.contains { lower.contains($0) }
        let hasExplain = explainSignals.contains { lower.contains($0) }
        let hasDesign = designSignals.contains { lower.contains($0) }
        if hasCode && hasExplain { return "code_explanation" }
        if hasCode { return "code_or_debug" }
        if hasDesign { return "system_design" }
        if hasExplain { return "explanation" }
        if wordCount(text) <= 6 { return "short_query" }
        return "general"
    }

    private func registerCanvasArtifact(_ artifact: inout CanvasArtifact, question: String?) {
        if let existingIndex = canvasCardAssignments[artifact.sourceCardId],
           canvases.indices.contains(existingIndex) {
            let existing = canvases[existingIndex]
            artifact.title = existing.title
            artifact.subtitle = canvasSubtitle(base: artifact.subtitle, followups: existing.followupCount)
            artifact.followupCount = existing.followupCount
            artifact.sourceQuestion = existing.sourceQuestion
            canvases[existingIndex] = artifact
            activeCanvasIndex = existingIndex
            renderActiveCanvas()
            emitLifecycle(
                "canvas_replace",
                detail: "source_card=\(artifact.sourceCardId) index=\(existingIndex) kind=\(artifact.kind.shortTitle) title_chars=\(artifact.title.count) body_chars=\(artifact.content.count)")
            return
        }

        if shouldAppendCanvasFollowup(question: question, artifact: artifact),
           let index = activeCanvasIndex,
           canvases.indices.contains(index) {
            let followupNumber = canvases[index].followupCount + 1
            canvases[index].followupCount = followupNumber
            canvases[index].subtitle = canvasSubtitle(
                base: canvases[index].kind.subtitle,
                followups: followupNumber)
            canvases[index].content = appendCanvasFollowup(
                to: canvases[index].content,
                question: question,
                artifact: artifact,
                number: followupNumber)
            canvasCardAssignments[artifact.sourceCardId] = index
            activeCanvasIndex = index
            renderActiveCanvas()
            emitLifecycle(
                "canvas_append_followup",
                detail: "source_card=\(artifact.sourceCardId) index=\(index) followups=\(followupNumber) kind=\(artifact.kind.shortTitle) question_chars=\((question ?? "").count) question_words=\(wordCount(question)) question_intent=\(questionIntentLabel(question)) body_chars=\(artifact.content.count)")
            return
        }

        let questionNumber = canvases.count + 1
        artifact.title = "Q\(questionNumber) \(artifact.kind.shortTitle)"
        artifact.subtitle = canvasSubtitle(base: artifact.subtitle, followups: 0)
        artifact.sourceQuestion = question
        canvases.append(artifact)
        activeCanvasIndex = canvases.count - 1
        canvasCardAssignments[artifact.sourceCardId] = canvases.count - 1
        renderActiveCanvas()
        emitLifecycle(
            "canvas_new",
            detail: "source_card=\(artifact.sourceCardId) index=\(canvases.count - 1) kind=\(artifact.kind.shortTitle) title_chars=\(artifact.title.count) body_chars=\(artifact.content.count)")
    }

    private func renderActiveCanvas() {
        guard !canvases.isEmpty else {
            canvasPane.setNavigation(index: 0, total: 0)
            return
        }
        let index = min(max(activeCanvasIndex ?? canvases.count - 1, 0), canvases.count - 1)
        activeCanvasIndex = index
        canvasPane.render(canvases[index])
        canvasPane.setNavigation(index: index, total: canvases.count)
    }

    private func openCanvasForCardId(_ cardId: String) {
        if let index = canvasCardAssignments[cardId],
           canvases.indices.contains(index) {
            activeCanvasIndex = index
        }
        renderActiveCanvas()
        setCanvasOpen(true)
        emitLifecycle(
            "canvas_open_from_card",
            detail: "source_card=\(cardId) index=\(activeCanvasIndex ?? -1) count=\(canvases.count)")
    }

    private func showPreviousCanvas() {
        guard let index = activeCanvasIndex, index > 0 else { return }
        activeCanvasIndex = index - 1
        renderActiveCanvas()
        if !canvasOpen {
            setCanvasOpen(true)
        }
    }

    private func showNextCanvas() {
        guard let index = activeCanvasIndex, index < canvases.count - 1 else { return }
        activeCanvasIndex = index + 1
        renderActiveCanvas()
        if !canvasOpen {
            setCanvasOpen(true)
        }
    }

    private func canvasSubtitle(base: String, followups: Int) -> String {
        guard followups > 0 else { return base }
        let suffix = followups == 1 ? "1 follow-up" : "\(followups) follow-ups"
        return "\(base) · \(suffix)"
    }

    private func shouldCloseCanvasForPlainAnswer(question: String?) -> Bool {
        guard
            canvasOpen,
            let index = activeCanvasIndex,
            canvases.indices.contains(index)
        else { return false }
        let lower = question?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased() ?? ""
        guard !lower.isEmpty else { return true }
        if isCanvasRelatedFollowup(
            lower,
            artifactKind: canvases[index].kind,
            sourceQuestion: canvases[index].sourceQuestion)
        {
            return false
        }
        return true
    }

    private func shouldAppendCanvasFollowup(question: String?, artifact: CanvasArtifact) -> Bool {
        guard
            let index = activeCanvasIndex,
            canvases.indices.contains(index),
            canvases[index].kind == artifact.kind,
            let question = question?.trimmingCharacters(in: .whitespacesAndNewlines),
            !question.isEmpty
        else {
            return false
        }
        let lower = question.lowercased()
        if looksLikeNewCanvasQuestion(lower) {
            return false
        }
        if looksLikeCanvasExplanationFollowup(lower) && !looksLikeCanvasMutationFollowup(lower) {
            return false
        }
        return looksLikeCanvasMutationFollowup(lower)
    }

    private func shouldPreserveCanvasForExplanatoryFollowup(
        question: String?,
        artifact: CanvasArtifact
    ) -> Bool {
        shouldPreserveCanvasForExplanatoryFollowup(question: question, artifactKind: artifact.kind)
    }

    private func shouldPreserveCanvasForExplanatoryFollowup(
        question: String?,
        artifactKind: CanvasKind
    ) -> Bool {
        guard
            let index = activeCanvasIndex,
            canvases.indices.contains(index),
            canvases[index].kind == artifactKind,
            let question = question?.trimmingCharacters(in: .whitespacesAndNewlines),
            !question.isEmpty
        else {
            return false
        }
        let lower = question.lowercased()
        if looksLikeNewCanvasQuestion(lower) || looksLikeCanvasMutationFollowup(lower) {
            return false
        }
        return isCanvasRelatedFollowup(
            lower,
            artifactKind: artifactKind,
            sourceQuestion: canvases[index].sourceQuestion)
    }

    private func isCanvasRelatedFollowup(
        _ lower: String,
        artifactKind: CanvasKind,
        sourceQuestion: String?
    ) -> Bool {
        if looksLikeNewCanvasQuestion(lower) {
            return false
        }
        if looksLikeDirectCanvasReference(lower) {
            return true
        }
        if sharesCanvasQuestionTerm(lower, sourceQuestion: sourceQuestion) {
            return true
        }
        switch artifactKind {
        case .code:
            return looksLikeCodeCanvasFollowup(lower)
        case .systemDesign:
            return looksLikeSystemDesignCanvasFollowup(lower)
        case .screen:
            return lower.contains("screenshot")
                || lower.contains("screen")
                || lower.contains("image")
                || lower.contains("visible")
                || lower.contains("shown")
                || lower.contains("expected output")
        case .document:
            return lower.contains("document")
                || lower.contains("resume")
                || lower.contains("file")
                || lower.contains("attached")
        case .structured:
            return false
        }
    }

    private func looksLikeDirectCanvasReference(_ lower: String) -> Bool {
        let directSignals = [
            "this code",
            "that code",
            "same code",
            "above code",
            "current code",
            "previous code",
            "existing code",
            "this solution",
            "that solution",
            "same solution",
            "above solution",
            "current solution",
            "previous solution",
            "this approach",
            "that approach",
            "same approach",
            "above approach",
            "current approach",
            "previous approach",
            "this design",
            "that design",
            "same design",
            "above design",
            "current design",
            "previous design",
            "canvas",
            "workbench",
            "why did you use",
            "why are you using",
            "why can't we do",
            "why cant we do",
            "walk me through this",
            "explain this",
            "explain that",
            "make it",
            "change it",
        ]
        return directSignals.contains { lower.contains($0) }
    }

    private func looksLikeCodeCanvasFollowup(_ lower: String) -> Bool {
        let codeSignals = [
            "algorithm",
            "complexity",
            "runtime",
            "space",
            "edge case",
            "test case",
            "failing test",
            "unit test",
            "pointer",
            "pointers",
            "vector",
            "array",
            "list",
            "hash",
            "map",
            "set",
            "tree",
            "graph",
            "heap",
            "stack",
            "queue",
            "dp",
            "dynamic programming",
            "memo",
            "recursion",
            "recursive",
            "loop",
            "iteration",
            "index",
            "indices",
            "function",
            "method",
            "class",
            "variable",
            "query",
            "sql",
            "schema",
            "api",
        ]
        return looksLikeCanvasExplanationFollowup(lower)
            && codeSignals.contains { lower.contains($0) }
    }

    private func looksLikeSystemDesignCanvasFollowup(_ lower: String) -> Bool {
        let designSignals = [
            "architecture",
            "design",
            "scale",
            "scaling",
            "latency",
            "throughput",
            "cache",
            "queue",
            "database",
            "storage",
            "api",
            "contract",
            "load balancer",
            "region",
            "replica",
            "shard",
            "partition",
            "tradeoff",
            "failure",
            "observability",
            "rollout",
            "vpc",
            "subnet",
        ]
        return looksLikeCanvasExplanationFollowup(lower)
            && designSignals.contains { lower.contains($0) }
    }

    private func sharesCanvasQuestionTerm(_ lower: String, sourceQuestion: String?) -> Bool {
        let questionTokens = meaningfulCanvasTokens(lower)
        guard !questionTokens.isEmpty else { return false }
        let sourceTokens = meaningfulCanvasTokens(sourceQuestion?.lowercased() ?? "")
        guard !sourceTokens.isEmpty else { return false }
        return !questionTokens.isDisjoint(with: sourceTokens)
    }

    private func meaningfulCanvasTokens(_ text: String) -> Set<String> {
        let stopWords: Set<String> = [
            "about", "above", "after", "again", "answer", "anything", "because",
            "before", "being", "below", "better", "between", "can", "cannot",
            "could", "current", "does", "done", "explain", "from", "have",
            "into", "just", "like", "make", "more", "need", "other", "please",
            "previous", "question", "should", "show", "solution", "tell", "that",
            "their", "there", "these", "thing", "this", "those", "through",
            "using", "what", "when", "where", "which", "while", "with", "would",
            "your",
        ]
        let tokens = text
            .components(separatedBy: CharacterSet.alphanumerics.inverted)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() }
            .filter { token in
                token.count >= 4
                    && !stopWords.contains(token)
                    && !token.allSatisfy(\.isNumber)
            }
        return Set(tokens)
    }

    private func looksLikeCanvasExplanationFollowup(_ lower: String) -> Bool {
        let explanationSignals = [
            "explain",
            "why ",
            "why are",
            "why is",
            "why do",
            "why does",
            "why can",
            "why can't",
            "why cant",
            "how does",
            "how do",
            "how is",
            "what does",
            "what is",
            "what's",
            "this code",
            "that code",
            "same code",
            "above code",
            "current code",
            "previous code",
            "existing code",
            "this solution",
            "that solution",
            "same solution",
            "current solution",
            "previous solution",
            "above solution",
            "walk me through",
            "help me understand",
            "reason",
            "clarify",
        ]
        return explanationSignals.contains { lower.hasPrefix($0) || lower.contains(" \($0)") }
    }

    private func looksLikeCanvasMutationFollowup(_ lower: String) -> Bool {
        let mutationSignals = [
            "change ",
            "update ",
            "fix ",
            "add ",
            "remove ",
            "replace ",
            "rewrite ",
            "refactor ",
            "optimize ",
            "optimise ",
            "convert ",
            "patch ",
            "edit ",
            "modify ",
            "delete ",
            "insert ",
            "rename ",
            "move ",
            "make it",
            "make this",
            "make the",
            "switch ",
            "turn this",
            "turn it",
            "use a different",
            "use another",
            "show the code",
            "show complete code",
            "complete code",
            "continue the code",
        ]
        if mutationSignals.contains(where: { lower.hasPrefix($0) || lower.contains(" \($0)") }) {
            return true
        }
        return lower.contains(" instead") && (
            lower.contains(" use ")
                || lower.contains(" switch ")
                || lower.contains(" replace ")
                || lower.contains(" change ")
        )
    }

    private func looksLikeNewCanvasQuestion(_ lower: String) -> Bool {
        let trimmed = lower.trimmingCharacters(in: .whitespacesAndNewlines)
        let newQuestionSignals = [
            "q2",
            "q3",
            "q4",
            "q5",
            "q6",
            "q7",
            "question 2",
            "question 3",
            "question 4",
            "question 5",
            "question 6",
            "question 7",
            "next question",
            "next problem",
            "next coding",
            "new problem",
            "new coding question",
            "new screen",
            "new screenshot",
            "new image",
            "another question",
            "another coding",
            "another problem",
            "different question",
            "different problem",
            "new question",
            "separate question",
            "write a ",
            "build a ",
            "implement ",
            "create a ",
            "design a ",
            "solve ",
        ]
        return newQuestionSignals.contains { trimmed.hasPrefix($0) }
    }

    private func appendCanvasFollowup(
        to existing: String,
        question: String?,
        artifact: CanvasArtifact,
        number: Int
    ) -> String {
        if artifact.kind == .code {
            return mergeCodeCanvasUpdate(
                existing: existing,
                update: artifact.content,
                question: question,
                number: number)
        }

        var parts = [
            existing.trimmingCharacters(in: .whitespacesAndNewlines),
            "",
            "---",
            "",
            "FOLLOW-UP \(number)",
        ]
        if let question = question?.trimmingCharacters(in: .whitespacesAndNewlines),
           !question.isEmpty {
            parts.append("Question: \(question)")
        }
        let body = artifact.content.trimmingCharacters(in: .whitespacesAndNewlines)
        if !body.isEmpty {
            parts.append("")
            parts.append(body)
        }
        return parts.joined(separator: "\n")
    }

    private func mergeCodeCanvasUpdate(
        existing: String,
        update: String,
        question: String?,
        number: Int
    ) -> String {
        let base = existing.trimmingCharacters(in: .whitespacesAndNewlines)
        let updateBody = update.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !updateBody.isEmpty else { return base }

        let existingCode = extractCanvasCodeSection(from: base)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let updateCode = extractCanvasCodeSection(from: updateBody)
            .trimmingCharacters(in: .whitespacesAndNewlines)

        if shouldReplaceCodeSectionForFollowup(
            question: question,
            existingCode: existingCode,
            updateCode: updateCode,
            updateBody: updateBody)
        {
            return replacementCodeCanvas(
                existingCanvas: base,
                existingCode: existingCode,
                updateCode: updateCode,
                updateBody: updateBody,
                number: number)
        }

        let patchBody = patchBodyForFollowup(updateBody: updateBody, updateCode: updateCode)
        guard !patchBody.isEmpty else { return base }
        return [
            base,
            "",
            "PATCH \(number)",
            "-------",
            patchBody,
        ].joined(separator: "\n")
    }

    private func shouldReplaceCodeSectionForFollowup(
        question: String?,
        existingCode: String,
        updateCode: String,
        updateBody: String
    ) -> Bool {
        guard !existingCode.isEmpty, !updateCode.isEmpty else { return false }
        if updateBodyLooksLikePatch(updateBody) {
            return false
        }
        if questionAsksForFullReplacement(question) {
            return true
        }
        let existingLines = codeLineCount(existingCode)
        let updateLines = codeLineCount(updateCode)
        guard existingLines > 0, updateLines > 0 else { return false }
        return updateLines >= max(3, existingLines / 2)
    }

    private func replacementCodeCanvas(
        existingCanvas: String,
        existingCode: String,
        updateCode: String,
        updateBody: String,
        number: Int
    ) -> String {
        var sections = [
            "CODE\n----\n" + updateCode,
        ]
        let updateComplexity = extractComplexitySummary(from: updateBody)
        let complexity = updateComplexity.isEmpty
            ? extractComplexitySummary(from: existingCanvas)
            : updateComplexity
        if !complexity.isEmpty {
            sections.append("COMPLEXITY\n----------\n" + complexity)
        }
        let changed = changedLinesSummary(oldCode: existingCode, newCode: updateCode, limit: 48)
        if !changed.isEmpty {
            sections.append("CHANGED LINES \(number)\n---------------\n" + changed)
        }
        return sections.joined(separator: "\n\n")
    }

    private func patchBodyForFollowup(updateBody: String, updateCode: String) -> String {
        if !updateCode.isEmpty {
            return updateCode
        }
        return updateBody
    }

    private func updateBodyLooksLikePatch(_ body: String) -> Bool {
        let trimmed = body.trimmingCharacters(in: .whitespacesAndNewlines)
        let upper = trimmed.uppercased()
        return upper.hasPrefix("PATCH")
            || upper.hasPrefix("DIFF")
            || upper.hasPrefix("CHANGED BLOCK")
            || upper.hasPrefix("CHANGED LINES")
            || trimmed.hasPrefix("@@")
            || trimmed.hasPrefix("diff --git")
            || trimmed.contains("\n@@")
            || trimmed.contains("\ndiff --git")
    }

    private func questionAsksForFullReplacement(_ question: String?) -> Bool {
        let lower = question?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased() ?? ""
        guard !lower.isEmpty else { return false }
        let signals = [
            "full code",
            "complete code",
            "whole code",
            "entire code",
            "full replacement",
            "replace everything",
            "rewrite all",
            "rewrite the whole",
            "from scratch",
        ]
        return signals.contains { lower.contains($0) }
    }

    private func codeLineCount(_ code: String) -> Int {
        code.components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
            .count
    }

    private func changedLinesSummary(oldCode: String, newCode: String, limit: Int) -> String {
        let oldLines = oldCode.components(separatedBy: .newlines)
        let newLines = newCode.components(separatedBy: .newlines)
        let maxCount = max(oldLines.count, newLines.count)
        var output: [String] = []

        for index in 0..<maxCount {
            let oldLine = index < oldLines.count ? oldLines[index] : nil
            let newLine = index < newLines.count ? newLines[index] : nil
            if oldLine == newLine {
                continue
            }
            if let oldLine, !oldLine.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                output.append("- " + oldLine)
            }
            if let newLine, !newLine.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                output.append("+ " + newLine)
            }
            if output.count >= limit {
                output.append("...")
                break
            }
        }

        return output.joined(separator: "\n")
    }

    private func shouldAutoOpenCanvas(for card: RenderedCard, artifact: CanvasArtifact) -> Bool {
        guard card.kind == "answer" else { return false }
        switch artifact.kind {
        case .code, .systemDesign, .screen:
            return true
        case .document, .structured:
            return false
        }
    }

    private func setCanvasOpen(_ open: Bool) {
        let previousOpen = canvasOpen
        if !open, canvasFullWindow {
            restoreCanvasWindow()
        }
        if open {
            renderActiveCanvas()
        }
        canvasOpen = open
        canvasPane.isHidden = !open
        updateCanvasWidth()
        canvasToggleButton.isHidden = canvases.isEmpty
        canvasToggleButton.contentTintColor = open ? themedAccentColor : themedDimTextColor
        canvasToggleButton.toolTip = open ? "Collapse canvas" : "Open canvas"
        if open {
            ensureRoomForCanvas()
        } else {
            restoreCompactWidth()
        }
        if previousOpen != open {
            emitLifecycle("canvas_open_state", detail: "open=\(open) count=\(canvases.count) active=\(activeCanvasIndex ?? -1)")
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
        fullSizeButton.toolTip = windowFullSize
            ? "Restore compact Bluey"
            : "Fill this screen"
    }

    private func updateThemeButtonChrome() {
        let symbol = lightThemeEnabled ? "moon.fill" : "sun.max.fill"
        let fallback = lightThemeEnabled ? "☾" : "☼"
        styleHeaderIconButton(themeButton, symbol: symbol, fallback: fallback)
        themeButton.contentTintColor = lightThemeEnabled ? NSColor.black.withAlphaComponent(0.68) : BlueyTheme.cyan
        themeButton.toolTip = lightThemeEnabled ? "Switch to dark theme" : "Switch to light theme"
    }

    private func updateInteractionModeChrome(showToast: Bool = true) {
        let symbol = passThroughMode ? "cursorarrow.rays" : "hand.tap"
        let fallback = passThroughMode ? "P" : "I"
        styleHeaderIconButton(interactionModeButton, symbol: symbol, fallback: fallback)
        interactionModeButton.contentTintColor = passThroughMode ? themedAccentColor : themedTextColor
        interactionModeButton.toolTip = passThroughMode
            ? "Move-anywhere on: controls click normally, and blank Bluey space drags the panel."
            : "Interactive on: controls click normally, and blank Bluey space moves/resizes the panel."
        if showToast {
            showSystemToast(
                title: passThroughMode ? "Move-anywhere on" : "Interactive on",
                body: passThroughMode
                    ? "Hold any blank Bluey space to move the panel. Controls remain clickable."
                    : "Blank Bluey space now moves/resizes Bluey. Controls and text remain clickable.",
                duration: 2.0)
        }
    }

    private func expandWindowFullSize() {
        guard let window else { return }
        if preWindowFullSizeFrame == nil {
            preWindowFullSizeFrame = window.frame
        }
        windowFullSize = true
        updateFullSizeButtonChrome()
        needsDisplay = true

        let visibleScreen = window.screen?.visibleFrame
            ?? NSScreen.main?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let fullScreen = window.screen?.frame
            ?? NSScreen.main?.frame
            ?? visibleScreen
        let maxWidth = fullScreen.width
        let maxHeight = fullScreen.height
        if let overlayWindow = window as? OverlayWindow {
            overlayWindow.fillsVisibleFrame = true
            overlayWindow.preserveProgrammaticFrameHeight = false
            overlayWindow.contentCornerRadius = 0
            overlayWindow.minimumFrameWidth = min(360, maxWidth)
            overlayWindow.maximumFrameWidth = maxWidth
            overlayWindow.minimumFrameHeight = min(360, maxHeight)
            overlayWindow.maximumFrameHeight = maxHeight
        }
        window.minSize = NSSize(width: min(360, maxWidth), height: min(360, maxHeight))
        window.contentMinSize = window.minSize
        window.maxSize = NSSize(width: maxWidth, height: maxHeight)
        window.contentMaxSize = window.maxSize

        let frame = fullScreen
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
        let targetFrame = preWindowFullSizeFrame
        windowFullSize = false
        preWindowFullSizeFrame = nil
        restoreCompactWidth()
        updateCanvasWidth()
        updateFullSizeButtonChrome()
        needsDisplay = true
        if let overlayWindow = window as? OverlayWindow {
            overlayWindow.fillsVisibleFrame = false
            overlayWindow.contentCornerRadius = ExpandedPanelMetrics.cornerRadius
        }
        let screen = window.screen?.visibleFrame
            ?? NSScreen.main?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let fallbackFrame = targetFrame
            ?? OverlayPlacementStore.loadExpandedFrame(in: screen)
            ?? ExpandedPanelMetrics.compactFrame(in: screen)
        let fittedFrame = ExpandedPanelMetrics.fitExpandedFrameToVisibleScreen(
            fallbackFrame,
            visibleFrame: screen)
        if let overlayWindow = window as? OverlayWindow {
            overlayWindow.preserveProgrammaticFrameHeight = false
            overlayWindow.lockedFrameHeight = fittedFrame.height
        }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.16
            context.timingFunction = CAMediaTimingFunction(name: .easeOut)
            window.animator().setFrame(fittedFrame, display: true)
            self.layoutSubtreeIfNeeded()
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.22) { [weak self, weak window] in
            guard let self, let window else { return }
            if let overlayWindow = window as? OverlayWindow {
                overlayWindow.lockedFrameHeight = fittedFrame.height
                window.setFrame(fittedFrame, display: true)
                overlayWindow.lockedFrameHeight = nil
                overlayWindow.preserveProgrammaticFrameHeight = true
            } else {
                window.setFrame(fittedFrame, display: true)
            }
            self.layoutSubtreeIfNeeded()
            self.keepFixedChromeInBounds()
            self.onWindowFrameChanged?(window.frame)
        }
    }

    private func updateCanvasWidth() {
        guard canvasOpen else {
            canvasWidthConstraint?.constant = 0
            return
        }
        let available = max(0, bounds.width)
        let width = min(max(520, available * 0.42), 760)
        canvasWidthConstraint?.constant = min(width, max(360, available - 420))
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
        let maxWidth = ExpandedPanelMetrics.fittingMaximumWidth(for: screen)
        let maxHeight = ExpandedPanelMetrics.fittingMaximumHeight(for: screen)
        if let overlayWindow = window as? OverlayWindow {
            overlayWindow.fillsVisibleFrame = false
            overlayWindow.minimumFrameWidth = min(ExpandedPanelMetrics.minCompactWidth, maxWidth)
            overlayWindow.maximumFrameWidth = maxWidth
            overlayWindow.minimumFrameHeight = ExpandedPanelMetrics.minHeight
            overlayWindow.maximumFrameHeight = maxHeight
        }
        window.minSize = NSSize(width: min(ExpandedPanelMetrics.minCompactWidth, maxWidth), height: ExpandedPanelMetrics.minHeight)
        window.contentMinSize = window.minSize
        window.maxSize = NSSize(width: maxWidth, height: maxHeight)
        window.contentMaxSize = window.maxSize
        let frame = ExpandedPanelMetrics.focusFrame(
            in: screen,
            preferredWidth: ExpandedPanelMetrics.maxCanvasWidth)
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
        if let overlayWindow = window as? OverlayWindow {
            overlayWindow.fillsVisibleFrame = false
        }
        if let targetFrame {
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
    }

    private func ensureRoomForCanvas() {
        guard let window else { return }
        let targetWidth = ExpandedPanelMetrics.maxCanvasWidth
        let screen = window.screen?.visibleFrame
            ?? NSScreen.main?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let clampedTargetWidth = ExpandedPanelMetrics.fittingWidth(for: screen, preferred: targetWidth)
        let minimumWidth = ExpandedPanelMetrics.fittingMinimumWidth(for: screen, targetWidth: clampedTargetWidth)
        let maximumWidth = ExpandedPanelMetrics.fittingMaximumWidth(for: screen)
        let maximumHeight = ExpandedPanelMetrics.fittingMaximumHeight(for: screen)
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
        let maximumWidth = ExpandedPanelMetrics.fittingMaximumWidth(for: screen)
        let maximumHeight = ExpandedPanelMetrics.fittingMaximumHeight(for: screen)
        if let overlayWindow = window as? OverlayWindow {
            overlayWindow.fillsVisibleFrame = false
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
        guard !looksLikePrivateInstructionDisclosure(body) else { return nil }

        if let artifact = card.artifact {
            guard !looksLikePrivateInstructionDisclosure(artifact.body) else { return nil }
            let kind = CanvasKind.fromArtifactType(artifact.artifactType)
            if card.kind == "answer", kind != .code, kind != .systemDesign {
                emitLifecycle(
                    "canvas_ignore_answer_artifact",
                    detail: "card=\(card.id) kind=\(kind.shortTitle) title_chars=\(artifact.title.count) body_chars=\(artifact.body.count)")
                return nil
            }
            let confidence = artifact.confidence.map { "Confidence \(Int(($0 * 100).rounded()))%" }
            let artifactBody = artifact.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? body
                : artifact.body
            let content: String
            if kind == .code {
                content = formatCodeCanvas(body: artifactBody, codeBlocks: extractCodeBlocks(from: artifactBody))
                guard canvasCodeHasRealCode(content) else { return nil }
            } else {
                content = artifactBody
            }
            return CanvasArtifact(
                kind: kind,
                title: artifact.title.isEmpty ? kind.title : artifact.title,
                subtitle: confidence ?? kind.subtitle,
                content: content,
                sourceCardId: card.id)
        }

        if card.kind == "answer" {
            return nil
        }

        let lower = body.lowercased()
        let codeBlocks = extractCodeBlocks(from: body)
        if !codeBlocks.isEmpty || looksLikeCode(lower) {
            let content = formatCodeCanvas(body: body, codeBlocks: codeBlocks)
            guard canvasCodeHasRealCode(content) else { return nil }
            return CanvasArtifact(
                kind: .code,
                title: "Code canvas",
                subtitle: CanvasKind.code.subtitle,
                content: content,
                sourceCardId: card.id)
        }

        if looksLikeSystemDesign(lower) && hasStructuredShape(body) {
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

        return nil
    }

    private func appendTranscriptSnippet(_ card: RenderedCard) {
        let title = card.title.trimmingCharacters(in: .whitespacesAndNewlines)
        let body = trimConsumedTranscriptPrefix(from: displayTranscriptText(card.body))
        guard !body.isEmpty else { return }

        let label = transcriptSourceLabel(title)
        if shouldSuppressCrossSourceTranscriptPreview(label: label, body: body) { return }
        let preview = mergedLiveTranscriptPreview(label: label, body: body, final: true)
        rememberTranscriptForAnswer(label: label, body: body, final: true)
        updateLiveTranscriptStrip(
            label: label,
            body: preview,
            state: recordingActive ? "TRANSCRIBING" : "CAPTURED",
            active: recordingActive,
            scrollToEnd: true)
    }

    func appendLiveTranscript(source: String, text: String, final: Bool) {
        let body = trimConsumedTranscriptPrefix(from: displayTranscriptText(text))
        let label = transcriptSourceLabel(source)
        let state = recordingActive ? "TRANSCRIBING" : (final ? "CAPTURED" : "HEARD")
        guard !body.isEmpty else {
            setTranscriptState(state, active: recordingActive)
            updateLiveTranscriptStrip(
                label: label,
                body: "audio is live",
                state: state,
                active: recordingActive,
                scrollToEnd: false)
            return
        }
        if shouldSuppressCrossSourceTranscriptPreview(label: label, body: body) { return }
        let preview = mergedLiveTranscriptPreview(label: label, body: body, final: final)
        rememberTranscriptForAnswer(label: label, body: body, final: final)
        updateLiveTranscriptStrip(
            label: label,
            body: preview,
            state: state,
            active: recordingActive,
            scrollToEnd: true)
    }

    private func mergedLiveTranscriptPreview(label: String, body: String, final: Bool) -> String {
        let cleanLabel = label.trimmingCharacters(in: .whitespacesAndNewlines)
        let cleanBody = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanLabel.isEmpty, !cleanBody.isEmpty else { return cleanBody }
        let existing = liveTranscriptPreviewBodies[cleanLabel]?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let merged: String
        merged = mergedTranscriptBody(existing, cleanBody)
        let bounded = boundedTranscriptTail(merged, maxChars: ChromeMetrics.transcriptPreviewMemoryChars)
        liveTranscriptPreviewBodies[cleanLabel] = bounded
        return bounded
    }

    private func shouldSuppressCrossSourceTranscriptPreview(label: String, body: String) -> Bool {
        let cleanLabel = transcriptSourceLabel(label)
        guard cleanLabel == "Mic" || cleanLabel == "System" else { return false }
        let normalized = normalizeTranscriptMemoryLine(body)
        guard !normalized.isEmpty else { return false }
        let otherLabel = cleanLabel == "Mic" ? "System" : "Mic"
        guard let existing = liveTranscriptPreviewBodies[otherLabel] else { return false }
        let existingNormalized = normalizeTranscriptMemoryLine(existing)
        guard isSameTranscriptMemoryBody(existingNormalized, normalized) else { return false }
        if cleanLabel == "Mic" {
            liveTranscriptPreviewBodies.removeValue(forKey: otherLabel)
            latestLiveTranscriptLinesBySource.removeValue(forKey: otherLabel)
            autoSendTranscriptLinesBySource.removeValue(forKey: otherLabel)
            return false
        }
        return true
    }

    private func updateLiveTranscriptStrip(
        label: String,
        body: String,
        state: String,
        active: Bool,
        scrollToEnd: Bool
    ) {
        let cleanBody = body.trimmingCharacters(in: .whitespacesAndNewlines)
        let cleanLabel = label.trimmingCharacters(in: .whitespacesAndNewlines)
        lastTranscriptStripSource = cleanLabel
        setTranscriptState(active ? "TRANSCRIBING" : state, active: active)
        guard !cleanBody.isEmpty else {
            updateTranscriptStripText(cleanLabel.isEmpty ? "Listening" : "\(cleanLabel) audio is live", scrollToEnd: false)
            updateTranscriptClearButtonVisibility()
            return
        }

        let displayBody = boundedTranscriptTail(
            cleanBody,
            maxChars: ChromeMetrics.transcriptRailDisplayChars)
        let display = cleanLabel.isEmpty ? displayBody : "\(cleanLabel): \(displayBody)"
        updateTranscriptStripText(display, scrollToEnd: scrollToEnd)
        updateTranscriptClearButtonVisibility()
    }

    private func rememberTranscriptForAnswer(label: String, body: String, final: Bool) {
        let cleanLabel = label.trimmingCharacters(in: .whitespacesAndNewlines)
        let cleanBody = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanBody.isEmpty else { return }
        let key = cleanLabel.isEmpty ? "Audio" : cleanLabel
        let line = cleanLabel.isEmpty ? cleanBody : "\(cleanLabel): \(cleanBody)"
        if transcriptLineWasJustConsumed(line) {
            emitLifecycle(
                "transcript_buffer_skip_consumed",
                detail: "source=\(transcriptSourceLabel(key)) final=\(final) body_chars=\(cleanBody.count)"
            )
            if !final {
                latestLiveTranscriptLine = nil
                latestLiveTranscriptLinesBySource.removeValue(forKey: key)
            }
            updateTranscriptClearButtonVisibility()
            return
        }
        rememberAutoSendTranscriptLine(source: key, body: cleanBody)
        if final {
            latestLiveTranscriptLine = nil
            latestLiveTranscriptLinesBySource.removeValue(forKey: key)
            if !replaceExistingTranscriptSnippetIfNeeded(with: line) && transcriptSnippets.last != line {
                transcriptSnippets.append(line)
            }
            if transcriptSnippets.count > 12 {
                transcriptSnippets.removeFirst(transcriptSnippets.count - 12)
            }
        } else {
            latestLiveTranscriptLine = line
            latestLiveTranscriptLinesBySource[key] = line
        }
        updateTranscriptClearButtonVisibility()
    }

    private func replaceExistingTranscriptSnippetIfNeeded(with line: String) -> Bool {
        guard !transcriptSnippets.isEmpty else { return false }
        let parsed = parsedTranscriptMemoryLine(line)
        let incomingLabel = parsed.label
        for index in transcriptSnippets.indices.reversed() {
            let existing = transcriptSnippets[index]
            let existingLabel = parsedTranscriptMemoryLine(existing).label
            guard existingLabel == incomingLabel else { continue }
            if shouldReplaceTranscriptMemoryLine(existing, with: line) {
                transcriptSnippets[index] = longerTranscriptMemoryLine(existing, line)
                return true
            }
        }
        return false
    }

    private func rememberAutoSendTranscriptLine(source: String, body: String) {
        guard autoSendListenCaptureActive else { return }
        let key = transcriptSourceLabel(source)
        guard key == "Mic" || key == "System" else { return }
        let cleanBody = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanBody.isEmpty else { return }
        let existing = autoSendTranscriptLinesBySource[key] ?? ""
        autoSendTranscriptLinesBySource[key] = mergedTranscriptBody(existing, cleanBody)
    }

    private func clearLocalTranscriptContext(replacementText: String) {
        emitLifecycle(
            "transcript_context_cleared",
            detail: "snippets=\(transcriptSnippets.count) live_sources=\(latestLiveTranscriptLinesBySource.count) autosend_sources=\(autoSendTranscriptLinesBySource.count) preview_sources=\(liveTranscriptPreviewBodies.count) consumed_fingerprints=\(consumedTranscriptFingerprints.count)"
        )
        transcriptSnippets.removeAll()
        latestLiveTranscriptLine = nil
        latestLiveTranscriptLinesBySource.removeAll()
        autoSendListenCaptureActive = false
        autoSendTranscriptLinesBySource.removeAll()
        liveTranscriptPreviewBodies.removeAll()
        consumedTranscriptFingerprints.removeAll()
        lastTranscriptStripSource = nil
        updateTranscriptStripText(replacementText, scrollToEnd: false)
        updateTranscriptClearButtonVisibility()
    }

    private func hasTranscriptContextToClear() -> Bool {
        !transcriptSnippets.isEmpty || latestLiveTranscriptLine != nil || !latestLiveTranscriptLinesBySource.isEmpty || !liveTranscriptPreviewBodies.isEmpty
    }

    private func updateTranscriptClearButtonVisibility() {
        transcriptClearButton.isHidden = !hasTranscriptContextToClear()
    }

    private func appendLiveTranscriptLines(to lines: inout [String]) {
        if latestLiveTranscriptLinesBySource.isEmpty {
            if let live = latestLiveTranscriptLine, lines.last != live {
                lines.append(live)
            }
            return
        }

        for key in ["Mic", "System", "Audio"] {
            if let live = latestLiveTranscriptLinesBySource[key], lines.last != live {
                lines.append(live)
            }
        }
        for key in latestLiveTranscriptLinesBySource.keys.sorted()
            where key != "Mic" && key != "System" && key != "Audio" {
            if let live = latestLiveTranscriptLinesBySource[key], lines.last != live {
                lines.append(live)
            }
        }
    }

    private func transcriptQuestionForAnswer() -> String? {
        var lines = transcriptSnippets
        appendLiveTranscriptLines(to: &lines)
        let joined = compactTranscriptQuestionLines(Array(lines.suffix(12)))
            .joined(separator: "\n")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !joined.isEmpty else { return nil }
        return boundedTranscriptTail(joined, maxChars: ChromeMetrics.transcriptPreviewMemoryChars)
    }

    private func liveTranscriptPreviewQuestionForAnswer() -> String? {
        var candidates: [String] = []
        if let latestLiveTranscriptLine {
            candidates.append(latestLiveTranscriptLine)
        }
        for key in ["Mic", "System", "Audio"] {
            if let line = latestLiveTranscriptLinesBySource[key] {
                candidates.append(line)
            }
        }
        for key in ["Mic", "System", "Audio"] {
            if let body = liveTranscriptPreviewBodies[key]?.trimmingCharacters(in: .whitespacesAndNewlines),
               !body.isEmpty {
                candidates.append("\(key): \(body)")
            }
        }
        let railText = transcriptLabel.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        if !railText.isEmpty {
            candidates.append(railText)
        }

        for candidate in candidates {
            if let compact = compactLiveTranscriptQuestionCandidate(candidate) {
                return compact
            }
        }
        return nil
    }

    private func compactLiveTranscriptQuestionCandidate(_ transcript: String) -> String? {
        let trimmed = transcript.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let lines = trimmed
            .components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
        guard !lines.isEmpty else { return nil }
        let compact = lines.joined(separator: " ")
            .replacingOccurrences(of: #"\s+"#, with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard compact.count >= 3 else { return nil }
        let lower = compact.lowercased()
        let placeholderFragments = [
            "captions appear here",
            "live captions preview",
            "starting audio",
            "audio is live",
            "listening for follow-up"
        ]
        guard !placeholderFragments.contains(where: { lower.contains($0) }) else { return nil }
        return boundedTranscriptTail(compact, maxChars: ChromeMetrics.transcriptPreviewMemoryChars)
    }

    private func consumeTranscriptBufferForAnswer() {
        var lines = transcriptSnippets
        appendLiveTranscriptLines(to: &lines)
        if lines.isEmpty, let preview = liveTranscriptPreviewQuestionForAnswer() {
            lines.append(preview)
        }
        var newlyConsumed = 0
        for line in lines {
            let fingerprint = transcriptMemoryFingerprint(line)
            guard !fingerprint.isEmpty else { continue }
            if !consumedTranscriptFingerprints.contains(fingerprint) {
                consumedTranscriptFingerprints.append(fingerprint)
                newlyConsumed += 1
            }
        }
        if consumedTranscriptFingerprints.count > 24 {
            consumedTranscriptFingerprints.removeFirst(consumedTranscriptFingerprints.count - 24)
        }
        emitLifecycle(
            "transcript_buffer_consumed",
            detail: "lines=\(lines.count) new_fingerprints=\(newlyConsumed) retained_fingerprints=\(consumedTranscriptFingerprints.count) recording=\(recordingActive)"
        )
        transcriptSnippets.removeAll()
        latestLiveTranscriptLine = nil
        latestLiveTranscriptLinesBySource.removeAll()
        autoSendListenCaptureActive = false
        autoSendTranscriptLinesBySource.removeAll()
        liveTranscriptPreviewBodies.removeAll()
        lastTranscriptStripSource = nil
        updateTranscriptStripText(
            recordingActive ? "Listening for follow-up..." : "Live captions preview",
            scrollToEnd: false)
        updateTranscriptClearButtonVisibility()
    }

    private func composedQuestionForAnswer(typed raw: String) -> String? {
        let typed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        let transcript = transcriptQuestionForAnswer()
        if typed.isEmpty {
            guard let transcript = transcript ?? liveTranscriptPreviewQuestionForAnswer() else { return nil }
            return liveTranscriptVisibleQuestion(from: transcript) ?? transcript
        }
        return typed
    }

    private func liveTranscriptVisibleQuestion(from transcript: String) -> String? {
        let trimmed = transcript.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        guard trimmed.count <= 220 else { return nil }
        let lines = trimmed
            .components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
        guard !lines.isEmpty, lines.count <= 2 else { return nil }
        let compact = lines.joined(separator: " ")
            .replacingOccurrences(of: #"\s+"#, with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard compact.count >= 3 else { return nil }
        let lower = compact.lowercased()
        let placeholderFragments = [
            "captions appear here",
            "live captions preview",
            "starting audio",
            "audio is live",
            "listening for follow-up"
        ]
        guard !placeholderFragments.contains(where: { lower.contains($0) }) else { return nil }
        return compact
    }

    private func liveTranscriptAnswerPrompt(forSources sources: [String]? = nil) -> String {
        let normalized = sources?
            .map(transcriptSourceLabel)
            .filter { !$0.isEmpty }
        let scopedSource: String
        if let normalized, !normalized.isEmpty {
            scopedSource = normalized.joined(separator: " and ") + " "
        } else {
            scopedSource = ""
        }
        return "Answer the latest \(scopedSource)live captions from the current session transcript. Treat the transcript as the user's current question or working context."
    }

    private func isLiveTranscriptAnswerPrompt(_ question: String) -> Bool {
        let normalized = question
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        return normalized.hasPrefix("answer the latest ")
            && normalized.contains("live captions from the current session transcript")
    }

    private func fallbackQuestionForAttachedContext() -> String? {
        let hasPendingAttachments = !pendingContextItemIds.isEmpty
        if screenContextReadyForAnswer && hasPendingAttachments {
            return "Answer using the attached screen capture, documents, and current session context."
        }
        if screenContextReadyForAnswer {
            return "Analyse the attached screen capture and answer with the key details."
        }
        if hasPendingAttachments {
            return "Answer using the attached documents and current session context."
        }
        return nil
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
        var accepted: [(label: String, body: String, normalized: String)] = []
        for label in order {
            guard let body = bodies[label]?.trimmingCharacters(in: .whitespacesAndNewlines), !body.isEmpty else {
                continue
            }
            let normalized = normalizeTranscriptMemoryLine(body)
            guard !normalized.isEmpty else { continue }
            if let duplicateIndex = accepted.firstIndex(where: { isSameTranscriptMemoryBody($0.normalized, normalized) }) {
                if transcriptLabelPriority(label) < transcriptLabelPriority(accepted[duplicateIndex].label) {
                    accepted[duplicateIndex] = (label, body, normalized)
                }
                continue
            }
            accepted.append((label, body, normalized))
        }
        let compacted = accepted.map { "\($0.label): \($0.body)" }
        if compacted.count == 1, let only = compacted.first {
            return [parsedTranscriptMemoryLine(only).body]
        }
        return compacted
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
        if isSameTranscriptMemoryBody(oldNorm, newNorm) {
            return longerTranscriptMemoryLine(old, new)
        }

        let oldWords = old.split(whereSeparator: { $0.isWhitespace }).map(String.init)
        let newWords = new.split(whereSeparator: { $0.isWhitespace }).map(String.init)
        let maxOverlap = min(oldWords.count, newWords.count, 32)
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

    private func boundedTranscriptTail(_ text: String, maxChars: Int) -> String {
        let clean = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard maxChars > 0, clean.count > maxChars else { return clean }
        let suffixStart = clean.index(clean.endIndex, offsetBy: -maxChars)
        let suffix = String(clean[suffixStart...]).trimmingCharacters(in: .whitespacesAndNewlines)
        return "... " + suffix
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

    private func transcriptMemoryFingerprint(_ line: String) -> String {
        normalizeTranscriptMemoryLine(parsedTranscriptMemoryLine(line).body)
    }

    private func transcriptLabelPriority(_ label: String) -> Int {
        switch transcriptSourceLabel(label) {
        case "Mic": return 0
        case "System": return 1
        case "Audio": return 2
        default: return 3
        }
    }

    private func isSameTranscriptMemoryBody(_ first: String, _ second: String) -> Bool {
        guard !first.isEmpty, !second.isEmpty else { return false }
        if first == second || first.contains(second) || second.contains(first) {
            return true
        }
        let firstSet = Set(first.split(separator: " ").map(String.init))
        let secondSet = Set(second.split(separator: " ").map(String.init))
        let shorter = min(firstSet.count, secondSet.count)
        let longer = max(firstSet.count, secondSet.count)
        let overlap = firstSet.intersection(secondSet).count
        return shorter >= 4 && longer <= shorter + 4 && overlap >= max(1, shorter - 1)
    }

    private func transcriptLineWasJustConsumed(_ line: String) -> Bool {
        let fingerprint = transcriptMemoryFingerprint(line)
        guard !fingerprint.isEmpty else { return false }
        let compactFingerprint = compactTranscriptMemoryLine(fingerprint)
        let words = fingerprint.split(separator: " ").count
        for consumed in consumedTranscriptFingerprints {
            guard !consumed.isEmpty else { continue }
            if consumed == fingerprint { return true }
            let compactConsumed = compactTranscriptMemoryLine(consumed)
            if !compactConsumed.isEmpty, compactConsumed == compactFingerprint {
                return true
            }
            let consumedWords = consumed.split(separator: " ").count
            let shorter = min(words, consumedWords)
            let longer = max(words, consumedWords)
            if shorter > 0,
               longer <= shorter + 4,
               (consumed.contains(fingerprint) || fingerprint.contains(consumed)) {
                return true
            }
        }
        return false
    }

    private func trimConsumedTranscriptPrefix(from body: String) -> String {
        let clean = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty else { return clean }
        let compactBody = compactTranscriptMemoryLine(clean)
        guard compactBody.count >= 8 else { return clean }
        let consumedPrefixes = consumedTranscriptFingerprints
            .map { compactTranscriptMemoryLine($0) }
            .filter { $0.count >= 8 }
            .sorted { $0.count > $1.count }

        for consumed in consumedPrefixes {
            guard compactBody.hasPrefix(consumed) else { continue }
            if compactBody == consumed { return "" }
            guard let suffix = suffixAfterAlnumPrefix(consumed, in: clean), !suffix.isEmpty else {
                return ""
            }
            return suffix
        }
        return clean
    }

    private func suffixAfterAlnumPrefix(_ prefix: String, in text: String) -> String? {
        let prefixChars = Array(prefix)
        guard !prefixChars.isEmpty else { return text }

        var matched = 0
        var scalarIndex = text.unicodeScalars.startIndex
        while scalarIndex < text.unicodeScalars.endIndex {
            let scalar = text.unicodeScalars[scalarIndex]
            if let normalized = normalizedAsciiAlnum(scalar) {
                guard matched < prefixChars.count, normalized == prefixChars[matched] else {
                    return nil
                }
                matched += 1
                if matched == prefixChars.count {
                    let nextScalarIndex = text.unicodeScalars.index(after: scalarIndex)
                    let stringIndex = String.Index(nextScalarIndex, within: text) ?? text.endIndex
                    let trimSet = CharacterSet.whitespacesAndNewlines.union(.punctuationCharacters)
                    return String(text[stringIndex...]).trimmingCharacters(in: trimSet)
                }
            }
            scalarIndex = text.unicodeScalars.index(after: scalarIndex)
        }
        return nil
    }

    private func normalizedAsciiAlnum(_ scalar: UnicodeScalar) -> Character? {
        switch scalar.value {
        case 48...57:
            return Character(UnicodeScalar(scalar.value)!)
        case 65...90:
            return Character(UnicodeScalar(scalar.value + 32)!)
        case 97...122:
            return Character(UnicodeScalar(scalar.value)!)
        default:
            return nil
        }
    }

    private func normalizeTranscriptMemoryLine(_ value: String) -> String {
        value
            .lowercased()
            .replacingOccurrences(of: #"[^a-z0-9]+"#, with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func compactTranscriptMemoryLine(_ value: String) -> String {
        normalizeTranscriptMemoryLine(value)
            .replacingOccurrences(of: " ", with: "")
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
        transcriptStripShouldFollowTail = scrollToEnd
        transcriptLabel.attributedStringValue = attributedTranscriptStripText(text)
        resizeTranscriptLabelToContent()
        guard scrollToEnd else {
            transcriptScroll.contentView.scroll(to: .zero)
            transcriptScroll.reflectScrolledClipView(transcriptScroll.contentView)
            return
        }
        scrollTranscriptRailToEnd()
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.layoutTranscriptRailForCurrentText()
        }
    }

    private func layoutTranscriptRailForCurrentText() {
        resizeTranscriptLabelToContent()
        guard transcriptStripShouldFollowTail else { return }
        scrollTranscriptRailToEnd()
    }

    private func scrollTranscriptRailToEnd() {
        let maxX = max(0, transcriptLabel.frame.width - transcriptScroll.contentView.bounds.width)
        transcriptScroll.contentView.scroll(to: NSPoint(x: maxX, y: 0))
        transcriptScroll.reflectScrolledClipView(transcriptScroll.contentView)
    }

    private func attributedTranscriptStripText(_ text: String) -> NSAttributedString {
        let font = transcriptLabel.font ?? NSFont.systemFont(ofSize: 11.5, weight: .medium)
        let sourceFont = NSFont.systemFont(ofSize: 11.5, weight: .bold)
        let attributed = NSMutableAttributedString(
            string: text,
            attributes: [
                .font: font,
                .foregroundColor: themedTextColor,
            ])
        if let colon = attributed.string.firstIndex(of: ":") {
            let labelLength = attributed.string.distance(from: attributed.string.startIndex, to: colon)
            if labelLength > 0, labelLength <= 24 {
                attributed.addAttributes(
                    [
                        .font: sourceFont,
                        .foregroundColor: BlueyTheme.green,
                    ],
                    range: NSRange(location: 0, length: labelLength))
                return attributed
            }
        }
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
        let attributed = transcriptLabel.attributedStringValue
        let measuredWidth: CGFloat
        if attributed.length > 0 {
            measuredWidth = ceil(attributed.boundingRect(
                with: NSSize(width: CGFloat.greatestFiniteMagnitude, height: height),
                options: [.usesLineFragmentOrigin, .usesFontLeading]
            ).width)
        } else {
            let font = transcriptLabel.font ?? NSFont.systemFont(ofSize: 11.5, weight: .medium)
            measuredWidth = ceil((transcriptLabel.stringValue as NSString).size(
                withAttributes: [.font: font]).width)
        }
        let textWidth = measuredWidth + 28
        transcriptLabel.frame = NSRect(
            x: 0,
            y: max(0, (height - 18) / 2),
            width: max(viewport, textWidth),
            height: 18)
    }

    private func makeAttachmentChip(_ item: OverlayContextItem) -> NSView {
        let isImage = isAttachmentImageKind(item.kind)
        let chipHeight: CGFloat = isImage ? 28 : 26
        let iconSize: CGFloat = isImage ? 22 : 14
        let chip = NSView()
        chip.translatesAutoresizingMaskIntoConstraints = false
        chip.wantsLayer = true
        chip.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.030).cgColor
        chip.layer?.cornerRadius = 12
        chip.layer?.borderWidth = 1
        chip.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.18).cgColor
        let titleText = item.title.isEmpty ? "Attached file" : item.title
        let tooltipParts = [
            titleText,
            item.kind.uppercased(),
            item.path,
        ].compactMap { value -> String? in
            guard let value, !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                return nil
            }
            return value
        }
        chip.toolTip = tooltipParts.joined(separator: "\n")

        let icon = NSImageView()
        icon.translatesAutoresizingMaskIntoConstraints = false
        icon.imageScaling = isImage ? .scaleProportionallyUpOrDown : .scaleProportionallyDown
        icon.wantsLayer = true
        icon.layer?.cornerRadius = isImage ? 5 : 0
        icon.layer?.masksToBounds = isImage
        icon.layer?.borderWidth = isImage ? 1 : 0
        icon.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.16).cgColor
        if isImage, let path = item.path, let image = NSImage(contentsOfFile: path) {
            image.isTemplate = false
            icon.image = image
            icon.contentTintColor = nil
        } else if let image = symbolImage(fileSymbol(for: item.kind)) {
            image.isTemplate = true
            icon.image = image
            icon.contentTintColor = fileAccent(for: item.kind)
        }

        let title = NSTextField(labelWithString: titleText)
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = NSFont.systemFont(ofSize: 10.5, weight: .semibold)
        title.textColor = BlueyTheme.text
        title.lineBreakMode = .byTruncatingMiddle
        title.maximumNumberOfLines = 1
        title.toolTip = chip.toolTip

        let remove = RemoveAttachmentButton(title: "", target: self, action: #selector(removeAttachmentClicked(_:)))
        remove.translatesAutoresizingMaskIntoConstraints = false
        remove.contextId = item.id
        remove.isBordered = false
        remove.wantsLayer = true
        remove.layer?.cornerRadius = 8
        remove.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.025).cgColor
        remove.layer?.borderWidth = 0
        remove.contentTintColor = BlueyTheme.textDim
        if let image = symbolImage("xmark") {
            image.isTemplate = true
            remove.image = image
            remove.imagePosition = .imageOnly
            remove.imageScaling = .scaleProportionallyDown
        } else {
            remove.title = "x"
        }
        remove.toolTip = "Remove this file from future answers"

        let open = AttachmentOpenButton(title: "", target: self, action: #selector(openAttachmentClicked(_:)))
        open.translatesAutoresizingMaskIntoConstraints = false
        open.filePath = item.path
        open.isBordered = false
        open.toolTip = item.path == nil ? chip.toolTip : "Open \(titleText)"

        chip.addSubview(icon)
        chip.addSubview(title)
        chip.addSubview(open)
        chip.addSubview(remove)
        NSLayoutConstraint.activate([
            chip.heightAnchor.constraint(equalToConstant: chipHeight),
            chip.widthAnchor.constraint(lessThanOrEqualToConstant: isImage ? 162 : 176),
            chip.widthAnchor.constraint(greaterThanOrEqualToConstant: isImage ? 104 : 96),

            icon.leadingAnchor.constraint(equalTo: chip.leadingAnchor, constant: 7),
            icon.centerYAnchor.constraint(equalTo: chip.centerYAnchor),
            icon.widthAnchor.constraint(equalToConstant: iconSize),
            icon.heightAnchor.constraint(equalToConstant: iconSize),

            title.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: isImage ? 6 : 5),
            title.centerYAnchor.constraint(equalTo: chip.centerYAnchor),
            title.trailingAnchor.constraint(equalTo: remove.leadingAnchor, constant: -4),

            open.topAnchor.constraint(equalTo: chip.topAnchor),
            open.leadingAnchor.constraint(equalTo: chip.leadingAnchor),
            open.bottomAnchor.constraint(equalTo: chip.bottomAnchor),
            open.trailingAnchor.constraint(equalTo: remove.leadingAnchor, constant: -2),

            remove.trailingAnchor.constraint(equalTo: chip.trailingAnchor, constant: -6),
            remove.centerYAnchor.constraint(equalTo: chip.centerYAnchor),
            remove.widthAnchor.constraint(equalToConstant: 16),
            remove.heightAnchor.constraint(equalToConstant: 16),
        ])
        return chip
    }

    @objc private func openAttachmentClicked(_ sender: AttachmentOpenButton) {
        guard let path = sender.filePath?.trimmingCharacters(in: .whitespacesAndNewlines),
              !path.isEmpty else { return }
        NSWorkspace.shared.open(URL(fileURLWithPath: path))
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

        let contextBadge = makeSessionContextBadge(session)

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
        row.addSubview(contextBadge)
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
            title.trailingAnchor.constraint(equalTo: contextBadge.leadingAnchor, constant: -6),

            contextBadge.trailingAnchor.constraint(equalTo: rename.leadingAnchor, constant: -6),
            contextBadge.centerYAnchor.constraint(equalTo: title.centerYAnchor),
            contextBadge.widthAnchor.constraint(equalToConstant: session.contextCount > 0 ? 58 : 0),
            contextBadge.heightAnchor.constraint(equalToConstant: 20),

            subtitle.leadingAnchor.constraint(equalTo: title.leadingAnchor),
            subtitle.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 2),
            subtitle.trailingAnchor.constraint(equalTo: rename.leadingAnchor, constant: -6),

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

    private func makeSessionContextBadge(_ session: OverlaySessionItem) -> NSTextField {
        let count = max(0, session.contextCount)
        let imageCount = max(0, session.imageCount)
        let label = NSTextField(labelWithString: count > 0 ? "\(count) \(imageCount > 0 ? "items" : "docs")" : "")
        label.translatesAutoresizingMaskIntoConstraints = false
        label.font = NSFont.systemFont(ofSize: 9.5, weight: .bold)
        label.textColor = imageCount > 0 ? BlueyTheme.cyan : BlueyTheme.text
        label.alignment = .center
        label.lineBreakMode = .byTruncatingTail
        label.maximumNumberOfLines = 1
        label.isHidden = count == 0
        label.wantsLayer = true
        label.layer?.cornerRadius = 10
        label.layer?.backgroundColor = (imageCount > 0 ? BlueyTheme.cyan : NSColor.white)
            .withAlphaComponent(imageCount > 0 ? 0.12 : 0.055)
            .cgColor
        label.layer?.borderWidth = count > 0 ? 1 : 0
        label.layer?.borderColor = (imageCount > 0 ? BlueyTheme.cyan : BlueyTheme.hairline)
            .withAlphaComponent(imageCount > 0 ? 0.34 : 1.0)
            .cgColor
        label.toolTip = count > 0
            ? "\(count) attached context item\(count == 1 ? "" : "s") in this recording"
            : nil
        useCenteredSingleLineCell(label)
        return label
    }

    private func configureRenameRow(_ row: NSView, session: OverlaySessionItem) -> NSView {
        let field = ArrowCursorTextField()
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
            if let field {
                self?.applyAccentInsertionPoint(to: field)
            }
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
        setHeaderSubtitle()
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
            contextCount: session.contextCount,
            imageCount: session.imageCount,
            isActive: session.isActive)
        editingSessionId = nil
        emitSessionRename(id: session.id, title: title)
        setSessions(sessionItems)
    }

    private func fileSymbol(for kind: String) -> String {
        switch kind {
        case "image", "diagram", "screen", "screenshot": return "photo"
        case "code": return "curlybraces"
        case "text": return "doc.plaintext"
        case "document": return "doc.text"
        default: return "doc"
        }
    }

    private func isAttachmentImageKind(_ kind: String) -> Bool {
        kind == "image" || kind == "diagram" || kind == "screen" || kind == "screenshot"
    }

    private func fileAccent(for kind: String) -> NSColor {
        switch kind {
        case "image", "diagram", "screen", "screenshot": return NSColor(red: 0.58, green: 0.74, blue: 1.0, alpha: 1.0)
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
        let syntaxSignals = [
            "class solution",
            "def ",
            "fn ",
            "function ",
            "const ",
            "let ",
            "var ",
            "return ",
            "=>",
            "import ",
            "#include",
            "console.",
            "println!",
            "select ",
            "insert into ",
            "delete from ",
            "update ",
        ]
        let codingContextSignals = [
            "algorithm",
            "bug",
            "compile",
            "diff",
            "edge case",
            "implementation",
            "leetcode",
            "patch",
            "runtime",
            "sql query",
            "unit test",
            "time complexity",
            "space complexity",
            "test case",
        ]
        let syntaxCount = syntaxSignals.filter { lower.contains($0) }.count
        let contextCount = codingContextSignals.filter { lower.contains($0) }.count
        return syntaxCount >= 2 || (syntaxCount >= 1 && contextCount >= 1)
    }

    private func looksLikeSystemDesign(_ lower: String) -> Bool {
        if looksLikeInterviewProfileAnswer(lower) {
            return false
        }
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

    private func looksLikeInterviewProfileAnswer(_ lower: String) -> Bool {
        if lower.contains("tell me about yourself") || lower.contains("tell me about myself") {
            return true
        }
        let profileSignals = [
            "i'm ",
            "i am ",
            "i've ",
            "i’ve ",
            "i was at ",
            "before that i",
            "where i worked",
            "what drew me",
            "this role",
            "my background",
            "my experience",
            "senior software engineer",
            "master's",
            "masters",
        ]
        let behavioralSignals = [
            "tell me about a time",
            "describe a time",
            "give me an example",
            "situation",
            "task",
            "action",
            "result",
            "stakeholder",
            "conflict",
        ]
        return profileSignals.filter { lower.contains($0) }.count >= 3
            || behavioralSignals.filter { lower.contains($0) }.count >= 4
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
        let codeText = codeBlocks.isEmpty
            ? extractInlineImplementationCode(from: body)
            : codeBlocks.joined(separator: "\n\n// ---\n\n")
        let complexity = extractComplexitySummary(from: body)
        var sections: [String] = []

        if !codeText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            sections.append("CODE\n----\n" + codeText.trimmingCharacters(in: .whitespacesAndNewlines))
        }

        if !complexity.isEmpty {
            sections.append("COMPLEXITY\n----------\n" + complexity)
        }

        return sections.joined(separator: "\n\n")
    }

    private func canvasCodeHasRealCode(_ body: String) -> Bool {
        let code = extractCanvasCodeSection(from: body)
        let trimmed = code.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return false }
        let lower = trimmed.lowercased()
        let nonEmptyLines = trimmed
            .components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
        let signals = [
            "def ",
            "fn ",
            "func ",
            "function ",
            "class ",
            "struct ",
            "enum ",
            "return ",
            "select ",
            " from ",
            " where ",
            " group by",
            " order by",
            " join ",
            "insert ",
            "update ",
            "delete ",
            "for ",
            "while ",
            "if ",
            "else",
            "try",
            "catch ",
            "import ",
            "#include",
            "let ",
            "var ",
            "const ",
            "public ",
            "private ",
            "static ",
            "=>",
            "->",
            "==",
            "!=",
            "<=",
            ">=",
            "+=",
            "-=",
            "dp[",
            "graph[",
            ".append(",
            ".sort(",
            "@@",
            "diff --git",
        ]
        let hasSignal = signals.contains { lower.contains($0) }
        let hasPunctuation = trimmed.contains("{")
            || trimmed.contains("}")
            || trimmed.contains(";")
            || trimmed.contains("=")
            || (trimmed.contains("(") && trimmed.contains(")"))
            || (trimmed.contains("[") && trimmed.contains("]"))
        return hasSignal || (nonEmptyLines.count >= 2 && hasPunctuation)
    }

    private func extractCanvasCodeSection(from body: String) -> String {
        let normalized = body.replacingOccurrences(of: "\r\n", with: "\n")
        var lines: [String] = []
        var inCode = false
        var sawHeader = false
        for line in normalized.components(separatedBy: .newlines) {
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            let header = trimmed.uppercased()
            if isCanvasCodeHeader(header) {
                inCode = true
                sawHeader = true
                continue
            }
            if isCanvasStopHeader(header) {
                if inCode {
                    break
                }
                sawHeader = true
                continue
            }
            if !trimmed.isEmpty && trimmed.allSatisfy({ $0 == "-" || $0 == "=" }) {
                continue
            }
            if inCode {
                lines.append(line)
            }
        }
        return sawHeader ? lines.joined(separator: "\n") : normalized
    }

    private func isCanvasCodeHeader(_ header: String) -> Bool {
        header == "CODE"
            || header == "PATCH"
            || header.hasPrefix("PATCH ")
            || header == "DIFF"
            || header.hasPrefix("DIFF ")
            || header == "CHANGED BLOCK"
            || header.hasPrefix("CHANGED BLOCK ")
            || header == "CHANGED LINES"
            || header.hasPrefix("CHANGED LINES ")
    }

    private func isCanvasStopHeader(_ header: String) -> Bool {
        [
            "COMPLEXITY",
            "TIME",
            "SPACE",
            "NOTES",
            "EXPLANATION",
            "APPROACH",
        ].contains(header)
    }

    private func extractInlineImplementationCode(from text: String) -> String {
        let lines = text.components(separatedBy: .newlines)
        var best: [String] = []
        var current: [String] = []
        var inFence = false

        func finishCurrent() {
            let trimmed = current
                .drop { $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
                .reversed()
                .drop { $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
                .reversed()
            let candidate = Array(trimmed)
            if candidate.joined(separator: "\n").count > best.joined(separator: "\n").count {
                best = candidate
            }
            current.removeAll()
        }

        for rawLine in lines {
            let trimmed = rawLine.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmed.hasPrefix("```") {
                inFence.toggle()
                continue
            }
            guard !inFence else { continue }

            if isCanvasProseSectionHeading(trimmed) {
                finishCurrent()
                continue
            }

            if isLikelyImplementationLine(rawLine) {
                current.append(rawLine)
                continue
            }

            if !current.isEmpty, isCodeContinuationLine(rawLine) {
                current.append(rawLine)
                continue
            }

            finishCurrent()
        }
        finishCurrent()
        return best.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func isCanvasProseSectionHeading(_ line: String) -> Bool {
        let normalized = line
            .trimmingCharacters(in: CharacterSet(charactersIn: "#*-_ "))
            .lowercased()
        guard !normalized.isEmpty else { return false }
        return [
            "notes",
            "note",
            "approach",
            "explanation",
            "details",
            "why it works",
            "walkthrough",
            "intuition",
            "complexity",
            "time complexity",
            "space complexity",
        ].contains(normalized)
    }

    private func isLikelyImplementationLine(_ line: String) -> Bool {
        let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, !isCanvasProseSectionHeading(trimmed) else { return false }
        let lower = trimmed.lowercased()
        let starters = [
            "class ",
            "def ",
            "fn ",
            "func ",
            "function ",
            "public ",
            "private ",
            "static ",
            "struct ",
            "enum ",
            "import ",
            "from ",
            "#include",
            "return ",
            "if ",
            "elif ",
            "else",
            "for ",
            "while ",
            "try",
            "except",
            "catch ",
            "switch ",
            "case ",
            "let ",
            "var ",
            "const ",
        ]
        if starters.contains(where: { lower.hasPrefix($0) }) {
            return true
        }
        let syntaxSignals = [
            " = ",
            "==",
            "!=",
            "<=",
            ">=",
            "+=",
            "-=",
            "->",
            "=>",
            "):",
            "{",
            "}",
            "];",
            "self.",
            "dp[",
            "graph[",
            "range(",
            ".append(",
            ".sort(",
            "len(",
            "list[",
            "dict[",
        ]
        return syntaxSignals.contains { lower.contains($0) }
    }

    private func isCodeContinuationLine(_ line: String) -> Bool {
        if line.hasPrefix(" ") || line.hasPrefix("\t") {
            return true
        }
        let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed == "}" || trimmed == "};" || trimmed == ")" || trimmed == "];"
    }

    private func extractComplexitySummary(from text: String) -> String {
        var lines: [String] = []
        var seen = Set<String>()
        var inComplexity = false

        for rawLine in text.components(separatedBy: .newlines) {
            let cleaned = cleanComplexityLine(rawLine)
            let lower = cleaned.lowercased()
            if lower.isEmpty {
                if inComplexity, !lines.isEmpty { break }
                continue
            }

            if lower == "complexity" || lower == "complexity:" {
                inComplexity = true
                continue
            }
            if lower.contains("time complexity") || lower.hasPrefix("time:") || lower.hasPrefix("time ") {
                addComplexityLine(cleaned, to: &lines, seen: &seen)
                inComplexity = true
                continue
            }
            if lower.contains("space complexity") || lower.hasPrefix("space:") || lower.hasPrefix("space ") {
                addComplexityLine(cleaned, to: &lines, seen: &seen)
                inComplexity = true
                continue
            }
            if inComplexity, lower.contains("o(") {
                addComplexityLine(cleaned, to: &lines, seen: &seen)
            } else if inComplexity, !lines.isEmpty {
                break
            }
        }

        return lines.joined(separator: "\n")
    }

    private func cleanComplexityLine(_ line: String) -> String {
        var output = line.trimmingCharacters(in: .whitespacesAndNewlines)
        while output.hasPrefix("- ") || output.hasPrefix("* ") {
            output = String(output.dropFirst(2)).trimmingCharacters(in: .whitespacesAndNewlines)
        }
        while output.hasPrefix("#") {
            output = String(output.dropFirst()).trimmingCharacters(in: .whitespacesAndNewlines)
        }
        return output
            .replacingOccurrences(of: "**", with: "")
            .replacingOccurrences(of: "`", with: "")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func addComplexityLine(_ line: String, to lines: inout [String], seen: inout Set<String>) {
        let key = line.lowercased()
        guard !line.isEmpty, !seen.contains(key) else { return }
        seen.insert(key)
        lines.append(line)
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
            return ("managed", "instant", "instant")
        case 2:
            return ("managed", "balanced", "balanced")
        case 3:
            return ("managed", "deep", "deep")
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
    private var remoteInputPassthroughTimer: Timer?
    private var remoteControlHeuristicTimer: Timer?
    private var localKeyMonitor: Any?
    private var externalFileDragMonitor: Any?
    private var activeAppObserver: NSObjectProtocol?
    private var trustedRemoteInputEventTap: CFMachPort?
    private var trustedRemoteInputRunLoopSource: CFRunLoopSource?
    private var remoteInputPassthroughUntil = 0.0
    private var externalFileDragCaptureUntil = 0.0
    private var lastExpandedInteractiveMouseAt = CACurrentMediaTime()
    private var currentRunState: PillRunState = .ready
    private var lastPillRecordingToggleAt = Date.distantPast
    private var overlayOpacity = 0.94
    private var expandedModeActive = false
    private var stickyPillFrame: NSRect?
    private var stickyExpandedFrame: NSRect?
    private var lastTargetBundleIdentifier: String?

    /// Pending boot card, if a Boot command arrived before windows materialised.
    private var pendingBoot: (title: String, lines: [String])?

    deinit {
        if let activeAppObserver {
            NSWorkspace.shared.notificationCenter.removeObserver(activeAppObserver)
        }
    }

    func start() {
        rememberTargetApplication(NSWorkspace.shared.frontmostApplication)
        activeAppObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication
            self?.rememberTargetApplication(app)
        }

        // Pill window: compact launcher, centered by default.
        let pillSize = PillMetrics.size
        let screen = OverlayScreenPlacement.activeVisibleFrame()
        stickyPillFrame = OverlayPlacementStore.loadPillFrame(in: screen)
        stickyExpandedFrame = OverlayPlacementStore.loadExpandedFrame(in: screen)
        pillWindow = OverlayWindow(
            contentRect: stickyPillFrame ?? PillMetrics.centeredFrame(in: screen),
            draggable: false)
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
        pillView.onMoved = { [weak self] frame in self?.rememberPillFrame(frame) }

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
        startLocalKeyRouting()
        startExternalFileDragCaptureMonitor()
        if trustedRemoteInputTapEnabled {
            startTrustedRemoteInputPassthroughMonitor()
        } else {
            emitLifecycle(
                "remote_input_passthrough_monitor",
                status: "disabled",
                detail: "trusted event tap is opt-in to avoid input monitoring prompts")
        }
        startRemoteControlHeuristicMonitor()
        startIpcLoop()
    }

    private func rememberTargetApplication(_ app: NSRunningApplication?) {
        guard let app,
              app.processIdentifier != getpid(),
              let bundle = app.bundleIdentifier?.trimmingCharacters(in: .whitespacesAndNewlines),
              !bundle.isEmpty
        else { return }
        lastTargetBundleIdentifier = bundle
    }

    private func startLocalKeyRouting() {
        guard localKeyMonitor == nil else { return }
        localKeyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard
                let self,
                self.expandedWindow?.isVisible == true,
                let expandedView = self.expandedView,
                expandedView.routeKeyDownToComposer(event)
            else {
                return event
            }
            return nil
        }
    }

    private func bringPillToFront(force: Bool = false) {
        guard force || !expandedModeActive else { return }
        placePillForDisplay()
        pillWindow.ignoresMouseEvents = isRemoteInputPassthroughActive
        pillWindow.acceptsMouseMovedEvents = true
        pillWindow.setIsVisible(true)
        pillWindow.orderFrontRegardless()
        pillWindow.makeKeyAndOrderFront(nil)
        pillView.needsDisplay = true
        pillView.needsLayout = true
        pillView.layoutSubtreeIfNeeded()
        pillView.displayIfNeeded()
        pillWindow.displayIfNeeded()
    }

    private func placePillForDisplay() {
        let screen = OverlayScreenPlacement.activeVisibleFrame()
        let frame = stickyPillFrame.map { clampedPillFrame($0, in: screen) }
            ?? PillMetrics.centeredFrame(in: screen)
        pillWindow.setFrame(frame, display: true)
    }

    private func rememberPillFrame(_ frame: NSRect) {
        let screen = OverlayScreenPlacement.activeVisibleFrame()
        stickyPillFrame = clampedPillFrame(frame, in: screen)
        if let stickyPillFrame {
            OverlayPlacementStore.savePillFrame(stickyPillFrame, in: screen)
        }
    }

    private func clampedPillFrame(_ frame: NSRect, in visibleFrame: NSRect) -> NSRect {
        let inset: CGFloat = 8
        var clamped = frame
        clamped.size = PillMetrics.size
        clamped.origin.x = min(
            max(visibleFrame.minX + inset, clamped.origin.x),
            visibleFrame.maxX - clamped.size.width - inset)
        clamped.origin.y = min(
            max(visibleFrame.minY + inset, clamped.origin.y),
            visibleFrame.maxY - clamped.size.height - inset)
        return clamped
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

    private var isRemoteInputPassthroughActive: Bool {
        CACurrentMediaTime() < remoteInputPassthroughUntil
    }

    private var isExternalFileDragCaptureActive: Bool {
        CACurrentMediaTime() < externalFileDragCaptureUntil
    }

    @discardableResult
    private func applyRemoteInputPassthroughIfActive() -> Bool {
        guard isRemoteInputPassthroughActive else { return false }
        if let expandedWindow,
           expandedWindow.isVisible,
           expandedView?.isInteractiveAtScreenPoint(NSEvent.mouseLocation) == true {
            expandedWindow.acceptsMouseMovedEvents = true
            expandedWindow.ignoresMouseEvents = false
        } else {
            expandedWindow?.ignoresMouseEvents = true
        }
        pillWindow?.ignoresMouseEvents = true
        return true
    }

    @discardableResult
    private func applyExternalFileDragCaptureIfActive() -> Bool {
        guard isExternalFileDragCaptureActive else { return false }
        expandedWindow?.acceptsMouseMovedEvents = true
        expandedWindow?.ignoresMouseEvents = false
        return true
    }

    private func allowRemoteInputPassthrough(durationMs: Int, reason: String) {
        let clampedMs = min(max(durationMs, 80), 1_500)
        let until = CACurrentMediaTime() + Double(clampedMs) / 1000.0
        remoteInputPassthroughUntil = max(remoteInputPassthroughUntil, until)

        expandedWindow?.ignoresMouseEvents = true
        pillWindow?.ignoresMouseEvents = true
        expandedWindow?.acceptsMouseMovedEvents = false
        pillWindow?.acceptsMouseMovedEvents = false

        remoteInputPassthroughTimer?.invalidate()
        remoteInputPassthroughTimer = Timer(timeInterval: Double(clampedMs) / 1000.0, repeats: false) { [weak self] _ in
            self?.restoreMousePolicyAfterRemoteInputPassthrough()
        }
        if let remoteInputPassthroughTimer {
            RunLoop.main.add(remoteInputPassthroughTimer, forMode: .common)
        }
        emitLifecycle("remote_input_passthrough", status: "armed", detail: "\(reason):\(clampedMs)ms")
    }

    private func clearRemoteInputPassthrough() {
        remoteInputPassthroughUntil = 0
        remoteInputPassthroughTimer?.invalidate()
        remoteInputPassthroughTimer = nil
        restoreMousePolicyAfterRemoteInputPassthrough()
    }

    private func restoreMousePolicyAfterRemoteInputPassthrough() {
        guard !isRemoteInputPassthroughActive else {
            applyRemoteInputPassthroughIfActive()
            return
        }

        expandedWindow?.acceptsMouseMovedEvents = true
        pillWindow?.acceptsMouseMovedEvents = true
        if expandedModeActive {
            pillWindow?.ignoresMouseEvents = true
            updateExpandedMousePolicy()
        } else {
            expandedWindow?.ignoresMouseEvents = true
            pillWindow?.ignoresMouseEvents = false
        }
        emitLifecycle("remote_input_passthrough", status: "cleared")
    }

    private func startTrustedRemoteInputPassthroughMonitor() {
        let mask = cgEventMask([
            .leftMouseDown, .leftMouseUp, .leftMouseDragged,
            .rightMouseDown, .rightMouseUp, .rightMouseDragged,
            .otherMouseDown, .otherMouseUp, .otherMouseDragged,
            .mouseMoved, .scrollWheel,
        ])
        guard let tap = CGEvent.tapCreate(
            tap: .cgSessionEventTap,
            place: .headInsertEventTap,
            options: .listenOnly,
            eventsOfInterest: mask,
            callback: blueyTrustedRemoteInputEventTapCallback,
            userInfo: Unmanaged.passUnretained(self).toOpaque())
        else {
            emitLifecycle(
                "remote_input_passthrough_monitor",
                status: "unavailable",
                detail: "trusted remote input event tap was not granted")
            return
        }

        trustedRemoteInputEventTap = tap
        trustedRemoteInputRunLoopSource = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)
        if let trustedRemoteInputRunLoopSource {
            CFRunLoopAddSource(CFRunLoopGetMain(), trustedRemoteInputRunLoopSource, .commonModes)
        }
        CGEvent.tapEnable(tap: tap, enable: true)
        emitLifecycle("remote_input_passthrough_monitor", status: "ready")
    }

    fileprivate func trustedRemoteInputEventDetected() {
        if remoteInputPassthroughUntil - CACurrentMediaTime() > 0.25 {
            return
        }
        allowRemoteInputPassthrough(
            durationMs: remoteInputPassthroughDefaultMs,
            reason: "bluey-trusted-event")
    }

    fileprivate func remoteControlInputEventDetected(sourcePid: pid_t?) {
        guard
            let sourcePid,
            sourcePid > 0,
            sourcePid != getpid(),
            let app = NSRunningApplication(processIdentifier: sourcePid),
            let label = remoteControlAppLabel(app)
        else { return }

        if remoteInputPassthroughUntil - CACurrentMediaTime() > 0.20 {
            return
        }
        allowRemoteInputPassthrough(
            durationMs: remoteInputPassthroughHeuristicMs,
            reason: "remote-control-event:\(label)")
    }

    private func startRemoteControlHeuristicMonitor() {
        remoteControlHeuristicTimer?.invalidate()
        remoteControlHeuristicTimer = Timer.scheduledTimer(withTimeInterval: 0.75, repeats: true) { [weak self] _ in
            self?.armPassthroughForVisibleRemoteControlAppIfNeeded()
        }
        if let remoteControlHeuristicTimer {
            RunLoop.main.add(remoteControlHeuristicTimer, forMode: .common)
        }
        emitLifecycle("remote_input_passthrough_heuristic", status: "ready")
    }

    private func armPassthroughForVisibleRemoteControlAppIfNeeded() {
        guard let label = activeRemoteControlAppLabel() else { return }
        if remoteInputPassthroughUntil - CACurrentMediaTime() > 0.25 {
            return
        }
        allowRemoteInputPassthrough(
            durationMs: remoteInputPassthroughHeuristicMs,
            reason: "remote-control-app:\(label)")
    }

    private func startExpandedPassthroughTracking() {
        expandedPassthroughTimer?.invalidate()
        expandedPassthroughTimer = Timer.scheduledTimer(withTimeInterval: 0.015, repeats: true) { [weak self] _ in
            self?.updateExpandedMousePolicy()
        }
    }

    private func startExternalFileDragCaptureMonitor() {
        externalFileDragMonitor = NSEvent.addGlobalMonitorForEvents(
            matching: [.leftMouseDragged, .leftMouseUp]
        ) { [weak self] event in
            DispatchQueue.main.async {
                self?.handleExternalFileDragProbe(event)
            }
        }
        emitLifecycle("external_file_drag_capture", status: "ready")
    }

    private func handleExternalFileDragProbe(_ event: NSEvent) {
        guard
            expandedModeActive,
            let expandedWindow,
            expandedWindow.isVisible
        else {
            externalFileDragCaptureUntil = 0
            return
        }

        if event.type == .leftMouseUp {
            externalFileDragCaptureUntil = 0
            updateExpandedMousePolicy()
            return
        }

        let point = NSEvent.mouseLocation
        guard expandedWindow.frame.insetBy(dx: -24, dy: -24).contains(point) else {
            return
        }

        let captureWindow = dragPasteboardContainsFileURLs() ? 0.75 : 0.25
        externalFileDragCaptureUntil = CACurrentMediaTime() + captureWindow
        applyExternalFileDragCaptureIfActive()
    }

    private func dragPasteboardContainsFileURLs() -> Bool {
        let pasteboard = NSPasteboard(name: .drag)
        if pasteboard.canReadObject(
            forClasses: [NSURL.self],
            options: [.urlReadingFileURLsOnly: true]
        ) {
            return true
        }
        let types = pasteboard.types ?? []
        return types.contains(.fileURL)
            || types.contains(NSPasteboard.PasteboardType("public.file-url"))
            || types.contains(NSPasteboard.PasteboardType("NSFilenamesPboardType"))
    }

    private func updateExpandedMousePolicy() {
        guard
            let expandedWindow,
            expandedWindow.isVisible
        else { return }

        if applyRemoteInputPassthroughIfActive() {
            return
        }
        if applyExternalFileDragCaptureIfActive() {
            return
        }

        expandedWindow.acceptsMouseMovedEvents = true
        let point = NSEvent.mouseLocation
        let isInteractive = expandedView?.isInteractiveAtScreenPoint(point) ?? true
        let now = CACurrentMediaTime()
        if isInteractive {
            lastExpandedInteractiveMouseAt = now
        }
        let keepControlPressAlive = NSEvent.pressedMouseButtons != 0
            && now - lastExpandedInteractiveMouseAt < 0.45
        let shouldReceiveMouse = isInteractive || keepControlPressAlive
        expandedWindow.ignoresMouseEvents = !shouldReceiveMouse
    }

    private func expand() {
        ensureExpandedWindow()
        guard let expandedWindow else { return }
        expandedModeActive = true
        placeExpandedWindowForOpen()
        lastExpandedInteractiveMouseAt = CACurrentMediaTime()
        hidePillWhileExpanded()
        NSApp.activate(ignoringOtherApps: true)
        expandedWindow.ignoresMouseEvents = isRemoteInputPassthroughActive
        expandedWindow.acceptsMouseMovedEvents = true
        expandedWindow.orderFrontRegardless()
        expandedWindow.makeKeyAndOrderFront(nil)
        updateExpandedMousePolicy()
        DispatchQueue.main.async { [weak self] in
            self?.placeExpandedWindowForOpen()
            self?.hidePillWhileExpanded()
            self?.updateExpandedMousePolicy()
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
            draggable: false,
            resizable: false)
        window.contentCornerRadius = ExpandedPanelMetrics.cornerRadius
        window.preserveProgrammaticFrameHeight = true
        let maxExpandedWidth = ExpandedPanelMetrics.fittingMaximumWidth(for: screen)
        let maxExpandedHeight = ExpandedPanelMetrics.fittingMaximumHeight(for: screen)
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
        view.onWindowFrameChanged = { [weak self] frame in
            self?.rememberExpandedFrame(frame)
        }
        view.onInteractionModeChanged = { [weak self] in
            self?.updateExpandedMousePolicy()
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
        let frame = stickyExpandedFrame.map {
            ExpandedPanelMetrics.fitExpandedFrameToVisibleScreen($0, visibleFrame: screen)
        } ?? ExpandedPanelMetrics.compactFrame(in: screen)
        expandedWindow.setFrame(frame, display: true)
        stickyExpandedFrame = expandedWindow.frame
    }

    private func rememberExpandedFrame(_ frame: NSRect) {
        guard !frame.isEmpty else { return }
        let screen = OverlayScreenPlacement.activeVisibleFrame()
        stickyExpandedFrame = ExpandedPanelMetrics.fitExpandedFrameToVisibleScreen(frame, visibleFrame: screen)
        if let stickyExpandedFrame {
            OverlayPlacementStore.saveExpandedFrame(stickyExpandedFrame, in: screen)
        }
    }

    private func collapse() {
        if let expandedWindow, expandedWindow.isVisible {
            rememberExpandedFrame(expandedWindow.frame)
        }
        expandedModeActive = false
        expandedWindow?.ignoresMouseEvents = isRemoteInputPassthroughActive
        expandedWindow?.orderOut(nil)
        pillWindow?.ignoresMouseEvents = isRemoteInputPassthroughActive
        bringPillToFront(force: true)
        emitSimple("hidden")
        emitLifecycle("collapsed")
    }

    private func hidePillWhileExpanded() {
        guard let pillWindow else { return }
        pillWindow.ignoresMouseEvents = true
        pillWindow.orderOut(nil)
        pillWindow.setIsVisible(false)
    }

    private func setRunState(_ state: PillRunState) {
        let wasCapturing = currentRunState == .listening || currentRunState == .connecting
        let isCapturing = state == .listening || state == .connecting
        currentRunState = state
        if isCapturing && !wasCapturing {
            expandedView?.prepareAutoSendListenCapture()
        }
        pillView?.setRunState(state)
        expandedView?.setListeningState(state)
    }

    private func toggleListeningFromPill() {
        let now = Date()
        if now.timeIntervalSince(lastPillRecordingToggleAt) < 0.45 {
            return
        }
        lastPillRecordingToggleAt = now

        expand()
        if currentRunState == .listening || currentRunState == .connecting {
            emitSimple("recording_stop_requested")
            setRunState(.paused)
            expandedView?.scheduleAutoSendAfterExternalStop()
        } else {
            expandedView?.prepareAutoSendListenCapture()
            emitSimple("recording_start_requested")
            setRunState(.connecting)
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
            let value = min(max(o, Double(minimumOverlayBackgroundOpacity)), 1.0)
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
        case .setPassthrough(let enabled, let durationMs):
            if enabled {
                allowRemoteInputPassthrough(
                    durationMs: durationMs ?? remoteInputPassthroughDefaultMs,
                    reason: "ipc")
            } else {
                clearRemoteInputPassthrough()
            }
        case .pushCard(let card):
            ensureExpandedWindow()
            expandedView?.pushCard(RenderedCard(
                id: card.id, kind: card.kind, title: card.title,
                body: card.body, done: true, costLabel: card.costLabel,
                artifact: card.artifact, attachments: card.attachments ?? []))
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
            artifact: nil,
            attachments: [])
        if signInURL != nil || title.localizedCaseInsensitiveContains("sign in") {
            view.showSignedOutLogin(url: signInURL)
            pillView?.setHealthState(.needsAttention)
        } else {
            view.showSignedInReady()
            pillView?.setHealthState(.ready)
        }
        view.pushCard(card)
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
        let frame = clampedPillFrame(NSRect(origin: origin, size: PillMetrics.size), in: screen)
        pillWindow.setFrame(frame, display: true)
        if pos == "center" {
            stickyPillFrame = nil
            OverlayPlacementStore.clearPillFrame()
        } else {
            stickyPillFrame = frame
            OverlayPlacementStore.savePillFrame(frame, in: screen)
        }
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

private func cgEventMask(_ types: [CGEventType]) -> CGEventMask {
    var mask = CGEventMask(0)
    for type in types {
        mask |= CGEventMask(1) << CGEventMask(type.rawValue)
    }
    return mask
}

private func activeRemoteControlAppLabel() -> String? {
    guard let app = NSWorkspace.shared.frontmostApplication else { return nil }
    guard app.processIdentifier != getpid() else { return nil }
    return remoteControlAppLabel(app)
}

private func remoteControlAppLabel(_ app: NSRunningApplication) -> String? {
    let name = app.localizedName ?? ""
    let bundle = app.bundleIdentifier ?? ""
    let executable = app.executableURL?.lastPathComponent ?? ""
    let haystack = [name, bundle, executable]
        .joined(separator: " ")
        .lowercased()
    guard remoteControlAppNeedles.contains(where: { haystack.contains($0) }) else {
        return nil
    }
    return compactRemoteControlLabel(name: name, bundle: bundle, executable: executable)
}

private func compactRemoteControlLabel(name: String, bundle: String, executable: String) -> String {
    let raw = [name, bundle, executable]
        .first { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
        ?? "remote-control"
    let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "._-"))
    let compact = raw.unicodeScalars.map { scalar -> Character in
        allowed.contains(scalar) ? Character(scalar) : "-"
    }
    let value = String(compact).trimmingCharacters(in: CharacterSet(charactersIn: "-._"))
    return value.isEmpty ? "remote-control" : String(value.prefix(48))
}

private func sourcePidForRemoteInputEvent(type: CGEventType, event: CGEvent) -> pid_t? {
    let mouseDownOrGesture: Set<CGEventType> = [
        .leftMouseDown, .leftMouseDragged,
        .rightMouseDown, .rightMouseDragged,
        .otherMouseDown, .otherMouseDragged,
        .scrollWheel,
    ]
    guard mouseDownOrGesture.contains(type) else { return nil }
    let pid = event.getIntegerValueField(.eventSourceUnixProcessID)
    guard pid > 0, pid <= Int64(Int32.max) else { return nil }
    return pid_t(pid)
}

private func blueyTrustedRemoteInputEventTapCallback(
    proxy: CGEventTapProxy,
    type: CGEventType,
    event: CGEvent,
    refcon: UnsafeMutableRawPointer?
) -> Unmanaged<CGEvent>? {
    guard let refcon else {
        return Unmanaged.passUnretained(event)
    }

    let trustedRemoteBridgeEvent =
        event.getIntegerValueField(.eventSourceUserData) == blueyTrustedRemoteInputEventSourceUserData
    let sourcePid = sourcePidForRemoteInputEvent(type: type, event: event)
    guard trustedRemoteBridgeEvent || sourcePid != nil else {
        return Unmanaged.passUnretained(event)
    }
    let app = Unmanaged<OverlayApp>.fromOpaque(refcon).takeUnretainedValue()
    if Thread.isMainThread {
        if trustedRemoteBridgeEvent {
            app.trustedRemoteInputEventDetected()
        } else {
            app.remoteControlInputEventDetected(sourcePid: sourcePid)
        }
    } else {
        DispatchQueue.main.sync {
            if trustedRemoteBridgeEvent {
                app.trustedRemoteInputEventDetected()
            } else {
                app.remoteControlInputEventDetected(sourcePid: sourcePid)
            }
        }
    }
    return Unmanaged.passUnretained(event)
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
