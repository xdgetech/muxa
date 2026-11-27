import MuxaV2
import SwiftUI

/// "Tasks and talks": Task, talk, Routine and member groups as the host
/// sorted them, five until the user asks for all. Open shows the Task.
struct HostTaskList: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    #if os(macOS)
    @Environment(\.openWindow) private var openWindow
    #endif
    let report: HostReport
    @State private var showAll = false

    private static let shown = 5

    var body: some View {
        let work = report.groups.filter(\.isAgentWork)
        let total = work.reduce(UInt64(0)) { $0 + $1.memory }
        HostCard {
            Text("Tasks and talks · \(work.count) · \(HostFormat.size(total))")
        } content: {
            ForEach(Array((showAll ? work : Array(work.prefix(Self.shown))).enumerated()), id: \.offset) { _, g in
                row(g)
            }
            if work.count > Self.shown {
                Button(showAll ? "Show fewer" : "Show all \(work.count)") { showAll.toggle() }
                    .buttonStyle(.plain)
                    .font(.system(size: 13))
                    .foregroundStyle(Palette.accent)
                    .padding(.horizontal, 14)
                    .frame(maxWidth: .infinity, minHeight: minTouch, alignment: .leading)
                    .contentShape(Rectangle())
                    .accessibilityIdentifier("host.tasks.all")
            }
            if work.isEmpty {
                Text("No Task or talk runs.").font(.system(size: 13)).foregroundStyle(Palette.textSecondary)
                    .padding(.horizontal, 14).padding(.bottom, 12)
            }
        }
    }

    private func row(_ g: UsageGroup) -> some View {
        let terminal = model.terminals.first { $0.id == g.terminal }
        // The agent's own report of waiting on the user wins over "working".
        let waits = ["blocked", "waiting"].contains(terminal?.status?.state ?? "")
        let state = g.idleSinceMs.map { HostFormat.idle(since: $0, at: report.sampledMs) } ?? (waits ? "needs you" : "working")
        let color = waits ? TaskState.needsYou.color : g.idleSinceMs == nil ? TaskState.working.color : Palette.textSecondary
        let meta = [g.workspace, state, HostFormat.cpu(g.cpu)].compactMap { $0 }.joined(separator: " · ")
        return HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(g.name).font(.system(size: 14)).foregroundStyle(Palette.text).lineLimit(1)
                Text(meta).font(.system(size: 11.5))
                    .foregroundStyle(color)
                    .lineLimit(1)
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("host.task.\(g.terminal ?? g.name)")
            Spacer(minLength: 8)
            Text(HostFormat.size(g.memory)).font(.system(size: 13)).monospacedDigit().foregroundStyle(Palette.text)
            Button("Open") { if let terminal { open(terminal) } }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .disabled(terminal == nil)
                .accessibilityIdentifier("host.open.\(g.terminal ?? g.name)")
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .overlay(alignment: .top) { Rectangle().fill(Palette.cardDivider).frame(height: 1) }
    }

    private func open(_ t: MuxaV2.Terminal) {
        nav.open(t)
        #if os(macOS)
        openWindow(id: "main")
        #endif
    }
}

/// "Elsewhere on the host": simulators, Docker, apps and the rest.
struct HostElsewhereList: View {
    let groups: [UsageGroup]

    var body: some View {
        HostCard {
            Text("Elsewhere on the host")
        } content: {
            ForEach(Array(groups.enumerated()), id: \.offset) { _, g in
                HStack(spacing: 10) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(Self.title(g)).font(.system(size: 14)).foregroundStyle(Palette.text).lineLimit(1)
                        Text(Self.detail(g)).font(.system(size: 11.5)).foregroundStyle(Palette.textSecondary).lineLimit(1)
                    }
                    Spacer(minLength: 8)
                    Text(HostFormat.size(g.memory)).font(.system(size: 13)).monospacedDigit().foregroundStyle(Palette.text)
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 8)
                .overlay(alignment: .top) { Rectangle().fill(Palette.cardDivider).frame(height: 1) }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("host.group.\(g.kind.rawValue)")
            }
        }
    }

    static func title(_ g: UsageGroup) -> String {
        g.kind == .claudeOutside ? "Claude Code outside muxa" : g.name
    }

    static func detail(_ g: UsageGroup) -> String {
        var parts = [g.processes == 1 ? "1 process" : "\(g.processes) processes", "CPU \(HostFormat.cpu(g.cpu))"]
        if g.kind == .simulator { parts.append("Booted by Xcode or a test") }
        return parts.joined(separator: " · ")
    }
}

