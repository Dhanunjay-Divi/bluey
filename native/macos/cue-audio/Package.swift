// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "cue-audio",
    platforms: [.macOS(.v13)],
    targets: [
        .target(
            name: "AudioBridge",
            path: "Sources/AudioBridge",
            publicHeadersPath: "include",
            cSettings: [
                .unsafeFlags([
                    "-std=c11",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-Wconversion",
                    "-Wshadow",
                    "-Wstrict-prototypes",
                ]),
            ],
            linkerSettings: [
                .linkedFramework("AudioToolbox"),
                .linkedFramework("CoreMedia"),
            ]
        ),
        .target(
            name: "CueAudioCore",
            path: "Sources/CueAudioCore"
        ),
        .executableTarget(
            name: "cue-audio",
            dependencies: [
                "AudioBridge",
                "CueAudioCore",
            ],
            path: "Sources/cue-audio",
            linkerSettings: [
                .linkedFramework("AVFoundation"),
                .linkedFramework("AudioToolbox"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("ScreenCaptureKit"),
            ]
        ),
        .executableTarget(
            name: "audio-bridge-tests",
            dependencies: ["AudioBridge"],
            path: "Tests/AudioBridgeTests"
        ),
        .executableTarget(
            name: "cue-audio-core-tests",
            dependencies: ["CueAudioCore"],
            path: "Tests/CueAudioCoreTests"
        ),
    ]
)
