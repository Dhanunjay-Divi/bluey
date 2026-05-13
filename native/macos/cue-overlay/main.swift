import AppKit
import Foundation
import UniformTypeIdentifiers

private let savedFrameKey = "bluey.overlay.frame"
private let savedOpacityKey = "bluey.overlay.opacity"
private let savedModelKey = "bluey.overlay.model"
private let savedModeKey = "bluey.overlay.mode"
private let savedThemeKey = "bluey.overlay.theme"
private let savedCollapsedFrameKey = "bluey.overlay.collapsed.frame"

private enum BlueyTheme {
    static let ink = NSColor(calibratedRed: 0.008, green: 0.012, blue: 0.018, alpha: 1)
    static let panel = NSColor(calibratedRed: 0.014, green: 0.024, blue: 0.034, alpha: 1)
    static let blue = NSColor(calibratedRed: 0.20, green: 0.62, blue: 1.00, alpha: 1)
    static let cyan = NSColor(calibratedRed: 0.26, green: 0.92, blue: 0.96, alpha: 1)
    static let lime = NSColor(calibratedRed: 0.50, green: 0.96, blue: 0.62, alpha: 1)
    static let text = NSColor(calibratedRed: 0.92, green: 0.97, blue: 1.00, alpha: 1)
}

final class OverlayPanel: NSPanel {
    override var canBecomeKey: Bool {
        true
    }

    override var canBecomeMain: Bool {
        true
    }
}

final class CollapsedPillPanel: NSPanel {
    override var canBecomeKey: Bool {
        true
    }

    override var canBecomeMain: Bool {
        false
    }
}

final class PassthroughEffectView: NSVisualEffectView {
    weak var headerView: NSView?
    weak var composerView: NSView?
    weak var resumeView: NSView?
    weak var scrollView: NSView?
    var edgeThickness: CGFloat = 14

    override func hitTest(_ point: NSPoint) -> NSView? {
        if isPointInResizeEdge(point) {
            return super.hitTest(point)
        }

        for view in [headerView, composerView, resumeView] {
            guard let view, !view.isHidden, view.alphaValue > 0.01 else { continue }
            let rect = view.convert(view.bounds, to: self)
            if rect.contains(point) {
                return super.hitTest(point)
            }
        }

        return nil
    }

    private func isPointInResizeEdge(_ point: NSPoint) -> Bool {
        point.x <= edgeThickness ||
            point.x >= bounds.maxX - edgeThickness ||
            point.y <= edgeThickness ||
            point.y >= bounds.maxY - edgeThickness
    }
}

final class DragHeaderView: NSView {
    override func mouseDown(with event: NSEvent) {
        window?.performDrag(with: event)
    }
}

final class CenteredTextFieldCell: NSTextFieldCell {
    override func drawingRect(forBounds rect: NSRect) -> NSRect {
        let original = super.drawingRect(forBounds: rect)
        let height = cellSize(forBounds: rect).height
        let delta = max(0, (rect.height - height) / 2)
        return original.insetBy(dx: 0, dy: delta)
    }

    override func edit(withFrame rect: NSRect, in controlView: NSView, editor textObj: NSText, delegate: Any?, event: NSEvent?) {
        super.edit(withFrame: drawingRect(forBounds: rect), in: controlView, editor: textObj, delegate: delegate, event: event)
    }

    override func select(withFrame rect: NSRect, in controlView: NSView, editor textObj: NSText, delegate: Any?, start selStart: Int, length selLength: Int) {
        super.select(withFrame: drawingRect(forBounds: rect), in: controlView, editor: textObj, delegate: delegate, start: selStart, length: selLength)
    }
}

final class ResizeGripView: NSView {
    private var startFrame: NSRect = .zero
    private var startMouse: NSPoint = .zero

    override func hitTest(_ point: NSPoint) -> NSView? {
        bounds.contains(point) ? self : nil
    }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .crosshair)
    }

    override func mouseDown(with event: NSEvent) {
        guard let window else { return }
        startFrame = window.frame
        startMouse = NSEvent.mouseLocation
    }

    override func mouseDragged(with event: NSEvent) {
        guard let window else { return }
        let current = NSEvent.mouseLocation
        let dx = current.x - startMouse.x
        let dy = current.y - startMouse.y
        let minSize = window.minSize
        let visibleFrame = window.screen?.visibleFrame ?? NSScreen.main?.visibleFrame
        let maxWidth = visibleFrame.map { max(minSize.width, $0.maxX - startFrame.minX - 8) } ?? CGFloat.greatestFiniteMagnitude
        let maxHeight = visibleFrame.map { max(minSize.height, startFrame.maxY - $0.minY - 8) } ?? CGFloat.greatestFiniteMagnitude
        let width = min(max(minSize.width, startFrame.width + dx), maxWidth)
        let height = min(max(minSize.height, startFrame.height - dy), maxHeight)
        let frame = NSRect(
            x: startFrame.minX,
            y: startFrame.maxY - height,
            width: width,
            height: height
        )
        window.setFrame(frame, display: true, animate: false)
    }

    override func mouseUp(with event: NSEvent) {
        if let frame = window?.frame {
            UserDefaults.standard.set(NSStringFromRect(frame), forKey: savedFrameKey)
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        let plate = NSBezierPath(roundedRect: bounds.insetBy(dx: 1, dy: 1), xRadius: 9, yRadius: 9)
        BlueyTheme.blue.withAlphaComponent(0.18).setFill()
        plate.fill()
        BlueyTheme.cyan.withAlphaComponent(0.36).setStroke()
        plate.lineWidth = 1
        plate.stroke()

        BlueyTheme.text.withAlphaComponent(0.58).setStroke()

        let path = NSBezierPath()
        path.lineWidth = 1.8
        for offset in stride(from: CGFloat(7), through: CGFloat(21), by: CGFloat(6)) {
            path.move(to: NSPoint(x: bounds.maxX - offset, y: bounds.minY + 5))
            path.line(to: NSPoint(x: bounds.maxX - 5, y: bounds.minY + offset))
        }
        path.stroke()
    }
}

final class ResizeFrameOverlayView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        let rect = bounds.insetBy(dx: 0.75, dy: 0.75)
        let path = NSBezierPath(roundedRect: rect, xRadius: 28, yRadius: 28)
        BlueyTheme.blue.withAlphaComponent(0.16).setStroke()
        path.lineWidth = 1.5
        path.stroke()

        BlueyTheme.cyan.withAlphaComponent(0.28).setStroke()
        let corner = NSBezierPath()
        corner.lineWidth = 2
        let l: CGFloat = 22
        corner.move(to: NSPoint(x: rect.minX + 14, y: rect.maxY))
        corner.line(to: NSPoint(x: rect.minX + 14 + l, y: rect.maxY))
        corner.move(to: NSPoint(x: rect.minX, y: rect.maxY - 14))
        corner.line(to: NSPoint(x: rect.minX, y: rect.maxY - 14 - l))
        corner.move(to: NSPoint(x: rect.maxX - 14, y: rect.minY))
        corner.line(to: NSPoint(x: rect.maxX - 14 - l, y: rect.minY))
        corner.move(to: NSPoint(x: rect.maxX, y: rect.minY + 14))
        corner.line(to: NSPoint(x: rect.maxX, y: rect.minY + 14 + l))
        corner.stroke()
    }
}

final class BlueyMarkView: NSView {
    var showWordmark = true
    var statusOn = true
    var lightTheme = false {
        didSet {
            needsDisplay = true
        }
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        let markRect = NSRect(x: 0, y: bounds.midY - 14, width: 28, height: 28)
        Self.drawMark(in: markRect)

        if showWordmark {
            let title = "Bluey" as NSString
            let attrs: [NSAttributedString.Key: Any] = [
                .foregroundColor: lightTheme
                    ? NSColor(calibratedRed: 0.018, green: 0.045, blue: 0.068, alpha: 0.95)
                    : BlueyTheme.text.withAlphaComponent(0.95),
                .font: NSFont.systemFont(ofSize: 14, weight: .semibold)
            ]
            title.draw(at: NSPoint(x: 36, y: bounds.midY - 8), withAttributes: attrs)
        }

        let dotRect = NSRect(x: bounds.maxX - 13, y: bounds.midY - 4, width: 8, height: 8)
        (statusOn ? BlueyTheme.lime : NSColor.systemRed).withAlphaComponent(0.94).setFill()
        NSBezierPath(ovalIn: dotRect).fill()
    }

    static func drawMark(in rect: NSRect) {
        let shape = NSBezierPath(roundedRect: rect, xRadius: rect.width * 0.28, yRadius: rect.height * 0.28)
        let gradient = NSGradient(colors: [
            NSColor(calibratedRed: 0.035, green: 0.095, blue: 0.155, alpha: 1),
            NSColor(calibratedRed: 0.052, green: 0.190, blue: 0.300, alpha: 1),
            NSColor(calibratedRed: 0.070, green: 0.300, blue: 0.420, alpha: 1)
        ])
        gradient?.draw(in: shape, angle: -35)

        let terminal = NSBezierPath(roundedRect: rect.insetBy(dx: rect.width * 0.22, dy: rect.height * 0.29),
                                    xRadius: rect.width * 0.12,
                                    yRadius: rect.width * 0.12)
        BlueyTheme.ink.withAlphaComponent(0.96).setFill()
        terminal.fill()
        BlueyTheme.cyan.withAlphaComponent(0.88).setStroke()
        terminal.lineWidth = rect.width * 0.045
        terminal.stroke()

        let chevron = NSBezierPath()
        chevron.move(to: NSPoint(x: rect.minX + rect.width * 0.33, y: rect.minY + rect.height * 0.60))
        chevron.line(to: NSPoint(x: rect.minX + rect.width * 0.43, y: rect.minY + rect.height * 0.50))
        chevron.line(to: NSPoint(x: rect.minX + rect.width * 0.33, y: rect.minY + rect.height * 0.40))
        NSColor.white.withAlphaComponent(0.96).setStroke()
        chevron.lineWidth = rect.width * 0.055
        chevron.lineCapStyle = .round
        chevron.lineJoinStyle = .round
        chevron.stroke()

        let promptLine = NSBezierPath()
        promptLine.move(to: NSPoint(x: rect.minX + rect.width * 0.50, y: rect.minY + rect.height * 0.39))
        promptLine.line(to: NSPoint(x: rect.minX + rect.width * 0.66, y: rect.minY + rect.height * 0.39))
        BlueyTheme.blue.withAlphaComponent(0.95).setStroke()
        promptLine.lineWidth = rect.width * 0.055
        promptLine.lineCapStyle = .round
        promptLine.stroke()

        let sparkCenter = NSPoint(x: rect.minX + rect.width * 0.72, y: rect.minY + rect.height * 0.72)
        let spark = NSBezierPath()
        spark.move(to: NSPoint(x: sparkCenter.x, y: sparkCenter.y + rect.height * 0.15))
        spark.line(to: NSPoint(x: sparkCenter.x + rect.width * 0.04, y: sparkCenter.y + rect.height * 0.04))
        spark.line(to: NSPoint(x: sparkCenter.x + rect.width * 0.15, y: sparkCenter.y))
        spark.line(to: NSPoint(x: sparkCenter.x + rect.width * 0.04, y: sparkCenter.y - rect.height * 0.04))
        spark.line(to: NSPoint(x: sparkCenter.x, y: sparkCenter.y - rect.height * 0.15))
        spark.line(to: NSPoint(x: sparkCenter.x - rect.width * 0.04, y: sparkCenter.y - rect.height * 0.04))
        spark.line(to: NSPoint(x: sparkCenter.x - rect.width * 0.15, y: sparkCenter.y))
        spark.line(to: NSPoint(x: sparkCenter.x - rect.width * 0.04, y: sparkCenter.y + rect.height * 0.04))
        spark.close()
        BlueyTheme.lime.withAlphaComponent(0.98).setFill()
        spark.fill()
    }
}

