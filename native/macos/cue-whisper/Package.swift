// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "CueWhisper",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "CueWhisper",
            path: "Sources/CueWhisper"
        ),
    ]
)
