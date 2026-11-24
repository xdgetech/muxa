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

    /// Shift+Return puts a new line in a vertical field; Return still submits.
    /// The Mac's field editor treats both as submit, so the key inserts the
    /// line itself, at the cursor.
    func newlineOnShiftReturn() -> some View {
        #if os(macOS)
        onKeyPress(.return, phases: .down) { press in
            guard press.modifiers.contains(.shift) else { return .ignored }
            // The field editor of the field with the keyboard; the key window first.
            let windows = [NSApp.keyWindow].compactMap { $0 } + NSApp.windows
            guard let editor = windows.lazy.compactMap({ $0.firstResponder as? NSTextView }).first else { return .ignored }
            editor.insertText("\n", replacementRange: editor.selectedRange())
            return .handled
        }
        #else
        self
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
