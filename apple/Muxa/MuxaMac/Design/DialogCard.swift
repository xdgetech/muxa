import SwiftUI

/// muxa's popup card on the Mac (ADR 0093): the title and lead line, the
/// choice rows or fields, and a footer under a hairline with the key hints,
/// Cancel (esc) and the action (⏎). ↑↓ pick a row; ⌘1… pick one directly.
struct DialogCard<Fields: View>: View {
    @Environment(\.dismiss) private var dismiss
    let title: String
    let message: String?
    let cancel: String
    let actions: [DialogAction]
    let choices: [DialogChoice]
    let fields: () -> Fields
    @State private var picked = 0
    @FocusState private var focused: Bool

    static var width: CGFloat { 400 }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            DialogHeader(title: title, lead: message)
            if !choices.isEmpty { rows }
            fields()
                .padding(.horizontal, 20)
                .padding(.bottom, Fields.self == EmptyView.self ? 0 : 12)
            DialogFooter(hint: choices.count > 1 ? "↑↓ select" : nil) {
                if onlyCancel {
                    // A notice: its one button is the action, and esc closes it too.
                    Button { close() } label: { ActionLabel(title: cancel, key: "⏎", primary: true) }
                        .buttonStyle(ActionButtonStyle(kind: .primary, compact: true))
                        .keyboardShortcut(.defaultAction)
                        .accessibilityIdentifier("dialog.cancel")
                } else {
                    Button { close() } label: { ActionLabel(title: cancel, key: "esc") }
                        .buttonStyle(ActionButtonStyle(kind: .secondary, compact: true))
                        .keyboardShortcut(.cancelAction)
                        .accessibilityIdentifier("dialog.cancel")
                }
                ForEach(Array(actions.enumerated()), id: \.element.id) { i, a in
                    let main = i == actions.count - 1 && choices.isEmpty
                    Button { close(then: a.perform) } label: {
                        ActionLabel(title: a.title, key: main ? "⏎" : nil, primary: main && a.role != .destructive)
                    }
                    .buttonStyle(ActionButtonStyle(kind: a.role == .destructive ? .destructive : main ? .primary : .secondary,
                                                   compact: true))
                    .keyboardShortcut(main ? .defaultAction : nil)
                    .disabled(a.disabled)
                    .accessibilityIdentifier(a.id)
                }
                if choices.indices.contains(picked) {
                    Button { close(then: choices[picked].perform) } label: {
                        ActionLabel(title: choices[picked].verb, key: "⏎", primary: true)
                    }
                    .buttonStyle(ActionButtonStyle(kind: .primary, compact: true))
                    .keyboardShortcut(.defaultAction)
                    .accessibilityIdentifier("dialog.primary")
                }
            }
        }
        .font(.system(size: 13))
        .foregroundStyle(Palette.text)
        .frame(width: Self.width)
        .background(Palette.surface)
        .focusable(!choices.isEmpty)
        .focusEffectDisabled()
        .focused($focused)
        .onKeyPress(.upArrow) { step(-1) }
        .onKeyPress(.downArrow) { step(1) }
        .onAppear { focused = !choices.isEmpty }
        .onExitCommand { close() }
    }

    private var onlyCancel: Bool { actions.isEmpty && choices.isEmpty }

    private var rows: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(choices.enumerated()), id: \.element.id) { i, c in
                if let section = c.section {
                    Text(section.uppercased())
                        .font(.system(size: 11, weight: .semibold))
                        .tracking(0.6)
                        .foregroundStyle(Palette.textTertiary)
                        .padding(.horizontal, 10)
                        .padding(.top, 8)
                        .padding(.bottom, 2)
                }
                row(c, at: i)
            }
        }
        .padding(.horizontal, 12)
        .padding(.bottom, 10)
    }

    private func row(_ c: DialogChoice, at i: Int) -> some View {
        let on = i == picked
        return Button { close(then: c.perform) } label: {
            HStack(spacing: 10) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(c.title).fontWeight(.medium).lineLimit(1)
                    if let detail = c.detail {
                        Text(detail).font(.system(size: 11.5)).foregroundStyle(Palette.textTertiary).lineLimit(1)
                    }
                }
                Spacer(minLength: 8)
                if i < 9 { KeyHint("⌘\(i + 1)", color: Palette.textSecondary) }
            }
            .padding(.horizontal, 10)
            .frame(minHeight: 44)
            .background(on ? Palette.rowSelected : .clear, in: RoundedRectangle(cornerRadius: 8))
            .overlay { if on { RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.accent) } }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .keyboardShortcut(i < 9 ? KeyboardShortcut(KeyEquivalent(Character("\(i + 1)")), modifiers: .command) : nil)
        .onHover { if $0 { picked = i } }
        .accessibilityAddTraits(on ? [.isButton, .isSelected] : .isButton)
        .accessibilityIdentifier(c.id)
    }

    private func step(_ by: Int) -> KeyPress.Result {
        guard !choices.isEmpty else { return .ignored }
        picked = (picked + by + choices.count) % choices.count
        return .handled
    }

    /// Closes the card first, so an action that opens another popup can.
    private func close(then action: (() -> Void)? = nil) {
        dismiss()
        if let action { DispatchQueue.main.async(execute: action) }
    }
}

/// The popups' title and lead line; sheets with forms use it too.
struct DialogHeader: View {
    let title: String
    var lead: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.system(size: 15, weight: .semibold)).accessibilityAddTraits(.isHeader)
            if let lead {
                Text(lead)
                    .foregroundStyle(Palette.textSecondary)
                    .lineSpacing(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 18)
        .padding(.bottom, 14)
    }
}

/// The popups' footer: a hairline, the key hints left, the buttons right.
struct DialogFooter<Buttons: View>: View {
    var hint: String?
    @ViewBuilder var buttons: Buttons

    var body: some View {
        HStack(spacing: 8) {
            if let hint { Text(hint).font(.system(size: 11.5)).foregroundStyle(Palette.textTertiary) }
            Spacer(minLength: 8)
            buttons
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 12)
        .overlay(alignment: .top) { Rectangle().fill(Palette.divider).frame(height: 1) }
    }
}
