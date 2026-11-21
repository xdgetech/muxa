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
