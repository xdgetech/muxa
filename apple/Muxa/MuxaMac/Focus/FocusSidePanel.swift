import MuxaV2
import SwiftUI

/// Focus's right column, 392 pt: the Inbox, or the active Task's Reader or
/// Chat, one at a time. Esc inside it closes it; the terminal keeps its Esc.
struct FocusSidePanel: View {
    @Environment(NextNavigation.self) private var nav
    let terminal: MuxaV2.Terminal?

    static func isShown(nav: NextNavigation, terminal: MuxaV2.Terminal?) -> Bool {
        nav.showInbox || (terminal != nil && (nav.showReader || nav.showChat))
    }

    var body: some View {
        VStack(spacing: 0) {
            if nav.showReader, let t = terminal {
                header("Reader", lead: t.title)
                ReaderPanel(terminal: t).id(t.id)
            } else if nav.showChat, let t = terminal {
                header("Chat", lead: t.title)
                ChatPanel(terminal: t).id(t.id)
            } else {
                header("Inbox", lead: "things for you to do")
                InboxPanel()
            }
        }
        .frame(width: 392)
        .frame(maxHeight: .infinity)
        .background(Palette.surface)
        .overlay(alignment: .leading) { Rectangle().fill(Palette.border).frame(width: 1) }
        .onExitCommand(perform: close)
    }

    private func header(_ title: String, lead: String) -> some View {
        HStack(spacing: 8) {
            Text(title)
                .font(.system(size: 15, weight: .semibold))
                .accessibilityAddTraits(.isHeader)
            Spacer(minLength: 8)
            Text(lead).font(.system(size: 12)).foregroundStyle(Palette.textTertiary).lineLimit(1)
            Button(action: close) {
                Text("esc")
                    .font(.system(size: 11, weight: .medium, design: .monospaced))
                    .foregroundStyle(Palette.textSecondary)
                    .padding(.horizontal, 8)
                    .frame(height: 28)
                    .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Palette.borderStrong))
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Close \(title)")
            .accessibilityIdentifier("panel.close")
        }
        .padding(.horizontal, 14)
        .padding(.top, 12)
        .padding(.bottom, 8)
    }

    private func close() {
        nav.showInbox = false
        nav.showReader = false
        nav.showChat = false
    }
}
