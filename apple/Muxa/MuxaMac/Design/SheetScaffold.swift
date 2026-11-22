import SwiftUI

/// The mocks' sheet: a fixed width, a title and a lead line, the content,
/// and Cancel and the primary action at the bottom right.
struct SheetScaffold<Content: View>: View {
    let title: String
    var lead: String?
    var width: CGFloat = 520
    let primary: String
    var primaryKind = ActionButtonStyle.Kind.primary
    var primaryDisabled = false
    var primaryID: String?
    let cancel: () -> Void
    let confirm: () -> Void
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 6) {
                Text(title).font(.system(size: 17, weight: .bold))
                if let lead {
                    Text(lead)
                        .foregroundStyle(Palette.textSecondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .padding(.horizontal, 22)
            .padding(.top, 18)
            .padding(.bottom, 6)
            content
                .padding(.horizontal, 22)
                .padding(.vertical, 10)
            HStack(spacing: 8) {
                Spacer()
                Button(action: cancel) { ActionLabel(title: "Cancel", key: "esc") }
                    .buttonStyle(ActionButtonStyle(kind: .secondary, compact: true))
                    .keyboardShortcut(.cancelAction)
                Button(action: confirm) { ActionLabel(title: primary, key: "⏎", primary: primaryKind == .primary) }
                    .buttonStyle(ActionButtonStyle(kind: primaryKind, compact: true))
                    .keyboardShortcut(.defaultAction)
                    .disabled(primaryDisabled)
                    .accessibilityIdentifier(primaryID ?? "sheet.primary")
            }
            .padding(.horizontal, 22)
            .padding(.top, 16)
            .padding(.bottom, 18)
        }
        .font(.system(size: 13))
        .foregroundStyle(Palette.text)
        .frame(width: width)
        .background(Palette.surface)
    }
}

/// A form row with a 110 pt label column, for a card of rows.
struct FieldRow<Field: View>: View {
    let label: String
    var divider = true
    @ViewBuilder var field: Field

    var body: some View {
        HStack(spacing: 12) {
            Text(label).foregroundStyle(Palette.textSecondary).frame(width: 110, alignment: .leading)
            field.textFieldStyle(.plain).frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 14)
        .frame(minHeight: 44)
        .overlay(alignment: .bottom) {
            if divider { Rectangle().fill(Palette.cardDivider).frame(height: 1) }
        }
    }
}
