import MuxaV2
import SwiftUI

/// The phone's Routines tab: each Routine, its schedule and last result.
struct PhoneRoutines: View {
    @Environment(NextModel.self) private var model
    @State private var path: [String] = []

    var body: some View {
        NavigationStack(path: $path) {
            List {
                ScreenTitle(title: "Routines") {}
                    .cardRow(vertical: 8, horizontal: 18)
                ForEach(model.routines) { r in
                    Button { path.append(r.id) } label: {
                        HStack(spacing: 8) {
                            RoutineRow(routine: r)
                            Image(systemName: "chevron.right")
                                .font(.system(size: 12, weight: .bold))
                                .foregroundStyle(Palette.textTertiary)
                        }
                        .padding(.horizontal, 14)
                        .padding(.vertical, 4)
                        .card()
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .routineElement(r)
                    .cardRow()
                }
                RoutineProblems()
            }
            .listStyle(.plain)
            .groundList()
            .toolbar(.hidden, for: .navigationBar)
            .navigationTitle("Routines")
            .navigationDestination(for: String.self) { id in
                if let r = model.routines.first(where: { $0.id == id }) {
                    RoutinePane(routine: r)
                }
            }
            .refreshable { await model.refreshRoutines() }
            .task { await model.refreshRoutines() }
        }
        .tint(Palette.accent)
    }
}

/// The phone's Settings tab: the connection and the saved hosts.
struct PhoneSettings: View {
    @Environment(NextModel.self) private var model
    @State private var editing: HostDraft?

    var body: some View {
        NavigationStack {
            List {
                Section("Connection") {
                    LabeledContent(model.connectionTitle, value: model.isConnected ? "connected" : "reconnecting")
                    Button("Disconnect", systemImage: "xmark.circle", role: .destructive) {
                        Task { await model.disconnect() }
                    }
                }
                .listRowBackground(Palette.card)
                HostsSection(editing: $editing)
            }
            .groundList()
            .navigationTitle("Settings")
            .sheet(item: $editing) { NextHostEditor(draft: $0) }
        }
        .tint(Palette.accent)
    }
}
