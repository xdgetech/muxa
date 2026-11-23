import SwiftUI

/// The mocks' tab bar: titles with a 2 pt underline under the picked one.
struct UnderlineTabs<Tag: Hashable>: View {
    let tabs: [(title: String, tag: Tag, id: String)]
    @Binding var selection: Tag

    var body: some View {
        HStack(spacing: 2) {
            ForEach(tabs, id: \.id) { tab in
                let on = tab.tag == selection
                Button { selection = tab.tag } label: {
                    Text(tab.title)
                        .font(.system(size: 13, weight: on ? .semibold : .medium))
                        .foregroundStyle(on ? Palette.text : Palette.textSecondary)
                        .padding(.horizontal, 14)
                        .frame(height: 34)
                        .overlay(alignment: .bottom) {
                            Rectangle().fill(on ? Palette.text : .clear).frame(height: 2)
                        }
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier(tab.id)
                .accessibilityAddTraits(on ? .isSelected : [])
            }
            Spacer(minLength: 0)
        }
        .overlay(alignment: .bottom) { Rectangle().fill(Palette.border).frame(height: 1) }
    }
}

/// The Home/Focus switch of the header: two segments with their keys.
struct ModeSwitch: View {
    let focus: Bool
    let home: () -> Void
    let showFocus: () -> Void

    var body: some View {
        HStack(spacing: 4) {
            segment("Home", key: Shortcut.home.display, on: !focus, id: "mode.home", action: home)
            segment("Focus", key: Shortcut.focus.display, on: focus, id: "mode.focus", action: showFocus)
        }
        .padding(3)
        .frame(width: 240)
        .background(Palette.ground, in: RoundedRectangle(cornerRadius: 9))
        .overlay(RoundedRectangle(cornerRadius: 9).strokeBorder(Palette.border))
    }

    private func segment(_ title: String, key: String, on: Bool, id: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Text(title).fontWeight(on ? .semibold : .regular)
                KeyHint(key)
            }
            .font(.system(size: 13))
            .foregroundStyle(on ? Palette.text : Palette.textSecondary)
            .frame(maxWidth: .infinity, minHeight: 30)
            .background(on ? Palette.selected : .clear, in: RoundedRectangle(cornerRadius: 7))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(id)
    }
}