final class ChipControl: NSControl {
    private let chipTitle: String
    private let image: NSImage?
    private var pressed = false
    var lightTheme = false {
        didSet {
            needsDisplay = true
        }
    }

    init(title: String, systemName: String, target: AnyObject?, action: Selector?) {
        chipTitle = title
        image = NSImage(systemSymbolName: systemName, accessibilityDescription: title)
        super.init(frame: .zero)
        self.target = target
        self.action = action
        translatesAutoresizingMaskIntoConstraints = false
        toolTip = title
        wantsLayer = true
    }

    required init?(coder: NSCoder) {
        nil
    }

    override func mouseDown(with event: NSEvent) {
        pressed = true
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        let shouldSend = bounds.contains(convert(event.locationInWindow, from: nil))
        pressed = false
        needsDisplay = true
        if shouldSend {
            sendAction(action, to: target)
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        let rect = bounds.insetBy(dx: 0.75, dy: 0.75)
        let path = NSBezierPath(roundedRect: rect, xRadius: rect.height / 2, yRadius: rect.height / 2)
        let fill = lightTheme
            ? (
                pressed
                    ? NSColor(calibratedRed: 0.73, green: 0.90, blue: 1.00, alpha: 0.96)
                    : NSColor(calibratedRed: 0.96, green: 0.99, blue: 1.00, alpha: 0.90)
            )
            : (
                pressed
                    ? BlueyTheme.blue.withAlphaComponent(0.34)
                    : NSColor(calibratedRed: 0.025, green: 0.060, blue: 0.090, alpha: 0.78)
            )
        fill.setFill()
        path.fill()
        (pressed ? BlueyTheme.cyan : BlueyTheme.blue).withAlphaComponent(pressed ? 0.74 : 0.46).setStroke()
        path.lineWidth = 1.35
        path.stroke()

        let attrs: [NSAttributedString.Key: Any] = [
            .foregroundColor: lightTheme
                ? NSColor(calibratedRed: 0.03, green: 0.08, blue: 0.12, alpha: 0.94)
                : BlueyTheme.text.withAlphaComponent(0.92),
            .font: NSFont.systemFont(ofSize: 12, weight: .semibold)
        ]
        let title = chipTitle as NSString
        let titleSize = title.size(withAttributes: attrs)
        let iconSize: CGFloat = image == nil ? 0 : 13
        let gap: CGFloat = image == nil ? 0 : 6
        let totalWidth = iconSize + gap + titleSize.width
        var x = bounds.midX - totalWidth / 2
        let centerY = bounds.midY

        if let image {
            image.isTemplate = true
            NSColor.white.withAlphaComponent(0.96).set()
            image.draw(
                in: NSRect(x: x, y: centerY - iconSize / 2, width: iconSize, height: iconSize),
                from: .zero,
                operation: .sourceOver,
                fraction: 0.95
            )
            x += iconSize + gap
        }
        title.draw(at: NSPoint(x: x, y: centerY - titleSize.height / 2), withAttributes: attrs)
    }
}

final class CollapsedPillView: NSView {
    weak var controller: OverlayController?
    var ignoreClicksUntil: Date?
    var lightTheme = false {
        didSet {
            needsDisplay = true
        }
    }
    private var dragStartMouse: NSPoint = .zero
    private var dragStartFrame: NSRect = .zero
    private var didDrag = false

    override var acceptsFirstResponder: Bool {
        true
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        bounds.contains(point) ? self : nil
    }

    override func mouseDown(with event: NSEvent) {
        dragStartMouse = NSEvent.mouseLocation
        dragStartFrame = window?.frame ?? .zero
        didDrag = false
        window?.makeFirstResponder(self)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let window else { return }
        let current = NSEvent.mouseLocation
        let dx = current.x - dragStartMouse.x
        let dy = current.y - dragStartMouse.y
        if abs(dx) > 2 || abs(dy) > 2 {
            didDrag = true
        }
        var origin = NSPoint(x: dragStartFrame.minX + dx, y: dragStartFrame.minY + dy)
        let screenFrame = window.screen?.visibleFrame ?? NSScreen.main?.visibleFrame
        if let screenFrame {
            origin.x = min(max(origin.x, screenFrame.minX + 8), screenFrame.maxX - dragStartFrame.width - 8)
            origin.y = min(max(origin.y, screenFrame.minY + 8), screenFrame.maxY - dragStartFrame.height - 8)
        }
        window.setFrameOrigin(origin)
    }

    override func mouseUp(with event: NSEvent) {
        if let ignoreClicksUntil, Date() < ignoreClicksUntil {
            return
        }
        if didDrag {
            if let frame = window?.frame {
                UserDefaults.standard.set(NSStringFromRect(frame), forKey: savedCollapsedFrameKey)
            }
            return
        }
        controller?.expandFromPill()
    }

    override func keyDown(with event: NSEvent) {
        switch event.keyCode {
        case 36, 49:
            controller?.expandFromPill()
        default:
            super.keyDown(with: event)
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)

        let rect = bounds.insetBy(dx: 0.5, dy: 0.5)
        let path = NSBezierPath(roundedRect: rect, xRadius: rect.height / 2, yRadius: rect.height / 2)
        (lightTheme
            ? NSColor(calibratedRed: 0.96, green: 0.985, blue: 1.00, alpha: 0.90)
            : BlueyTheme.panel.withAlphaComponent(0.86)
        ).setFill()
        path.fill()
        BlueyTheme.cyan.withAlphaComponent(lightTheme ? 0.38 : 0.45).setStroke()
        path.lineWidth = 1.2
        path.stroke()

        BlueyMarkView.drawMark(in: NSRect(x: 10, y: bounds.midY - 13, width: 26, height: 26))

        let title = "Bluey" as NSString
        let attributes: [NSAttributedString.Key: Any] = [
            .foregroundColor: lightTheme
                ? NSColor(calibratedRed: 0.018, green: 0.045, blue: 0.068, alpha: 0.95)
                : BlueyTheme.text.withAlphaComponent(0.95),
            .font: NSFont.systemFont(ofSize: 12, weight: .semibold)
        ]
        title.draw(at: NSPoint(x: 43, y: bounds.midY - 7), withAttributes: attributes)

        let dotRect = NSRect(x: bounds.maxX - 17, y: bounds.maxY - 16, width: 7, height: 7)
        BlueyTheme.lime.withAlphaComponent(0.94).setFill()
        NSBezierPath(ovalIn: dotRect).fill()
    }
}

final class OverlayController: NSObject, NSWindowDelegate {
    private let panel: NSPanel
    private let stack: NSStackView
    private var collapsedPanel: NSPanel?
    private weak var collapsedPillView: CollapsedPillView?
    private var effectView: NSVisualEffectView!
    private var headerView: NSView!
    private var brandView: BlueyMarkView!
    private var scrollView: NSScrollView!
    private var composerView: NSView!
    private var composerCardView: NSView!
    private var composerHeightConstraint: NSLayoutConstraint!
    private var composerContentConstraints: [NSLayoutConstraint] = []
    private var questionField: NSTextField!
    private var modelPicker: NSPopUpButton!
    private var modePicker: NSPopUpButton!
    private var themeButton: NSButton!
    private var recordingButton: NSButton!
    private var recordingDot: NSView!
    private var resumeLatestButton: NSButton!
    private var themedButtons: [NSButton] = []
    private var chipControls: [ChipControl] = []
    private var cardHistory: [[String: Any]] = []
    private var cardViewsById: [String: NSView] = [:]
    private var cardBodyLabels: [String: NSTextField] = [:]
    private var cardBodyKinds: [String: String] = [:]
    private var answerBodyStacks: [String: NSStackView] = [:]
    private var localEventMonitor: Any?
    private var globalEventMonitor: Any?
    private var localScrollEventMonitor: Any?
    private var globalScrollEventMonitor: Any?
    private var opacitySlider: NSSlider!
    private var backgroundOpacity: CGFloat = 0.92
    private var recordingActive = false
    private var lightTheme = false
    private var feedTailPinned = true
    private var feedHasUnreadLatest = false
    private var position: String = "top_right"
    private var bootTimer: Timer?

