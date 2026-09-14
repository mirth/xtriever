import SwiftUI

/// The iOS Wikipedia demo (Feature 009): a thin SwiftUI app over the Xtriever package.
@main
struct WikiDemoApp: App {
    @StateObject private var model = DemoModel()

    /// Under XCTest the app is only a host: opening the index here would put a second copy of
    /// the models and the id map in the process the measurement test is measuring (009 report
    /// F-002). XCUITest launches the app as its own process without this variable, so the UI
    /// walk still exercises the real thing.
    private var isTestHost: Bool { ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] != nil }

    var body: some Scene {
        WindowGroup {
            if isTestHost {
                Text("test host").foregroundStyle(.secondary)
            } else {
                RootView(model: model)
            }
        }
    }
}
