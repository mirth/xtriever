import SwiftUI
import XtrieverSpike

/// Host application for the on-device measurement tests.
///
/// It exists because a device destination cannot run a hostless XCTest bundle. The measurements
/// themselves live in `DeviceMeasurementTests`, so this UI is deliberately inert — it shows enough
/// to confirm the binding is linked and the fixtures are bundled, and nothing more. Adding buttons
/// that duplicate the tests would create a second, hand-driven path whose results nobody records.
@main
struct XtrieverSpikeApp: App {
    var body: some Scene {
        WindowGroup {
            VStack(alignment: .leading, spacing: 12) {
                Text("Xtriever spike harness").font(.headline)
                Text("Device: \(Measure.deviceModel)")
                Text("Thermal: \(Measure.thermalState)")
                Text("Build: \(Measure.isDebugBuild ? "Debug" : "Release")")
                Text(SpikeHarness.modelIsAvailable ? "Model bundled ✓" : "Model NOT bundled ✗")
                    .foregroundStyle(SpikeHarness.modelIsAvailable ? .green : .red)
                Text("Run the measurements with `xcodebuild test` — see harness/ios/DEVICE-RUN.md")
                    .font(.footnote).foregroundStyle(.secondary)
            }
            .padding()
        }
    }
}
