import MuxaV2
import SwiftUI

/// The Routines of the Workspace's `.muxa/routines.toml`, also while the
/// Routines gate is off.
struct WorkspaceRoutinesForm: View {
    let workspace: SettingsWorkspace
    @State private var editing: RoutineDraft?

    var body: some View {
        SettingsPage { sections }
            .sheet(item: $editing) { RoutineEditor(workspace: workspace.name, draft: $0) }
    }

    @ViewBuilder
    private var sections: some View {
        if let problem = workspace.routinesProblem {
            Section {
                Text("routines.toml cannot be used as it is: \(problem)")
                    .font(.callout)
                    .foregroundStyle(Theme.failure)
            }
        }
        Section {
            ForEach(workspace.routines) { r in
                Button { editing = RoutineDraft(r) } label: {
                    VStack(alignment: .leading, spacing: 3) {
                        HStack {
                            Text(r.name).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                            Spacer()
                            if !r.spec.enabled { StateTag(text: "off") }
                        }
                        Text(Self.when(r.spec)).font(.caption).foregroundStyle(Palette.textSecondary)
                    }
                    .padding(.vertical, 2)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("settings.routine.\(r.name)")
            }
            Button("Add Routine", systemImage: "plus") { editing = RoutineDraft() }
                .accessibilityIdentifier("routine.add")
        } header: {
            Text("Routines")
        } footer: {
            if !workspace.routinesEnabled { Text("Routines are not allowed in this Workspace: none runs. Allow them in General.") }
        }
    }

    static func when(_ r: RoutineSpec) -> String {
        let zone = r.timezone.map { " · \($0)" } ?? ""
        if let at = r.at { return "Daily at \(at)\(zone)" }
        if let cron = r.schedule { return "\(cron)\(zone)" }
        return "Only on Run now"
    }
}

struct RoutineDraft: Identifiable {
    enum Timing: String, CaseIterable, Identifiable {
        case manual = "Run now only", daily = "Daily at", cron = "Cron"
        var id: Self { self }
    }

    var existing: RoutineSpec?
    var name = ""
    var timing = Timing.daily
    var at = "09:00"
    var cron = "0 8 * * 1-5"
    var timezone = TimeZone.current.identifier
    var worker = ""
    var file = ""
    var text = ""
    /// The file's content as read; nil when it could not be read.
    var readText: String?
    var enabled = true
    var items = ""
    var id: String { existing?.name ?? "+" }

    init() {}

    init(_ r: SettingsRoutine) {
        let s = r.spec
        existing = s
        name = s.name
        timing = s.at != nil ? .daily : s.schedule != nil ? .cron : .manual
        at = s.at ?? at
        cron = s.schedule ?? cron
        timezone = s.timezone ?? timezone
        worker = s.worker ?? ""
        file = s.instructions
        text = r.instructionsText ?? ""
        readText = r.instructionsText
        enabled = s.enabled
        items = s.items.map(String.init) ?? ""
    }

    func spec() -> RoutineSpec? {
        guard let name = Lines.optional(name) else { return nil }
        let file = Lines.optional(file) ?? "routines/\(name).md"
        var s = existing ?? RoutineSpec(name: name, instructions: file)
        s.name = name
        s.instructions = file
        s.at = timing == .daily ? Lines.optional(at) : nil
        s.schedule = timing == .cron ? Lines.optional(cron) : nil
        s.timezone = timing == .manual ? nil : Lines.optional(timezone)
        s.worker = Lines.optional(worker)
        s.enabled = enabled
        s.items = UInt32(Lines.trimmed(items))
        return s
    }
}

/// Adds or changes a Routine: when it runs, who runs it, and its instructions.
struct RoutineEditor: View {
    @Environment(NextModel.self) private var model
    let workspace: String
    @State private var draft: RoutineDraft

    init(workspace: String, draft: RoutineDraft) {
        self.workspace = workspace
        _draft = State(initialValue: draft)
    }

    var body: some View {
        SettingsEditor(
            title: draft.existing?.name ?? "New Routine", ops: ops,
            remove: draft.existing.map {
                (.routineRemove(workspace: workspace, name: $0.name), "Its instruction file stays in .muxa/.")
            }
        ) {
            Section {
                if draft.existing == nil {
                    TextField("Name, e.g. morning-brief", text: $draft.name)
                        .plainEntry()
                        .accessibilityIdentifier("routine.name")
                }
                Toggle("Enabled", isOn: $draft.enabled)
                    .accessibilityIdentifier("routine.enabled")
            }
            schedule
            Section {
                Picker("Worker", selection: $draft.worker) {
                    Text("The Workspace's agent").tag("")
                    ForEach(model.settings.settings?.workers ?? []) { Text($0.name).tag($0.name) }
                }
                .accessibilityIdentifier("routine.worker")
                TextField("Items per run, 1 to 50 (default 10)", text: $draft.items)
                    .numberEntry()
                    .accessibilityIdentifier("routine.items")
            }
            instructions
        }
    }

    private var schedule: some View {
        Section {
            Picker("Runs", selection: $draft.timing) {
                ForEach(RoutineDraft.Timing.allCases) { Text($0.rawValue).tag($0) }
            }
            .accessibilityIdentifier("routine.timing")
            switch draft.timing {
            case .manual: EmptyView()
            case .daily:
                TextField("HH:MM", text: $draft.at)
                    .monospaced()
                    .plainEntry()
                    .accessibilityIdentifier("routine.at")
            case .cron:
                TextField("minute hour day month weekday", text: $draft.cron)
                    .monospaced()
                    .plainEntry()
                    .accessibilityIdentifier("routine.cron")
            }
            if draft.timing != .manual {
                TextField("Time zone, e.g. Europe/Berlin", text: $draft.timezone)
                    .plainEntry()
                    .accessibilityIdentifier("routine.timezone")
            }
        } header: {
            Text("Schedule")
        } footer: {
            if draft.timing == .cron { Text("One minute and one hour: at most one run a day. Sunday is 0 or 7.") }
        }
    }

    private var instructions: some View {
        Section {
            TextField("routines/\(Lines.optional(draft.name) ?? "name").md", text: $draft.file)
                .monospaced()
                .plainEntry()
                .accessibilityIdentifier("routine.file")
            TextEditor(text: $draft.text)
                .font(.system(.callout, design: .monospaced))
                .frame(minHeight: 180)
                .scrollContentBackground(.hidden)
                .accessibilityIdentifier("routine.instructions")
        } header: {
            Text("Instructions")
        } footer: {
            Text("A Markdown file in .muxa/, read on every run. At most 12 KiB.")
        }
    }

    private func ops() -> [SettingsOp]? {
        guard let spec = draft.spec() else { return nil }
        // An unchanged text is not sent, so a file the host could not read stays as it is.
        let text = draft.existing != nil && draft.text == (draft.readText ?? "") ? nil : draft.text
        return draft.existing == nil
            ? [.routineAdd(workspace: workspace, routine: spec, instructionsText: text)]
            : [.routineUpdate(workspace: workspace, routine: spec, instructionsText: text)]
    }
}
