import MuxaV2
import SwiftUI

/// The Mac's Focus (ADR 0066 decision 1, ADR 0070), as the `Main` desktop
/// mock: a header with the Workspace's Tasks as tabs, the sidebar, the panes
/// with their key legend, and one panel (Inbox, Reader or Chat) on the right.
struct FocusView: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Binding var selected: String?

    /// The Task whose loop sheet is open.
    @State private var loopFor: MuxaV2.Terminal?

    private var active: MuxaV2.Terminal? {
        model.terminals.first { $0.id == selected }
    }

    var body: some View {
        VStack(spacing: 0) {
            FocusHeader(active: active)
            HStack(spacing: 0) {
                if nav.sidebar != .detailOnly {
                    FocusSidebar(selected: $selected)
                }
                VStack(spacing: 0) {
                    if active != nil {
                        PaneSplit { loopFor = $0 }
                    } else {
                        ContentUnavailableView("Select a Task", systemImage: "terminal")
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                    KeyLegend(items: Self.legend)
                }
                .frame(maxWidth: .infinity)
                .background(Palette.ground)
                if nav.showInbox {
                    FocusSidePanel()
                }
            }
        }
        .font(.system(size: 13))
        .foregroundStyle(Palette.text)
        .background(Palette.ground)
        .overlay {
            if nav.showReader, let t = active { ReaderOverlay(terminal: t) }
        }
        .modifier(CloseTaskConfirm())
        .sheet(item: $loopFor) { t in LoopSheet(terminal: t) }
        .onAppear(perform: openFirstTask)
    }

    /// Focus always shows a Task when there is one: the first that needs the
    /// user, else the first live one.
    private func openFirstTask() {
        guard selected == nil, let first = (model.focusNeedsYou + model.focusTasks).first else { return }
        selected = first.id
    }

    private static var legend: [(key: String, label: String)] {
        [
            (Shortcut.goToTask.display, "go to Task"),
            (Shortcut.split.display, "split"),
            (Shortcut.pane.display, "pane"),
            (Shortcut.nextTask.display, "next Task"),
            (Shortcut.nextNeedsYou.display, "next waiting"),
            (Shortcut.sidebar.display, "sidebar"),
            (Shortcut.capture.display, "capture"),
        ].filter { !$0.0.isEmpty }
    }
}
