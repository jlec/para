// swift-tools-version:5.10
import PackageDescription

let package = Package(
    name: "ParaBridge",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .library(
            name: "ParaBridge",
            type: .static,
            targets: ["ParaBridge"]
        )
    ],
    dependencies: [
        .package(url: "https://github.com/FluidInference/FluidAudio.git", exact: "0.15.5")
    ],
    targets: [
        .target(
            name: "ParaBridge",
            dependencies: [
                .product(name: "FluidAudio", package: "FluidAudio")
            ],
            path: "Sources/ParaBridge"
        )
    ]
)
