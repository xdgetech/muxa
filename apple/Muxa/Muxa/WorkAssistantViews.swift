import MuxaProtocol
import MuxaSession
import SwiftUI

/// A request on a Work item (ADR 0048 §6): what was asked, its delivery with its age,
/// Cancel while the run may still deliver, and Open Session while the run's session lives.
struct RequestSection: View {
    @Environment(AppModel.self) private var model
    let request: AssistantRequest
    @State private var cancelling = false
    @State private var error: String?

    var body: some View {
        Section {
            LabeledContent("Asked", value: "\(request.scope.shown.capitalized) · version \(request.version)")
                .accessibilityIdentifier("work-request-\(request.id)")
            LabeledContent("Delivery", value: "\(request.delivery.shown) · \(request.deliveryAt.shownAge)")
                .accessibilityIdentifier("work-delivery")
            if let purpose = request.purpose {
                Text(purpose).textSelection(.enabled).foregroundStyle(.secondary)
            }
            if let advice = request.delivery.advice {
                Label(advice, systemImage: Icon.warning).font(.callout).foregroundStyle(Theme.attention)
            }
            if let s = model.runSession(request) {
                Button("Open Session", systemImage: Icon.openSession) {
                    Task { await model.open(s, as: .chat) }
                }
                .accessibilityIdentifier("work-open-session")
            }
            if request.delivery.isActive {
                Button("Cancel Request", systemImage: Icon.cancel, role: .destructive, action: cancel)
                    .disabled(cancelling)
                    .accessibilityIdentifier("work-cancel")
            }
            if let error {
                Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure)
                    .accessibilityIdentifier("work-request-error")
            }
        } header: {
            Text("Assistant · \(request.workspace)")
        }
    }

    private func cancel() {
        cancelling = true
        Task {
            defer { cancelling = false }
            error = await model.cancelRequest(request)
        }
    }
}

/// The request's current Result, then the older versions' collapsed under Earlier.
struct ResultSections: View {
    let request: AssistantRequest

    var body: some View {
        if let result = request.currentResult {
            ResultBody(request: request, result: result, header: "Result")
        }
        EarlierResults(request: request)
    }
}

/// Results of older versions, newest first, collapsed.
struct EarlierResults: View {
    let request: AssistantRequest

    var body: some View {
        let earlier = request.earlierResults
        if !earlier.isEmpty {
            Section {
                DisclosureGroup("Earlier (\(earlier.count))") {
                    ForEach(Array(earlier.enumerated()), id: \.offset) { _, old in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(old.summary).textSelection(.enabled)
                            Text([old.worker, "Version \(old.version) · \(old.at.shownDate)"].compactMap { $0 }
                                .joined(separator: " · "))
                                .font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
                .accessibilityIdentifier("work-result-earlier")
            }
        }
    }
}

/// One Result: its summary, proposed notes, findings, questions, next steps, files and
/// possible duplicates. Files are paths on the host: the Reader needs a pane and cannot
/// open them yet. `tag` keeps identifiers apart when a chain shows one per step.
struct ResultBody: View {
    @Environment(AppModel.self) private var model
    let request: AssistantRequest
    let result: AssistResult
    let header: String
    var tag: String?

    var body: some View {
        Section(header) {
            Text(result.summary).textSelection(.enabled)
                .accessibilityIdentifier(id("work-result-summary"))
            Text("Version \(result.version) · \(result.at.shownDate)").font(.caption).foregroundStyle(.secondary)
        }
        if model.usesPlans, let notes = result.notes {
            NotesSection(request: request, result: result, notes: notes, tag: tag)
        }
        if !result.findings.isEmpty {
            Section("Findings") {
                ForEach(Array(result.findings.enumerated()), id: \.offset) { _, finding in
                    FindingRow(finding: finding)
                }
            }
        }
        lines("Open Questions", result.openQuestions, id: id("work-result-question"))
        lines("Next", result.next, id: id("work-result-next"))
        if !result.files.isEmpty {
            Section {
                ForEach(result.files, id: \.self) { PathText(path: $0) }
            } header: {
                Text("Files")
            } footer: {
                Text("Paths on the host. The Reader cannot open them from here yet.")
            }
        }
        duplicates(result.possibleDuplicates)
    }

    private func id(_ base: String) -> String { tag.map { "\(base)-\($0)" } ?? base }

    @ViewBuilder
    private func lines(_ title: String, _ texts: [String], id: String) -> some View {
        if !texts.isEmpty {
            Section(title) {
                ForEach(Array(texts.enumerated()), id: \.offset) { i, text in
                    Text(text).textSelection(.enabled).accessibilityIdentifier("\(id)-\(i)")
                }
            }
        }
    }

    /// Other Work items the assistant thinks are the same; a listed one opens.
    @ViewBuilder
    private func duplicates(_ found: [PossibleDuplicate]) -> some View {
        if !found.isEmpty {
            Section("Possible Duplicates") {
                ForEach(found, id: \.work) { d in
                    let other = model.workItem(d.work)
                    VStack(alignment: .leading, spacing: 2) {
                        if other != nil {
                            Button(other?.title ?? d.work) { model.select(.work(d.work)) }
                        } else {
                            Text(d.work).font(.body.monospaced())
                        }
                        Text(d.reason).font(.caption).foregroundStyle(.secondary)
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier(id("work-duplicate-\(d.work)"))
                }
            }
        }
    }
}

/// A finding and its sources: a URL opens in the browser, a path stays text.
private struct FindingRow: View {
    let finding: Finding

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(finding.text).textSelection(.enabled)
            ForEach(Array(finding.sources.enumerated()), id: \.offset) { _, source in
                if let link = source.webURL {
                    Link(source.title ?? link.absoluteString, destination: link).font(.caption)
                }
                if let path = source.path { PathText(path: path) }
            }
        }
    }
}

private struct PathText: View {
    let path: String

