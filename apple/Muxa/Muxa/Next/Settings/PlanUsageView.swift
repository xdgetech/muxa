import MuxaV2
import SwiftUI

/// The Usage page (ADR 0103 §8): each subscription's plan windows as its
/// agents last reported them, then today's tokens by Workspace and by Task.
/// A login with no running agent shows its last report, and "as of" says when.
struct PlanUsageView: View {
    @Environment(NextModel.self) private var model

    var body: some View {
        SettingsPage {
            let subs = model.settings.settings?.subscriptions ?? []
            if subs.isEmpty {
                Section {
                    Text("No subscriptions yet. Add your logins in Settings → Subscriptions, and their limits show here.")
                        .foregroundStyle(Palette.textSecondary)
                        .accessibilityIdentifier("usage.empty")
                }
            }
            ForEach(subs) { sub in
                let usage = model.settings.limits(sub.name)
                Section {
                    if let usage, !usage.windows.isEmpty {
                        ForEach(usage.windows, id: \.minutes) { PlanMeter(window: $0) }
                    } else {
                        Text("No data yet. It shows after the first agent on this login answers.")
                            .foregroundStyle(Palette.textSecondary)
                    }
                } header: {
                    HStack {
                        Text(sub.name)
                        if let plan = sub.plan { Text(plan).foregroundStyle(Palette.textTertiary) }
                        Spacer()
                        if let at = usage?.asOf {
                            Text("as of \(at.formatted(date: .omitted, time: .shortened))")
                                .foregroundStyle(Palette.textTertiary)
                        }
                    }
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("usage.\(sub.name)")
            }
            if let tokens = model.settings.tokenUsage {
                TokenSections(tokens: tokens)
            }
            Section {
                EmptyView()
            } footer: {
                Text("Limits come from the agents' own reports: Claude Code's status line while a session runs, and Codex's "
                    + "session logs. At 80% muxa warns; from 95% new agents take the next subscription in their list. "
                    + "Tokens come from the agents' transcripts on the host.")
            }
        }
        .navigationTitle("Usage")
        .inlineTitle()
        .refreshable { await model.settings.loadPlanUsage() }
        .task {
            if model.settings.settings == nil { await model.settings.load() } else { await model.settings.loadPlanUsage() }
        }
    }
}

/// Today's tokens (ADR 0103 §6): each Workspace, then the ten Task folders
/// that used most. A Task whose agent still runs goes by its name and opens
/// in Focus; an ended one has nothing to open.
private struct TokenSections: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(\.openWindow) private var openWindow
    let tokens: TokenUsage

    var body: some View {
        Section("Today by Workspace") {
            if tokens.workspaces.isEmpty {
                Text("No agent used tokens today.").foregroundStyle(Palette.textSecondary)
            }
            ForEach(tokens.workspaces) { w in
                LabeledContent(w.workspace, value: TokenText.line(input: w.input, cached: w.cached, output: w.output))
            }
        }
        .accessibilityIdentifier("usage.workspaces")
        if !tokens.tasks.isEmpty {
            Section("Today by Task") {
                ForEach(tokens.tasks.prefix(10)) { t in
                    if let agent = agent(t) {
                        Button { OpenInFocus.show(agent, nav: nav, openWindow: openWindow) } label: { row(t, name: agent.name ?? t.title) }
                            .buttonStyle(.plain)
                            .accessibilityHint("Opens the Task")
                            .accessibilityIdentifier("usage.task.\(t.title)")
                    } else {
                        row(t, name: t.title).accessibilityIdentifier("usage.task.\(t.title)")
                    }
                }
            }
        }
    }

    private func row(_ t: TaskTokens, name: String) -> some View {
        LabeledContent {
            Text(TokenText.line(input: t.input, cached: t.cached, output: t.output))
        } label: {
            Text(name)
            Text(t.workspace)
        }
        .contentShape(Rectangle())
    }

    private func agent(_ t: TaskTokens) -> MuxaV2.Terminal? {
        model.terminals.first { $0.labels["muxa.dir"] == t.dir && $0.labels["muxa.role"] == "agent" }
    }
}
