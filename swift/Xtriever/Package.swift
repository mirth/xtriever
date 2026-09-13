// swift-tools-version: 5.9
import PackageDescription

// The Xtriever Swift package (Feature 007): the binary framework built from `xtriever-ffi`, the
// uniffi-generated bindings, and a thin async layer (`XtrieverIndex`). Consumed by path from the
// demo app (`apps/ios-wiki-demo/`, Feature 009) and hosted on a device by
// `swift/XtrieverHarnessApp/` for the measurement tests.
//
// `Frameworks/XtrieverFFI.xcframework`, `Sources/Xtriever/Generated/` and
// `Sources/Xtriever/XtrieverData/` are produced by scripts/build-ios-package.sh and are
// gitignored: the XCFramework alone is ~260 MB, the bindings are generated from the Rust, and the
// staged resources (models, indexes) are pinned elsewhere.
let package = Package(
    name: "Xtriever",
    platforms: [.iOS(.v16)],
    products: [
        .library(name: "Xtriever", targets: ["Xtriever"])
    ],
    targets: [
        .binaryTarget(
            name: "XtrieverFFI",
            path: "Frameworks/XtrieverFFI.xcframework"
        ),
        .target(
            name: "Xtriever",
            dependencies: ["XtrieverFFI"],
            // Declared on the LIBRARY target, not the test target: `Bundle.module` is generated
            // per-target, and `HarnessResources` (the code that resolves these paths) lives here.
            // A device has no host filesystem, so the fixture index, the models and the SciFact
            // index must ride along in the bundle. Staged by scripts/build-ios-package.sh.
            resources: [.copy("XtrieverData")]
        ),
        .testTarget(
            name: "XtrieverTests",
            dependencies: ["Xtriever"]
        ),
    ]
)
