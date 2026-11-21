import MuxaV2
import SwiftUI

/// Focus (ADR 0066 decision 1): one Task's terminal, with Needs you and the
/// Tasks on the left. The Inbox is a panel, closed by default.
struct FocusView: View {
    @Environment(NextModel.self) private var model
    @Binding var selected: String?
    var showHome: () -> Void

    @Environment(NextNavigation.self) private var nav

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
                    TerminalPane(terminal: t)
                        .id(t.id)
                        .ignoresSafeArea(.keyboard)
                        .navigationTitle(t.branch ?? t.name ?? t.id)
                        .inlineTitle()
                        .toolbar {
                            Button("Inbox", systemImage: "tray.full") { nav.showInbox.toggle() }
                                .accessibilityIdentifier("focus.inbox")
                        }
                        .inspector(isPresented: Bindable(nav).showInbox) { InboxPanel() }
                } else {
                    ContentUnavailableView("Select a Task", systemImage: "terminal")
                }
            }
        }
        .navigationSplitViewStyle(.balanced)
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
