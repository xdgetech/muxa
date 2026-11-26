import MuxaV2
import SwiftUI

/// Home's left column: the title, Capture, the filter chips and the rows in
/// the user's order. Drag a row or press `⌥↑` `⌥↓` to move it; `↑` `↓` pick.
struct HomeInboxColumn: View {
    @Environment(NextModel.self) private var model
    @Environment(MacChrome.self) private var chrome
    @Binding var selected: String?
    @Binding var filter: InboxFilter
    /// `⏎` on a row: its Task in Focus.
    var open: (InboxItem) -> Void
    @FocusState private var focused: Bool
    @State private var dropTarget: String?
    @State private var deleteOlder: Int?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text("Inbox").font(.system(size: 22, weight: .bold))
                Text(lead).foregroundStyle(Palette.textTertiary).lineLimit(1)
                Spacer(minLength: 0)
                moreMenu
            }
            captureButton
            InboxFilterChips(filter: $filter, items: model.inbox.items)
            rows
        }
        .padding(EdgeInsets(top: 16, leading: 20, bottom: 10, trailing: 16))
        .background { ReorderKeys(selected: selected) }
        .inboxDeleteOlder($deleteOlder)
    }

    private var moreMenu: some View {
        Menu {
            InboxDeleteOlderMenu(days: $deleteOlder)
        } label: {
            Image(systemName: "ellipsis.circle").foregroundStyle(Palette.textSecondary)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .disabled(!model.isConnected)
        .help("Inbox actions")
        .accessibilityIdentifier("inbox.more")
    }

    var visible: [InboxItem] { filter.apply(model.inbox.items) }

    private var lead: String {
        let total = model.inbox.items.count
        return filter.isActive ? "\(visible.count) of \(total) · clear the filter to reorder"
            : "\(total) to do · your order, drag to change"
    }

    private var captureButton: some View {
        Button { chrome.showCapture() } label: {
            HStack(spacing: 10) {
                Image(systemName: "plus").font(.system(size: 14, weight: .semibold))
                Text("Capture…").frame(maxWidth: .infinity, alignment: .leading)
                KeyHint(Shortcut.capture.display)
            }
            .font(.system(size: 14))
            .foregroundStyle(Palette.textTertiary)
            .padding(.horizontal, 12)
            .frame(height: 40)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 10))
            .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Palette.borderStrong))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!model.isConnected)
        .accessibilityIdentifier("home.capture")
    }

    private var rows: some View {
        ScrollView {
            LazyVStack(spacing: 2) {
                ForEach(visible) { item in
                    row(item, position: position(of: item.id))
                }
                if visible.isEmpty {
                    Text(model.inbox.items.isEmpty ? "Nothing to do." : "Nothing matches the filter.")
                        .foregroundStyle(Palette.textSecondary)
                        .padding(.vertical, 12)
                }
                if filter.showDone, !model.inbox.done.isEmpty {
                    SectionLabel(title: "Done", count: model.inbox.done.count)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.top, 14)
                        .padding(.bottom, 4)
                    ForEach(model.inbox.done) { row($0, position: nil) }
                }
            }
            .padding(.top, 4)
        }
        .scrollIndicators(.automatic)
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .onMoveCommand(perform: step)
        .onKeyPress(.return) {
            guard let item = selectedItem else { return .ignored }
            open(item)
            return .handled
        }
        .accessibilityIdentifier("inbox.list")
        .onAppear { focused = true }
    }

    private func row(_ item: InboxItem, position: Int?) -> some View {
        let canDrag = position != nil && !filter.isActive
        return HomeInboxRow(item: item, position: position, selected: item.id == selected, draggable: canDrag)
            .overlay(alignment: .top) {
                if dropTarget == item.id {
                    Rectangle().fill(Palette.accent).frame(height: 2).offset(y: -1)
                }
            }
            .onTapGesture {
                selected = item.id
                focused = true
            }
            .accessibilityAction { selected = item.id }
            .modifier(Reorder(enabled: canDrag, id: item.id, target: $dropTarget) { drop($0, before: item.id) })
    }

    /// Where an item stands in the whole order, from 1.
    private func position(of id: String) -> Int? {
        model.inbox.items.firstIndex { $0.id == id }.map { $0 + 1 }
    }

    private var selectedItem: InboxItem? {
        guard let selected else { return nil }
        return (model.inbox.items + model.inbox.done).first { $0.id == selected }
    }

    private func step(_ direction: MoveCommandDirection) {
        let ids = visible.map(\.id) + (filter.showDone ? model.inbox.done.map(\.id) : [])
        guard !ids.isEmpty else { return }
        let at = selected.flatMap { ids.firstIndex(of: $0) }
        switch direction {
        case .up: selected = ids[max((at ?? ids.count) - 1, 0)]
        case .down: selected = ids[min((at ?? -1) + 1, ids.count - 1)]
        default: break
        }
    }

    /// The dragged item goes where `target` stands, in the whole order.
    private func drop(_ dragged: String, before target: String) {
        let items = model.inbox.items
        guard dragged != target,
              let from = items.firstIndex(where: { $0.id == dragged }),
              let to = items.firstIndex(where: { $0.id == target }) else { return }
        model.move(in: items, from: IndexSet(integer: from), to: to)
        selected = dragged
    }
}

/// Drag and drop of a row's id, only while the order is whole.
private struct Reorder: ViewModifier {
    let enabled: Bool
    let id: String
    @Binding var target: String?
    let drop: (String) -> Void

    func body(content: Content) -> some View {
        if enabled {
            content
                .draggable(id)
                .dropDestination(for: String.self) { ids, _ in
                    guard let dragged = ids.first else { return false }
                    drop(dragged)
                    return true
                } isTargeted: { over in
                    if over { target = id } else if target == id { target = nil }
                }
        } else {
            content
        }
    }
}
