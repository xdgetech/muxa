import MuxaProtocol
import MuxaSession
import SwiftUI

/// A Work item (ADR 0046): Capture, then the assistant's request and Result, the
/// Tasks, and the decision it waits on; then what the user can do. Only the user
/// completes or withdraws it; a Task or a Result never does.
struct WorkDetail: View {
    @Environment(AppModel.self) private var model
    let id: String
    /// Why `get-work` failed: the texts this device has seen show instead.
    @State private var readError: String?

    var body: some View {
        if let item = model.workItem(id) ?? model.session.workDetails[id]?.entry {
            let requests = model.session.requests(of: item)
            List {
                header(item)
                captures(item)
                ForEach(requests) { request in
                    RequestSection(request: request)
                    ResultSections(request: request)
                }
                tasks(item)
                if item.state == .waitingDecision {
                    Section {
                        Label("A Task waits on your decision in Decisions.", systemImage: Icon.waiting)
                            .foregroundStyle(Theme.attention)
                    }
                }
                if item.isOpen, !(requests.last.map { $0.delivery.isActive || $0.delivery == .cancelRequested } ?? false) {
                    AskSection(item: item, again: !requests.isEmpty)
                }
                actions(item)
            }
            .accessibilityIdentifier("work-detail")
            .navigationTitle("Work")
            .navigationBarTitleDisplayMode(.inline)
            // A new request raises the rev: read the item again for its Captures and requests.
            .task(id: item.rev) { readError = await model.readWork(id) }
            .onChange(of: requests.map { $0.currentResult?.version }, initial: true) { model.markResultsSeen(item) }
        } else {
            ContentUnavailableView("Not in the Inbox", systemImage: Icon.work,
                                   description: Text("This Work item is no longer listed."))
        }
    }

    @ViewBuilder
    private func header(_ item: WorkItem) -> some View {
        Section {
            Text(item.title).font(.title3.weight(.semibold))
                .accessibilityIdentifier("work-title")
            LabeledContent("State", value: item.state.shown)
                .accessibilityIdentifier("work-state")
            if let workspace = item.workspace { LabeledContent("Workspace", value: workspace) }
            if item.laterAt != nil {
                Label("Moved down with Later", systemImage: Icon.later).foregroundStyle(.secondary)
            }
        }
    }

    /// Every Capture the item links, as `get-work` read them. Until then, or when the
    /// host could not read it, the texts this device has seen and a count of the rest.
    @ViewBuilder
    private func captures(_ item: WorkItem) -> some View {
        if !item.captures.isEmpty {
            let read = model.session.workDetails[item.id]?.captures
            let known = read ?? item.captures.compactMap { model.session.knownCaptures[$0] }
            Section("Captured") {
                ForEach(known) { c in
                    Text(c.text).textSelection(.enabled)
                        .accessibilityIdentifier("work-capture-\(c.id)")
                }
                if read == nil, known.count < item.captures.count {
                    Text(item.captures.count - known.count == 1 ? "1 more capture" : "\(item.captures.count - known.count) more captures")
                        .foregroundStyle(.secondary)
                }
                if let readError {
                    Label(readError, systemImage: Icon.warning).font(.caption).foregroundStyle(.secondary)
                }
            }
        }
    }

    @ViewBuilder
    private func tasks(_ item: WorkItem) -> some View {
        if !item.tasks.isEmpty {
            Section("Tasks") {
                ForEach(item.tasks, id: \.self) { task in
                    let record = model.session.taskRecords.first { $0.id == task }
                    Button(record?.title ?? task) { model.select(.task(task)) }
                        .accessibilityIdentifier("work-task-\(task)")
                }
            }
        }
    }

    @ViewBuilder
    private func actions(_ item: WorkItem) -> some View {
        Section {
            if item.isOpen {
                if model.canKeepTasks {
                    Button("Start a Task", systemImage: Icon.newTask) { Task { await model.startTask(for: item) } }
                        .accessibilityIdentifier("work-start-task")
                }
                Button("Complete", systemImage: Icon.done) { model.change(item, .complete) }
                    .accessibilityIdentifier("work-complete")
                Button("Later", systemImage: Icon.later) { model.change(item, .later) }
                    .accessibilityIdentifier("work-later")
                Button("Withdraw", systemImage: Icon.withdraw) { model.change(item, .withdraw) }
                    .accessibilityIdentifier("work-withdraw")
            } else {
                Button("Reopen", systemImage: Icon.reopen) { model.change(item, .reopen) }
                    .accessibilityIdentifier("work-reopen")
            }
        } footer: {
            Text("Complete and Withdraw leave its Tasks and requests as they are.")
        }
    }
}

/// An unsorted Capture: the whole text, and where it goes.
struct CaptureDetail: View {
    @Environment(AppModel.self) private var model
    let id: String

    var body: some View {
        if let capture = model.listedCapture(id) {
            List {
                Section {
                    Text(capture.text).textSelection(.enabled).accessibilityIdentifier("capture-full-text")
                    Text([capture.createdAt.shownDate, capture.workspace].compactMap { $0 }.joined(separator: " · "))
                        .font(.caption).foregroundStyle(.secondary)
                }
                Section {
                    Button("Make a Work Item", systemImage: Icon.work) { model.sort(capture, .work) }
                        .accessibilityIdentifier("capture-make-work")
                    let open = model.session.openWork
                    if !open.isEmpty {
                        Menu {
                            ForEach(open) { item in
                                Button(item.title) { model.sort(capture, .link, into: item) }
                            }
                        } label: {
                            Label("Add to a Work Item", systemImage: "link")
                        }
                        .accessibilityIdentifier("capture-link")
                    }
                    Button("Keep as Note", systemImage: Icon.note) { model.sort(capture, .note) }
                        .accessibilityIdentifier("capture-keep-note")
                } footer: {
                    Text("A note stays on the host without a Work item. Nothing starts an agent.")
                }
            }
            .navigationTitle("Capture")
            .navigationBarTitleDisplayMode(.inline)
        } else {
            ContentUnavailableView("Sorted", systemImage: Icon.capture,
                                   description: Text("This capture is no longer in Unsorted."))
        }
    }
}
