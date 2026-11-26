import SwiftUI

/// A button of a popup. The last one is the action ⏎ runs.
struct DialogAction: Identifiable {
    let title: String
    var role: ButtonRole?
    var id: String
    var disabled = false
    let perform: () -> Void

    init(_ title: String, role: ButtonRole? = nil, id: String? = nil, disabled: Bool = false, perform: @escaping () -> Void) {
        self.title = title
        self.role = role
        self.id = id ?? title
        self.disabled = disabled
        self.perform = perform
    }
}

/// A row of a popup that offers a choice, such as which chat to open.
struct DialogChoice: Identifiable {
    let id: String
    let title: String
    var detail: String?
    /// Rows with a section start a group under its title.
    var section: String?
    /// The footer's action while this row is picked.
    let verb: String
    let perform: () -> Void
}

extension View {
    /// The app's one popup (ADR 0093): a title, a lead line, optional rows or
    /// fields, and Cancel and the actions. The Mac draws muxa's card; iOS
    /// keeps the system's alert, or its action sheet for `choices` or when
    /// `actionSheet` is set.
    func dialog<Fields: View>(
        _ title: String,
        isPresented: Binding<Bool>,
        message: String? = nil,
        actionSheet: Bool = false,
        cancel: String = "Cancel",
        actions: [DialogAction] = [],
        choices: [DialogChoice] = [],
        @ViewBuilder fields: @escaping () -> Fields = { EmptyView() }
    ) -> some View {
        #if os(macOS)
        sheet(isPresented: isPresented) {
            DialogCard(title: title, message: message, cancel: cancel, actions: actions, choices: choices, fields: fields)
        }
        #else
        modifier(SystemDialog(title: title, isPresented: isPresented, message: message, actionSheet: actionSheet, cancel: cancel,
                              actions: actions, choices: choices, fields: fields))
        #endif
    }
}

#if !os(macOS)
private struct SystemDialog<Fields: View>: ViewModifier {
    let title: String
    @Binding var isPresented: Bool
    let message: String?
    let actionSheet: Bool
    let cancel: String
    let actions: [DialogAction]
    let choices: [DialogChoice]
    let fields: () -> Fields

    func body(content: Content) -> some View {
        if choices.isEmpty && !actionSheet {
            content.alert(title, isPresented: $isPresented) { buttons } message: { message.map(Text.init) }
        } else {
            content.confirmationDialog(title, isPresented: $isPresented, titleVisibility: .visible) {
                buttons
            } message: { message.map(Text.init) }
        }
    }

    @ViewBuilder
    private var buttons: some View {
        fields()
        ForEach(choices) { c in
            Button(c.verb == c.title ? c.title : "\(c.verb) \(c.title)", action: c.perform).accessibilityIdentifier(c.id)
        }
        ForEach(actions) { a in
            Button(a.title, role: a.role, action: a.perform).disabled(a.disabled).accessibilityIdentifier(a.id)
        }
        Button(cancel, role: .cancel) {}
    }
}
#endif
