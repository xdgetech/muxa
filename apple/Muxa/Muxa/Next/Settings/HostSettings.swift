import MuxaV2
import SwiftUI

/// Settings › Host resources (ADR 0105): the host's figures, the Tasks and
/// talks by memory, the rest of the host, and the disk use of Task places on request.
struct HostSettings: View {
    @Environment(NextModel.self) private var model
    @Environment(\.formGround) private var ground

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                if let report = model.host {
                    header(report)
                    HostTiles(report: report)
                    HostTaskList(report: report)
                    HostElsewhereList(groups: report.groups.filter { !$0.isAgentWork })
                } else {
                    HStack(spacing: 8) {
                        ProgressView().controlSize(.small)
                        Text(model.isConnected ? "Asking the host…" : "Not connected.")
                    }
                    .foregroundStyle(Palette.textSecondary)
                }
                HostDiskSection()
            }
            .padding(16)
            .frame(maxWidth: 900, alignment: .leading)
        }
        .background(ground)
        .navigationTitle("Host resources")
        .inlineTitle()
        .watchesHost()
        .accessibilityIdentifier("host.page")
    }

    private func header(_ r: HostReport) -> some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            let ago = max(0, Int(context.date.timeIntervalSince1970) - Int(r.sampledMs / 1000))
            Text("\(model.connectionTitle) · sampled \(ago) s ago")
                .font(.system(size: 12))
                .foregroundStyle(Palette.textSecondary)
                .monospacedDigit()
        }
    }
}

/// CPU, Memory, Swap and Disk free: four in a row where they fit, else two by two.
private struct HostTiles: View {
    let report: HostReport

    var body: some View {
        let tiles = self.tiles
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 12) { ForEach(tiles) { HostTile(tile: $0).frame(minWidth: 150) } }
            Grid(horizontalSpacing: 10, verticalSpacing: 10) {
                GridRow { HostTile(tile: tiles[0]); HostTile(tile: tiles[1]) }
                GridRow { HostTile(tile: tiles[2]); HostTile(tile: tiles[3]) }
            }
        }
    }

    private var tiles: [HostTile.Content] {
        let r = report
        let mem = r.memory
        let system = r.systemDisk
        let build = r.buildDisk
        return [
            .init(id: "cpu", label: "CPU", value: HostFormat.cpu(r.cpu.percent),
                  detail: "\(r.cpu.cores) cores · load \(r.load.first.map { String(format: "%.1f", $0) } ?? "–")",
                  alert: r.cpuAlert),
            .init(id: "memory", label: "Memory · pressure \(mem.pressure.rawValue)",
                  value: "\(HostFormat.size(mem.used)) / \(HostFormat.size(mem.total))",
                  detail: mem.compressed.map { "\(HostFormat.size($0)) compressed" } ?? "",
                  alert: r.memoryAlert),
            .init(id: "swap", label: "Swap", value: HostFormat.size(r.swap.used),
                  detail: "of \(HostFormat.size(r.swap.total))", alert: false),
            .init(id: "disk", label: "Disk free", value: system.map { HostFormat.size($0.free) } ?? "–",
                  detail: build.map { "build disk \(HostFormat.size($0.free))" } ?? "",
                  alert: [system, build].compactMap { $0 }.contains(where: r.alerted)),
        ]
    }
}

private struct HostTile: View {
    struct Content: Identifiable {
        var id: String
        var label: String
        var value: String
        var detail: String
        var alert: Bool
    }

    let tile: Content

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 10)
        VStack(alignment: .leading, spacing: 3) {
            Text(tile.label).font(.system(size: 11, weight: .semibold))
                .foregroundStyle(tile.alert ? Palette.warnText : Palette.textTertiary)
            Text(tile.value).font(.system(size: 20, weight: .semibold))
                .foregroundStyle(tile.alert ? Palette.warnText : Palette.text)
            Text(tile.detail.isEmpty ? " " : tile.detail).font(.system(size: 11.5))
                .foregroundStyle(tile.alert ? Palette.warnText : Palette.textSecondary)
        }
        .lineLimit(1)
        .minimumScaleFactor(0.8)
        .monospacedDigit()
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(tile.alert ? Palette.warnBack : Palette.card, in: shape)
        .overlay(shape.stroke(tile.alert ? Palette.warnBorder : Palette.border))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel([tile.label, tile.value, tile.detail].filter { !$0.isEmpty }.joined(separator: ", ")
            + (tile.alert ? ", warning" : ""))
        .accessibilityIdentifier("host.tile.\(tile.id)")
    }
}

/// A small amber dot while the host flags a value; nothing otherwise.
struct HostAlertDot: View {
    let alerts: [String]

    var body: some View {
        if !alerts.isEmpty {
            Circle().fill(Palette.warnText).frame(width: 8, height: 8).accessibilityLabel("warning")
        }
    }
}

/// A card of rows with a small title line, as the mock draws each list.
struct HostCard<Header: View, Content: View>: View {
    @ViewBuilder var header: Header
    @ViewBuilder var content: Content

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 10)
        VStack(alignment: .leading, spacing: 0) {
            header
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(Palette.textTertiary)
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
                .frame(maxWidth: .infinity, alignment: .leading)
            content
        }
        .background(Palette.card, in: shape)
        .overlay(shape.stroke(Palette.border))
        .clipShape(shape)
    }
}
