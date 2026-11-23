import MuxaV2
import SwiftUI

/// Home's top bar as the desktop mock: the mark, the host, Home/Focus/Bots,
/// who needs you, New Task and the shortcuts sheet.
struct HomeHeader: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(MacChrome.self) private var chrome
    var openTerminal: (Terminal) -> Void
    var showFocus: () -> Void

    var body: some View {
        MacHeaderBar {
            HeaderBrand()
            ModeSwitch(mode: .home)
        } trailing: {
            NeedsYouPill(open: openTerminal, showAll: showFocus)
            HeaderButton(title: "New Task", systemImage: "plus", key: Shortcut.newTask.display) {
                nav.newTask = TaskSeed()
            }
            .disabled(!model.isConnected)
            .accessibilityIdentifier("home.newtask")
            HeaderButton(title: "Keys", key: Shortcut.shortcuts.display) { chrome.showShortcuts = true }
                .accessibilityIdentifier("home.keys")
        }
    }
}

/// Agents waiting on the user, as the amber header pill. The same choice as
/// `NeedsYouBanner`: one waiting Task opens, several open Focus.
private struct NeedsYouPill: View {
    @Environment(NextModel.self) private var model
    var open: (Terminal) -> Void
    var showAll: () -> Void

    var body: some View {
        let waiting = model.needsYou
        if let first = waiting.first {
            let one = waiting.count == 1
            Button {
                if one { open(first) } else { showAll() }
            } label: {
                HStack(spacing: 8) {
                    StateDot(color: TaskState.needsYou.color, size: 8)
                    Text(one ? "\(first.title) needs you" : "\(waiting.count) agents need you")
                        .lineLimit(1)
                    KeyHint(Shortcut.nextNeedsYou.display, color: Palette.textSecondary)
                }
                .font(.system(size: 13))
                .foregroundStyle(Palette.text)
                .padding(.horizontal, 12)
                .frame(height: 34)
                .background(Palette.bannerBack, in: RoundedRectangle(cornerRadius: 8))
                .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.bannerBorder))
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier(one ? "needsyou.\(first.id)" : "home.needsyou")
        }
    }
}
