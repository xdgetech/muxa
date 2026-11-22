import MuxaV2
import SwiftUI

/// Focus's left column: who needs the user, the live Tasks, the folded other
/// terminals, and the Routines' health at the foot. Plain views on the
/// surface tone, not a sidebar List, which draws a material.
struct FocusSidebar: View {
    @Environment(NextModel.self) private var model
    @Binding var selected: String?
    /// The other terminals start closed: Tasks are what Focus is for.
    @State private var showOthers = false

    private var tasks: [MuxaV2.Terminal] {
        model.liveTasks.filter { !model.needsYou.contains($0) }
    }

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    if !model.needsYou.isEmpty { needsYou }
                    header { SectionLabel(title: "Tasks") }
                    ForEach(tasks) { row($0, style: .task) }
                    if !model.otherTerminals.isEmpty { others }
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 10)
            }
            RoutinesFooter()
        }
        .frame(width: 280)
        .frame(maxHeight: .infinity)
        .background(Palette.surface)
        .overlay(alignment: .trailing) { Rectangle().fill(Palette.border).frame(width: 1) }
    }

    @ViewBuilder private var needsYou: some View {
        HStack {
            SectionLabel(title: "Needs you · \(model.needsYou.count)", color: TaskState.needsYou.color)
            Spacer()
            KeyHint("\(Shortcut.nextNeedsYou.display) next")
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 4)
        ForEach(model.needsYou) { row($0, style: .needsYou) }
    }

    @ViewBuilder private var others: some View {
        header {
            Button { showOthers.toggle() } label: {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 9, weight: .bold))
                        .rotationEffect(.degrees(showOthers ? 90 : 0))
                    SectionLabel(title: OtherTerminals.title)
                    Spacer()
                    Text(OtherTerminals.count(model.otherTerminals.count))
                        .font(.system(size: 11))
                }
                .foregroundStyle(Palette.textTertiary)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .otherTerminalsToggle(open: showOthers)
        }
        if showOthers {
            ForEach(model.otherTerminals) { row($0, style: .other) }
        }
    }

    private func header<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        content()
            .padding(.horizontal, 10)
            .padding(.top, 14)
            .padding(.bottom, 4)
    }

    private func row(_ t: MuxaV2.Terminal, style: FocusTaskRow.Style) -> some View {
        FocusTaskRow(terminal: t, style: style, loop: model.loops[t.id], selected: t.id == selected) {
            selected = t.id
        }
    }
}

/// "Routines · N running · N failed": opens the Routines page.
private struct RoutinesFooter: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav

    var body: some View {
        let running = model.routines.filter { $0.health == .running }.count
        let failed = model.routines.filter { $0.health == .failed }.count
        Button {
            nav.focus = false
            nav.tab = .routines
        } label: {
            HStack(spacing: 10) {
                Image(systemName: "clock").font(.system(size: 13, weight: .medium))
                Text("Routines")
                Spacer()
                Text("\(running) running · \(failed) failed").font(.system(size: 11))
            }
            .foregroundStyle(Palette.textSecondary)
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .overlay(alignment: .top) { Rectangle().fill(Palette.border).frame(height: 1).padding(.horizontal, 8) }
        .help("Routines (\(Shortcut.routines.display))")
        .accessibilityIdentifier("focus.routines")
    }
}
