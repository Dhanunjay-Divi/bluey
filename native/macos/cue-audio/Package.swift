// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "cue-audio",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "cue-audio",
            path: "Sources/cue-audio",
            linkerSettings: [
                .linkedFramework("AVFoundation"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("ScreenCaptureKit"),
            ]
        )
    ]
)
