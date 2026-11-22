import MuxaV2
import SwiftUI

/// Focus (ADR 0066 decision 1): one Task's terminal, with Needs you and the
/// Tasks on the left. The Inbox is a panel, closed by default.
struct FocusView: View {
    @Environment(NextModel.self) private var model
    @Binding var selected: String?
    var showHome: () -> Void

    @Environment(NextNavigation.self) private var nav
    /// The Task whose loop sheet is open.
    @State private var loopFor: MuxaV2.Terminal?

    var body: some View {
        NavigationSplitView(columnVisibility: Bindable(nav).sidebar) {
            List(selection: $selected) {
                if !model.needsYou.isEmpty {
                    Section("Needs you") { rows(model.needsYou) }
                }
                Section("Tasks") { rows(tasks) }
                if !others.isEmpty {
                    Section("Terminals") { rows(others) }
                }
            }
            .navigationTitle("Focus")
            .toolbar {
                ToolbarItem(placement: .leadingBar) {
                    Button("Home", systemImage: "tray", action: showHome).accessibilityIdentifier("mode.home")
                }
            }
        } detail: {
            Group {
                if let t = model.terminals.first(where: { $0.id == selected }) {
                    panes(t)
                        .navigationTitle(t.branch ?? t.name ?? t.id)
                        .inlineTitle()
                        .toolbar {
                            Button(loopTitle(t), systemImage: "arrow.triangle.2.circlepath") { loopFor = t }
                                .disabled(!t.isTaskAgent)
                                .accessibilityIdentifier("focus.loop")
                            Button("Reader", systemImage: "doc.text.magnifyingglass") { nav.showReader.toggle() }
                                .touchShortcut("r")
                                .disabled(t.taskDir == nil)
                                .accessibilityIdentifier("focus.reader")
                            Button("Inbox", systemImage: "tray.full") { nav.showInbox.toggle() }
                                .accessibilityIdentifier("focus.inbox")
                        }
                        .inspector(isPresented: panel) {
                            Group {
                                if nav.showReader { ReaderPanel(terminal: t).id(t.id) } else { InboxPanel() }
                            }
                            .inspectorColumnWidth(min: 300, ideal: 460, max: 900)
                        }
                } else {
                    ContentUnavailableView("Select a Task", systemImage: "terminal")
                }
            }
        }
        .navigationSplitViewStyle(.balanced)
        .sheet(item: $loopFor) { t in LoopSheet(terminal: t) }
    }

    private func loopTitle(_ t: MuxaV2.Terminal) -> String {
        guard let lp = model.loops[t.id], lp.isActive else { return "Loop" }
        return "Loop \(lp.attempts)/\(lp.maxAttempts)"
    }

    /// The panel beside the terminal: the Inbox or the Reader.
    private var panel: Binding<Bool> {
        Binding(
            get: { nav.showInbox || nav.showReader },
            set: { shown in
                if !shown {
                    nav.showInbox = false
                    nav.showReader = false
                }
            })
    }

    @ViewBuilder
    private func panes(_ t: MuxaV2.Terminal) -> some View {
        #if os(macOS)
        PaneSplit()
        #else
        TerminalPane(terminal: t)
            .id(t.id)
            .ignoresSafeArea(.keyboard)
        #endif
    }

    private var tasks: [Terminal] {
        model.terminals.filter { $0.live && $0.isTaskAgent && !model.needsYou.contains($0) }
    }

    private var others: [Terminal] {
        model.terminals.filter { $0.live && !$0.isTaskAgent }
    }

    private func rows(_ list: [Terminal]) -> some View {
        ForEach(list) { t in
            VStack(alignment: .leading, spacing: 2) {
                HStack {
                    Text(t.branch ?? t.name ?? t.id)
                    Spacer()
                    if let state = t.status?.state {
                        Text(state).font(.caption).foregroundStyle(state == "blocked" ? .orange : .secondary)
                    }
                }
                if let dir = t.taskDir { Text(dir).font(.caption2).foregroundStyle(.secondary).lineLimit(1) }
                if let lp = model.loops[t.id] {
                    Text(LoopText.badge(lp)).font(.caption2).foregroundStyle(lp.reason == "limit" ? .orange : .secondary)
                }
            }
            .tag(t.id)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("terminal.\(t.id)")
        }
    }
}

/// The Inbox beside a terminal: the open items, Done and Later by swipe.
struct InboxPanel: View {
    @Environment(NextModel.self) private var model

    var body: some View {
        let now = Int64(Date().timeIntervalSince1970)
        List {
            CaptureField()
            ForEach(model.inbox.items.filter { !$0.isLater(now: now) }) { item in
                ItemRow(item: item, now: now)
                    .swipeActions {
                        Button("Done") { Task { await model.inbox(.inboxDone(item: item.id)) } }.tint(.green)
                        Button("Later") {
                            Task { await model.inbox(.inboxLater(item: item.id, until: LaterChoice.tomorrow.until())) }
                        }
                        .tint(.orange)
                    }
            }
        }
        .navigationTitle("Inbox")
    }
}
