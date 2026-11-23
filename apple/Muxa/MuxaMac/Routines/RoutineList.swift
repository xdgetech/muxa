import MuxaV2
import SwiftUI

/// The left column: the Routines in Failed, Running and Paused, then the
/// Workspaces whose Routines are turned off, and what cannot be read.
struct RoutineList: View {
    @Environment(NextModel.self) private var model
    @Binding var selected: String?
    let current: Routine?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 2) {
                ForEach(RoutineGroup.allCases, id: \.self) { group in
                    let rows = model.routines.filter { $0.group == group }
                    if !rows.isEmpty {
                        SectionLabel(title: group.title, count: rows.count, color: group.color)
                            .padding(.horizontal, 8)
                            .padding(.top, 8)
                            .padding(.bottom, 4)
                        ForEach(rows) { r in
                            RoutineListRow(routine: r, selected: r.id == current?.id) { selected = r.id }
                        }
                    }
                }
                if model.routines.isEmpty && model.routinesOff.isEmpty && model.routineProblems.isEmpty {
                    empty
                }
                RoutinesOffSection()
                problems
                if let error = model.requestError {
                    Text(error).font(.system(size: 12)).foregroundStyle(Palette.late).padding(8)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 14)
        }
    }

    private var empty: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("No Routines").font(.system(size: 13, weight: .semibold))
            Text("Add one to a Workspace in Settings (\(Shortcut.settings.display)).")
                .font(.system(size: 12))
                .foregroundStyle(Palette.textTertiary)
        }
        .padding(8)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("routines.empty")
    }

    @ViewBuilder
    private var problems: some View {
        if !model.routineProblems.isEmpty {
            SectionLabel(title: "Cannot read", count: model.routineProblems.count, color: Palette.late)
                .padding(.horizontal, 8)
                .padding(.top, 14)
                .padding(.bottom, 4)
            ForEach(model.routineProblems, id: \.workspace) { p in
                VStack(alignment: .leading, spacing: 2) {
                    Text(p.workspace).fontWeight(.medium)
                    Text(p.reason).font(.system(size: 12)).foregroundStyle(Palette.late)
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 4)
                .accessibilityElement(children: .combine)
            }
        }
    }
}

/// A 48 pt row: the name over `workspace · when`.
struct RoutineListRow: View {
    let routine: Routine
    let selected: Bool
    let pick: () -> Void

    var body: some View {
        Button(action: pick) {
            VStack(alignment: .leading, spacing: 2) {
                Text(routine.name).font(.system(size: 13.5, weight: .medium)).foregroundStyle(Palette.text)
                Text("\(routine.owner) · \(routine.timing)")
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.textTertiary)
            }
            .lineLimit(1)
            .padding(.horizontal, 10)
            .padding(.vertical, 4)
            .frame(maxWidth: .infinity, minHeight: 48, alignment: .leading)
            .selectableRow(selected)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .opacity(routine.group == .paused ? 0.6 : 1)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .routineElement(routine)
    }
}

/// `TURNED OFF`: Workspaces whose `.muxa/routines.toml` has Routines but
/// whose gate is off, so none runs. Turn on opens the gate.
struct RoutinesOffSection: View {
    @Environment(NextModel.self) private var model
    @State private var turning: String?

    var body: some View {
        if !model.routinesOff.isEmpty {
            VStack(alignment: .leading, spacing: 6) {
                SectionLabel(title: "Turned off", count: model.routinesOff.count)
                    .padding(.horizontal, 8)
                    .padding(.bottom, 2)
                ForEach(model.routinesOff, id: \.workspace) { card($0) }
            }
            .padding(.top, 10)
            .overlay(alignment: .top) { Rectangle().fill(Palette.border).frame(height: 1) }
            .padding(.top, 10)
        }
    }

    private func card(_ off: RoutinesOff) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(off.workspace).fontWeight(.medium)
                Text("\(off.count) \(off.count == 1 ? "Routine" : "Routines") in .muxa/routines.toml · none run")
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.textTertiary)
            }
            .accessibilityElement(children: .combine)
            Spacer(minLength: 0)
            Button("Turn on") {
                turning = off.workspace
                Task {
                    await model.turnOnRoutines(in: off.workspace)
                    turning = nil
                }
            }
            .buttonStyle(ActionButtonStyle(kind: .primary, compact: true))
            .disabled(turning != nil)
            .accessibilityIdentifier("routines.on.\(off.workspace)")
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 4)
        .frame(minHeight: 48)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.border))
    }
}
