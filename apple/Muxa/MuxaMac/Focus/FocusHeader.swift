import MuxaV2
import SwiftUI

/// Focus's top bar: the same start and Home/Focus switch as Home, the Tasks
/// as tabs, Go to Task and the Inbox panel's button.
struct FocusHeader: View {
    let active: MuxaV2.Terminal?

    var body: some View {
        MacHeaderBar {
            HStack(spacing: 16) {
                HeaderBrand()
                ModeSwitch(mode: .focus)
                FocusTabStrip(active: active)
            }
            .frame(maxHeight: .infinity)
        } trailing: {
            HStack(spacing: 10) {
                GoToTaskButton()
                InboxButton()
            }
        }
        .font(.system(size: 13))
    }
}

/// Opens the Go to Task palette; drawn as the mock's search field.
private struct GoToTaskButton: View {
    @Environment(NextNavigation.self) private var nav

    var body: some View {
        Button { nav.showPalette = true } label: {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass").font(.system(size: 12, weight: .semibold))
                Text("Go to Task")
                Spacer(minLength: 0)
                KeyHint(Shortcut.goToTask.display)
            }
            .foregroundStyle(Palette.textTertiary)
            .padding(.horizontal, 10)
            .frame(width: 220, height: 34)
            .background(Palette.ground, in: RoundedRectangle(cornerRadius: 8))
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.border))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("focus.goto")
    }
}

/// Shows and hides the Inbox panel, with the count of open items.
private struct InboxButton: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav

    private var count: Int {
        let now = Int64(Date().timeIntervalSince1970)
        return model.inbox.items.filter { !$0.isLater(now: now) }.count
    }

    var body: some View {
        let on = nav.showInbox
        Button { nav.showInbox.toggle() } label: {
            HStack(spacing: 8) {
                Image(systemName: "tray").font(.system(size: 12, weight: .semibold))
                Text("Inbox")
                if count > 0 {
                    Text("\(count)")
                        .font(.system(size: 11, weight: .bold))
                        .padding(.horizontal, 6)
                        .background(Palette.rowBorder, in: Capsule())
                }
                KeyHint(Shortcut.inboxPanel.display)
            }
            .font(.system(size: 13, weight: .medium))
            .foregroundStyle(Palette.text)
            // The tab strip gives way when the header is short; this button never does.
            .fixedSize()
            .padding(.horizontal, 12)
            .frame(height: 34)
            .background(on ? Palette.rowSelected : .clear, in: RoundedRectangle(cornerRadius: 8))
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(on ? Palette.rowBorder : Palette.borderStrong))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("focus.inbox")
        .accessibilityAddTraits(on ? .isSelected : [])
    }
}
