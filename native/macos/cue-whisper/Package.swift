// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "CueWhisper",
    platforms: [.macOS(.v13)],
    dependencies: [
        .package(url: "https://github.com/exPHAT/SwiftWhisper", from: "1.0.0"),
    ],
    targets: [
        .executableTarget(
            name: "CueWhisper",
            dependencies: ["SwiftWhisper"],
            path: "Sources/CueWhisper"
        ),
    ]
)
