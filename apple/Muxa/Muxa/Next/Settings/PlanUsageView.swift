import MuxaV2
import SwiftUI

/// The Usage page (ADR 0091 §8): each subscription's plan windows as its
/// agents last reported them. A login with no running agent shows its last
/// report, and "as of" says when that was.
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
            Section {
                EmptyView()
            } footer: {
                Text("Limits come from the agents' own reports: Claude Code's status line while a session runs, and Codex's "
                    + "session logs. At 80% muxa warns; from 95% new agents take the next subscription in their list.")
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
