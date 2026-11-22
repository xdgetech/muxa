import MuxaV2
import SwiftUI

/// Home's top bar as the desktop mock: the mark, the host, Home/Focus, who
/// needs you, Routines, New Task and the shortcuts sheet.
struct HomeHeader: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(MacChrome.self) private var chrome
    var openTerminal: (Terminal) -> Void
    var showFocus: () -> Void

    var body: some View {
        MacHeaderBar {
            HStack(spacing: 8) {
                LogoTile()
                Text("muxa").font(.system(size: 13, weight: .semibold))
                HostMenu()
                    .menuStyle(.button)
                    .buttonStyle(.plain)
                    .menuIndicator(.hidden)
                    .fixedSize()
            }
            ModeSwitch(focus: false, home: {}, showFocus: showFocus)
        } trailing: {
            NeedsYouPill(open: openTerminal, showAll: showFocus)
            RoutinesButton()
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
                    Text(one ? "\(first.title) needs you" : "\(waiting.count) Tasks need you")
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

/// Opens the Routines page; says how many Routines failed their last run.
private struct RoutinesButton: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav

    var body: some View {
        let failed = model.routines.filter { $0.last?.ok == false }.count
        Button { nav.tab = .routines } label: {
            HStack(spacing: 8) {
                Image(systemName: "clock").font(.system(size: 12, weight: .semibold))
                Text("Routines")
                if failed > 0 { Text("\(failed) failed").foregroundStyle(Palette.late) }
            }
            .font(.system(size: 13))
            .foregroundStyle(Palette.textMid)
            .padding(.horizontal, 12)
            .frame(height: 34)
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.borderStrong))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Routines (\(Shortcut.routines.display))")
        .accessibilityIdentifier("home.routines")
        .task { await model.refreshRoutines() }
    }
}
