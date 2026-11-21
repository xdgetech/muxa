import MuxaV2
import SwiftUI

/// Focus's panes side by side (ADR 0070 decision 1). The active one has the
/// keyboard and an accent edge; `⌘\` adds a pane and `⌘1`…`⌘9` pick one.
struct PaneSplit: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav

    var body: some View {
        HSplitView {
            ForEach(Array(nav.panes.enumerated()), id: \.element) { index, id in
                if let t = model.terminals.first(where: { $0.id == id }) {
                    pane(t, index: index)
                }
            }
        }
        .onChange(of: model.terminals.map(\.id)) { _, ids in nav.keep(only: Set(ids)) }
    }

    private func pane(_ t: MuxaV2.Terminal, index: Int) -> some View {
        let active = index == nav.active
        return VStack(spacing: 0) {
            if nav.panes.count > 1 {
                HStack {
                    Text("\(index + 1)  \(t.branch ?? t.name ?? t.id)")
                        .font(.caption.weight(active ? .semibold : .regular))
                        .foregroundStyle(active ? Theme.accent : .secondary)
                    Spacer()
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .accessibilityIdentifier("pane.\(index + 1)")
            }
            TerminalPane(terminal: t, active: active, focusRequest: nav.focusRequest) { nav.activate(index) }
                .id(t.id)
        }
        .frame(minWidth: 240)
        .overlay(alignment: .top) {
            if nav.panes.count > 1, active { Rectangle().fill(Theme.accent).frame(height: 2) }
        }
    }
}
