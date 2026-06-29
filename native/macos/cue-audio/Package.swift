// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "cue-audio",
    // macOS 14 for SCContentSharingPicker (--pick mode). Capture itself still
    // runs on 13 at runtime via availability guards; only the app-picker needs 14.
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "cue-audio",
            path: "Sources/cue-audio",
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("AVFoundation"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("ScreenCaptureKit"),
            ]
        )
    ]
)
