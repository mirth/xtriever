import SwiftUI

/// The preparation gate, then the search screen (spec FR-002).
struct RootView: View {
    @ObservedObject var model: DemoModel
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        Group {
            if case .ready(let info) = model.preparation {
                SearchView(model: model, info: info)
            } else {
                PreparationView(model: model)
            }
        }
        .task { await model.start() }
        .onChange(of: scenePhase) { phase in
            // Back from the background: the index may have been torn down with the process;
            // a failed engine state re-opens rather than assuming (edge case 3).
            if phase == .active, case .failed(.engine) = model.preparation { Task { await model.start() } }
        }
    }
}
