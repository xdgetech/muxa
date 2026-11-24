import SwiftUI
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

/// The few places where the iOS and the Mac app differ in SwiftUI API.
extension View {
    /// A small title in the bar on iOS; the Mac has one title style.
    func inlineTitle() -> some View {
        #if os(iOS)
        navigationBarTitleDisplayMode(.inline)
        #else
        self
        #endif
    }

    /// A field for a host name, a user or a command: no capitals, no corrections.
    func plainEntry(url: Bool = false) -> some View {
        #if os(iOS)
        keyboardType(url ? .URL : .default).textInputAutocapitalization(.never).autocorrectionDisabled()
        #else
        autocorrectionDisabled()
        #endif
    }

    /// `⌘⇧key` on iOS. The Mac gives the command a menu item with this
    /// shortcut instead, and two of one shortcut would clash.
    func touchShortcut(_ key: KeyEquivalent) -> some View {
        #if os(iOS)
        keyboardShortcut(key, modifiers: [.command, .shift])
        #else
        self
        #endif
    }

    /// A vertical chat field: Return sends, Shift+Return puts a new line in it.
    /// The Mac's field editor treats both keys as submit, so Shift+Return
    /// inserts the line itself. iOS takes Return in a vertical field as a new
    /// line and never submits, so one typed new line sends there; a pasted
    /// text keeps its lines.
    func returnSends(_ text: Binding<String>, send: @escaping () -> Void) -> some View {
        #if os(macOS)
        onSubmit(send).onKeyPress(.return, phases: .down) { press in
            guard press.modifiers.contains(.shift) else { return .ignored }
            // The field editor of the field with the keyboard; the key window first.
            let windows = [NSApp.keyWindow].compactMap { $0 } + NSApp.windows
            guard let editor = windows.lazy.compactMap({ $0.firstResponder as? NSTextView }).first else { return .ignored }
            editor.insertText("\n", replacementRange: editor.selectedRange())
            return .handled
        }
        #else
        onSubmit(send).onChange(of: text.wrappedValue) { old, new in
            guard let typed = withoutTypedReturn(old: old, new: new) else { return }
            text.wrappedValue = typed
            send()
        }
        #endif
    }

    func numberEntry() -> some View {
        #if os(iOS)
        keyboardType(.numberPad)
        #else
        self
        #endif
    }
}

extension ToolbarItemPlacement {
    /// The bar's leading edge: the top bar on iOS, the navigation area on the Mac.
    static var leadingBar: Self {
        #if os(iOS)
        .topBarLeading
        #else
        .navigation
        #endif
    }
}

enum Clipboard {
    static func copy(_ text: String) {
        #if canImport(UIKit)
        UIPasteboard.general.string = text
        #else
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        #endif
    }
}

extension Image {
    /// An image from encoded bytes (PNG, JPEG…), or nil.
    init?(data: Data) {
        #if canImport(UIKit)
        guard let image = UIImage(data: data) else { return nil }
        self.init(uiImage: image)
        #else
        guard let image = NSImage(data: data) else { return nil }
        self.init(nsImage: image)
        #endif
    }
}

/// `new` without the one new line typed into `old`; nil for any other edit.
func withoutTypedReturn(old: String, new: String) -> String? {
    guard new.count == old.count + 1 else { return nil }
    let at = zip(old, new).prefix { $0 == $1 }.count
    let i = new.index(new.startIndex, offsetBy: at)
    guard new[i] == "\n" else { return nil }
    var rest = new
    rest.remove(at: i)
    return rest == old ? rest : nil
}
