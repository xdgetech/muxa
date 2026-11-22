import MuxaV2
import SwiftUI

/// The Mac's Focus (ADR 0066 decision 1, ADR 0070): the Tasks' panes, with
/// Needs you and the Tasks on the left. The Inbox is a panel, closed by default.
struct FocusView: View {
    @Environment(NextModel.self) private var model
    @Binding var selected: String?
    var showHome: () -> Void

    @Environment(NextNavigation.self) private var nav
    /// The Task whose loop sheet is open.
    @State private var loopFor: MuxaV2.Terminal?
    /// The other terminals start closed: Tasks are what Focus is for.
    @State private var showOthers = false

    var body: some View {
        NavigationSplitView(columnVisibility: Bindable(nav).sidebar) {
            List(selection: $selected) {
                if !model.needsYou.isEmpty {
                    Section("Needs you") { rows(model.needsYou) }
                }
                Section("Tasks") { rows(tasks) }
                if !model.otherTerminals.isEmpty {
                    Section(isExpanded: $showOthers) {
                        rows(model.otherTerminals)
                    } header: {
                        Button(OtherTerminals.title) { showOthers.toggle() }
                            .buttonStyle(.plain)
                            .otherTerminalsToggle(open: showOthers)
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background(Palette.surface)
            .navigationTitle("Focus")
            .toolbar {
                ToolbarItem(placement: .leadingBar) {
                    Button("Home", systemImage: "tray", action: showHome).accessibilityIdentifier("mode.home")
                }
                ToolbarItem(placement: .primaryAction) {
                    NewTaskButton(id: "focus.newtask")
                }
            }
        } detail: {
            Group {
                if let t = model.terminals.first(where: { $0.id == selected }) {
                    PaneSplit()
                        .navigationTitle(t.title)
                        .toolbar {
                            Button(LoopText.title(model.loops[t.id]), systemImage: "arrow.triangle.2.circlepath") {
                                loopFor = t
                            }
                            .disabled(!t.isTaskAgent)
                            .accessibilityIdentifier("focus.loop")
                            Button("Chat", systemImage: "bubble.left.and.text.bubble.right") { nav.showChat.toggle() }
                                .disabled(!t.isTaskAgent)
                                .accessibilityIdentifier("focus.chat")
                            Button("Reader", systemImage: "doc.text.magnifyingglass") { nav.showReader.toggle() }
                                .disabled(t.taskDir == nil)
                                .accessibilityIdentifier("focus.reader")
                            Button("Inbox", systemImage: "tray.full") { nav.showInbox.toggle() }
                                .accessibilityIdentifier("focus.inbox")
                        }
                        .inspector(isPresented: panel) {
                            Group {
                                if nav.showReader {
                                    ReaderPanel(terminal: t).id(t.id)
                                } else if nav.showChat {
                                    ChatPanel(terminal: t).id(t.id)
                                } else {
                                    InboxPanel()
                                }
                            }
                            .inspectorColumnWidth(min: 300, ideal: 460, max: 900)
                        }
                } else {
                    ContentUnavailableView("Select a Task", systemImage: "terminal")
                }
            }
            .background(Palette.ground)
        }
        .navigationSplitViewStyle(.balanced)
        .sheet(item: $loopFor) { t in LoopSheet(terminal: t) }
    }

    /// The panel beside the terminal: the Inbox, the Reader or Chat.
    private var panel: Binding<Bool> {
        Binding(
            get: { nav.showInbox || nav.showReader || nav.showChat },
            set: { shown in
                if !shown {
                    nav.showInbox = false
                    nav.showReader = false
                    nav.showChat = false
                }
            })
    }

    private var tasks: [Terminal] {
        model.liveTasks.filter { !model.needsYou.contains($0) }
    }

    private func rows(_ list: [Terminal]) -> some View {
        ForEach(list) { t in
            VStack(alignment: .leading, spacing: 2) {
                HStack {
                    StateDot(color: t.state.color, size: 8)
                    Text(t.title)
                    Spacer()
                    if let state = t.status?.state {
                        Text(state).font(.caption).foregroundStyle(t.state.color)
                    }
                }
                if let dir = t.taskDir {
                    Text(dir).font(.caption2).foregroundStyle(Palette.textTertiary).lineLimit(1)
                }
                if let lp = model.loops[t.id] {
                    Text(LoopText.badge(lp)).font(.caption2)
                        .foregroundStyle(lp.reason == "limit" ? Theme.attention : Palette.textTertiary)
                }
            }
            .tag(t.id)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("terminal.\(t.id)")
        }
    }
}