    override init() {
        let initialRect = NSRect(x: 0, y: 0, width: 860, height: 460)
        panel = OverlayPanel(
            contentRect: initialRect,
            styleMask: [.titled, .fullSizeContentView, .resizable, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        stack = NSStackView()
        super.init()
        setupWindow()
        setupCollapsedPill()
        restoreOpacity()
        restoreTheme()
        setupContent()
        applyTheme()
        updateRecordingButton()
        restoreFrameOrPosition()
        setupHotkeys()
        emit(["type": "ready", "platform": "macos", "capture_excluded": true])
    }

    deinit {
        for monitor in [localEventMonitor, globalEventMonitor, localScrollEventMonitor, globalScrollEventMonitor] {
            if let monitor {
                NSEvent.removeMonitor(monitor)
            }
        }
    }

    func startReadingCommands() {
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            while let line = readLine() {
                self?.handleLine(line)
            }
            DispatchQueue.main.async {
                NSApplication.shared.terminate(nil)
            }
        }
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        collapseToPill()
        return false
    }

    private func setupWindow() {
        panel.delegate = self
        panel.isReleasedWhenClosed = false
        panel.isFloatingPanel = true
        panel.level = .screenSaver
        panel.collectionBehavior = [
            .canJoinAllSpaces,
            .fullScreenAuxiliary,
            .stationary,
            .ignoresCycle
        ]
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        panel.backgroundColor = .clear
        panel.isOpaque = false
        panel.hasShadow = true
        panel.ignoresMouseEvents = false
        panel.hidesOnDeactivate = false
        panel.canHide = false
        panel.minSize = NSSize(width: 820, height: 320)
        applyCaptureExclusion()

        panel.standardWindowButton(.closeButton)?.isHidden = true
        panel.standardWindowButton(.miniaturizeButton)?.isHidden = true
        panel.standardWindowButton(.zoomButton)?.isHidden = true
    }

    private func applyCaptureExclusion() {
        panel.sharingType = .none
        panel.level = .screenSaver
        collapsedPanel?.sharingType = .none
        collapsedPanel?.level = .screenSaver
    }

    private func setupCollapsedPill() {
        let pill = CollapsedPillPanel(
            contentRect: NSRect(x: 0, y: 0, width: 96, height: 42),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        pill.isReleasedWhenClosed = false
        pill.isFloatingPanel = true
        pill.level = .screenSaver
        pill.collectionBehavior = [
            .canJoinAllSpaces,
            .fullScreenAuxiliary,
            .stationary,
            .ignoresCycle
        ]
        pill.backgroundColor = .clear
        pill.isOpaque = false
        pill.hasShadow = true
        pill.ignoresMouseEvents = false
        pill.canHide = false
        pill.sharingType = .none

        let shell = CollapsedPillView(frame: NSRect(x: 0, y: 0, width: 96, height: 42))
        shell.autoresizingMask = [.width, .height]
        shell.controller = self
        shell.lightTheme = lightTheme
        shell.toolTip = "Show Bluey"
        pill.contentView = shell
        collapsedPillView = shell
        collapsedPanel = pill
    }

    private func setupContent() {
        let effect = PassthroughEffectView()
        effectView = effect
        effect.translatesAutoresizingMaskIntoConstraints = false
        effect.material = .hudWindow
        effect.blendingMode = .behindWindow
        effect.state = .active
        effect.wantsLayer = true
        effect.layer?.cornerRadius = 30
        effect.layer?.masksToBounds = true
        effect.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.16).cgColor
        effect.layer?.borderWidth = 1

        let header = makeHeader()
        headerView = header
        let composer = makeComposer()
        let resumeButton = makeResumeLatestButton()
        effect.headerView = header
        effect.composerView = composer
        effect.resumeView = resumeButton
        let frameHint = ResizeFrameOverlayView()
        frameHint.translatesAutoresizingMaskIntoConstraints = false
        let grip = ResizeGripView()
        grip.translatesAutoresizingMaskIntoConstraints = false
        let scroll = NSScrollView()
        scrollView = scroll
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.scrollerStyle = .overlay
        scroll.verticalScrollElasticity = .allowed
        scroll.borderType = .noBorder
        effect.scrollView = scroll

        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 12
        stack.edgeInsets = NSEdgeInsets(top: 12, left: 14, bottom: 14, right: 14)
        stack.translatesAutoresizingMaskIntoConstraints = false

        scroll.documentView = stack
        panel.contentView = effect
        effect.addSubview(frameHint)
        effect.addSubview(header)
        effect.addSubview(composer)
        effect.addSubview(scroll)
        effect.addSubview(resumeButton)
        effect.addSubview(grip)

        composerHeightConstraint = composer.heightAnchor.constraint(equalToConstant: 118)
        let headerPreferredWidth = header.widthAnchor.constraint(equalTo: effect.widthAnchor, multiplier: 0.88)
        headerPreferredWidth.priority = .defaultHigh
        NSLayoutConstraint.activate([
            frameHint.leadingAnchor.constraint(equalTo: effect.leadingAnchor),
            frameHint.trailingAnchor.constraint(equalTo: effect.trailingAnchor),
            frameHint.topAnchor.constraint(equalTo: effect.topAnchor),
            frameHint.bottomAnchor.constraint(equalTo: effect.bottomAnchor),

            header.centerXAnchor.constraint(equalTo: effect.centerXAnchor),
            headerPreferredWidth,
            header.widthAnchor.constraint(lessThanOrEqualToConstant: 840),
            header.widthAnchor.constraint(greaterThanOrEqualToConstant: 760),
            header.topAnchor.constraint(equalTo: effect.topAnchor, constant: 8),
            header.heightAnchor.constraint(equalToConstant: 54),

            composer.leadingAnchor.constraint(equalTo: effect.leadingAnchor),
            composer.trailingAnchor.constraint(equalTo: effect.trailingAnchor),
            composer.bottomAnchor.constraint(equalTo: effect.bottomAnchor),
            composerHeightConstraint,

            scroll.leadingAnchor.constraint(equalTo: effect.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: effect.trailingAnchor),
            scroll.topAnchor.constraint(equalTo: header.bottomAnchor, constant: 12),
            scroll.bottomAnchor.constraint(equalTo: composer.topAnchor, constant: -10),
            stack.widthAnchor.constraint(equalTo: scroll.contentView.widthAnchor),

            resumeButton.trailingAnchor.constraint(equalTo: effect.trailingAnchor, constant: -24),
            resumeButton.bottomAnchor.constraint(equalTo: composer.topAnchor, constant: -12),
            resumeButton.widthAnchor.constraint(equalToConstant: 118),
            resumeButton.heightAnchor.constraint(equalToConstant: 32),

            grip.trailingAnchor.constraint(equalTo: effect.trailingAnchor, constant: -8),
            grip.bottomAnchor.constraint(equalTo: effect.bottomAnchor, constant: -8),
            grip.widthAnchor.constraint(equalToConstant: 30),
            grip.heightAnchor.constraint(equalToConstant: 30)
        ])
        NSLayoutConstraint.activate(composerContentConstraints)
    }

    private func makeHeader() -> NSView {
        let header = DragHeaderView()
        header.translatesAutoresizingMaskIntoConstraints = false
        header.wantsLayer = true
        header.layer?.backgroundColor = surfaceColor().cgColor
        header.layer?.cornerRadius = 27
        header.layer?.borderColor = controlBorderColor().cgColor
        header.layer?.borderWidth = 1

        let brand = BlueyMarkView()
        brandView = brand
        brand.translatesAutoresizingMaskIntoConstraints = false
        brand.lightTheme = lightTheme
        brand.toolTip = "Bluey is running and connected to the overlay"

        opacitySlider = NSSlider(value: Double(backgroundOpacity), minValue: 0.18, maxValue: 1.0, target: self, action: #selector(opacityChanged))
        opacitySlider.translatesAutoresizingMaskIntoConstraints = false
        opacitySlider.controlSize = .small
        opacitySlider.toolTip = "Opacity"

        let askButton = iconButton(systemName: "sparkles", fallback: "Focus ask box", action: #selector(askPressed))
        let helpButton = iconButton(systemName: "questionmark.circle", fallback: "Explain controls", action: #selector(helpPressed))
        let sessionButton = iconButton(systemName: "plus.rectangle.on.folder", fallback: "Session", action: #selector(sessionPressed))
        let attachButton = iconButton(systemName: "paperclip", fallback: "Attach session files", action: #selector(attachPressed))
        themeButton = iconButton(systemName: "circle.lefthalf.filled", fallback: "Toggle black/white background", action: #selector(themePressed))
        let instructionsButton = iconButton(systemName: "note.text", fallback: "Set answer rules", action: #selector(instructionsPressed))
        let clearButton = iconButton(systemName: "trash", fallback: "Clear", action: #selector(clearPressed))
        let hideButton = iconButton(systemName: "eye.slash", fallback: "Hide", action: #selector(hidePressed))
        let closeButton = iconButton(systemName: "xmark", fallback: "Quit Bluey", action: #selector(closePressed))

        modelPicker = makePicker(
            items: ["Bluey Auto", "OpenAI Direct", "Groq Realtime", "Cerebras Fast", "Local"],
            savedKey: savedModelKey,
            tooltip: "Model route",
            action: #selector(modelChanged)
        )
        modePicker = makePicker(
            items: ["General", "Code", "System Design", "Meeting", "Writing"],
            savedKey: savedModeKey,
            tooltip: "Answer mode",
            action: #selector(modeChanged)
        )

        header.addSubview(brand)
        header.addSubview(askButton)
        header.addSubview(helpButton)
        header.addSubview(sessionButton)
        header.addSubview(modelPicker)
        header.addSubview(modePicker)
        header.addSubview(opacitySlider)
        header.addSubview(attachButton)
        header.addSubview(themeButton)
        header.addSubview(instructionsButton)
        header.addSubview(clearButton)
        header.addSubview(hideButton)
        header.addSubview(closeButton)

        NSLayoutConstraint.activate([
            brand.leadingAnchor.constraint(equalTo: header.leadingAnchor, constant: 12),
            brand.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            brand.widthAnchor.constraint(equalToConstant: 94),
            brand.heightAnchor.constraint(equalToConstant: 34),

            askButton.leadingAnchor.constraint(equalTo: brand.trailingAnchor, constant: 8),
            askButton.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            helpButton.leadingAnchor.constraint(equalTo: askButton.trailingAnchor, constant: 8),
            helpButton.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            sessionButton.leadingAnchor.constraint(equalTo: helpButton.trailingAnchor, constant: 8),
            sessionButton.centerYAnchor.constraint(equalTo: header.centerYAnchor),

            closeButton.trailingAnchor.constraint(equalTo: header.trailingAnchor, constant: -12),
            closeButton.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            hideButton.trailingAnchor.constraint(equalTo: closeButton.leadingAnchor, constant: -8),
            hideButton.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            clearButton.trailingAnchor.constraint(equalTo: hideButton.leadingAnchor, constant: -8),
            clearButton.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            instructionsButton.trailingAnchor.constraint(equalTo: clearButton.leadingAnchor, constant: -6),
            instructionsButton.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            themeButton.trailingAnchor.constraint(equalTo: instructionsButton.leadingAnchor, constant: -6),
            themeButton.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            attachButton.trailingAnchor.constraint(equalTo: themeButton.leadingAnchor, constant: -6),
            attachButton.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            opacitySlider.trailingAnchor.constraint(equalTo: attachButton.leadingAnchor, constant: -10),
            opacitySlider.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            opacitySlider.widthAnchor.constraint(equalToConstant: 48),

            modePicker.trailingAnchor.constraint(equalTo: opacitySlider.leadingAnchor, constant: -10),
            modePicker.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            modePicker.widthAnchor.constraint(equalToConstant: 128),
            modelPicker.trailingAnchor.constraint(equalTo: modePicker.leadingAnchor, constant: -10),
            modelPicker.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            modelPicker.widthAnchor.constraint(equalToConstant: 126),

            modelPicker.leadingAnchor.constraint(greaterThanOrEqualTo: sessionButton.trailingAnchor, constant: 14)
        ])

        return header
    }

    private func makePicker(items: [String], savedKey: String, tooltip: String, action: Selector) -> NSPopUpButton {
        let picker = NSPopUpButton()
        picker.translatesAutoresizingMaskIntoConstraints = false
        picker.controlSize = .small
        picker.bezelStyle = .texturedRounded
        picker.isBordered = false
        picker.wantsLayer = true
        picker.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.48).cgColor
        picker.layer?.cornerRadius = 12
        picker.layer?.borderColor = NSColor.white.withAlphaComponent(0.08).cgColor
        picker.layer?.borderWidth = 1
        picker.contentTintColor = NSColor.white.withAlphaComponent(0.88)
        picker.toolTip = tooltip
        picker.target = self
        picker.action = action
        picker.addItems(withTitles: items)

        if let saved = UserDefaults.standard.string(forKey: savedKey),
           items.contains(saved) {
            picker.selectItem(withTitle: saved)
        }

        return picker
    }

    private func makeComposer() -> NSView {
        let outer = NSView()
        outer.translatesAutoresizingMaskIntoConstraints = false
        outer.isHidden = false
        outer.alphaValue = 1
        composerView = outer

        let card = NSView()
        composerCardView = card
        card.translatesAutoresizingMaskIntoConstraints = false
        card.wantsLayer = true
        card.layer?.backgroundColor = surfaceColor().cgColor
        card.layer?.cornerRadius = 28
        card.layer?.borderColor = controlBorderColor().cgColor
        card.layer?.borderWidth = 1

        questionField = NSTextField()
        questionField.translatesAutoresizingMaskIntoConstraints = false
        questionField.cell = CenteredTextFieldCell(textCell: "")
        questionField.font = NSFont.systemFont(ofSize: 14, weight: .medium)
        questionField.textColor = activeTextColor().withAlphaComponent(0.94)
        questionField.placeholderString = "Ask me anything..."
        questionField.backgroundColor = inputBackgroundColor()
        questionField.isBezeled = false
        questionField.isBordered = false
        questionField.focusRingType = .none
        questionField.target = self
        questionField.action = #selector(sendQuestion)
        questionField.lineBreakMode = .byTruncatingTail
        questionField.cell?.usesSingleLineMode = true
        questionField.cell?.wraps = false
        questionField.alignment = .left
        questionField.wantsLayer = true
        questionField.layer?.cornerRadius = 18
        questionField.layer?.borderColor = controlBorderColor().cgColor
        questionField.layer?.borderWidth = 1
        questionField.setContentHuggingPriority(.defaultLow, for: .horizontal)
        questionField.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        let recordWrap = NSView()
        recordWrap.translatesAutoresizingMaskIntoConstraints = false
        recordingButton = trayIconButton(
            systemName: "mic",
            fallback: "Start recording",
            action: #selector(recordingPressed),
            background: BlueyTheme.ink.withAlphaComponent(0.90)
        )
        recordingDot = NSView()
        recordingDot.translatesAutoresizingMaskIntoConstraints = false
        recordingDot.wantsLayer = true
        recordingDot.layer?.backgroundColor = BlueyTheme.lime.withAlphaComponent(0.22).cgColor
        recordingDot.layer?.cornerRadius = 4
        recordingDot.toolTip = "Dim means recording is off; bright green means recording is on"
        recordWrap.addSubview(recordingButton)
        recordWrap.addSubview(recordingDot)

        let sendButton = trayIconButton(
            systemName: "paperplane.fill",
            fallback: "Send question",
            action: #selector(sendQuestion),
            background: BlueyTheme.blue.withAlphaComponent(0.88)
        )

        let inputRow = NSStackView()
        inputRow.translatesAutoresizingMaskIntoConstraints = false
        inputRow.orientation = .horizontal
        inputRow.alignment = .centerY
        inputRow.spacing = 9
        inputRow.distribution = .fill
        inputRow.addArrangedSubview(recordWrap)
        inputRow.addArrangedSubview(questionField)
        inputRow.addArrangedSubview(sendButton)

        let answerButton = chipButton("Answer", systemName: "bolt.fill", action: #selector(answerPressed))
        answerButton.toolTip = "Ask using the current text box"
        let recapButton = chipButton("Recap", systemName: "text.badge.checkmark", action: #selector(recapPressed))
        let capturePageButton = chipButton("Analyse Screen", systemName: "magnifyingglass", action: #selector(pagePressed))
        capturePageButton.toolTip = "Analyse the active browser page or screen context"

        let row = NSStackView()
        row.translatesAutoresizingMaskIntoConstraints = false
        row.orientation = .horizontal
        row.alignment = .centerY
        row.spacing = 10
        row.distribution = .fillEqually
        row.addArrangedSubview(answerButton)
        row.addArrangedSubview(recapButton)
        row.addArrangedSubview(capturePageButton)

        outer.addSubview(card)
        card.addSubview(inputRow)
        card.addSubview(row)

        let cardPreferredWidth = card.widthAnchor.constraint(equalTo: outer.widthAnchor, multiplier: 0.72)
        cardPreferredWidth.priority = .defaultHigh
        composerContentConstraints = [
            card.centerXAnchor.constraint(equalTo: outer.centerXAnchor),
            cardPreferredWidth,
            card.widthAnchor.constraint(lessThanOrEqualToConstant: 760),
            card.widthAnchor.constraint(greaterThanOrEqualToConstant: 540),
            card.topAnchor.constraint(equalTo: outer.topAnchor, constant: 8),
            card.bottomAnchor.constraint(equalTo: outer.bottomAnchor, constant: -10)
        ]

        NSLayoutConstraint.activate([
            recordWrap.widthAnchor.constraint(equalToConstant: 44),
            recordWrap.heightAnchor.constraint(equalToConstant: 38),
            recordingButton.leadingAnchor.constraint(equalTo: recordWrap.leadingAnchor),
            recordingButton.centerYAnchor.constraint(equalTo: recordWrap.centerYAnchor),
            recordingDot.trailingAnchor.constraint(equalTo: recordWrap.trailingAnchor, constant: -4),
            recordingDot.topAnchor.constraint(equalTo: recordWrap.topAnchor, constant: 2),
            recordingDot.widthAnchor.constraint(equalToConstant: 8),
            recordingDot.heightAnchor.constraint(equalToConstant: 8),

            inputRow.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 10),
            inputRow.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -10),
            inputRow.topAnchor.constraint(equalTo: card.topAnchor, constant: 9),
            questionField.heightAnchor.constraint(equalToConstant: 38),

            row.centerXAnchor.constraint(equalTo: card.centerXAnchor),
            row.widthAnchor.constraint(equalToConstant: 440),
            row.topAnchor.constraint(equalTo: inputRow.bottomAnchor, constant: 9),
            row.bottomAnchor.constraint(equalTo: card.bottomAnchor, constant: -9)
        ])

        return outer
    }

    private func iconButton(systemName: String, fallback: String, action: Selector) -> NSButton {
        let button = NSButton()
        button.translatesAutoresizingMaskIntoConstraints = false
        button.bezelStyle = .regularSquare
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.backgroundColor = controlBackgroundColor().cgColor
        button.layer?.cornerRadius = 17
        button.layer?.borderColor = controlBorderColor().cgColor
        button.layer?.borderWidth = 1
        button.contentTintColor = activeTextColor().withAlphaComponent(0.88)
        button.target = self
        button.action = action
        button.toolTip = fallback
        themedButtons.append(button)

        if let image = NSImage(systemSymbolName: systemName, accessibilityDescription: fallback) {
            button.image = image
            button.imagePosition = .imageOnly
            button.title = ""
        } else {
            button.title = fallback
        }

        NSLayoutConstraint.activate([
            button.widthAnchor.constraint(equalToConstant: 32),
            button.heightAnchor.constraint(equalToConstant: 32)
        ])
        return button
    }

    private func trayIconButton(systemName: String, fallback: String, action: Selector, background: NSColor) -> NSButton {
        let button = NSButton()
        button.translatesAutoresizingMaskIntoConstraints = false
        button.bezelStyle = .regularSquare
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.backgroundColor = background.cgColor
        button.layer?.cornerRadius = 17
        button.layer?.borderColor = controlBorderColor().cgColor
        button.layer?.borderWidth = 1
        button.contentTintColor = activeTextColor().withAlphaComponent(0.94)
        button.target = self
        button.action = action
        button.toolTip = fallback
        themedButtons.append(button)

        if let image = NSImage(systemSymbolName: systemName, accessibilityDescription: fallback) {
            button.image = image
            button.imagePosition = .imageOnly
            button.title = ""
        } else {
            button.title = fallback
        }

        NSLayoutConstraint.activate([
            button.widthAnchor.constraint(equalToConstant: 38),
            button.heightAnchor.constraint(equalToConstant: 34)
        ])
        return button
    }

    private func chipButton(_ title: String, systemName: String, action: Selector) -> NSControl {
        let button = ChipControl(title: title, systemName: systemName, target: self, action: action)
        button.lightTheme = lightTheme
        chipControls.append(button)
        NSLayoutConstraint.activate([
            button.widthAnchor.constraint(equalToConstant: title.count > 8 ? 142 : 118),
            button.heightAnchor.constraint(equalToConstant: 30)
        ])
        return button
    }

    private func makeResumeLatestButton() -> NSButton {
        let button = NSButton(title: "Latest", target: self, action: #selector(resumeLatestPressed))
        resumeLatestButton = button
        button.translatesAutoresizingMaskIntoConstraints = false
        button.bezelStyle = .regularSquare
        button.isBordered = false
        button.image = NSImage(systemSymbolName: "arrow.down.circle.fill", accessibilityDescription: "Jump to latest")
        button.imagePosition = .imageLeading
        button.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
        button.toolTip = "Jump to the latest Bluey answer"
        button.wantsLayer = true
        button.layer?.cornerRadius = 16
        button.isHidden = true
        button.alphaValue = 0
        themedButtons.append(button)
        styleResumeLatestButton()
        return button
    }

    private func styleResumeLatestButton() {
        guard let button = resumeLatestButton else { return }
        button.layer?.backgroundColor = (lightTheme
            ? NSColor.white.withAlphaComponent(0.92)
            : BlueyTheme.ink.withAlphaComponent(0.90)
        ).cgColor
        button.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.62).cgColor
        button.layer?.borderWidth = 1
        button.contentTintColor = activeTextColor().withAlphaComponent(0.95)
    }

    @objc private func resumeLatestPressed() {
        feedTailPinned = true
        feedHasUnreadLatest = false
        updateResumeLatestButton()
        scrollToLatestCard(force: true)
    }

    @objc private func clearPressed() {
        clearCards()
    }

    @objc private func themePressed() {
        lightTheme.toggle()
        UserDefaults.standard.set(lightTheme ? "light" : "dark", forKey: savedThemeKey)
        applyTheme()
        rerenderCardsForCurrentTheme()
    }

    @objc private func helpPressed() {
        let alert = NSAlert()
        alert.messageText = "Bluey controls"
        alert.informativeText = controlLegendText()
        alert.alertStyle = .informational
        alert.addButton(withTitle: "Got it")
        _ = runOverlayAlert(alert)
    }

    @objc private func recordingPressed() {
        recordingActive.toggle()
        updateRecordingButton()
        emit(["type": recordingActive ? "recording_start_requested" : "recording_stop_requested"])
    }

    @objc private func modelChanged() {
        if let selected = modelPicker.selectedItem?.title {
            UserDefaults.standard.set(selected, forKey: savedModelKey)
        }
    }

    @objc private func modeChanged() {
        if let selected = modePicker.selectedItem?.title {
            UserDefaults.standard.set(selected, forKey: savedModeKey)
        }
    }

    @objc private func answerPressed() {
        sendQuestion()
    }

    @objc private func recapPressed() {
        emit(["type": "recap_requested"])
    }

    @objc private func sessionPressed() {
        let alert = NSAlert()
        alert.messageText = "Session"
        alert.informativeText = "Continue the active session with its transcript and attached context, or start a clean session. New Session archives the current one first."
        alert.alertStyle = .informational
        alert.addButton(withTitle: "Continue")
        alert.addButton(withTitle: "New Session")
        alert.addButton(withTitle: "Cancel")

        let response = runOverlayAlert(alert)
        if response == .alertFirstButtonReturn {
            emit(["type": "session_continue_requested"])
        } else if response == .alertSecondButtonReturn {
            emit(["type": "session_new_requested"])
        }
    }

    @objc private func pagePressed() {
        let alert = NSAlert()
        alert.messageText = "Analyse screen context?"
        alert.informativeText = "Bluey will read the active browser page or available screen context, attach it, and generate an answer. The overlay is excluded from normal screen capture."
        alert.alertStyle = .informational
        alert.addButton(withTitle: "Analyse Screen")
        alert.addButton(withTitle: "Cancel")

        if runOverlayAlert(alert) == .alertFirstButtonReturn {
            emit(["type": "analyze_screen_requested"])
        }
    }

    private func setupHotkeys() {
        localEventMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self else { return event }
            return self.handleKeyEvent(event, canConsume: true) ? nil : event
        }

        globalEventMonitor = NSEvent.addGlobalMonitorForEvents(matching: .keyDown) { [weak self] event in
            _ = self?.handleKeyEvent(event, canConsume: false)
        }

        localScrollEventMonitor = NSEvent.addLocalMonitorForEvents(matching: .scrollWheel) { [weak self] event in
            self?.scrollFeedIfPointerIsInside(event)
            return event
        }

        globalScrollEventMonitor = NSEvent.addGlobalMonitorForEvents(matching: .scrollWheel) { [weak self] event in
            DispatchQueue.main.async {
                self?.scrollFeedIfPointerIsInside(event)
            }
        }
    }

    private func handleKeyEvent(_ event: NSEvent, canConsume: Bool) -> Bool {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        if flags.contains(.command),
           flags.contains(.shift),
           event.keyCode == 49 {
            toggleComposerOrOverlay()
            return canConsume
        }

        if event.keyCode == 53, composerIsVisible {
            panel.makeFirstResponder(nil)
            return canConsume
        }

        return false
    }

    private func scrollFeedIfPointerIsInside(_ event: NSEvent) {
        guard panel.isVisible,
              !scrollView.isHidden,
              scrollView.alphaValue > 0.01,
              let effect = effectView as? PassthroughEffectView,
              let documentView = scrollView.documentView else {
            return
        }

        let windowPoint = panel.convertPoint(fromScreen: NSEvent.mouseLocation)
        let pointInEffect = effect.convert(windowPoint, from: nil)
        let feedRect = scrollView.convert(scrollView.bounds, to: effect)
        guard feedRect.contains(pointInEffect) else { return }

        stack.layoutSubtreeIfNeeded()
        let clipView = scrollView.contentView
        var origin = clipView.bounds.origin
        let scale: CGFloat = event.hasPreciseScrollingDeltas ? 1 : 8
        origin.y += event.scrollingDeltaY * scale
        let maxY = max(0, documentView.bounds.height - clipView.bounds.height)
        origin.y = min(max(origin.y, 0), maxY)

        clipView.scroll(to: origin)
        scrollView.reflectScrolledClipView(clipView)
        feedTailPinned = isFeedNearLatest()
        if feedTailPinned {
            feedHasUnreadLatest = false
        }
        updateResumeLatestButton()
    }

    @objc private func attachPressed() {
        let action = NSAlert()
        action.messageText = "Session attachments"
        action.informativeText = "Attach new files, or show the documents, screenshots, page captures, and notes already attached to this session."
        action.alertStyle = .informational
        action.addButton(withTitle: "Attach Files")
        action.addButton(withTitle: "Show Attached")
        action.addButton(withTitle: "Cancel")

        let response = runOverlayAlert(action)
        if response == .alertSecondButtonReturn {
            emit(["type": "context_list_requested"])
            return
        }
        guard response == .alertFirstButtonReturn else { return }

        let picker = NSOpenPanel()
        picker.title = "Choose readable files for this Bluey session"
        picker.prompt = "Attach"
        picker.message = "Bluey uses readable text, Markdown, code, PDF, DOC, DOCX, and RTF files as answer context."
        picker.allowsMultipleSelection = true
        picker.canChooseDirectories = false
        picker.canChooseFiles = true
        picker.resolvesAliases = true
        picker.allowedContentTypes = allowedContextContentTypes()

        prepareModalWindow(picker)
        if picker.runModal() == .OK {
            let paths = picker.urls.map { $0.path }
            if !paths.isEmpty {
                emit(["type": "attach_files_requested", "paths": paths])
            }
        }
    }

    private func allowedContextContentTypes() -> [UTType] {
        [
            "md", "markdown", "txt", "log", "csv", "tsv", "rst", "adoc",
            "rs", "swift", "c", "h", "cpp", "hpp", "js", "jsx", "ts", "tsx", "py", "go",
            "java", "kt", "kts", "cs", "rb", "php", "sql", "sh", "ps1", "toml", "yaml",
            "yml", "json", "html", "css", "scss",
            "pdf", "doc", "docx", "rtf"
        ].compactMap { UTType(filenameExtension: $0) }
    }

    @objc private func askPressed() {
        setComposerVisible(true)
        panel.makeFirstResponder(questionField)
    }

    @objc private func instructionsPressed() {
        let alert = NSAlert()
        alert.messageText = "Answer style"
        alert.informativeText = "Tell Bluey how to answer during this session."
        alert.alertStyle = .informational
        alert.addButton(withTitle: "Save")
        alert.addButton(withTitle: "Cancel")

        let scroll = NSScrollView(frame: NSRect(x: 0, y: 0, width: 380, height: 110))
        scroll.borderType = .bezelBorder
        scroll.hasVerticalScroller = true

        let textView = NSTextView(frame: scroll.bounds)
        textView.font = NSFont.systemFont(ofSize: 13)
        textView.string = "Answer briefly. Focus on implementation risks and next steps."
        textView.isRichText = false
        textView.autoresizingMask = [.width, .height]
        scroll.documentView = textView
        alert.accessoryView = scroll

        if runOverlayAlert(alert) == .alertFirstButtonReturn {
            emit(["type": "instructions_updated", "text": textView.string.trimmingCharacters(in: .whitespacesAndNewlines)])
        }
    }

    @objc private func closePressed() {
        let alert = NSAlert()
        alert.messageText = "Quit Bluey?"
        alert.informativeText = "This stops Bluey completely, the same as running `bluey off`. Sessions are saved for the dashboard/history. Use the eye-slash button if you only want to hide the overlay."
        alert.alertStyle = .informational
        alert.addButton(withTitle: "Quit Bluey")
        alert.addButton(withTitle: "Cancel")

        if runOverlayAlert(alert) == .alertFirstButtonReturn {
            emit(["type": "close_requested"])
        }
    }

    private func updateRecordingButton() {
        guard let recordingButton, let recordingDot else { return }
        let symbol = recordingActive ? "mic.fill" : "mic"
        let fallback = recordingActive ? "Stop recording" : "Start recording"
        if let image = NSImage(systemSymbolName: symbol, accessibilityDescription: fallback) {
            recordingButton.image = image
            recordingButton.title = ""
        } else {
            recordingButton.title = recordingActive ? "Stop" : "Mic"
        }
        recordingButton.toolTip = fallback
        recordingButton.layer?.backgroundColor = (
            recordingActive
                ? BlueyTheme.lime.withAlphaComponent(0.84)
                : controlBackgroundColor()
        ).cgColor
        recordingButton.layer?.borderColor = (
            recordingActive
                ? BlueyTheme.lime.withAlphaComponent(0.95)
                : controlBorderColor()
        ).cgColor
        recordingButton.contentTintColor = recordingActive
            ? BlueyTheme.ink.withAlphaComponent(0.92)
            : activeTextColor().withAlphaComponent(0.94)
        recordingDot.layer?.backgroundColor = (
            recordingActive
                ? BlueyTheme.lime.withAlphaComponent(0.96)
                : BlueyTheme.lime.withAlphaComponent(0.22)
        ).cgColor
    }

    private func prepareModalWindow(_ window: NSWindow) {
        NSApplication.shared.activate(ignoringOtherApps: true)
        window.level = NSWindow.Level(rawValue: panel.level.rawValue + 1)
        window.collectionBehavior = [
            .canJoinAllSpaces,
            .fullScreenAuxiliary,
            .stationary
        ]
        window.contentView?.layoutSubtreeIfNeeded()
        positionModalWindowAwayFromOverlay(window)
        window.orderFrontRegardless()
    }

    private func positionModalWindowAwayFromOverlay(_ window: NSWindow) {
        guard let screen = panel.screen ?? NSScreen.main else { return }

        let visible = screen.visibleFrame
        let overlay = panel.frame.insetBy(dx: -24, dy: -24)
        let size = window.frame.size
        let margin: CGFloat = 28

        func clamped(_ origin: NSPoint) -> NSPoint {
            let maxX = max(visible.minX, visible.maxX - size.width)
            let maxY = max(visible.minY, visible.maxY - size.height)
            return NSPoint(
                x: min(max(origin.x, visible.minX), maxX),
                y: min(max(origin.y, visible.minY), maxY)
            )
        }

        let centeredY = overlay.midY - size.height / 2
        let centeredX = overlay.midX - size.width / 2
        let candidates = [
            NSPoint(x: overlay.maxX + margin, y: centeredY),
            NSPoint(x: overlay.minX - size.width - margin, y: centeredY),
            NSPoint(x: centeredX, y: overlay.minY - size.height - margin),
            NSPoint(x: centeredX, y: overlay.maxY + margin),
            NSPoint(x: visible.minX + margin, y: visible.maxY - size.height - margin),
            NSPoint(x: visible.maxX - size.width - margin, y: visible.maxY - size.height - margin),
            NSPoint(x: visible.minX + margin, y: visible.minY + margin),
            NSPoint(x: visible.maxX - size.width - margin, y: visible.minY + margin)
        ].map(clamped)

        if let origin = candidates.first(where: { candidate in
            !NSRect(origin: candidate, size: size).intersects(overlay)
        }) {
            window.setFrameOrigin(origin)
            return
        }

        let overlayCenter = NSPoint(x: overlay.midX, y: overlay.midY)
        let best = candidates.max { left, right in
            distanceSquared(from: left, to: overlayCenter) < distanceSquared(from: right, to: overlayCenter)
        }
        if let best {
            window.setFrameOrigin(best)
        }
    }

    private func distanceSquared(from origin: NSPoint, to point: NSPoint) -> CGFloat {
        let dx = origin.x - point.x
        let dy = origin.y - point.y
        return dx * dx + dy * dy
    }

    private func runOverlayAlert(_ alert: NSAlert) -> NSApplication.ModalResponse {
        prepareModalWindow(alert.window)
        return alert.runModal()
    }

    @objc private func opacityChanged() {
        applyOpacity(CGFloat(opacitySlider.doubleValue), save: true)
    }

    @objc private func hidePressed() {
        collapseToPill()
    }

    @objc func expandFromPill() {
        showOverlay(focusComposer: true)
    }

    @objc private func hideComposer() {
        setComposerVisible(false)
    }

    @objc private func sendQuestion() {
        let question = questionField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        let prompt = question.isEmpty
            ? "Answer the latest clear question from the current transcript, screen context, and attached files. If there is no clear question yet, summarize what Bluey needs next."
            : question

        emitAsk(question: prompt)
        questionField.stringValue = ""
        setComposerVisible(true)
    }

    private func emitAsk(question: String) {
        let route = selectedRoute()
        emit([
            "type": "ask_requested",
            "question": question,
            "provider": route.provider,
            "model": route.model,
            "mode": selectedMode()
        ])
    }

    private func selectedRoute() -> (provider: String, model: String) {
        switch modelPicker.selectedItem?.title ?? "Bluey Auto" {
        case "OpenAI Direct", "OpenAI Reasoning":
            return ("openai", "")
        case "Groq Realtime":
            return ("groq", "llama-3.1-8b-instant")
        case "Cerebras Fast":
            return ("cerebras", "llama3.1-8b")
        case "Local":
            return ("local", "bluey-local-answer-v0")
        default:
            return ("auto", "")
        }
    }

    private func selectedMode() -> String {
        modePicker.selectedItem?.title ?? "General"
    }

    private var composerIsVisible: Bool {
        composerView?.isHidden == false
    }

    private func toggleComposer() {
        setComposerVisible(!composerIsVisible)
    }

    private func toggleComposerOrOverlay() {
        if panel.isVisible {
            setComposerVisible(true)
            panel.makeFirstResponder(questionField)
        } else {
            showOverlay(focusComposer: true)
        }
    }

    private func showOverlay(focusComposer: Bool = false) {
        collapsedPanel?.orderOut(nil)
        applyCaptureExclusion()
        panel.orderFrontRegardless()
        emit(["type": "shown"])

        if focusComposer || !composerIsVisible {
            setComposerVisible(true)
        }
    }

    private func collapseToPill(emitHidden: Bool = true) {
        saveFrame()
        panel.orderOut(nil)
        positionCollapsedPill()
        applyCaptureExclusion()
        collapsedPillView?.ignoreClicksUntil = Date().addingTimeInterval(0.35)
        collapsedPanel?.contentView?.needsDisplay = true
        collapsedPanel?.orderFrontRegardless()
        if let contentView = collapsedPanel?.contentView {
            collapsedPanel?.makeFirstResponder(contentView)
        }
        if emitHidden {
            emit(["type": "hidden"])
        }
    }

    private func positionCollapsedPill() {
        guard let collapsedPanel,
              let screen = panel.screen ?? NSScreen.main else { return }
        let frame = screen.visibleFrame
        let size = collapsedPanel.frame.size
        let margin: CGFloat = 14
        if let saved = UserDefaults.standard.string(forKey: savedCollapsedFrameKey) {
            var savedFrame = NSRectFromString(saved)
            if savedFrame.width > 0, savedFrame.height > 0 {
                savedFrame.size = size
                savedFrame.origin.x = min(max(savedFrame.origin.x, frame.minX + margin), frame.maxX - size.width - margin)
                savedFrame.origin.y = min(max(savedFrame.origin.y, frame.minY + margin), frame.maxY - size.height - margin)
                collapsedPanel.setFrame(savedFrame, display: true, animate: false)
                return
            }
        }
        let x = min(max(panel.frame.maxX - size.width, frame.minX + margin), frame.maxX - size.width - margin)
        let y = min(max(panel.frame.maxY - size.height, frame.minY + margin), frame.maxY - size.height - margin)
        collapsedPanel.setFrameOrigin(NSPoint(x: x, y: y))
    }

    private func setComposerVisible(_ visible: Bool) {
        guard composerView != nil else { return }

        if visible {
            applyCaptureExclusion()
            panel.orderFrontRegardless()
            NSApplication.shared.activate(ignoringOtherApps: true)
            composerHeightConstraint.constant = 118
            if composerContentConstraints.contains(where: { !$0.isActive }) {
                NSLayoutConstraint.activate(composerContentConstraints)
            }
        }

        if visible {
            composerView.isHidden = false
        }

        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.14
            composerView.animator().alphaValue = visible ? 1 : 0
            panel.contentView?.layoutSubtreeIfNeeded()
        } completionHandler: { [weak self] in
            guard let self, !visible else { return }
            self.composerView.isHidden = true
            NSLayoutConstraint.deactivate(self.composerContentConstraints)
            self.composerHeightConstraint.constant = 0
            self.panel.contentView?.layoutSubtreeIfNeeded()
        }

        if visible {
            panel.makeKey()
            panel.makeFirstResponder(questionField)
        } else if panel.firstResponder === questionField.currentEditor() {
            panel.makeFirstResponder(nil)
        }
    }

    func windowDidMove(_ notification: Notification) {
        saveFrame()
    }

    func windowDidResize(_ notification: Notification) {
        saveFrame()
    }

    private func handleLine(_ line: String) {
        guard let data = line.data(using: .utf8) else {
            emitError("command was not UTF-8")
            return
        }

        do {
            guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let type = object["type"] as? String else {
                emitError("command missing type")
                return
            }

            DispatchQueue.main.async { [weak self] in
                self?.handleCommand(type: type, object: object)
            }
        } catch {
            emitError("invalid JSON command: \(error.localizedDescription)")
        }
    }

    private func handleCommand(type: String, object: [String: Any]) {
        switch type {
        case "ping":
            emit(["type": "pong"])
        case "show":
            showOverlay()
        case "hide":
            collapseToPill()
        case "toggle":
            if panel.isVisible {
                collapseToPill()
            } else {
                showOverlay()
            }
        case "clear":
            clearCards()
        case "boot":
            let title = object["title"] as? String ?? "Bluey online"
            let lines = object["lines"] as? [String] ?? []
            renderBoot(title: title, lines: lines)
            applyCaptureExclusion()
            collapsedPanel?.orderOut(nil)
            panel.orderFrontRegardless()
        case "set_opacity":
            if let opacity = object["opacity"] as? Double {
                applyOpacity(CGFloat(opacity), save: true)
            }
        case "set_position":
            if let newPosition = object["position"] as? String {
                position = newPosition
                positionWindow(save: true)
            }
        case "push_card":
            if let card = object["card"] as? [String: Any] {
                let wasVisible = panel.isVisible
                renderCard(card)
                applyCaptureExclusion()
                if wasVisible {
                    collapsedPanel?.orderOut(nil)
                    panel.orderFrontRegardless()
                }
            }
        case "update_card":
            if let id = object["id"] as? String {
                let body = object["body"] as? String
                let done = object["done"] as? Bool ?? false
                updateCard(id: id, body: body, done: done)
            }
        case "shutdown":
            emit(["type": "exited"])
            NSApplication.shared.terminate(nil)
        default:
            emitError("unknown command: \(type)")
        }
    }

    private func clearCards() {
        bootTimer?.invalidate()
        bootTimer = nil
        cardHistory.removeAll()
        cardBodyLabels.removeAll()
        cardBodyKinds.removeAll()
        answerBodyStacks.removeAll()
        feedTailPinned = true
        feedHasUnreadLatest = false
        updateResumeLatestButton()
        clearCardViews()
    }

    private func clearCardViews() {
        stack.arrangedSubviews.forEach { view in
            stack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        cardViewsById.removeAll()
    }

    private func renderBoot(title: String, lines: [String]) {
        bootTimer?.invalidate()

        let cardView = NSView()
        cardView.translatesAutoresizingMaskIntoConstraints = false
        cardView.wantsLayer = true
        cardView.layer?.backgroundColor = surfaceColor().cgColor
        cardView.layer?.cornerRadius = 14
        cardView.layer?.borderColor = BlueyTheme.lime.withAlphaComponent(0.44).cgColor
        cardView.layer?.borderWidth = 1

        let inner = NSStackView()
        inner.orientation = .vertical
        inner.alignment = .leading
        inner.spacing = 6
        inner.edgeInsets = NSEdgeInsets(top: 10, left: 12, bottom: 10, right: 12)
        inner.translatesAutoresizingMaskIntoConstraints = false

        let eyebrow = label("BLUEY", size: 11, weight: .semibold, color: BlueyTheme.cyan)
        let titleLabel = label(title, size: 15, weight: .semibold, color: activeTextColor())
        let bodyLabel = label("", size: 12, weight: .regular, color: BlueyTheme.lime.withAlphaComponent(lightTheme ? 0.98 : 0.92))
        bodyLabel.font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)

        inner.addArrangedSubview(eyebrow)
        inner.addArrangedSubview(titleLabel)
        inner.addArrangedSubview(bodyLabel)

        cardView.addSubview(inner)
        stack.addArrangedSubview(cardView)

        NSLayoutConstraint.activate([
            cardView.widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -28),
            inner.leadingAnchor.constraint(equalTo: cardView.leadingAnchor),
            inner.trailingAnchor.constraint(equalTo: cardView.trailingAnchor),
            inner.topAnchor.constraint(equalTo: cardView.topAnchor),
            inner.bottomAnchor.constraint(equalTo: cardView.bottomAnchor)
        ])

        let bootLines = lines.isEmpty ? [
            "overlay link established",
            "session memory loaded",
            "context controls armed",
            "ready"
        ] : lines
        var rendered: [String] = []
        var index = 0
        bootTimer = Timer.scheduledTimer(withTimeInterval: 0.18, repeats: true) { [weak self, weak bodyLabel] timer in
            guard let bodyLabel else {
                timer.invalidate()
                return
            }
            if index >= bootLines.count {
                timer.invalidate()
                self?.bootTimer = nil
                bodyLabel.stringValue = rendered.joined(separator: "\n") + "\n> ready"
                return
            }
            rendered.append("> " + bootLines[index])
            bodyLabel.stringValue = rendered.joined(separator: "\n") + "\n_"
            index += 1
        }

        while stack.arrangedSubviews.count > 60 {
            if let first = stack.arrangedSubviews.first {
                stack.removeArrangedSubview(first)
                first.removeFromSuperview()
            }
        }
        scrollToLatestCard()
    }

    private func renderCard(_ card: [String: Any], remember: Bool = true) {
        if remember {
            cardHistory.append(card)
            while cardHistory.count > 60 {
                cardHistory.removeFirst()
            }
        }

        let id = card["id"] as? String ?? UUID().uuidString
        let rawKind = card["kind"] as? String ?? "system"
        let title = card["title"] as? String ?? "Bluey"
        let body = card["body"] as? String ?? ""
        let source = card["source"] as? String
        let isTranscript = rawKind == "transcript"
        let isAnswer = rawKind == "answer"
        let accent = isTranscript ? transcriptAccent(title: title, source: source) : colorForKind(rawKind)

        let cardView = NSView()
        cardView.translatesAutoresizingMaskIntoConstraints = false
        cardView.wantsLayer = true
        cardView.layer?.backgroundColor = backgroundColorForKind(rawKind).cgColor
        cardView.layer?.cornerRadius = isTranscript ? 12 : 16
        cardView.layer?.borderColor = accent.withAlphaComponent(isTranscript ? 0.26 : 0.38).cgColor
        cardView.layer?.borderWidth = 1

        let inner = NSStackView()
        inner.orientation = .vertical
        inner.alignment = .leading
        inner.spacing = isTranscript ? 4 : 7
        inner.edgeInsets = isTranscript
            ? NSEdgeInsets(top: 8, left: 11, bottom: 8, right: 11)
            : NSEdgeInsets(top: 12, left: 13, bottom: 12, right: 13)
        inner.translatesAutoresizingMaskIntoConstraints = false

        let displayKind = displayKindForCard(kind: rawKind, title: title, source: source)
        let displayTitle = displayTitleForCard(kind: rawKind, title: title)
        let eyebrow = label(displayKind, size: 11, weight: .bold, color: accent)
        let titleLabel = displayTitle.map { label($0, size: isAnswer ? 15.5 : 15, weight: .semibold, color: activeTextColor()) }
        let bodyLabel = isAnswer ? nil : label("", size: isTranscript ? 12.7 : 13.2, weight: .regular, color: activeTextColor().withAlphaComponent(isTranscript ? 0.82 : 0.90))
        if let bodyLabel {
            applyBodyText(body, to: bodyLabel, kind: rawKind, done: true)
        }
        let answerBodyStack = isAnswer ? makeAnswerBodyStack() : nil
        if let answerBodyStack {
            renderAnswerBody(body, into: answerBodyStack, streaming: false)
        }
        let sourceLabel = source.flatMap { value -> NSTextField? in
            guard !value.isEmpty, !isTranscript else { return nil }
            return label(value, size: 11, weight: .regular, color: mutedTextColor())
        }

        inner.addArrangedSubview(eyebrow)
        if let titleLabel {
            inner.addArrangedSubview(titleLabel)
        }
        if let answerBodyStack {
            inner.addArrangedSubview(answerBodyStack)
        } else if let bodyLabel {
            inner.addArrangedSubview(bodyLabel)
        }

        if let sourceLabel {
            inner.addArrangedSubview(sourceLabel)
        }

        cardView.addSubview(inner)
        stack.addArrangedSubview(cardView)
        cardViewsById[id] = cardView
        if let bodyLabel {
            cardBodyLabels[id] = bodyLabel
        }
        if let answerBodyStack {
            answerBodyStacks[id] = answerBodyStack
        }
        cardBodyKinds[id] = rawKind

        NSLayoutConstraint.activate([
            cardView.widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -28),
            inner.leadingAnchor.constraint(equalTo: cardView.leadingAnchor),
            inner.trailingAnchor.constraint(equalTo: cardView.trailingAnchor),
            inner.topAnchor.constraint(equalTo: cardView.topAnchor),
            inner.bottomAnchor.constraint(equalTo: cardView.bottomAnchor),
            eyebrow.widthAnchor.constraint(lessThanOrEqualTo: cardView.widthAnchor, constant: -24)
        ])
        if let bodyLabel {
            bodyLabel.widthAnchor.constraint(lessThanOrEqualTo: cardView.widthAnchor, constant: -24).isActive = true
        }
        if let answerBodyStack {
            answerBodyStack.widthAnchor.constraint(equalTo: inner.widthAnchor).isActive = true
        }
        if let titleLabel {
            titleLabel.widthAnchor.constraint(lessThanOrEqualTo: cardView.widthAnchor, constant: -24).isActive = true
        }
        if let sourceLabel {
            sourceLabel.widthAnchor.constraint(lessThanOrEqualTo: cardView.widthAnchor, constant: -24).isActive = true
        }

        while stack.arrangedSubviews.count > 60 {
            if let first = stack.arrangedSubviews.first {
                stack.removeArrangedSubview(first)
                first.removeFromSuperview()
            }
        }
        scrollToLatestCard()

        emit(["type": "card_rendered", "id": id])
    }

    private func updateCard(id: String, body: String?, done: Bool) {
        if let body {
            if let stack = answerBodyStacks[id] {
                renderAnswerBody(body, into: stack, streaming: !done)
            } else if let label = cardBodyLabels[id] {
                applyBodyText(body, to: label, kind: cardBodyKinds[id] ?? "answer", done: done)
            }
            for index in cardHistory.indices {
                if cardHistory[index]["id"] as? String == id {
                    cardHistory[index]["body"] = body
                    break
                }
            }
        }

        if done {
            cardBodyLabels[id]?.textColor = activeTextColor().withAlphaComponent(0.92)
        }

        scrollToLatestCard()
    }

    private struct AnswerBlock {
        let isCode: Bool
        let language: String
        let text: String
    }

    private func makeAnswerBodyStack() -> NSStackView {
        let bodyStack = NSStackView()
        bodyStack.translatesAutoresizingMaskIntoConstraints = false
        bodyStack.orientation = .vertical
        bodyStack.alignment = .leading
        bodyStack.spacing = 9
        return bodyStack
    }

    private func renderAnswerBody(_ body: String, into bodyStack: NSStackView, streaming: Bool) {
        clearArrangedSubviews(bodyStack)
        let text = body.isEmpty && streaming ? "Thinking..." : body
        let blocks = parseAnswerBlocks(text)
        let codeBlocks = blocks.filter(\.isCode)
        let textBlocks = blocks.filter { !$0.isCode && !$0.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

        if !codeBlocks.isEmpty && !textBlocks.isEmpty {
            let split = NSStackView()
            split.translatesAutoresizingMaskIntoConstraints = false
            split.orientation = .horizontal
            split.alignment = .top
            split.distribution = .fillEqually
            split.spacing = 10

            let codeColumn = makeAnswerBodyStack()
            let explanationColumn = makeAnswerBodyStack()
            codeBlocks.forEach { codeColumn.addArrangedSubview(codeBlockView(language: $0.language, code: $0.text)) }
            textBlocks.forEach { explanationColumn.addArrangedSubview(answerTextView($0.text, streaming: false)) }

            split.addArrangedSubview(codeColumn)
            split.addArrangedSubview(explanationColumn)
            bodyStack.addArrangedSubview(split)
            split.widthAnchor.constraint(equalTo: bodyStack.widthAnchor).isActive = true
        } else {
            for block in blocks {
                if block.isCode {
                    bodyStack.addArrangedSubview(codeBlockView(language: block.language, code: block.text))
                } else if !block.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    bodyStack.addArrangedSubview(answerTextView(block.text, streaming: false))
                }
            }
        }

        if streaming {
            let live = label("streaming...", size: 11.2, weight: .semibold, color: BlueyTheme.cyan.withAlphaComponent(0.72))
            bodyStack.addArrangedSubview(live)
        }
    }

    private func parseAnswerBlocks(_ text: String) -> [AnswerBlock] {
        var blocks: [AnswerBlock] = []
        var textBuffer: [String] = []
        var codeBuffer: [String] = []
        var inCode = false
        var codeLanguage = ""

        func flushText() {
            let value = textBuffer.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
            if !value.isEmpty {
                blocks.append(AnswerBlock(isCode: false, language: "", text: value))
            }
            textBuffer.removeAll()
        }

        func flushCode() {
            blocks.append(AnswerBlock(isCode: true, language: codeLanguage, text: codeBuffer.joined(separator: "\n")))
            codeBuffer.removeAll()
            codeLanguage = ""
        }

        for line in text.components(separatedBy: "\n") {
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmed.hasPrefix("```") {
                if inCode {
                    flushCode()
                    inCode = false
                } else {
                    flushText()
                    codeLanguage = String(trimmed.dropFirst(3)).trimmingCharacters(in: .whitespacesAndNewlines)
                    inCode = true
                }
            } else if inCode {
                codeBuffer.append(line)
            } else {
                textBuffer.append(line)
            }
        }

        if inCode {
            flushCode()
        }
        flushText()

        if blocks.isEmpty {
            blocks.append(AnswerBlock(isCode: false, language: "", text: text))
        }
        return blocks
    }

    private func answerTextView(_ text: String, streaming: Bool) -> NSTextField {
        let field = label("", size: 13.4, weight: .regular, color: activeTextColor().withAlphaComponent(0.92))
        field.attributedStringValue = attributedAnswerText(text, streaming: streaming)
        return field
    }

    private func codeBlockView(language: String, code: String) -> NSView {
        let shell = NSView()
        shell.translatesAutoresizingMaskIntoConstraints = false
        shell.wantsLayer = true
        shell.layer?.backgroundColor = (
            lightTheme
                ? NSColor(calibratedRed: 0.90, green: 0.97, blue: 1.00, alpha: glassAlpha(min: 0.42, max: 0.72))
                : NSColor(calibratedRed: 0.010, green: 0.026, blue: 0.040, alpha: glassAlpha(min: 0.48, max: 0.94))
        ).cgColor
        shell.layer?.cornerRadius = 11
        shell.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(lightTheme ? 0.34 : 0.24).cgColor
        shell.layer?.borderWidth = 1

        let inner = NSStackView()
        inner.translatesAutoresizingMaskIntoConstraints = false
        inner.orientation = .vertical
        inner.alignment = .leading
        inner.spacing = 6
        inner.edgeInsets = NSEdgeInsets(top: 9, left: 10, bottom: 10, right: 10)

        let header = label(language.isEmpty ? "CODE" : language.uppercased(), size: 10.4, weight: .bold, color: BlueyTheme.cyan.withAlphaComponent(0.92))
        let codeLabel = label(code.isEmpty ? " " : code, size: 12.0, weight: .regular, color: lightTheme ? NSColor(calibratedRed: 0.02, green: 0.05, blue: 0.07, alpha: 0.96) : NSColor(calibratedRed: 0.88, green: 0.98, blue: 1.00, alpha: 0.96))
        codeLabel.font = NSFont.monospacedSystemFont(ofSize: 12.0, weight: .regular)
        codeLabel.lineBreakMode = .byCharWrapping

        inner.addArrangedSubview(header)
        inner.addArrangedSubview(codeLabel)
        shell.addSubview(inner)

        NSLayoutConstraint.activate([
            inner.leadingAnchor.constraint(equalTo: shell.leadingAnchor),
            inner.trailingAnchor.constraint(equalTo: shell.trailingAnchor),
            inner.topAnchor.constraint(equalTo: shell.topAnchor),
            inner.bottomAnchor.constraint(equalTo: shell.bottomAnchor),
            header.widthAnchor.constraint(lessThanOrEqualTo: shell.widthAnchor, constant: -20),
            codeLabel.widthAnchor.constraint(lessThanOrEqualTo: shell.widthAnchor, constant: -20)
        ])

        return shell
    }

    private func clearArrangedSubviews(_ view: NSStackView) {
        for subview in view.arrangedSubviews {
            view.removeArrangedSubview(subview)
            subview.removeFromSuperview()
        }
    }

    private func applyBodyText(_ body: String, to field: NSTextField, kind: String, done: Bool) {
        let text = body.isEmpty && !done ? "Thinking..." : body
        if kind == "answer" {
            field.attributedStringValue = attributedAnswerText(text, streaming: !done)
        } else {
            field.stringValue = text
            field.textColor = activeTextColor().withAlphaComponent(kind == "transcript" ? 0.82 : 0.90)
        }
    }

    private func attributedAnswerText(_ text: String, streaming: Bool) -> NSAttributedString {
        let output = NSMutableAttributedString()
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineSpacing = 2.4
        paragraph.paragraphSpacing = 3.0
        paragraph.lineBreakMode = .byWordWrapping

        let baseColor = activeTextColor().withAlphaComponent(0.93)
        let headingColor = BlueyTheme.cyan.withAlphaComponent(0.96)
        let codeColor = lightTheme
            ? NSColor(calibratedRed: 0.018, green: 0.045, blue: 0.068, alpha: 0.96)
            : NSColor(calibratedRed: 0.88, green: 0.98, blue: 1.00, alpha: 0.96)
        let codeBackground = lightTheme
            ? NSColor(calibratedRed: 0.88, green: 0.96, blue: 1.00, alpha: 0.58)
            : NSColor(calibratedRed: 0.014, green: 0.035, blue: 0.052, alpha: 0.86)

        let normal: [NSAttributedString.Key: Any] = [
            .foregroundColor: baseColor,
            .font: NSFont.systemFont(ofSize: 13.8, weight: .regular),
            .paragraphStyle: paragraph
        ]
        let heading: [NSAttributedString.Key: Any] = [
            .foregroundColor: headingColor,
            .font: NSFont.systemFont(ofSize: 13.9, weight: .bold),
            .paragraphStyle: paragraph
        ]
        let code: [NSAttributedString.Key: Any] = [
            .foregroundColor: codeColor,
            .backgroundColor: codeBackground,
            .font: NSFont.monospacedSystemFont(ofSize: 12.4, weight: .regular),
            .paragraphStyle: paragraph
        ]
        let codeHeader: [NSAttributedString.Key: Any] = [
            .foregroundColor: BlueyTheme.cyan.withAlphaComponent(0.95),
            .backgroundColor: codeBackground,
            .font: NSFont.monospacedSystemFont(ofSize: 11.8, weight: .semibold),
            .paragraphStyle: paragraph
        ]

        var inCode = false
        let lines = text.components(separatedBy: "\n")
        for (index, line) in lines.enumerated() {
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmed.hasPrefix("```") {
                let language = trimmed.dropFirst(3).trimmingCharacters(in: .whitespacesAndNewlines)
                let marker = inCode ? "end code" : (language.isEmpty ? "code" : "code - \(language)")
                output.append(NSAttributedString(string: marker, attributes: codeHeader))
                inCode.toggle()
            } else if inCode {
                output.append(NSAttributedString(string: line.isEmpty ? " " : line, attributes: code))
            } else if trimmed.hasPrefix("#") {
                let title = trimmed.trimmingCharacters(in: CharacterSet(charactersIn: "# ")).uppercased()
                output.append(NSAttributedString(string: title.isEmpty ? trimmed : title, attributes: heading))
            } else {
                output.append(NSAttributedString(string: line, attributes: normal))
            }

            if index < lines.count - 1 {
                output.append(NSAttributedString(string: "\n", attributes: inCode ? code : normal))
            }
        }

        if streaming {
            output.append(NSAttributedString(string: text.isEmpty ? "" : "\n", attributes: normal))
            output.append(NSAttributedString(string: "streaming...", attributes: [
                .foregroundColor: BlueyTheme.cyan.withAlphaComponent(0.72),
                .font: NSFont.systemFont(ofSize: 11.4, weight: .semibold),
                .paragraphStyle: paragraph
            ]))
        }

        return output
    }

    private func displayKindForCard(kind: String, title: String, source: String?) -> String {
        switch kind {
        case "question":
            return "You"
        case "answer":
            return "Bluey"
        case "transcript":
            return transcriptLabel(title: title, source: source)
        default:
            return kind.replacingOccurrences(of: "_", with: " ").uppercased()
        }
    }

    private func displayTitleForCard(kind: String, title: String) -> String? {
        switch kind {
        case "question":
            return nil
        case "answer":
            return "Response"
        case "transcript":
            return nil
        default:
            return title
        }
    }

    private func transcriptLabel(title: String, source: String?) -> String {
        let value = "\(title) \(source ?? "")".lowercased()
        if value.contains("mic") || value.contains("microphone") || value.contains("user") {
            return "Mic transcript"
        }
        if value.contains("system") {
            return "System transcript"
        }
        return "Transcript"
    }

    private func transcriptAccent(title: String, source: String?) -> NSColor {
        transcriptLabel(title: title, source: source).lowercased().contains("mic") ? BlueyTheme.lime : BlueyTheme.cyan
    }

    private func scrollToLatestCard(force: Bool = false) {
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.stack.layoutSubtreeIfNeeded()
            if !force, !self.feedTailPinned {
                self.feedHasUnreadLatest = true
                self.updateResumeLatestButton()
                return
            }
            self.scrollView?.contentView.scroll(to: NSPoint(x: 0, y: 0))
            self.scrollView?.reflectScrolledClipView(self.scrollView.contentView)
            self.feedTailPinned = true
            self.feedHasUnreadLatest = false
            self.updateResumeLatestButton()
        }
    }

    private func isFeedNearLatest() -> Bool {
        guard let documentView = scrollView?.documentView else { return true }
        let clipView = scrollView.contentView
        let maxY = max(0, documentView.bounds.height - clipView.bounds.height)
        if maxY <= 1 {
            return true
        }
        return clipView.bounds.origin.y <= 18
    }

    private func updateResumeLatestButton() {
        guard let button = resumeLatestButton else { return }
        let shouldShow = feedHasUnreadLatest && !feedTailPinned
        button.title = "Latest"
        styleResumeLatestButton()
        if shouldShow == !button.isHidden {
            return
        }
        if shouldShow {
            button.isHidden = false
        }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.14
            button.animator().alphaValue = shouldShow ? 1 : 0
        } completionHandler: {
            button.isHidden = !shouldShow
            button.alphaValue = shouldShow ? 1 : 0
        }
    }

    private func colorForKind(_ kind: String) -> NSColor {
        switch kind {
        case "question":
            return BlueyTheme.cyan
        case "answer":
            return BlueyTheme.blue
        case "context":
            return BlueyTheme.cyan
        case "transcript":
            return BlueyTheme.lime
        case "action_item":
            return NSColor.systemGreen
        case "decision":
            return NSColor.systemPurple
        case "warning":
            return NSColor.systemOrange
        default:
            return BlueyTheme.text.withAlphaComponent(0.72)
        }
    }

    private func backgroundColorForKind(_ kind: String) -> NSColor {
        switch kind {
        case "question":
            return lightTheme
                ? NSColor(calibratedRed: 0.88, green: 0.96, blue: 1.00, alpha: glassAlpha(min: 0.34, max: 0.88))
                : NSColor(calibratedRed: 0.020, green: 0.048, blue: 0.074, alpha: glassAlpha(min: 0.34, max: 0.84))
        case "answer":
            return lightTheme
                ? NSColor.white.withAlphaComponent(glassAlpha(min: 0.36, max: 0.88))
                : NSColor(calibratedRed: 0.025, green: 0.055, blue: 0.085, alpha: glassAlpha(min: 0.34, max: 0.82))
        case "transcript":
            return lightTheme
                ? NSColor(calibratedRed: 0.92, green: 1.00, blue: 0.98, alpha: glassAlpha(min: 0.34, max: 0.84))
                : NSColor(calibratedRed: 0.025, green: 0.085, blue: 0.078, alpha: glassAlpha(min: 0.32, max: 0.80))
        case "warning":
            return lightTheme
                ? NSColor(calibratedRed: 1.00, green: 0.94, blue: 0.84, alpha: glassAlpha(min: 0.34, max: 0.86))
                : NSColor(calibratedRed: 0.13, green: 0.08, blue: 0.03, alpha: glassAlpha(min: 0.30, max: 0.76))
        default:
            return lightTheme
                ? NSColor(calibratedRed: 0.96, green: 0.985, blue: 1.00, alpha: glassAlpha(min: 0.34, max: 0.86))
                : BlueyTheme.ink.withAlphaComponent(glassAlpha(min: 0.30, max: 0.70))
        }
    }

    private func restoreTheme() {
        lightTheme = UserDefaults.standard.string(forKey: savedThemeKey) == "light"
    }

    private func activeTextColor() -> NSColor {
        lightTheme
            ? NSColor(calibratedRed: 0.018, green: 0.045, blue: 0.068, alpha: 1)
            : BlueyTheme.text
    }

    private func mutedTextColor() -> NSColor {
        activeTextColor().withAlphaComponent(lightTheme ? 0.64 : 0.58)
    }

    private func glassAlpha(min minAlpha: CGFloat, max maxAlpha: CGFloat) -> CGFloat {
        let normalized = (backgroundOpacity - 0.18) / 0.82
        let clamped = Swift.max(0, Swift.min(1, normalized))
        return minAlpha + ((maxAlpha - minAlpha) * clamped)
    }

    private func surfaceColor() -> NSColor {
        lightTheme
            ? NSColor(calibratedRed: 0.96, green: 0.985, blue: 1.00, alpha: glassAlpha(min: 0.42, max: 0.90))
            : BlueyTheme.panel.withAlphaComponent(glassAlpha(min: 0.40, max: 0.90))
    }

    private func inputBackgroundColor() -> NSColor {
        lightTheme
            ? NSColor.white.withAlphaComponent(glassAlpha(min: 0.48, max: 0.86))
            : BlueyTheme.ink.withAlphaComponent(glassAlpha(min: 0.46, max: 0.82))
    }

    private func controlBackgroundColor() -> NSColor {
        lightTheme
            ? NSColor.white.withAlphaComponent(glassAlpha(min: 0.42, max: 0.78))
            : BlueyTheme.ink.withAlphaComponent(glassAlpha(min: 0.38, max: 0.68))
    }

    private func controlBorderColor() -> NSColor {
        BlueyTheme.cyan.withAlphaComponent(lightTheme ? 0.34 : 0.20)
    }

    private func hostBackgroundColor() -> NSColor {
        lightTheme
            ? NSColor.white.withAlphaComponent(glassAlpha(min: 0.06, max: 0.28))
            : BlueyTheme.ink.withAlphaComponent(glassAlpha(min: 0.20, max: 0.82))
    }

    private func applyTheme() {
        effectView?.material = lightTheme ? .popover : .hudWindow
        effectView?.layer?.backgroundColor = hostBackgroundColor().cgColor
        effectView?.layer?.borderColor = controlBorderColor().cgColor
        headerView?.layer?.backgroundColor = surfaceColor().cgColor
        headerView?.layer?.borderColor = controlBorderColor().cgColor
        composerCardView?.layer?.backgroundColor = surfaceColor().cgColor
        composerCardView?.layer?.borderColor = controlBorderColor().cgColor
        questionField?.textColor = activeTextColor().withAlphaComponent(0.94)
        questionField?.backgroundColor = inputBackgroundColor()
        questionField?.layer?.borderColor = controlBorderColor().cgColor
        modelPicker?.contentTintColor = activeTextColor().withAlphaComponent(0.90)
        modePicker?.contentTintColor = activeTextColor().withAlphaComponent(0.90)
        modelPicker?.layer?.backgroundColor = inputBackgroundColor().cgColor
        modePicker?.layer?.backgroundColor = inputBackgroundColor().cgColor
        themeButton?.toolTip = lightTheme ? "Switch to black background" : "Switch to white background"
        if let symbol = NSImage(
            systemSymbolName: lightTheme ? "moon.fill" : "sun.max.fill",
            accessibilityDescription: themeButton?.toolTip
        ) {
            themeButton?.image = symbol
        }
        for button in themedButtons {
            button.layer?.backgroundColor = controlBackgroundColor().cgColor
            button.layer?.borderColor = controlBorderColor().cgColor
            button.contentTintColor = activeTextColor().withAlphaComponent(0.90)
        }
        styleResumeLatestButton()
        for chip in chipControls {
            chip.lightTheme = lightTheme
        }
        refreshCardChrome()
        collapsedPillView?.lightTheme = lightTheme
        collapsedPillView?.needsDisplay = true
        brandView?.lightTheme = lightTheme
        updateRecordingButton()
    }

    private func refreshCardChrome() {
        for (id, view) in cardViewsById {
            let kind = cardBodyKinds[id] ?? "system"
            view.layer?.backgroundColor = backgroundColorForKind(kind).cgColor
            let accent = kind == "transcript" ? BlueyTheme.lime : colorForKind(kind)
            view.layer?.borderColor = accent.withAlphaComponent(kind == "transcript" ? 0.26 : 0.38).cgColor
        }
    }

    private func rerenderCardsForCurrentTheme() {
        let cards = cardHistory
        clearCardViews()
        for card in cards {
            renderCard(card, remember: false)
        }
    }

    private func controlLegendText() -> String {
        """
        Green dot: Bluey overlay is connected and ready.
        Sparkles: focus the bottom ask box.
        Question mark: show this control guide.
        Session: continue this session or archive it and start clean.
        Model menu: choose the answer route.
        Mode menu: tune answer style for general, code, system design, meeting, or writing.
        Opacity slider: adjust the background glass; text and icons stay readable.
        Paperclip: attach documents, code, screenshots, or notes, or show what is already attached.
        Black/white switch: toggle the overlay background while keeping Bluey's blue border language.
        Notepad: set how Bluey should answer.
        Trash: clear visible cards.
        Eye slash: collapse the overlay into a small Bluey button. Click the button to reopen.
        X: asks before quitting Bluey, the same as bluey off.
        Bottom mic: start or stop audio capture.
        Bottom mic dot: dim is off, bright green is recording.
        Ask field: type a question using transcript, screen, docs, and memory.
        Send: ask Bluey.
        Answer: send the typed question.
        Analyse Screen: read the active browser page or available screen context, attach it, and generate an answer.
        Middle cards: readable and click-through; wheel or trackpad scroll over them moves Bluey's history.
        """
    }

    private func label(_ text: String, size: CGFloat, weight: NSFont.Weight, color: NSColor) -> NSTextField {
        let field = NSTextField(labelWithString: text)
        field.translatesAutoresizingMaskIntoConstraints = false
        field.font = .systemFont(ofSize: size, weight: weight)
        field.textColor = color
        field.lineBreakMode = .byWordWrapping
        field.maximumNumberOfLines = 0
        field.usesSingleLineMode = false
        field.allowsDefaultTighteningForTruncation = false
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        return field
    }

    private func restoreFrameOrPosition() {
        if let saved = UserDefaults.standard.string(forKey: savedFrameKey) {
            let rect = NSRectFromString(saved)
            if rect.width >= panel.minSize.width,
               rect.height >= panel.minSize.height,
               NSScreen.screens.contains(where: { $0.visibleFrame.intersects(rect) }) {
                panel.setFrame(rect, display: false)
                return
            }
        }

        positionWindow(save: false)
    }

    private func restoreOpacity() {
        let saved = UserDefaults.standard.double(forKey: savedOpacityKey)
        let opacity = saved > 0 ? CGFloat(saved) : 0.92
        applyOpacity(opacity, save: false)
    }

    private func applyOpacity(_ value: CGFloat, save: Bool) {
        let opacity = max(0.18, min(1.0, value))
        backgroundOpacity = opacity
        panel.alphaValue = 1
        opacitySlider?.doubleValue = Double(opacity)
        applyTheme()
        if save {
            UserDefaults.standard.set(Double(opacity), forKey: savedOpacityKey)
        }
    }

    private func positionWindow(save shouldSave: Bool = false) {
        guard let screen = NSScreen.main else { return }
        let frame = screen.visibleFrame
        let size = panel.frame.size
        let margin: CGFloat = 24

        let origin: NSPoint
        switch position {
        case "top_left":
            origin = NSPoint(x: frame.minX + margin, y: frame.maxY - size.height - margin)
        case "bottom_left":
            origin = NSPoint(x: frame.minX + margin, y: frame.minY + margin)
        case "bottom_right":
            origin = NSPoint(x: frame.maxX - size.width - margin, y: frame.minY + margin)
        case "center":
            origin = NSPoint(x: frame.midX - size.width / 2, y: frame.midY - size.height / 2)
        default:
            origin = NSPoint(x: frame.maxX - size.width - margin, y: frame.maxY - size.height - margin)
        }

        panel.setFrameOrigin(origin)
        if shouldSave {
            saveFrame()
        }
    }

    private func saveFrame() {
        UserDefaults.standard.set(NSStringFromRect(panel.frame), forKey: savedFrameKey)
    }

    private func emitError(_ message: String) {
        emit(["type": "error", "message": message])
    }

    private func emit(_ object: [String: Any]) {
        if let data = try? JSONSerialization.data(withJSONObject: object),
           let line = String(data: data, encoding: .utf8) {
            print(line)
            fflush(stdout)
        }
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let controller = OverlayController()
controller.startReadingCommands()
app.run()