    var body: some View {
        Text(path).font(.caption.monospaced()).foregroundStyle(.secondary).textSelection(.enabled)
    }
}

/// Ask Assistant on a Work item with no request running; Ask Again sends the item's
/// current `rev`, so the host adds a version.
struct AskSection: View {
    @Environment(AppModel.self) private var model
    let item: WorkItem
    let again: Bool
    @State private var scope: AssistScope = .research
    @State private var purpose = ""
    /// For an item with no workspace: the host needs one.
    @State private var workspace: String?
    /// A staff member (level 12); nil lets the lead decide.
    @State private var worker: String?
    @State private var asking = false
    @State private var error: String?

    var body: some View {
        let staff = model.pickableStaff(in: item.workspace ?? workspace)
        if !staff.isEmpty {
            StaffPicker(staff: staff, selection: $worker, id: "work-ask-staff")
        }
        WriterSection(workspace: item.workspace ?? workspace)
        WorkspaceMemorySection(workspace: item.workspace ?? workspace)
        Section {
            Picker("Scope", selection: $scope) {
                ForEach(AssistScope.choices, id: \.self) { Text($0.shown.capitalized).tag($0) }
            }
            .pickerStyle(.segmented)
            .accessibilityIdentifier("work-ask-scope")
            TextField("What should it look at? (optional)", text: $purpose, axis: .vertical)
                .lineLimit(1...4)
                .accessibilityIdentifier("work-ask-purpose")
            if item.workspace == nil {
                Picker("Workspace", selection: $workspace) {
                    Text("Pick one").tag(String?.none)
                    ForEach(model.session.pickableWorkspaces) { w in Text(w.name).tag(String?.some(w.name)) }
                }
            }
            Button(again ? "Ask Again" : "Ask Assistant", systemImage: Icon.assistant, action: ask)
                .disabled(asking || !model.isConnected || (item.workspace == nil && workspace == nil)
                          || purpose.utf8.count > CaptureDraft.maxPurposeBytes)
                .accessibilityIdentifier("work-ask")
            if let error {
                Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure)
                    .accessibilityIdentifier("work-ask-error")
            }
        } header: {
            Text("Ask the Assistant")
        } footer: {
            Text("The workspace's agent works on it on the host and returns a Result here.")
        }
        .task { if model.session.workspaces.isEmpty, item.workspace == nil { await model.session.loadTasks() } }
        .onChange(of: workspace) { worker = nil }
    }

    private func ask() {
        asking = true
        error = nil
        Task {
            defer { asking = false }
            error = await model.askAssistant(item, scope: scope, purpose: purpose, workspace: workspace,
                                             worker: model.pickableStaff(in: item.workspace ?? workspace).isEmpty ? nil : worker)
        }
    }
}

extension ResultSource {
    /// An `http` or `https` URL; anything else is not opened.
    var webURL: URL? {
        guard let url, let parsed = URL(string: url), ["http", "https"].contains(parsed.scheme?.lowercased()) else { return nil }
        return parsed
    }
}

extension UInt64 {
    /// Epoch seconds as an age: "2 minutes ago".
    var shownAge: String {
        Date(timeIntervalSince1970: TimeInterval(self)).formatted(.relative(presentation: .named))
    }
}
