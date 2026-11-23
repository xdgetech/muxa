// swift-tools-version: 6.0
// SPDX-License-Identifier: GPL-3.0-only (see ../LICENSE)
import PackageDescription

let package = Package(
    name: "MuxaKit",
    // macOS 15: Citadel's exec channel needs it, and `swift test` runs on macOS.
    platforms: [.iOS(.v17), .macOS(.v15)],
    products: [
        .library(name: "MuxaTransport", targets: ["MuxaTransport"]),
        .library(name: "MuxaSession", targets: ["MuxaSession"]),
        .library(name: "MuxaUI", targets: ["MuxaUI"]),
        .library(name: "MuxaV2", targets: ["MuxaV2"]),
    ],
    dependencies: [
        .package(url: "https://github.com/migueldeicaza/SwiftTerm.git", from: "1.99.1"),
        .package(url: "https://github.com/orlandos-nl/Citadel.git", .upToNextMinor(from: "0.12.1")),
        // Citadel pins this fork of swift-nio-ssh (ADR 0003); keep the same range.
        .package(url: "https://github.com/Wellz26/swift-nio-ssh.git", "0.3.4"..<"0.4.0"),
        .package(url: "https://github.com/apple/swift-nio.git", from: "2.80.0"),
    ],
    targets: [
        .target(
            name: "MuxaTransport",
            dependencies: [
                .product(name: "Citadel", package: "Citadel"),
                .product(name: "NIOSSH", package: "swift-nio-ssh"),
                .product(name: "NIOCore", package: "swift-nio"),
            ]
        ),
        .target(
            name: "MuxaSession",
            dependencies: ["MuxaTransport"]
        ),
        // UIKit views for the app; empty on macOS.
        .target(
            name: "MuxaUI",
            dependencies: ["MuxaSession", .product(name: "SwiftTerm", package: "SwiftTerm")]
        ),
        // Protocol v2 (ADR 0067): frames, messages and a client that resumes.
        .target(name: "MuxaV2", dependencies: ["MuxaTransport"]),
        .testTarget(
            name: "MuxaV2Tests",
            dependencies: ["MuxaV2", "MuxaTransport"],
            // Plain wire JSON, so the Kotlin tests can read the same cases.
            resources: [.copy("Fixtures")]
        ),
        .testTarget(
            name: "MuxaTransportTests",
            dependencies: ["MuxaTransport"]
        ),
        .testTarget(
            name: "MuxaSessionTests",
            dependencies: ["MuxaSession", "MuxaTransport"]
        ),
    ]
)
