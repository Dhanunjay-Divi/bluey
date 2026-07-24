// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "cue-shot",
    // macOS 14 for the ScreenCaptureKit one-shot API (SCScreenshotManager).
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "cue-shot",
            path: "Sources/cue-shot",
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("CoreGraphics"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("ScreenCaptureKit"),
            ]
        )
    ]
)
