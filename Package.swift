// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "Radio",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "Radio",
            path: "Sources/Radio",
            swiftSettings: [.swiftLanguageMode(.v5)]
        )
    ]
)
