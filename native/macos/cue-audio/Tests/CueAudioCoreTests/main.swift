import CueAudioCore
import Darwin
import Foundation

private enum TestFailure: Error {
    case mismatch(String)
}

private func require(
    _ condition: @autoclosure () throws -> Bool,
    _ message: String
) throws {
    if try !condition() {
        throw TestFailure.mismatch(message)
    }
}

private func requireError(
    _ arguments: [String],
    _ expected: CaptureArgumentError
) throws {
    do {
        _ = try parseCaptureArguments(arguments)
        throw TestFailure.mismatch("accepted invalid arguments: \(arguments)")
    } catch let error as CaptureArgumentError {
        try require(
            error == expected,
            "expected \(expected.rawValue), got \(error.rawValue)"
        )
    }
}

private func runTests() throws {
    try require(
        try parseCaptureArguments([]) == CaptureArguments(),
        "default arguments changed"
    )
    try require(
        try parseCaptureArguments([
            "--source", "microphone",
            "--duration-ms", "30000",
        ]) == CaptureArguments(
            source: .microphone,
            durationMs: 30_000
        ),
        "microphone duration was not parsed"
    )
    try require(
        try parseCaptureArguments([
            "--continuous",
            "--source", "system",
        ]) == CaptureArguments(
            source: .system,
            continuous: true
        ),
        "continuous mode was not parsed"
    )

    let invalidCases: [([String], CaptureArgumentError)] = [
        (["--bogus"], .unknownOption),
        (["--source"], .missingValue),
        (["--duration-ms"], .missingValue),
        (["--source", "speaker"], .invalidSource),
        (["--duration-ms", ""], .invalidDuration),
        (["--duration-ms", "+1000"], .invalidDuration),
        (["--duration-ms", " 1000"], .invalidDuration),
        (["--duration-ms", "1000ms"], .invalidDuration),
        (["--duration-ms", "249"], .invalidDuration),
        (["--duration-ms", "30001"], .invalidDuration),
        (["--duration-ms", "999999999999999999999"], .invalidDuration),
        (["--source", "system", "--source", "microphone"], .duplicateOption),
        (["--duration-ms", "1000", "--duration-ms", "2000"], .duplicateOption),
        (["--continuous", "--continuous"], .duplicateOption),
        (["--continuous", "--duration-ms", "1000"], .conflictingOptions),
    ]
    for (arguments, expected) in invalidCases {
        try requireError(arguments, expected)
    }
}

do {
    try runTests()
    print("cue-audio argument tests passed")
} catch {
    fputs("cue-audio argument tests failed: \(error)\n", stderr)
    exit(1)
}
