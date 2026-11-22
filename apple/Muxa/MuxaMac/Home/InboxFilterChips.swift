import MuxaV2
import SwiftUI

/// The Inbox's filter as the mock's chips: All and each kind with its count,
/// then the Workspace and Later/Done as small menus.
struct InboxFilterChips: View {
    @Binding var filter: InboxFilter
    let items: [InboxItem]
    let now: Int64

    var body: some View {
        // Counts ignore the kind, so each chip says what picking it shows.
        let pool = InboxFilter(workspace: filter.workspace, showLater: filter.showLater).apply(items, now: now)
        ChipFlow(spacing: 6) {
            chip("All", count: pool.count, on: filter.kind == nil, id: "inbox.filter.all") { filter.kind = nil }
            ForEach(kinds(in: pool), id: \.self) { kind in
                chip(kind.title, count: pool.filter { $0.kind == kind }.count, on: filter.kind == kind,
                     id: "inbox.filter.\(kind.rawValue)") { filter.kind = filter.kind == kind ? nil : kind }
            }
            if !workspaces.isEmpty { workspaceMenu }
            showMenu
        }
    }

    /// Kinds present in the list, plus the picked one so it can be cleared.
    private func kinds(in pool: [InboxItem]) -> [ItemKind] {
        ItemKind.allCases.filter { kind in kind == filter.kind || pool.contains { $0.kind == kind } }
    }

    private var workspaces: [String] {
        Array(Set(items.compactMap(\.workspace))).sorted()
    }

    private func chip(_ title: String, count: Int, on: Bool, id: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 5) {
                Text(title)
                Text("\(count)").foregroundStyle(on ? Palette.ground.opacity(0.6) : Palette.textTertiary)
            }
            .chipFrame(on: on)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(id)
        .accessibilityAddTraits(on ? .isSelected : [])
    }

    private var workspaceMenu: some View {
        Menu {
            Picker("Workspace", selection: $filter.workspace) {
                Text("All workspaces").tag(String?.none)
                ForEach(workspaces, id: \.self) { Text($0).tag(Optional($0)) }
            }
            .pickerStyle(.inline)
        } label: {
            Text("\(filter.workspace ?? "All workspaces") ▾").chipFrame(on: false)
        }
        .chipMenu()
        .accessibilityIdentifier("inbox.filter.workspace")
    }

    private var showMenu: some View {
        Menu {
            Toggle("Show Later Items", isOn: $filter.showLater)
            Toggle("Show Done", isOn: $filter.showDone)
        } label: {
            Text(showTitle).chipFrame(on: false)
        }
        .chipMenu()
        .accessibilityIdentifier("inbox.filter")
    }

    private var showTitle: String {
        switch (filter.showLater, filter.showDone) {
        case (true, true): "Later, Done ▾"
        case (true, false): "Later ▾"
        case (false, true): "Done ▾"
        case (false, false): "Show ▾"
        }
    }
}

/// Lays the chips out in rows, wrapping as the mock's `flex-wrap`.
private struct ChipFlow: Layout {
    var spacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = arrange(width: proposal.width ?? .infinity, subviews: subviews)
        let height = rows.last.map { $0.y + $0.height } ?? 0
        return CGSize(width: proposal.width ?? rows.map(\.width).max() ?? 0, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        for row in arrange(width: bounds.width, subviews: subviews) {
            var x = bounds.minX
            for index in row.items {
                let size = subviews[index].sizeThatFits(.unspecified)
                subviews[index].place(at: CGPoint(x: x, y: bounds.minY + row.y), proposal: .init(size))
                x += size.width + spacing
            }
        }
    }

    private struct Row { var items: [Int] = []; var y: CGFloat = 0; var width: CGFloat = 0; var height: CGFloat = 0 }

    private func arrange(width: CGFloat, subviews: Subviews) -> [Row] {
        var rows = [Row()]
        for index in subviews.indices {
            let size = subviews[index].sizeThatFits(.unspecified)
            var row = rows.removeLast()
            if !row.items.isEmpty, row.width + spacing + size.width > width {
                let next = Row(y: row.y + row.height + spacing)
                rows.append(row)
                row = next
            }
            row.width += (row.items.isEmpty ? 0 : spacing) + size.width
            row.height = max(row.height, size.height)
            row.items.append(index)
            rows.append(row)
        }
        return rows
    }
}

private extension View {
    /// The mock's 28 pt pill; the picked one is filled.
    func chipFrame(on: Bool) -> some View {
        font(.system(size: 12.5))
            .foregroundStyle(on ? Palette.ground : Palette.textMid)
            .padding(.horizontal, 10)
            .frame(height: 28)
            .background(on ? Palette.text : .clear, in: Capsule())
            .overlay(Capsule().strokeBorder(on ? Palette.text : Palette.borderStrong))
            .contentShape(Capsule())
    }

    func chipMenu() -> some View {
        menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
    }
}
