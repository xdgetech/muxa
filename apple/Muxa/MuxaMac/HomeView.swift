import MuxaV2
import SwiftUI

/// The Mac's Home (ADR 0066, ADR 0070): the Inbox and the Routines, a list
/// and a detail.
struct HomeView: View {
    @Environment(NextModel.self) private var model
    var openTerminal: (Terminal) -> Void = { _ in }
    var showFocus: () -> Void = {}

    enum Part: String, CaseIterable, Identifiable {
        case inbox = "Inbox", routines = "Routines"
        var id: Self { self }
    }

    @State private var part = Part.inbox
    @State private var selectedItem: String?
    @State private var selectedRoutine: String?
    @State private var filter = InboxFilter()
    @State private var now = Int64(Date().timeIntervalSince1970)
    @State private var columns = NavigationSplitViewVisibility.all

    var body: some View {
        NavigationSplitView(columnVisibility: $columns) {
            VStack(spacing: 8) {
                modeBar.padding(.horizontal)
                CaptureField().padding(.horizontal)
                NeedsYouBanner(open: openTerminal, showAll: showFocus).padding(.horizontal)
                Picker("Show", selection: $part) {
                    ForEach(Part.allCases) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented)
                .padding(.horizontal)
                .accessibilityIdentifier("home.part")
                if part == .inbox { inboxList } else { routineList }
            }
            .background(Palette.surface)
            .navigationTitle("Home")
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    if part == .inbox { InboxFilterMenu(filter: $filter, items: model.inbox.items) }
                }
            }
            .safeAreaInset(edge: .bottom) { LinkBanner() }
        } detail: {
            NavigationStack { detail }.background(Palette.ground)
        }
        .navigationSplitViewStyle(.balanced)
        .task {
            // Later items come back on their own; the clock decides what shows.
            while !Task.isCancelled {
                now = Int64(Date().timeIntervalSince1970)
                try? await Task.sleep(for: .seconds(30))
            }
        }
    }

    // MARK: Inbox

    private var visible: [InboxItem] { filter.apply(model.inbox.items, now: now) }

    private var inboxList: some View {
        List(selection: $selectedItem) {
            ForEach(visible) { item in
                ItemRow(item: item, now: now)
                    .tag(item.id)
                    .swipeActions(edge: .trailing) { ItemSwipeActions(item: item) }
            }
            .onMove(perform: filter.isActive ? nil : { model.move(in: visible, from: $0, to: $1) })
            if visible.isEmpty {
                Text(model.inbox.items.isEmpty ? "Nothing to do." : "Nothing matches the filter.")
                    .foregroundStyle(Palette.textSecondary)
            }
            if filter.showDone, !model.inbox.done.isEmpty {
                Section("Done") {
                    ForEach(model.inbox.done) { ItemRow(item: $0, now: now).tag($0.id) }
                }
            }
        }
        .scrollContentBackground(.hidden)
        .accessibilityIdentifier("inbox.list")
        .background { ReorderKeys(selected: selectedItem) }
    }

    // MARK: Routines

    private var routineList: some View {
        List(selection: $selectedRoutine) {
            ForEach(model.routines) { RoutineRow(routine: $0).routineElement($0).tag($0.id) }
            RoutineProblems()
        }
        .scrollContentBackground(.hidden)
        .refreshable { await model.refreshRoutines() }
        .task { await model.refreshRoutines() }
    }

    // MARK: Detail

    @ViewBuilder
    private var detail: some View {
        switch part {
        case .inbox:
            if let id = selectedItem, let item = (model.inbox.items + model.inbox.done).first(where: { $0.id == id }) {
                let position = model.inbox.items.firstIndex(where: { $0.id == id }).map { $0 + 1 }
                InboxItemPane(item: item, position: position) { selectedItem = filter.next(after: id, in: visible) }
                    .id(item.id)
            } else {
                ContentUnavailableView(
                    model.inbox.items.isEmpty ? "Inbox zero" : "Select an item",
                    systemImage: "tray")
            }
        case .routines:
            if let id = selectedRoutine, let r = model.routines.first(where: { $0.id == id }) {
                RoutinePane(routine: r)
            } else {
                ContentUnavailableView("Select a Routine", systemImage: "clock.arrow.circlepath")
            }
        }
    }

    // MARK: Chrome

    /// The host and the way to Focus: always shown, also in a narrow sidebar.
    private var modeBar: some View {
        HStack {
            HostMenu()
            Spacer()
            NewTaskButton(id: "home.newtask")
                .font(.callout)
            Button("Focus", systemImage: "terminal", action: showFocus)
                .font(.callout)
                .accessibilityIdentifier("mode.focus")
        }
    }
}
