import MuxaV2
import SwiftUI

/// Home (ADR 0066): the Inbox and the Routines, a narrow list and a wide
/// detail. On a phone the list and the detail are two screens.
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
    @State private var kindFilter: ItemKind?
    @State private var workspaceFilter: String?
    @State private var showHidden = false
    @State private var showDone = false
    @State private var editMode = EditMode.inactive
    @State private var now = Int64(Date().timeIntervalSince1970)
    /// The list stays beside the detail on a tablet, also in portrait.
    @State private var columns = NavigationSplitViewVisibility.all

    var body: some View {
        NavigationSplitView(columnVisibility: $columns) {
            VStack(spacing: 8) {
                modeBar.padding(.horizontal)
                CaptureField().padding(.horizontal)
                NeedsYouStrip(open: openTerminal).padding(.horizontal)
                Picker("Show", selection: $part) {
                    ForEach(Part.allCases) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented)
                .padding(.horizontal)
                .accessibilityIdentifier("home.part")
                if part == .inbox { inboxList } else { routineList }
            }
            .navigationTitle("Home")
            .toolbar { toolbar }
            .safeAreaInset(edge: .bottom) { linkBanner }
        } detail: {
            NavigationStack { detail }
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

    private var visible: [InboxItem] {
        model.inbox.items.filter { item in
            (kindFilter == nil || item.kind == kindFilter)
                && (workspaceFilter == nil || item.workspace == workspaceFilter)
                && (showHidden || !item.isLater(now: now))
        }
    }

    private var filtered: Bool { kindFilter != nil || workspaceFilter != nil }

    private var inboxList: some View {
        List(selection: $selectedItem) {
            ForEach(visible) { item in
                ItemRow(item: item, now: now)
                    .tag(item.id)
                    .swipeActions(edge: .trailing) {
                        Button("Done") { Task { await model.inbox(.inboxDone(item: item.id)) } }.tint(.green)
                        Button("Later") {
                            Task { await model.inbox(.inboxLater(item: item.id, until: LaterChoice.tomorrow.until())) }
                        }
                        .tint(.orange)
                    }
            }
            .onMove(perform: mover)
            if visible.isEmpty {
                Text(model.inbox.items.isEmpty ? "Nothing to do." : "Nothing matches the filter.")
                    .foregroundStyle(.secondary)
            }
            if showDone, !model.inbox.done.isEmpty {
                Section("Done") {
                    ForEach(model.inbox.done) { ItemRow(item: $0, now: now).tag($0.id) }
                }
            }
        }
        .environment(\.editMode, $editMode)
        .accessibilityIdentifier("inbox.list")
        .background { reorderKeys }
    }

    /// `⌥↑` / `⌥↓` move the selected item one place.
    private var reorderKeys: some View {
        Group {
            Button("") { step(-1) }.keyboardShortcut(.upArrow, modifiers: .option)
            Button("") { step(1) }.keyboardShortcut(.downArrow, modifiers: .option)
        }
        .opacity(0)
        .accessibilityHidden(true)
    }

    private func step(_ by: Int) {
        guard let id = selectedItem, let at = model.inbox.items.firstIndex(where: { $0.id == id }) else { return }
        let to = at + by
        guard to >= 0, to < model.inbox.items.count else { return }
        Task { await model.inbox(.inboxMove(item: id, index: to)) }
    }

    /// Dragging reorders only the whole list: a filtered one has gaps.
    private var mover: ((IndexSet, Int) -> Void)? {
        filtered ? nil : { move(from: $0, to: $1) }
    }

    /// A drag in the unfiltered list; the server keeps the order.
    private func move(from source: IndexSet, to destination: Int) {
        guard let from = source.first else { return }
        let id = visible[from].id
        // `destination` counts the moved row still in place.
        let index = destination > from ? destination - 1 : destination
        Task { await model.inbox(.inboxMove(item: id, index: index)) }
    }

    // MARK: Routines

    private var routineList: some View {
        List(selection: $selectedRoutine) {
            ForEach(model.routines) { RoutineRow(routine: $0).tag($0.id) }
            if !model.routineProblems.isEmpty {
                Section("Cannot read") {
                    ForEach(model.routineProblems, id: \.workspace) { p in
                        VStack(alignment: .leading) {
                            Text(p.workspace)
                            Text(p.reason).font(.caption).foregroundStyle(.red)
                        }
                    }
                }
            }
        }
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
                let next = nextVisible(after: id)
                InboxItemPane(item: item, position: position) { selectedItem = next }
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

    private func nextVisible(after id: String) -> String? {
        guard let at = visible.firstIndex(where: { $0.id == id }) else { return nil }
        return visible.indices.contains(at + 1) ? visible[at + 1].id : (at > 0 ? visible[at - 1].id : nil)
    }

    // MARK: Chrome

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .primaryAction) {
            if part == .inbox {
                Menu {
                    Picker("Kind", selection: $kindFilter) {
                        Text("Every kind").tag(ItemKind?.none)
                        ForEach(ItemKind.allCases, id: \.self) { Text($0.title).tag(Optional($0)) }
                    }
                    Picker("Workspace", selection: $workspaceFilter) {
                        Text("Every Workspace").tag(String?.none)
                        ForEach(workspaces, id: \.self) { Text($0).tag(Optional($0)) }
                    }
                    Toggle("Show Later Items", isOn: $showHidden)
                    Toggle("Show Done", isOn: $showDone)
                } label: {
                    Image(systemName: filtered || showHidden || showDone
                        ? "line.3.horizontal.decrease.circle.fill" : "line.3.horizontal.decrease.circle")
                        .accessibilityLabel("Filter")
                }
            }
        }
        ToolbarItem(placement: .topBarLeading) {
            if part == .inbox, !filtered {
                Button(editMode.isEditing ? "Done" : "Reorder") {
                    editMode = editMode.isEditing ? .inactive : .active
                }
            }
        }
    }

    /// The host and the way to Focus: always shown, also in a narrow sidebar.
    private var modeBar: some View {
        HStack {
            Menu {
                Button("Disconnect", systemImage: "xmark.circle") { Task { await model.disconnect() } }
            } label: {
                Label(model.demo ? "Demo" : model.currentHost?.title ?? "Host", systemImage: "server.rack")
                    .font(.callout)
            }
            .accessibilityIdentifier("home.host")
            Spacer()
            Button("Focus", systemImage: "terminal", action: showFocus)
                .font(.callout)
                .accessibilityIdentifier("mode.focus")
        }
    }

    private var workspaces: [String] {
        Array(Set(model.inbox.items.compactMap(\.workspace))).sorted()
    }

    @ViewBuilder
    private var linkBanner: some View {
        if case .reconnecting(let why) = model.link {
            Label("Reconnecting… \(why)", systemImage: "wifi.exclamationmark")
                .font(.caption).foregroundStyle(.secondary)
                .frame(maxWidth: .infinity).padding(6).background(.bar)
        }
    }
}
