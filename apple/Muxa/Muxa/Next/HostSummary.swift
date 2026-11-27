import MuxaV2
import SwiftUI

/// `CPU 12% · Mem 96% · 41G free` (ADR 0105): the host's figures, each value
/// the host lists in `alerts` in the warning style. It opens Settings › Host resources.
struct HostSummary: View {
    /// Where it sits: the Mac's Focus sidebar foot, the Mac's Home strip,
    /// the iPad sidebar's card, or the iPhone Inbox's chip.
    enum Look { case sidebarFoot, strip, card, chip }

    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    #if os(macOS)
    @Environment(\.openSettings) private var openSettings
    #endif
    let look: Look

    var body: some View {
        let parts = HostSummaryPart.parts(model.host)
        Button(action: open) { label(parts) }
            .buttonStyle(.plain)
            .accessibilityLabel(HostSummaryPart.spoken(parts))
            .accessibilityHint("Opens Settings, Host resources")
            .accessibilityIdentifier("host.summary")
            .watchesHost()
    }

    @ViewBuilder
    private func label(_ parts: [HostSummaryPart]) -> some View {
        switch look {
        case .sidebarFoot:
            HStack(spacing: 8) {
                Image(systemName: Icon.host).font(.system(size: 12, weight: .medium))
                line(parts)
                Spacer(minLength: 0)
            }
            .font(.system(size: 11))
            .padding(.horizontal, 18)
            .padding(.vertical, 7)
            .contentShape(Rectangle())
        case .strip:
            line(parts).font(.system(size: 12)).contentShape(Rectangle())
        case .card:
            HStack(spacing: 8) {
                line(parts)
                Spacer(minLength: 0)
                Image(systemName: "chevron.right").font(.system(size: 11, weight: .bold)).foregroundStyle(Palette.textTertiary)
            }
            .font(.system(size: 12))
            .padding(.horizontal, 12)
            .frame(minHeight: minTouch)
            .background(Palette.card, in: RoundedRectangle(cornerRadius: 8))
            .overlay(RoundedRectangle(cornerRadius: 8).stroke(Palette.border))
            .contentShape(Rectangle())
        case .chip:
            HStack(spacing: 6) {
                line(parts)
                Image(systemName: "chevron.right").font(.system(size: 9, weight: .bold)).foregroundStyle(Palette.textTertiary)
            }
            .font(.system(size: 12))
            .padding(.horizontal, 10)
            .frame(minHeight: 32)
            .background(Palette.card, in: Capsule())
            .overlay(Capsule().stroke(Palette.border))
            .contentShape(Capsule())
        }
    }

    private func line(_ parts: [HostSummaryPart]) -> some View {
        HStack(spacing: 5) {
            ForEach(Array(parts.enumerated()), id: \.offset) { i, part in
                if i > 0 { Text("·").foregroundStyle(Palette.textTertiary) }
                if part.alert {
                    Text(part.text)
                        .foregroundStyle(Palette.warnText)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 1)
                        .background(Palette.warnBack, in: RoundedRectangle(cornerRadius: 4))
                        .overlay(RoundedRectangle(cornerRadius: 4).stroke(Palette.warnBorder))
                } else {
                    Text(part.text).foregroundStyle(Palette.textSecondary)
                }
            }
        }
        .lineLimit(1)
        .monospacedDigit()
    }

    private func open() {
        nav.settingsRequest = .host
        #if os(macOS)
        openSettings()
        #else
        nav.focus = false
        nav.tab = .hosts
        #endif
    }
}

/// One value of the summary and whether the host flags it.
struct HostSummaryPart: Equatable {
    var text: String
    var spoken: String
    var alert: Bool

    /// Dashes until the first report comes.
    static func parts(_ report: HostReport?) -> [HostSummaryPart] {
        guard let r = report else {
            return [
                HostSummaryPart(text: "CPU –", spoken: "CPU not known yet", alert: false),
                HostSummaryPart(text: "Mem –", spoken: "memory not known yet", alert: false),
                HostSummaryPart(text: "– free", spoken: "free disk not known yet", alert: false),
            ]
        }
        let cpu = Int(r.cpu.percent.rounded())
        let mem = HostFormat.percent(r.memory.used, of: r.memory.total)
        var parts = [
            HostSummaryPart(text: "CPU \(cpu)%", spoken: "CPU \(cpu) percent", alert: r.cpuAlert),
            HostSummaryPart(text: "Mem \(mem)%", spoken: "memory \(mem) percent", alert: r.memoryAlert),
        ]
        if let disk = r.systemDisk {
            let free = HostFormat.size(disk.free)
            parts.append(HostSummaryPart(text: "\(free) free", spoken: "\(free) free", alert: r.alerted(disk)))
        }
        return parts
    }

    static func spoken(_ parts: [HostSummaryPart]) -> String {
        "Host: " + parts.map { $0.alert ? "\($0.spoken), warning" : $0.spoken }.joined(separator: ", ")
    }
}

/// How the host's figures read: binary units with one letter, as the mock writes them.
enum HostFormat {
    private static let units: [(String, Double)] = [("T", 1_099_511_627_776), ("G", 1_073_741_824), ("M", 1_048_576)]

    /// `41G`, `7.6G`, `640M`, `1.2T`: one decimal below 10 of a unit.
    static func size(_ bytes: UInt64) -> String {
        let b = Double(bytes)
        for (unit, scale) in units where b >= scale {
            let v = b / scale
            return v < 10 ? String(format: "%.1f%@", v, unit) : "\(Int(v.rounded()))\(unit)"
        }
        return "\(Int((b / 1024).rounded()))K"
    }

    static func percent(_ part: UInt64, of total: UInt64) -> Int {
        total == 0 ? 0 : Int((Double(part) / Double(total) * 100).rounded())
    }

    static func cpu(_ percent: Double) -> String {
        percent < 1 && percent > 0 ? String(format: "%.1f%%", percent) : "\(Int(percent.rounded()))%"
    }

    /// `idle 2 d`, `idle 4 h`, `idle 12 min` from when the agent went idle.
    static func idle(since: Int64, at sampled: Int64) -> String {
        let seconds = max(0, (sampled - since) / 1000)
        if seconds >= 86_400 { return "idle \(seconds / 86_400) d" }
        if seconds >= 3_600 { return "idle \(seconds / 3_600) h" }
        return "idle \(max(1, seconds / 60)) min"
    }
}

extension View {
    /// Keeps `NextModel.host` current while this view is on screen.
    func watchesHost() -> some View { modifier(WatchesHost()) }
}

private struct WatchesHost: ViewModifier {
    @Environment(NextModel.self) private var model

    func body(content: Content) -> some View {
        content
            .onAppear { model.watchHost() }
            .onDisappear { model.unwatchHost() }
    }
}
