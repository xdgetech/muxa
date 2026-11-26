import MuxaV2
import SwiftUI

/// The Inbox beside a terminal: Capture, the open items, Done and Later by swipe.
struct InboxPanel: View {
    @Environment(NextModel.self) private var model

    var body: some View {
        List {
            CaptureField().cardRow()
            ForEach(model.inbox.items) { item in
                ItemCard(item: item)
                    .cardRow()
                    .swipeActions { ItemSwipeActions(item: item) }
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(Palette.ground)
        .navigationTitle("Inbox")
    }
}

extension View {
    /// A list row that draws its own card: no separator, no row background.
    func cardRow(vertical: CGFloat = 3, horizontal: CGFloat = 12) -> some View {
        listRowInsets(EdgeInsets(top: vertical, leading: horizontal, bottom: vertical, trailing: horizontal))
            .listRowSeparator(.hidden)
            .listRowBackground(Color.clear)
    }
}
