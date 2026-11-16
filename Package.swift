// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "Snowdrift",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "Snowdrift",
            path: "Sources/Snowdrift",
            // The per-frame loops reach class properties millions of times a second; Swift's runtime exclusivity
            // checks around each access cost as much as the physics itself.
            swiftSettings: [.unsafeFlags(["-enforce-exclusivity=unchecked"], .when(configuration: .release))],
            linkerSettings: [.unsafeFlags(["-Xlinker", "-rpath", "-Xlinker", "@executable_path/../Frameworks"])]
        ),
    ]
)
