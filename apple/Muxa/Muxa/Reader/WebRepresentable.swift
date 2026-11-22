import SwiftUI
import WebKit

/// A SwiftUI wrapper of a `WKWebView` on iOS and the Mac: the two
/// representable protocols differ only in their method names.
#if canImport(UIKit)
protocol WebRepresentable: UIViewRepresentable where UIViewType == WKWebView {
    func makeWebView(context: Context) -> WKWebView
    func updateWebView(_ view: WKWebView, context: Context)
    static func dismantleWebView(_ view: WKWebView, coordinator: Coordinator)
}

extension WebRepresentable {
    func makeUIView(context: Context) -> WKWebView { makeWebView(context: context) }
    func updateUIView(_ view: WKWebView, context: Context) { updateWebView(view, context: context) }
    static func dismantleUIView(_ view: WKWebView, coordinator: Coordinator) {
        dismantleWebView(view, coordinator: coordinator)
    }
    static func dismantleWebView(_ view: WKWebView, coordinator: Coordinator) {}
}
#else
protocol WebRepresentable: NSViewRepresentable where NSViewType == WKWebView {
    func makeWebView(context: Context) -> WKWebView
    func updateWebView(_ view: WKWebView, context: Context)
    static func dismantleWebView(_ view: WKWebView, coordinator: Coordinator)
}

extension WebRepresentable {
    func makeNSView(context: Context) -> WKWebView { makeWebView(context: context) }
    func updateNSView(_ view: WKWebView, context: Context) { updateWebView(view, context: context) }
    static func dismantleNSView(_ view: WKWebView, coordinator: Coordinator) {
        dismantleWebView(view, coordinator: coordinator)
    }
    static func dismantleWebView(_ view: WKWebView, coordinator: Coordinator) {}
}
#endif

extension WKWebView {
    /// The look the Reader's pages share: no link previews, and the page's
    /// own background behind a transparent view on iOS.
    func readerLook(identifier: String, white: Bool = false) {
        allowsLinkPreview = false
        #if canImport(UIKit)
        isOpaque = false
        backgroundColor = white ? .white : UIColor(Palette.ground)
        accessibilityIdentifier = identifier
        #else
        setAccessibilityIdentifier(identifier)
        #endif
    }
}
