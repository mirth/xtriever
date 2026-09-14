import SwiftUI

/// What the app is doing before a search can succeed (spec US1 scenario 1; edge cases 1–2).
struct PreparationView: View {
    @ObservedObject var model: DemoModel

    var body: some View {
        VStack(spacing: 16) {
            switch model.preparation {
            case .failed(let failure):
                Image(systemName: "exclamationmark.triangle").font(.largeTitle)
                Text(failure.message).font(.headline).multilineTextAlignment(.center)
                Text(failure.remedy).font(.footnote).foregroundStyle(.secondary).multilineTextAlignment(.center)
                Button("Retry") { Task { await model.start() } }.buttonStyle(.borderedProminent)
            default:
                ProgressView()
                Text(model.preparation.label).font(.headline).multilineTextAlignment(.center)
                Text("Simple English Wikipedia, offline, through the whole pipeline.")
                    .font(.footnote).foregroundStyle(.secondary)
            }
        }
        .padding()
    }
}
