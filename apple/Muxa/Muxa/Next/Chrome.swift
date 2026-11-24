import MuxaV2
import SwiftUI

/// The mocks' controls: segments, icon buttons, chips, dots and cards.

/// Touch targets are at least this tall (Apple's HIG).
let minTouch: CGFloat = 44

struct Segment<Value: Hashable>: Identifiable {
    let value: Value
    let title: String
    /// The accessibility identifier.
    let id: String
    var enabled = true
    /// `⌘⇧key` on iOS.
    var key: KeyEquivalent?
}

/// The mocks' segmented control: buttons, so each segment has its own identifier.
struct SegmentBar<Value: Hashable>: View {
    let segments: [Segment<Value>]
    @Binding var selection: Value
    var ground = Palette.surface

    var body: some View {
        HStack(spacing: 4) {
            ForEach(segments) { s in
                let picked = s.value == selection
                Button(s.title) { selection = s.value }
                    .buttonStyle(SegmentStyle(picked: picked))
                    .disabled(!s.enabled)
                    .shortcut(s.key)
                    .accessibilityIdentifier(s.id)
                    .accessibilityAddTraits(picked ? .isSelected : [])
            }
        }
        .padding(3)
        .background(ground, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Palette.border))
    }
}

private struct SegmentStyle: ButtonStyle {
    let picked: Bool
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: picked ? .semibold : .medium))
            .lineLimit(1)
            .minimumScaleFactor(0.75)
            .foregroundStyle(picked ? Palette.text : Palette.textSecondary)
            .frame(maxWidth: .infinity, minHeight: 38)
            .background(picked ? Palette.selected : .clear, in: RoundedRectangle(cornerRadius: 8))
            .contentShape(Rectangle())
            .opacity(enabled ? (configuration.isPressed ? 0.6 : 1) : 0.35)
    }
}

extension View {
    @ViewBuilder
    fileprivate func shortcut(_ key: KeyEquivalent?) -> some View {
        if let key { touchShortcut(key) } else { self }
    }

    /// A rounded card on the ground, as the mocks' rows and sections.
    func card(radius: CGFloat = 14, border: Bool = false) -> some View {
        background(Palette.card, in: RoundedRectangle(cornerRadius: radius))
            .overlay {
                if border { RoundedRectangle(cornerRadius: radius).stroke(Palette.border) }
            }
    }

    /// The mocks' text field: a dark well with a thin border.
    func fieldWell(radius: CGFloat = 12, ground: Color = Palette.surface) -> some View {
        padding(.horizontal, 12)
            .frame(minHeight: minTouch)
            .background(ground, in: RoundedRectangle(cornerRadius: radius))
            .overlay(RoundedRectangle(cornerRadius: radius).stroke(Palette.borderStrong))
    }
}

/// A 44 pt button with only a symbol; `title` is what VoiceOver says.
struct IconButton: View {
    let title: String
    let symbol: String
    var round = false
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 18, weight: .medium))
        }
        .buttonStyle(RaisedIconStyle(round: round))
        .accessibilityLabel(title)
    }
}

/// The mocks' "+" button: a raised square or circle with a border.
struct RaisedIconStyle: ButtonStyle {
    var round = false

    func makeBody(configuration: Configuration) -> some View {
        let shape = RoundedRectangle(cornerRadius: round ? minTouch / 2 : 12)
        return configuration.label
            .foregroundStyle(Palette.text)
            .frame(minWidth: minTouch, minHeight: minTouch)
            .background(Palette.raised, in: shape)
            .overlay(shape.stroke(Palette.borderStrong))
            .contentShape(shape)
            .opacity(configuration.isPressed ? 0.6 : 1)
    }
}

/// A symbol without a background, at a 44 pt target: the back chevron, Loop, Inbox.
struct BareIconStyle: ButtonStyle {
    var color = Palette.accent
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 18, weight: .semibold))
            .foregroundStyle(color)
            .frame(minWidth: minTouch, minHeight: minTouch)
            .contentShape(Rectangle())
            .opacity(enabled ? (configuration.isPressed ? 0.5 : 1) : 0.35)
    }
}

/// A filled button: the accent, or amber for Approve.
struct FilledButtonStyle: ButtonStyle {
    var fill = Palette.accent
    var text = Palette.ground
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 14, weight: .semibold))
            .foregroundStyle(text)
            .padding(.horizontal, 16)
            .frame(minHeight: minTouch)
            .background(fill, in: RoundedRectangle(cornerRadius: 10))
            .opacity(enabled ? (configuration.isPressed ? 0.7 : 1) : 0.4)
    }
}

/// A quiet button beside a filled one.
struct QuietButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 14, weight: .medium))
            .foregroundStyle(Palette.text)
            .padding(.horizontal, 14)
            .frame(minHeight: minTouch)
            .background(Palette.rowSelected, in: RoundedRectangle(cornerRadius: 10))
            .overlay(RoundedRectangle(cornerRadius: 10).stroke(Palette.buttonBorder))
            .opacity(enabled ? (configuration.isPressed ? 0.6 : 1) : 0.4)
    }
}

/// The round send button at the end of an input line.
struct SendButton: View {
    var title = "Send"
    var symbol = "arrow.up"
    var disabled = false
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 18, weight: .bold))
                .foregroundStyle(Palette.ground)
                .frame(width: minTouch, height: minTouch)
                .background(Palette.accent, in: Circle())
        }
        .buttonStyle(.plain)
        .disabled(disabled)
        .opacity(disabled ? 0.4 : 1)
        .accessibilityLabel(title)
    }
}

struct StateDot: View {
    let color: Color
    var size: CGFloat = 10

    var body: some View {
        Circle().fill(color).frame(width: size, height: size).accessibilityHidden(true)
    }
}

/// The kind as a colored chip of one width, so the titles line up.
struct KindChip: View {
    let kind: ItemKind

    var body: some View {
        let colors = kind.chipColors
        Text(kind.title)
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(colors.text)
            .frame(width: 56)
            .padding(.vertical, 3)
            .background(colors.back, in: RoundedRectangle(cornerRadius: 6))
    }
}

/// A group's title in a list: small capitals in the group's color, and a count.
struct GroupHeader: View {
    let title: String
    var color = Palette.textTertiary
    var count: Int?

    var body: some View {
        HStack(spacing: 6) {
            Text(title.uppercased()).foregroundStyle(color)
            if let count { Text("\(count)").foregroundStyle(Palette.textTertiary) }
            Spacer()
        }
        .font(.system(size: 11.5, weight: .semibold))
        .tracking(0.7)
        .padding(.horizontal, 6)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}

/// A Form on the mocks' colors: card rows on the ground, also as a sheet.
struct PaletteForm<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        Form { Group { content }.listRowBackground(Palette.card) }
            #if os(macOS)
            .formStyle(.grouped)
            #endif
            .groundList()
            .presentationBackground(Palette.ground)
    }
}
