import MuxaV2
import SwiftUI

/// The Routines page of the desktop mocks: a list beside the picked Routine.
struct RoutinesView: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @State private var selected: String?

    var body: some View {
        VStack(spacing: 0) {
            MacHeaderBar {
                Button { nav.tab = .inbox } label: {
                    Label("Home", systemImage: "chevron.left").foregroundStyle(Palette.textSecondary)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("routines.home")
                Text("Routines").font(.system(size: 17, weight: .bold))
            } trailing: {
                EmptyView()
            }
            HStack(spacing: 0) {
                List(selection: $selected) {
                    ForEach(model.routines) { RoutineRow(routine: $0).routineElement($0).tag($0.id) }
                    RoutineProblems()
                }
                .scrollContentBackground(.hidden)
                .column(width: 420)
                Group {
                    if let id = selected, let r = model.routines.first(where: { $0.id == id }) {
                        RoutinePane(routine: r)
                    } else {
                        ContentUnavailableView("Select a Routine", systemImage: "clock.arrow.circlepath")
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Palette.pane)
            }
        }
        .background(Palette.ground)
        .task { await model.refreshRoutines() }
    }
}
