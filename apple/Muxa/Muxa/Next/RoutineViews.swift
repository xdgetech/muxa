import MuxaV2
import SwiftUI

extension Routine {
    /// `09:00 Europe/Berlin`, `cron 0 9 * * 1-5`, `On review` or `On request`.
    var when: String {
        guard enabled else { return "Off" }
        if on == "review" { return "On review" }
        guard let at else { return "On request" }
        let base = "\(at) \(timezone ?? "")".trimmingCharacters(in: .whitespaces)
        return schedule.map { "\(base) · \($0)" } ?? base
    }

    /// `pr-watch · Personal`: its Bot, if any, and its Workspace.
    var owner: String {
        [bot, workspace].compactMap { $0 }.joined(separator: " · ")
    }

    enum Health { case running, failed, ok, never }

    var health: Health {
        if runningSince != nil { return .running }
        guard let last else { return .never }
        return last.ok ? .ok : .failed
    }
}

extension Routine.Health {
    var title: String? {
        switch self {
        case .running: "Running"
        case .failed: "Failed"
        case .ok: "OK"
        case .never: nil
        }
    }

    var color: Color {
        switch self {
        case .running: TaskState.working.color
        case .failed: Palette.late
        case .ok: TaskState.done.color
        case .never: TaskState.idle.color
        }
    }
}

struct RoutineRow: View {
    let routine: Routine

    var body: some View {
        let health = routine.health
        HStack(spacing: 10) {
            StateDot(color: health.color)
            VStack(alignment: .leading, spacing: 2) {
                Text(routine.name).foregroundStyle(Palette.text)
                Text("\(routine.owner) · \(routine.when)")
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.textTertiary)
            }
            Spacer()
            if let title = health.title {
                Text(title).font(.system(size: 12.5)).foregroundStyle(health.color)
            }
        }
        .frame(minHeight: minTouch)
    }
}

extension View {
    /// One element for a Routine's row, found by `routine.<name>`. The caller
    /// sets it on the outermost view: a List cell also takes an inner identifier.
    func routineElement(_ r: Routine) -> some View {
        accessibilityElement(children: .combine).accessibilityIdentifier("routine.\(r.name)")
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
            .listRowBackground(Palette.card)
            if let last = routine.last {
                Section("Last run") {
                    LabeledContent("At", value: Date(timeIntervalSince1970: TimeInterval(last.at)).formatted())
                    LabeledContent("Result", value: last.ok ? "OK" : "Failed")
                    if let n = last.published { LabeledContent("Items", value: "\(n)") }
                    if let why = last.reason { Text(why).foregroundStyle(Palette.late) }
                }
                .listRowBackground(Palette.card)
            }
            Section {
                Button("Run Now", systemImage: "play.fill") { Task { await model.runRoutine(routine) } }
                    .disabled(!routine.enabled || routine.runningSince != nil)
                    .accessibilityIdentifier("routine.run")
                if let error = model.requestError { Text(error).foregroundStyle(Palette.late) }
            }
            .listRowBackground(Palette.card)
        }
        .scrollContentBackground(.hidden)
        .background(Palette.ground)
        .navigationTitle(routine.name)
        .inlineTitle()
    }
}

/// Agents waiting on the user: short-lived, never Inbox items (ADR 0066 decision 3).
/// One waiting Task opens at a tap; several open the Tasks list.
struct NeedsYouBanner: View {
    @Environment(NextModel.self) private var model
    var open: (Terminal) -> Void
    var showAll: () -> Void

    var body: some View {
        let waiting = model.needsYou
        if let first = waiting.first {
            let one = waiting.count == 1
            Button {
                if one { open(first) } else { showAll() }
            } label: {
                HStack(spacing: 10) {
                    StateDot(color: TaskState.needsYou.color)
                    Text(one ? "\(first.title) needs you" : "\(waiting.count) agents need you")
                        .font(.system(size: 14))
                        .foregroundStyle(Palette.text)
                        .lineLimit(1)
                    Spacer()
                    Image(systemName: "chevron.right")
                        .font(.system(size: 13, weight: .bold))
                        .foregroundStyle(Palette.textSecondary)
                }
                .padding(.horizontal, 12)
                .frame(minHeight: minTouch)
                .background(Palette.bannerBack, in: RoundedRectangle(cornerRadius: 12))
                .overlay(RoundedRectangle(cornerRadius: 12).stroke(Palette.bannerBorder))
                .contentShape(RoundedRectangle(cornerRadius: 12))
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier(one ? "needsyou.\(first.id)" : "home.needsyou")
        }
    }
}

extension NextModel {
    /// The Routine as `.muxa/routines.toml` says it; nil until Settings are read.
    func routineSettings(_ r: Routine) -> SettingsRoutine? {
        settings.workspace(r.workspace)?.routines.first { $0.name == r.name }
    }

    /// Pauses or resumes a Routine through `routine-update`, at the current revision.
    func setRoutine(_ r: Routine, enabled: Bool) async {
        guard var spec = routineSettings(r)?.spec else {
            requestError = "The settings are not loaded yet."
            return
        }
        spec.enabled = enabled
        let failure = await settings.save([.routineUpdate(workspace: r.workspace, routine: spec)])
        await refreshRoutines()
        if let failure { requestError = failure }
    }
}
