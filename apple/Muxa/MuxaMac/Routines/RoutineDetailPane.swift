import MuxaV2
import SwiftUI

/// The picked Routine: its state, Overview and Brief, and Run now.
struct RoutineDetailPane: View {
    enum Tab: Hashable { case overview, brief }

    @Environment(NextModel.self) private var model
    @Environment(MacChrome.self) private var chrome
    let routine: Routine
    @State private var tab = Tab.overview

    var body: some View {
        let spec = model.routineSettings(routine)
        VStack(alignment: .leading, spacing: 0) {
            heading(spec)
            ScrollView {
                Group {
                    switch tab {
                    case .overview: overview(spec?.spec)
                    case .brief: brief(spec)
                    }
                }
                .frame(maxWidth: 820, alignment: .leading)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 28)
                .padding(.vertical, 18)
            }
            actions(spec?.spec)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Palette.pane)
        .onChange(of: routine.id) { tab = .overview }
    }

    private func heading(_ spec: SettingsRoutine?) -> some View {
        let group = routine.group
        return VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Badge(text: group.title, foreground: group.badgeForeground, background: group.badgeBackground)
                Text(subtitle).font(.system(size: 12)).foregroundStyle(Palette.textSecondary)
            }
            Text(routine.name)
                .font(.system(size: 21, weight: .bold))
                .accessibilityIdentifier("routine.title")
            UnderlineTabs(tabs: tabs(spec), selection: $tab).padding(.top, 4)
        }
        .padding(.horizontal, 28)
        .padding(.top, 18)
    }

    private var subtitle: String {
        var parts = [routine.workspace, routine.timing]
        if let next = routine.nextDue { parts.append("next \(routineStamp(next))") }
        if let since = routine.runningSince { parts.append("running since \(routineStamp(since))") }
        return parts.joined(separator: " · ")
    }

    private func tabs(_ spec: SettingsRoutine?) -> [(title: String, tag: Tab, id: String)] {
        var tabs: [(title: String, tag: Tab, id: String)] = [("Overview", .overview, "routine.tab.overview")]
        if spec != nil { tabs.append(("Brief", .brief, "routine.tab.brief")) }
        return tabs
    }

    // MARK: Overview

    private func overview(_ spec: RoutineSpec?) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            if let last = routine.last, !last.ok { lastError(last) }
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 10), count: 3), spacing: 10) {
                ForEach(facts(spec), id: \.0) { FactCard(label: $0.0, value: $0.1) }
            }
        }
    }

    private func lastError(_ last: RoutineLast) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            SectionLabel(title: "Last error", color: Palette.late)
            Text(last.reason ?? "The run failed and gave no reason.")
                .font(.system(size: 12.5, design: .monospaced))
                .lineSpacing(3)
                .foregroundStyle(Palette.warnText)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .background(Palette.terminal, in: RoundedRectangle(cornerRadius: 8))
                .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.warnBorder))
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("routine.error")
    }

    private func facts(_ spec: RoutineSpec?) -> [(String, String)] {
        var facts = [("Workspace", routine.workspace), ("Schedule", routine.timing)]
        facts.append(("Next run", routine.nextDue.map(routineStamp) ?? (routine.enabled ? "Not planned" : "Paused")))
        facts.append(("Last run", lastRun))
        if let worker = spec?.worker { facts.append(("Worker", worker)) }
        if let project = spec?.project { facts.append(("Project", project)) }
        return facts
    }

    private var lastRun: String {
        guard let last = routine.last else { return "Never" }
        var parts = [routineStamp(last.at), last.ok ? "OK" : "Failed"]
        if let n = last.published { parts.append(n == 1 ? "1 item" : "\(n) items") }
        return parts.joined(separator: " · ")
    }

    // MARK: Brief

    @ViewBuilder
    private func brief(_ spec: SettingsRoutine?) -> some View {
        if let spec {
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 10) {
                    Text(".muxa/\(spec.spec.instructions)").frame(maxWidth: .infinity, alignment: .leading)
                    Button("Edit") { edit(spec) }
                        .buttonStyle(ActionButtonStyle(kind: .secondary, compact: true))
                        .disabled(model.settings.readOnly)
                        .accessibilityIdentifier("routine.edit.brief")
                }
                .font(.system(size: 12))
                .foregroundStyle(Palette.textTertiary)
                Text(spec.instructionsText ?? "The instruction file cannot be read.")
                    .font(.system(size: 14.5))
                    .lineSpacing(5)
                    .foregroundStyle(spec.instructionsText == nil ? Palette.late : Palette.terminalText)
                    .textSelection(.enabled)
                    .accessibilityIdentifier("routine.brief")
            }
            .frame(maxWidth: 760, alignment: .leading)
        }
    }

    // MARK: Actions

    private func actions(_ spec: RoutineSpec?) -> some View {
        ActionBar {
            Button { Task { await model.runRoutine(routine) } } label: {
                ActionLabel(title: "Run now", key: "⌘R", primary: true)
            }
            .buttonStyle(ActionButtonStyle(kind: .primary))
            .keyboardShortcut("r", modifiers: .command)
            .disabled(!routine.enabled || routine.runningSince != nil)
            .accessibilityIdentifier("routine.run")
            if spec != nil, !model.settings.readOnly {
                Button(routine.enabled ? "Pause" : "Resume") {
                    Task { await model.setRoutine(routine, enabled: !routine.enabled) }
                }
                .buttonStyle(ActionButtonStyle(kind: .secondary))
                .disabled(model.settings.saving)
                .accessibilityIdentifier("routine.pause")
            }
            if let settings = model.routineSettings(routine) {
                Button("Edit") { edit(settings) }
                    .buttonStyle(ActionButtonStyle(kind: .secondary))
                    .disabled(model.settings.readOnly)
                    .accessibilityIdentifier("routine.edit")
            }
        }
    }

    /// Settings' Routine editor, over this page.
    private func edit(_ spec: SettingsRoutine) {
        chrome.botSheet = .routine(workspace: routine.workspace, draft: RoutineDraft(spec))
    }
}
