import AppKit

final class ShortcutCoachmarkView: NSView {
    private enum Metrics {
        static let arrowHeight: CGFloat = 12
        static let cornerRadius: CGFloat = 16
        static let horizontalInset: CGFloat = 18
    }

    let titleLabel = NSTextField(labelWithString: "Your shortcuts are right here")
    let bodyLabel = NSTextField(
        wrappingLabelWithString: "Open this anytime to see Listen, Answer, screen, history, and focus keys.")
    let showButton = NSButton(title: "Show shortcuts", target: nil, action: nil)
    let dismissButton = NSButton(title: "Got it", target: nil, action: nil)
    var onShowShortcuts: (() -> Void)?
    var onDismiss: (() -> Void)?
    var arrowCenterX: CGFloat = 276 {
        didSet { needsDisplay = true }
    }
    private var lightThemeEnabled = false

    override var acceptsFirstResponder: Bool { true }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor
        layer?.masksToBounds = false

        titleLabel.font = NSFont.systemFont(ofSize: 14.5, weight: .bold)
        titleLabel.maximumNumberOfLines = 1
        bodyLabel.font = NSFont.systemFont(ofSize: 11.8, weight: .medium)
        bodyLabel.maximumNumberOfLines = 2
        bodyLabel.lineBreakMode = .byWordWrapping

        for button in [showButton, dismissButton] {
            button.isBordered = false
            button.wantsLayer = true
            button.layer?.cornerRadius = 9
            button.font = NSFont.systemFont(ofSize: 11.5, weight: .bold)
            button.focusRingType = .default
        }
        showButton.target = self
        showButton.action = #selector(showShortcutsClicked)
        dismissButton.target = self
        dismissButton.action = #selector(dismissClicked)
        showButton.setAccessibilityLabel("Show Bluey shortcuts")
        dismissButton.setAccessibilityLabel("Dismiss shortcut tip")
        showButton.toolTip = "Open all Bluey controls and shortcuts"
        dismissButton.toolTip = "Dismiss this tip"
        showButton.nextKeyView = dismissButton
        dismissButton.nextKeyView = showButton

        addSubview(titleLabel)
        addSubview(bodyLabel)
        addSubview(showButton)
        addSubview(dismissButton)
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityLabel("Bluey shortcut tip")
        applyTheme(light: false)
    }

    required init?(coder: NSCoder) { fatalError() }

    override func layout() {
        super.layout()
        let cardHeight = max(0, bounds.height - Metrics.arrowHeight)
        let contentWidth = max(0, bounds.width - Metrics.horizontalInset * 2)
        titleLabel.frame = NSRect(
            x: Metrics.horizontalInset,
            y: cardHeight - 38,
            width: contentWidth,
            height: 20)
        bodyLabel.frame = NSRect(
            x: Metrics.horizontalInset,
            y: cardHeight - 84,
            width: contentWidth,
            height: 39)
        showButton.frame = NSRect(
            x: Metrics.horizontalInset,
            y: 14,
            width: 118,
            height: 30)
        dismissButton.frame = NSRect(
            x: bounds.width - Metrics.horizontalInset - 72,
            y: 14,
            width: 72,
            height: 30)
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        let cardRect = NSRect(
            x: 0.75,
            y: 0.75,
            width: max(0, bounds.width - 1.5),
            height: max(0, bounds.height - Metrics.arrowHeight - 1.5))
        let cardPath = NSBezierPath(
            roundedRect: cardRect,
            xRadius: Metrics.cornerRadius,
            yRadius: Metrics.cornerRadius)
        let fillColor = lightThemeEnabled
            ? BlueyLightTheme.surfaceRaised.withAlphaComponent(0.995)
            : BlueyTheme.panelDeep.withAlphaComponent(0.99)
        let borderColor = lightThemeEnabled
            ? BlueyLightTheme.accentBorder.withAlphaComponent(0.72)
            : BlueyTheme.cyan.withAlphaComponent(0.46)

        NSGraphicsContext.saveGraphicsState()
        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(lightThemeEnabled ? 0.22 : 0.38)
        shadow.shadowBlurRadius = 20
        shadow.shadowOffset = NSSize(width: 0, height: -7)
        shadow.set()
        fillColor.setFill()
        cardPath.fill()
        NSGraphicsContext.restoreGraphicsState()

        borderColor.setStroke()
        cardPath.lineWidth = 1.2
        cardPath.stroke()

        let arrowX = min(max(18, arrowCenterX), bounds.width - 18)
        let arrowBaseY = cardRect.maxY - 0.5
        let arrowPath = NSBezierPath()
        arrowPath.move(to: NSPoint(x: arrowX - 9, y: arrowBaseY))
        arrowPath.line(to: NSPoint(x: arrowX, y: bounds.maxY - 0.5))
        arrowPath.line(to: NSPoint(x: arrowX + 9, y: arrowBaseY))
        arrowPath.close()
        fillColor.setFill()
        arrowPath.fill()

        let arrowBorder = NSBezierPath()
        arrowBorder.move(to: NSPoint(x: arrowX - 9, y: arrowBaseY))
        arrowBorder.line(to: NSPoint(x: arrowX, y: bounds.maxY - 0.5))
        arrowBorder.line(to: NSPoint(x: arrowX + 9, y: arrowBaseY))
        borderColor.setStroke()
        arrowBorder.lineWidth = 1.2
        arrowBorder.stroke()
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 {
            onDismiss?()
            return
        }
        super.keyDown(with: event)
    }

    func applyTheme(light: Bool) {
        lightThemeEnabled = light
        titleLabel.textColor = light ? BlueyLightTheme.text : BlueyTheme.text
        bodyLabel.textColor = light ? BlueyLightTheme.textDim : BlueyTheme.textDim
        showButton.layer?.backgroundColor = (light
            ? BlueyLightTheme.accentBorder
            : BlueyTheme.cyan).cgColor
        showButton.contentTintColor = light ? NSColor.white : BlueyTheme.panelDeep
        dismissButton.layer?.backgroundColor = (light
            ? BlueyLightTheme.contentLow
            : NSColor.white.withAlphaComponent(0.08)).cgColor
        dismissButton.layer?.borderWidth = 1
        dismissButton.layer?.borderColor = (light
            ? BlueyLightTheme.border
            : NSColor.white.withAlphaComponent(0.16)).cgColor
        dismissButton.contentTintColor = light ? BlueyLightTheme.text : BlueyTheme.text
        needsDisplay = true
    }

    @objc private func showShortcutsClicked() {
        onShowShortcuts?()
    }

    @objc private func dismissClicked() {
        onDismiss?()
    }
}
