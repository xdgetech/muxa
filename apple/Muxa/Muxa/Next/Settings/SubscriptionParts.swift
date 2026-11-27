import MuxaV2
import SwiftUI

/// How a plan window reads: `5 h`, `7 d`, `45 min`.
enum PlanWindow {
    static func label(_ minutes: UInt32) -> String {
        if minutes % 1440 == 0 { return "\(minutes / 1440) d" }
        if minutes % 60 == 0 { return "\(minutes / 60) h" }
        return "\(minutes) min"
    }

    /// Green below 80%, amber to 95%, red from 95%: the thresholds of ADR 0103.
    static func color(_ percent: UInt32) -> Color {
        if percent >= 95 { return Theme.failure }
        if percent >= 80 { return Theme.attention }
        return Theme.connected
    }

    /// `82% of 5 h`, `5 h reset`, or `no data yet`, from the shortest window.
    static func line(_ usage: SubscriptionUsage?, now: Date = Date()) -> String {
        guard let w = usage?.windows.first else { return "no data yet" }
        if w.hasReset(now: now) { return "\(label(w.minutes)) reset" }
        return "\(w.usedPercent)% of \(label(w.minutes))"
    }
}

/// One window as a bar, its percent and when it resets (ADR 0103 §8).
struct PlanMeter: View {
    let window: UsageWindow
    var now = Date()

    var body: some View {
        HStack(spacing: 10) {
            Text(PlanWindow.label(window.minutes))
                .font(.caption)
                .foregroundStyle(Palette.textSecondary)
                .frame(width: 44, alignment: .leading)
            if window.hasReset(now: now) {
                Text("reset, no new data")
                    .font(.caption)
                    .foregroundStyle(Palette.textTertiary)
                Spacer()
            } else {
                GeometryReader { g in
                    ZStack(alignment: .leading) {
                        Capsule().fill(Palette.raised)
                        Capsule()
                            .fill(PlanWindow.color(window.usedPercent))
                            .frame(width: g.size.width * CGFloat(min(window.usedPercent, 100)) / 100)
                    }
                }
                .frame(height: 6)
                Text("\(window.usedPercent)%")
                    .font(.caption.monospacedDigit().weight(.semibold))
                    .foregroundStyle(PlanWindow.color(window.usedPercent))
                    .frame(width: 40, alignment: .trailing)
                Text("resets \(window.resetsAt.formatted(Self.resetStyle(window.resetsAt, now: now)))")
                    .font(.caption)
                    .foregroundStyle(Palette.textTertiary)
                    .frame(minWidth: 90, alignment: .leading)
            }
        }
        .accessibilityElement(children: .combine)
    }

    /// The time today, else the weekday and time.
    static func resetStyle(_ at: Date, now: Date) -> Date.FormatStyle {
        Calendar.current.isDate(at, inSameDayAs: now)
            ? .dateTime.hour().minute()
            : .dateTime.weekday(.abbreviated).hour().minute()
    }
}

/// An ordered list of subscription names, as a Workspace, a Bot or the
/// Capture assistant keeps it: the first of the agent's kind runs it.
struct SubscriptionOrder: View {
    @Environment(NextModel.self) private var model
    @Binding var names: [String]
    /// What may be added, in the host's order.
    let available: [String]
    let id: String
    /// Puts a login first of its kind at once (ADR 0103 §4); nil: no
    /// Current mark and no Use button.
    var use: ((String) -> Void)?

    var body: some View {
        let current = SubscriptionOrder.current(names, specs: model.settings.settings?.subscriptions ?? [])
        ForEach(Array(names.enumerated()), id: \.element) { index, name in
            HStack(spacing: 10) {
                Text("\(index + 1)").font(.caption.monospacedDigit()).foregroundStyle(Palette.textTertiary)
                VStack(alignment: .leading, spacing: 2) {
                    Text(name).foregroundStyle(Palette.text)
                    Text(PlanWindow.line(model.settings.limits(name)))
                        .font(.caption)
                        .foregroundStyle(Palette.textTertiary)
                }
                Spacer()
                if let use {
                    if current.contains(name) {
                        Text("Current")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(Palette.textSecondary)
                            .accessibilityIdentifier("\(id).current.\(name)")
                    } else {
                        Button("Use") { use(name) }
                            .buttonStyle(.borderless)
                            .disabled(model.settings.readOnly || model.settings.saving)
                            .accessibilityLabel("Use \(name) for next Tasks")
                            .accessibilityIdentifier("\(id).use.\(name)")
                    }
                }
                Button { move(name, up: true) } label: { Image(systemName: "arrow.up") }
                    .buttonStyle(.borderless)
                    .disabled(index == 0)
                    .accessibilityLabel("Move \(name) up")
                    .accessibilityIdentifier("\(id).up.\(name)")
                Button(role: .destructive) { names.removeAll { $0 == name } } label: { Image(systemName: "minus.circle") }
                    .buttonStyle(.borderless)
                    .accessibilityLabel("Remove \(name)")
                    .accessibilityIdentifier("\(id).remove.\(name)")
            }
        }
        let rest = available.filter { !names.contains($0) }
        Menu {
            ForEach(rest, id: \.self) { n in Button(n) { names.append(n) } }
        } label: {
            Label("Add a subscription", systemImage: "plus")
        }
        .disabled(rest.isEmpty)
        .accessibilityIdentifier("\(id).add")
    }

    private func move(_ name: String, up: Bool) {
        guard let at = names.firstIndex(of: name), at > 0 else { return }
        names.swapAt(at, at - 1)
    }

    /// The first name of each agent kind: the login the next agent of that
    /// kind takes while switching is off, or while it has room.
    static func current(_ names: [String], specs: [SubscriptionSpec]) -> Set<String> {
        var kinds = Set<String>()
        return Set(names.filter { n in
            guard let kind = specs.first(where: { $0.name == n })?.agent else { return false }
            return kinds.insert(kind).inserted
        })
    }

    /// `names` with `name` first: Use for next Tasks.
    static func first(_ name: String, in names: [String]) -> [String] {
        [name] + names.filter { $0 != name }
    }
}

extension SettingsModel {
    /// The host's subscriptions of `agent` (`claude-code`, `codex`), or all.
    func subscriptionNames(agent: String? = nil) -> [String] {
        (settings?.subscriptions ?? []).filter { agent == nil || $0.agent == agent }.map(\.name)
    }
}
