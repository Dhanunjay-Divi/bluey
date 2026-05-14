// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "cue-overlay",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "cue-overlay",
            path: "Sources/cue-overlay",
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("QuartzCore"),
            ]
        )
    ]
)
