import SwiftUI
import Xtriever

/// Host application for the on-device measurement tests (Feature 007, ported from the 001 spike).
///
/// It exists because a device destination cannot run a hostless XCTest bundle. The measurements
/// themselves live in `DeviceMeasurementTests`, so this UI is deliberately inert — it shows enough
/// to confirm the binding is linked and the fixtures are bundled, and nothing more. Adding buttons
/// that duplicate the tests would create a second, hand-driven path whose results nobody records.
@main
struct XtrieverHarnessApp: App {
    var body: some Scene {
        WindowGroup {
            VStack(alignment: .leading, spacing: 12) {
                Text("Xtriever device harness").font(.headline)
                Text("Device: \(Measure.deviceModel)")
                Text("Thermal: \(Measure.thermalState)")
                Text("Build: \(Measure.isDebugBuild ? "Debug" : "Release")")
                Text(HarnessResources.modelsAreBundled ? "Models bundled ✓" : "Models NOT bundled ✗")
                    .foregroundStyle(HarnessResources.modelsAreBundled ? .green : .red)
                Text(HarnessResources.scifactIsBundled ? "SciFact bundled ✓" : "SciFact NOT bundled ✗")
                    .foregroundStyle(HarnessResources.scifactIsBundled ? .green : .red)
                Text("Run the measurements with `xcodebuild test` — see swift/Xtriever/README.md")
                    .font(.footnote).foregroundStyle(.secondary)
            }
            .padding()
        }
    }
}
