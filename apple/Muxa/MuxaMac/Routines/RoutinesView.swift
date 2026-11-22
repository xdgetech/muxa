import MuxaV2
import SwiftUI

/// The Routines page of the desktop mocks: a list beside the picked Routine.
struct RoutinesView: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @State private var selected: String?

    /// The picked Routine, else the first in the list's order.
    private var current: Routine? {
        if let id = selected, let r = model.routines.first(where: { $0.id == id }) { return r }
        return RoutineGroup.allCases.lazy.compactMap { g in model.routines.first { $0.group == g } }.first
    }

    var body: some View {
        VStack(spacing: 0) {
            MacHeaderBar {
                Button { nav.tab = .inbox } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "chevron.left").font(.system(size: 12, weight: .bold))
                        Text("Home")
                    }
                    .foregroundStyle(Palette.textSecondary)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("routines.home")
                Text("Routines").font(.system(size: 17, weight: .bold))
                summary
            } trailing: {
                EmptyView()
            }
            HStack(spacing: 0) {
                RoutineList(selected: $selected, current: current)
                    .column(width: 420)
                if let r = current {
                    RoutineDetailPane(routine: r)
                } else {
                    Text("Pick a Routine")
                        .foregroundStyle(Palette.textTertiary)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .background(Palette.pane)
                }
            }
        }
        .font(.system(size: 13))
        .foregroundStyle(Palette.text)
        .background(Palette.ground)
        .task {
            await model.refreshRoutines()
            // For Brief, Worker, Project and Pause; the page works without them.
            await model.settings.load()
        }
    }

    private var summary: some View {
        let count = { (g: RoutineGroup) in model.routines.filter { $0.group == g }.count }
        return HStack(spacing: 0) {
            Text("\(count(.running)) running · \(count(.paused)) paused · ")
            Text("\(count(.failed)) failed").foregroundStyle(count(.failed) > 0 ? Palette.late : Palette.textSecondary)
        }
        .foregroundStyle(Palette.textSecondary)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("routines.summary")
    }
}
