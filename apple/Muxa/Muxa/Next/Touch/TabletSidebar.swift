import MuxaV2
import SwiftUI

/// The tablet's sidebar: a title and New Task (New Bot on Bots), the Inbox |
/// Tasks | Bots segments, the list, and the host and Settings at the foot.
struct TabletSidebar: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let mode: Binding<NextNavigation.Tab>
    @Binding var selectedItem: String?
    @Binding var selectedBot: BotsDestination?
    @Binding var botPath: [BotsDestination]
    @Binding var selectedSetting: SettingsPlace?
    @Binding var filter: InboxFilter
    let now: Int64
    @State private var reordering = false
    @State private var showOthers = false
    @State private var addingBot = false

    var body: some View {
        VStack(spacing: 0) {
            VStack(spacing: 14) {
                HStack(spacing: 10) {
                    Text(title).font(.system(size: 22, weight: .bold)).foregroundStyle(Palette.text)
                    Spacer()
                    if mode.wrappedValue == .inbox {
                        InboxFilterMenu(filter: $filter, items: model.inbox.items, reordering: $reordering)
                    }
                    if mode.wrappedValue == .bots {
                        Button("New Bot", systemImage: "plus") { addingBot = true }
                            .labelStyle(.iconOnly)
                            .buttonStyle(RaisedIconStyle())
                            .disabled(model.settings.settings == nil || model.settings.readOnly)
                            .accessibilityIdentifier("bots.new")
                    } else {
                        NewTaskButton(id: "home.newtask")
                            .labelStyle(.iconOnly)
                            .buttonStyle(RaisedIconStyle())
                    }
                }
                SegmentBar(segments: segments, selection: mode, ground: Palette.ground)
            }
            .padding(.horizontal, 16)
            .padding(.top, 20)
            .padding(.bottom, 12)
            switch mode.wrappedValue {
            case .tasks: tasks
            case .bots: bots
            case .hosts: settings
            default: inbox
            }
            footer
        }
        .background(Palette.surface)
        .sheet(isPresented: $addingBot) { NewBotForm() }
    }

    private var title: String {
        switch mode.wrappedValue {
        case .tasks: "Tasks"
        case .bots: "Bots"
        case .hosts: "Settings"
        default: "Inbox"
        }
    }

    private var segments: [Segment<NextNavigation.Tab>] {
        let count = model.inbox.items.filter { !$0.isLater(now: now) }.count
        return [
            Segment(value: .inbox, title: count > 0 ? "Inbox \(count)" : "Inbox", id: "mode.home"),
            Segment(value: .tasks, title: "Tasks", id: "mode.focus"),
            Segment(value: .bots, title: "Bots", id: "home.bots"),
        ]
    }

    // MARK: Inbox

    private var inbox: some View {
        let visible = filter.apply(model.inbox.items, now: now)
        return List {
            Group {
                NeedsYouBanner(open: nav.open) { nav.focus = true }
                CaptureField()
            }
            .cardRow(vertical: 4)
            ForEach(visible) { item in
                Button { selectedItem = item.id } label: {
                    ItemCard(item: item, now: now, picked: item.id == selectedItem)
                }
                .buttonStyle(.plain)
                .cardRow()
                .swipeActions(edge: .trailing) { ItemSwipeActions(item: item) }
            }
            .onMove(perform: filter.isActive ? nil : { model.move(in: visible, from: $0, to: $1) })
            if visible.isEmpty {
                Text(model.inbox.items.isEmpty ? "Nothing to do." : "Nothing matches the filter.")
                    .foregroundStyle(Palette.textSecondary)
                    .cardRow(vertical: 12, horizontal: 18)
            }
            if filter.showDone, !model.inbox.done.isEmpty {
                Section {
                    ForEach(model.inbox.done) { item in
                        Button { selectedItem = item.id } label: {
                            ItemCard(item: item, now: now, picked: item.id == selectedItem)
                        }
                        .buttonStyle(.plain)
                        .cardRow()
                    }
                } header: {
                    GroupHeader(title: "Done", color: TaskState.done.color)
                }
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .environment(\.editMode, .constant(reordering && !filter.isActive ? .active : .inactive))
        .accessibilityIdentifier("inbox.list")
        .background { ReorderKeys(selected: selectedItem) }
    }

    // MARK: Tasks

    private var tasks: some View {
        ScrollView {
            VStack(spacing: 16) {
                ForEach(model.tasksByState, id: \.state) { group in
                    taskGroup(group.state.title, color: group.state.color, group.tasks)
                }
                if !model.focusOthers.isEmpty { others }
                if model.focusTasks.isEmpty {
                    Text("No Tasks run. Start one with New Task.").foregroundStyle(Palette.textSecondary)
                }
            }
            .padding(.horizontal, 12)
            .padding(.top, 4)
            .padding(.bottom, 16)
        }
    }

    private func taskGroup(_ title: String, color: Color, _ list: [Terminal]) -> some View {
        VStack(spacing: 4) {
            GroupHeader(title: title, color: color, count: list.count).padding(.bottom, 4)
            ForEach(list) { t in
                SidebarTaskRow(terminal: t, color: color, picked: nav.terminal == t.id) { nav.terminal = t.id }
            }
        }
    }

    /// Live terminals that are not Tasks, closed until the user opens them.
    private var others: some View {
        let list = model.focusOthers
        return VStack(spacing: 4) {
            Button { showOthers.toggle() } label: {
                HStack(spacing: 6) {
                    GroupHeader(title: OtherTerminals.title, count: list.count)
                    Image(systemName: showOthers ? "chevron.up" : "chevron.down")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundStyle(Palette.textTertiary)
                        .padding(.trailing, 6)
                }
                .frame(minHeight: minTouch)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .otherTerminalsToggle(open: showOthers)
            if showOthers {
                ForEach(list) { t in
                    SidebarTaskRow(terminal: t, color: Palette.textTertiary, picked: nav.terminal == t.id) {
                        nav.terminal = t.id
                    }
                }
            }
        }
    }

    // MARK: Bots

    private var bots: some View {
        BotsList(picked: selectedBot, showsChevrons: false) { dest in
            selectedBot = dest
            botPath = []
        }
    }

    // MARK: Settings

    private var settings: some View {
        SettingsIndex(picked: selectedSetting) { selectedSetting = $0 }
    }

    private var footer: some View {
        HStack {
            HostMenu()
            Spacer()
            Button("Settings", systemImage: "gearshape") { mode.wrappedValue = .hosts }
                .labelStyle(.iconOnly)
                .buttonStyle(BareIconStyle(color: mode.wrappedValue == .hosts ? Palette.text : Palette.textTertiary))
                .accessibilityIdentifier("home.settings")
        }
        .padding(.horizontal, 16)
        .overlay(alignment: .top) { Rectangle().fill(Palette.border).frame(height: 1) }
        .overlay(alignment: .bottom) { LinkBanner() }
    }
}

/// A Task in the sidebar: its dot, where it runs, and its title.
private struct SidebarTaskRow: View {
    let terminal: Terminal
    let color: Color
    let picked: Bool
    var open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(alignment: .top, spacing: 10) {
                StateDot(color: color).padding(.top, 5)
                VStack(alignment: .leading, spacing: 3) {
                    Text(terminal.pathText).font(.system(size: 12)).foregroundStyle(Palette.textSecondary).lineLimit(1)
                    Text(terminal.title).font(.system(size: 14)).foregroundStyle(Palette.text).lineLimit(2)
                }
                Spacer(minLength: 0)
            }
            .padding(10)
            .frame(minHeight: minTouch)
            .background(picked ? Palette.rowSelected : .clear, in: RoundedRectangle(cornerRadius: 10))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("terminal.\(terminal.id)")
    }
}
