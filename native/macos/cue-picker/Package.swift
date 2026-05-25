// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "cue-picker",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "cue-picker",
            path: "Sources/cue-picker",
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("UniformTypeIdentifiers"),
            ]
        )
    ]
)
