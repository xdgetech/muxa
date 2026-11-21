import MuxaV2
import SwiftUI

extension Routine {
    /// `09:00 Europe/Berlin`, `cron 0 9 * * 1-5`, or `On request`.
    var when: String {
        guard enabled else { return "Off" }
        guard let at else { return "On request" }
        let base = "\(at) \(timezone ?? "")".trimmingCharacters(in: .whitespaces)
        return schedule.map { "\(base) · \($0)" } ?? base
    }

    enum Health { case running, failed, ok, never }

    var health: Health {
        if runningSince != nil { return .running }
        guard let last else { return .never }
        return last.ok ? .ok : .failed
    }
}

struct RoutineRow: View {
    let routine: Routine

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(routine.name)
                Text("\(routine.workspace) · \(routine.when)").font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            badge
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("routine.\(routine.name)")
    }

    @ViewBuilder
    private var badge: some View {
        switch routine.health {
        case .running: Label("Running", systemImage: "play.circle").foregroundStyle(.blue)
        case .failed: Label("Failed", systemImage: "exclamationmark.triangle").foregroundStyle(.red)
        case .ok: Image(systemName: "checkmark.circle").foregroundStyle(.green).accessibilityLabel("OK")
        case .never: EmptyView()
        }
    }
}

struct RoutinePane: View {
    @Environment(NextModel.self) private var model
    let routine: Routine

    var body: some View {
        Form {
            Section("Overview") {
                LabeledContent("Workspace", value: routine.workspace)
                LabeledContent("Runs", value: routine.when)
                if let next = routine.nextDue {
                    LabeledContent("Next run", value: Date(timeIntervalSince1970: TimeInterval(next)).formatted())
                }
                if let since = routine.runningSince {
                    LabeledContent("Running since", value: Date(timeIntervalSince1970: TimeInterval(since)).formatted())
                }
            }
            if let last = routine.last {
                Section("Last run") {
                    LabeledContent("At", value: Date(timeIntervalSince1970: TimeInterval(last.at)).formatted())
                    LabeledContent("Result", value: last.ok ? "OK" : "Failed")
                    if let n = last.published { LabeledContent("Items", value: "\(n)") }
                    if let why = last.reason { Text(why).foregroundStyle(.red) }
                }
            }
            Section {
                Button("Run Now") { Task { await model.runRoutine(routine) } }
                    .disabled(!routine.enabled || routine.runningSince != nil)
                    .accessibilityIdentifier("routine.run")
                if let error = model.requestError { Text(error).foregroundStyle(.red) }
            }
        }
        .navigationTitle(routine.name)
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// Agents waiting on the user: short-lived, never Inbox items (ADR 0066 decision 3).
struct NeedsYouStrip: View {
    @Environment(NextModel.self) private var model
    var open: (Terminal) -> Void

    var body: some View {
        if !model.needsYou.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack {
                    Text("Needs you").font(.caption.weight(.semibold)).foregroundStyle(.orange)
                    ForEach(model.needsYou) { t in
                        Button { open(t) } label: {
                            Label(t.branch ?? t.name ?? t.id, systemImage: "hand.raised")
                                .font(.callout)
                                .padding(.horizontal, 10).padding(.vertical, 4)
                                .background(Color.orange.opacity(0.15), in: Capsule())
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("needsyou.\(t.id)")
                    }
                }
            }
        }
    }
}
