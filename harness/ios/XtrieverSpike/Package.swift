// swift-tools-version: 5.9
import PackageDescription

// The Feature 001 on-device harness.
//
// A SwiftPM package rather than an .xcodeproj, deliberately: the point of this harness is to take
// the *same* measurements repeatedly and reproducibly (FR-022 requires at least two device runs),
// and `xcodebuild test` against a destination does that from one command. A button-driven app
// would make every run a manual ritual — see harness/ios/README.md.
//
// `Frameworks/XtrieverFFI.xcframework` and `Sources/XtrieverSpike/Generated/` are produced by
// scripts/build-ios-harness.sh and are gitignored; the XCFramework alone is ~262 MB.
let package = Package(
    name: "XtrieverSpike",
    platforms: [.iOS(.v16)],
    products: [
        .library(name: "XtrieverSpike", targets: ["XtrieverSpike"])
    ],
    targets: [
        .binaryTarget(
            name: "XtrieverFFI",
            path: "Frameworks/XtrieverFFI.xcframework"
        ),
        .target(
            name: "XtrieverSpike",
            dependencies: ["XtrieverFFI"],
            // Declared on the LIBRARY target, not the test target: `Bundle.module` is generated
            // per-target, and the code that resolves these paths (SpikeHarness) lives here. A
            // device has no host filesystem to read from, so fixtures — and, for the embedding
            // measurement, the 87.1 MiB of weights — must ride along in the bundle. Staged by
            // scripts/build-ios-harness.sh and gitignored.
            resources: [.copy("XtrieverData")]
        ),
        .testTarget(
            name: "XtrieverSpikeTests",
            dependencies: ["XtrieverSpike"],
            // A device has no host filesystem to read from, so fixtures (and, for the embedding
            // measurement, the 87.1 MiB of weights) must ride along in the bundle. Staged by
            // scripts/build-ios-harness.sh and gitignored.
            // NOT named "Resources": a directory with that name inside a generated .bundle makes
            // codesign reject the whole thing as "bundle format unrecognized, invalid, or
            // unsuitable", because it reads as a malformed bundle layout.
            resources: [.copy("XtrieverData")]
        ),
    ]
)
