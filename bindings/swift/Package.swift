// swift-tools-version:5.5
import PackageDescription

let package = Package(
    name: "DivvunRuntime",
    platforms: [
        .macOS(.v10_15),
        .iOS(.v13)
    ],
    products: [
        .library(
            name: "DivvunRuntime",
            targets: ["DivvunRuntime"]),
    ],
    dependencies: [],
    targets: [
        .systemLibrary(
            name: "CDivvunRuntime",
            pkgConfig: "divvun-runtime",
            providers: [
                .apt(["libdivvun-runtime-dev"]),
                .brew(["divvun-runtime"])
            ]
        ),
        .target(
            name: "DivvunRuntime",
            dependencies: ["CDivvunRuntime"]),
        .testTarget(
            name: "DivvunRuntimeTests",
            dependencies: ["DivvunRuntime"]),
    ]
)