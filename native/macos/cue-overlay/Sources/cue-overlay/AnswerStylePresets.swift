import AppKit

let blueyMaximumAnswerStyleInstructionBytes = 16 * 1024

func blueyBoundAnswerStyleInstructions(_ instructions: String) -> String {
    guard instructions.utf8.count > blueyMaximumAnswerStyleInstructionBytes else {
        return instructions
    }
    var result = ""
    result.reserveCapacity(blueyMaximumAnswerStyleInstructionBytes)
    var usedBytes = 0
    for character in instructions {
        let byteCount = String(character).utf8.count
        guard usedBytes + byteCount <= blueyMaximumAnswerStyleInstructionBytes else { break }
        result.append(character)
        usedBytes += byteCount
    }
    return result
}

enum BlueyAnswerStylePreset: Int, CaseIterable {
    case standard
    case concise
    case star
    case custom

    var title: String {
        switch self {
        case .standard: "Default"
        case .concise: "Concise"
        case .star: "STAR"
        case .custom: "Custom"
        }
    }

    var instructions: String? {
        switch self {
        case .standard:
            ""
        case .concise:
            "Give the direct, speakable answer first. Keep it concise, usually 2-4 sentences unless the task needs more detail. Use only verified session context and user-provided facts; never invent details."
        case .star:
            "For behavioral questions, answer in concise labeled Situation, Task, Action, and Result sections. Use only facts supported by the session, approved context, or the user. If a required fact is missing, say what is missing or leave a clear placeholder; never invent employers, projects, dates, people, metrics, or outcomes. For non-behavioral questions, answer directly and concisely."
        case .custom:
            nil
        }
    }

    var guidance: String {
        switch self {
        case .standard:
            "Bluey chooses the clearest structure for each question."
        case .concise:
            "A direct, speakable answer, usually in 2-4 sentences."
        case .star:
            "Evidence-backed Situation, Task, Action, and Result."
        case .custom:
            "Write session-specific answer instructions below."
        }
    }

    static func matching(instructions: String) -> BlueyAnswerStylePreset {
        let normalized = instructions.trimmingCharacters(in: .whitespacesAndNewlines)
        if normalized.isEmpty {
            return .standard
        }
        if normalized == concise.instructions {
            return .concise
        }
        if normalized == star.instructions {
            return .star
        }
        return .custom
    }
}

final class BlueyAnswerStylePicker: NSView {
    private let segmentedControl: NSSegmentedControl
    private let guidanceLabel: NSTextField

    var onSelectionChanged: ((BlueyAnswerStylePreset) -> Void)?

    var selectedPreset: BlueyAnswerStylePreset {
        BlueyAnswerStylePreset(rawValue: segmentedControl.selectedSegment) ?? .standard
    }

    override init(frame frameRect: NSRect) {
        segmentedControl = NSSegmentedControl(
            labels: BlueyAnswerStylePreset.allCases.map(\.title),
            trackingMode: .selectOne,
            target: nil,
            action: nil)
        guidanceLabel = NSTextField(wrappingLabelWithString: "")
        super.init(frame: frameRect)

        translatesAutoresizingMaskIntoConstraints = false
        segmentedControl.translatesAutoresizingMaskIntoConstraints = false
        guidanceLabel.translatesAutoresizingMaskIntoConstraints = false
        segmentedControl.target = self
        segmentedControl.action = #selector(selectionChanged)
        segmentedControl.segmentStyle = .rounded
        segmentedControl.selectedSegment = BlueyAnswerStylePreset.standard.rawValue
        segmentedControl.setAccessibilityLabel("Answer style")
        segmentedControl.setAccessibilityHelp(
            "Choose Default, Concise, STAR, or Custom for this session.")

        guidanceLabel.font = NSFont.systemFont(ofSize: 10.5, weight: .medium)
        guidanceLabel.textColor = .secondaryLabelColor
        guidanceLabel.alignment = .center
        guidanceLabel.maximumNumberOfLines = 2
        guidanceLabel.setContentCompressionResistancePriority(.required, for: .vertical)
        guidanceLabel.setAccessibilityLabel("Answer style description")

        addSubview(segmentedControl)
        addSubview(guidanceLabel)
        NSLayoutConstraint.activate([
            segmentedControl.topAnchor.constraint(equalTo: topAnchor),
            segmentedControl.leadingAnchor.constraint(equalTo: leadingAnchor),
            segmentedControl.trailingAnchor.constraint(equalTo: trailingAnchor),
            segmentedControl.heightAnchor.constraint(equalToConstant: 30),
            guidanceLabel.topAnchor.constraint(equalTo: segmentedControl.bottomAnchor, constant: 6),
            guidanceLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 4),
            guidanceLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -4),
            guidanceLabel.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
        refreshGuidance()
    }

    required init?(coder: NSCoder) { fatalError() }

    func select(matching instructions: String) {
        segmentedControl.selectedSegment = BlueyAnswerStylePreset.matching(
            instructions: instructions).rawValue
        refreshGuidance()
    }

    func resolvedInstructions(customInstructions: String) -> String {
        selectedPreset.instructions
            ?? blueyBoundAnswerStyleInstructions(
                customInstructions.trimmingCharacters(in: .whitespacesAndNewlines))
    }

    func applyTheme(light: Bool) {
        appearance = NSAppearance(named: light ? .aqua : .darkAqua)
        guidanceLabel.textColor = light
            ? NSColor.black.withAlphaComponent(0.70)
            : NSColor.white.withAlphaComponent(0.72)
    }

    @objc private func selectionChanged() {
        refreshGuidance()
        onSelectionChanged?(selectedPreset)
    }

    private func refreshGuidance() {
        guidanceLabel.stringValue = selectedPreset.guidance
        guidanceLabel.setAccessibilityValue(selectedPreset.guidance)
    }
}
