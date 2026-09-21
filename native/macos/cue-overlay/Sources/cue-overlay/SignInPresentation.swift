import AppKit

struct BlueySignInCardPalette {
    let surface: NSColor
    let primaryText: NSColor
    let secondaryText: NSColor
    let border: NSColor
    let accent: NSColor
}

func blueySignInCardPalette(lightThemeEnabled: Bool) -> BlueySignInCardPalette {
    if lightThemeEnabled {
        return BlueySignInCardPalette(
            surface: BlueyLightTheme.surfaceRaised.withAlphaComponent(1.0),
            primaryText: BlueyLightTheme.text,
            secondaryText: BlueyLightTheme.textDim,
            border: BlueyLightTheme.accentBorder.withAlphaComponent(0.54),
            accent: BlueyLightTheme.accent)
    }
    return BlueySignInCardPalette(
        surface: BlueyTheme.panelDeep.withAlphaComponent(1.0),
        primaryText: BlueyTheme.text,
        secondaryText: BlueyTheme.textDim,
        border: BlueyTheme.cyan.withAlphaComponent(0.30),
        accent: BlueyTheme.cyan)
}

func blueySignInVisibleBody(from text: String) -> String {
    text.components(separatedBy: .newlines)
        .filter { !blueyIsSignInMetadataLine($0) }
        .joined(separator: "\n")
        .replacingOccurrences(of: "knowledge base", with: "documents")
        .trimmingCharacters(in: .whitespacesAndNewlines)
}

func blueyIsSignInMetadataLine(_ line: String) -> Bool {
    let lower = line.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    return lower.hasPrefix("login_url:")
        || lower.hasPrefix("code:")
        || lower.hasPrefix("connect code:")
        || lower.hasPrefix("fallback code:")
        || lower.contains("desktop code ")
}

func blueyContrastRatio(foreground: NSColor, background: NSColor) -> CGFloat {
    let foregroundLuminance = blueyRelativeLuminance(foreground)
    let backgroundLuminance = blueyRelativeLuminance(background)
    let lighter = max(foregroundLuminance, backgroundLuminance)
    let darker = min(foregroundLuminance, backgroundLuminance)
    return (lighter + 0.05) / (darker + 0.05)
}

private func blueyRelativeLuminance(_ color: NSColor) -> CGFloat {
    guard let rgb = color.usingColorSpace(.sRGB) else { return 0 }
    func linear(_ component: CGFloat) -> CGFloat {
        component <= 0.04045
            ? component / 12.92
            : pow((component + 0.055) / 1.055, 2.4)
    }
    return 0.2126 * linear(rgb.redComponent)
        + 0.7152 * linear(rgb.greenComponent)
        + 0.0722 * linear(rgb.blueComponent)
}

let blueySignInPrimaryActionTitle = "Continue in browser"
let blueySignInFallbackActionTitle = "Copy fallback code"

final class BlueySignInFallbackButton: NSButton {
    private let fallbackCode: String

    init(code: String, palette: BlueySignInCardPalette) {
        fallbackCode = code
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        isBordered = false
        wantsLayer = true
        layer?.cornerRadius = 15
        layer?.backgroundColor = palette.accent.withAlphaComponent(0.10).cgColor
        layer?.borderWidth = 1
        layer?.borderColor = palette.border.cgColor
        font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
        attributedTitle = title(value: blueySignInFallbackActionTitle, color: palette.primaryText)
        if let copyImage = NSImage(systemSymbolName: "doc.on.doc", accessibilityDescription: nil) {
            copyImage.isTemplate = true
            image = copyImage
            imagePosition = .imageLeading
            imageScaling = .scaleProportionallyDown
            contentTintColor = palette.accent
        }
        imageHugsTitle = true
        alignment = .center
        target = self
        action = #selector(copyFallbackCode)
        toolTip = "Use only if the browser did not carry Bluey's connection code"
        setAccessibilityLabel("Copy fallback connection code")
        setAccessibilityHelp("Use only if the browser did not carry the code automatically.")
    }

    required init?(coder: NSCoder) { fatalError() }

    @objc private func copyFallbackCode() {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        guard pasteboard.setString(fallbackCode, forType: .string) else {
            NSSound.beep()
            return
        }
        attributedTitle = title(value: "Code copied", color: BlueyTheme.green)
        toolTip = "Fallback connection code copied"
        setAccessibilityLabel("Fallback connection code copied")
    }

    private func title(value: String, color: NSColor) -> NSAttributedString {
        NSAttributedString(
            string: value,
            attributes: [
                .font: font ?? NSFont.systemFont(ofSize: 11.5, weight: .semibold),
                .foregroundColor: color,
            ])
    }
}
