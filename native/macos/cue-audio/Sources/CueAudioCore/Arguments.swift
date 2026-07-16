public enum CaptureSource: String, Equatable, Sendable {
    case system
    case microphone
}

public struct CaptureArguments: Equatable, Sendable {
    public let source: CaptureSource
    public let durationMs: UInt32
    public let continuous: Bool

    public init(
        source: CaptureSource = .system,
        durationMs: UInt32 = 3_000,
        continuous: Bool = false
    ) {
        self.source = source
        self.durationMs = durationMs
        self.continuous = continuous
    }
}

public enum CaptureArgumentError: String, Error, Equatable, Sendable {
    case unknownOption = "unknown_option"
    case missingValue = "missing_value"
    case invalidSource = "invalid_source"
    case invalidDuration = "invalid_duration"
    case duplicateOption = "duplicate_option"
    case conflictingOptions = "conflicting_options"
}

public func parseCaptureArguments(_ arguments: [String]) throws -> CaptureArguments {
    var source = CaptureSource.system
    var durationMs: UInt32 = 3_000
    var continuous = false
    var sawSource = false
    var sawDuration = false
    var sawContinuous = false
    var index = 0

    while index < arguments.count {
        switch arguments[index] {
        case "--source":
            guard !sawSource else {
                throw CaptureArgumentError.duplicateOption
            }
            index += 1
            guard index < arguments.count else {
                throw CaptureArgumentError.missingValue
            }
            guard let parsedSource = CaptureSource(rawValue: arguments[index]) else {
                throw CaptureArgumentError.invalidSource
            }
            source = parsedSource
            sawSource = true

        case "--duration-ms":
            guard !sawDuration else {
                throw CaptureArgumentError.duplicateOption
            }
            index += 1
            guard index < arguments.count else {
                throw CaptureArgumentError.missingValue
            }
            durationMs = try parseDuration(arguments[index])
            sawDuration = true

        case "--continuous":
            guard !sawContinuous else {
                throw CaptureArgumentError.duplicateOption
            }
            continuous = true
            sawContinuous = true

        default:
            throw CaptureArgumentError.unknownOption
        }
        index += 1
    }

    guard !(sawContinuous && sawDuration) else {
        throw CaptureArgumentError.conflictingOptions
    }
    return CaptureArguments(
        source: source,
        durationMs: durationMs,
        continuous: continuous
    )
}

private func parseDuration(_ value: String) throws -> UInt32 {
    guard !value.isEmpty, value.utf8.allSatisfy({ $0 >= 48 && $0 <= 57 }) else {
        throw CaptureArgumentError.invalidDuration
    }
    guard let duration = UInt32(value), (250...30_000).contains(duration) else {
        throw CaptureArgumentError.invalidDuration
    }
    return duration
}