/// "Disk use of Task places": a scan on request, since sizes need `du`.
struct HostDiskSection: View {
    @Environment(NextModel.self) private var model
    @State private var rows: [DiskRow]?
    @State private var scanning = false

    var body: some View {
        HostCard {
            HStack {
                Text("Disk use of Task places")
                Spacer()
                Button(scanning ? "Scanning…" : rows == nil ? "Scan disk" : "Scan again") { Task { await scan() } }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                    .disabled(scanning || !model.isConnected)
                    .accessibilityIdentifier("host.scan")
            }
        } content: {
            if let rows {
                let summary = HostDiskSummary(rows: rows, terminals: model.terminals)
                ForEach(summary.places) { place($0) }
                ForEach(summary.leftovers) { leftover($0) }
                if rows.isEmpty { note("No Task places or leftovers.") }
                note("Remove leftovers with muxa-cli disk clean.")
            } else {
                note("Sizes need a scan of every worktree, so they show only when asked.")
            }
        }
    }

    private func scan() async {
        scanning = true
        defer { scanning = false }
        if let found = await model.scanDisk() { rows = found }
    }

    private func place(_ p: HostDiskSummary.Place) -> some View {
        line(p.name, size: p.bytes, extra: p.buildBytes.map { "+ \(HostFormat.size($0)) build" }, id: "host.disk.\(p.id)")
    }

    private func leftover(_ l: HostDiskSummary.Leftover) -> some View {
        let cleanable = l.cleanable == 0 ? nil : "\(l.cleanable) cleanable"
        return line("\(l.title) (\(l.count))", size: l.bytes, extra: cleanable, id: "host.leftover.\(l.kind)")
    }

    private func line(_ title: String, size: UInt64, extra: String?, id: String) -> some View {
        HStack(spacing: 10) {
            Text(title).font(.system(size: 14)).foregroundStyle(Palette.text).lineLimit(1)
            Spacer(minLength: 8)
            if let extra { Text(extra).font(.system(size: 12)).foregroundStyle(Palette.textSecondary) }
            Text(HostFormat.size(size)).font(.system(size: 13)).monospacedDigit().foregroundStyle(Palette.text)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .overlay(alignment: .top) { Rectangle().fill(Palette.cardDivider).frame(height: 1) }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(id)
    }

    private func note(_ text: String) -> some View {
        Text(text).font(.system(size: 12)).foregroundStyle(Palette.textSecondary)
            .padding(.horizontal, 14).padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .overlay(alignment: .top) { Rectangle().fill(Palette.cardDivider).frame(height: 1) }
    }
}

/// `disk` rows as the page shows them: each live Task's place by its title,
/// then the rest summed by kind.
struct HostDiskSummary {
    struct Place: Identifiable {
        var id: String
        var name: String
        var bytes: UInt64
        var buildBytes: UInt64?
    }

    struct Leftover: Identifiable {
        var kind: String
        var title: String
        var count = 0
        var bytes: UInt64 = 0
        var cleanable = 0
        var id: String { kind }
    }

    var places: [Place] = []
    var leftovers: [Leftover] = []

    private static let order = ["claude-worktree", "worktree", "build-folder", "task-folder", "task-worktree"]

    init(rows: [DiskRow], terminals: [MuxaV2.Terminal]) {
        let tasks = terminals.filter { $0.live && $0.isTaskAgent }
        var sums: [String: Leftover] = [:]
        for row in rows {
            let isPlace = row.kind == "task-worktree" || row.kind == "task-folder"
            if isPlace, let t = tasks.first(where: { $0.places.contains { $0.path == row.path } || $0.taskDir == row.path }) {
                places.append(Place(id: row.path, name: t.taskTitle ?? t.name ?? row.path, bytes: row.bytes ?? 0,
                                    buildBytes: row.buildBytes))
                continue
            }
            var sum = sums[row.kind] ?? Leftover(kind: row.kind, title: Self.title(row.kind))
            sum.count += 1
            sum.bytes += (row.bytes ?? 0) + (row.buildBytes ?? 0)
            if row.cleanable { sum.cleanable += 1 }
            sums[row.kind] = sum
        }
        leftovers = sums.values.sorted {
            (Self.order.firstIndex(of: $0.kind) ?? Self.order.count, $0.kind)
                < (Self.order.firstIndex(of: $1.kind) ?? Self.order.count, $1.kind)
        }
    }

    static func title(_ kind: String) -> String {
        switch kind {
        case "claude-worktree": "Claude Code worktrees"
        case "worktree": "Other worktrees"
        case "build-folder": "Build folders with no place"
        case "task-folder": "Loose Task folders"
        case "task-worktree": "Worktrees of ended Tasks"
        default: kind
        }
    }
}
