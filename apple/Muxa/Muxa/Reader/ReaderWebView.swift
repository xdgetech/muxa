#if canImport(UIKit)
import MuxaUI
#endif
import MuxaSession
import SwiftUI
import WebKit

/// Shows one Markdown document, or one unified diff, with the bundled renderer (`reader.html`). The
/// page loads nothing else: its CSP allows no network, and every navigation but
/// the first is refused here. Links come back through `onLink`.
struct ReaderWebView: WebRepresentable {
    enum Kind: Equatable { case markdown, diff }

    let text: String
    let path: String
    var kind: Kind = .markdown
    /// A link's `href` and the document it was in.
    var onLink: (_ href: String, _ from: String) -> Void

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeWebView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.websiteDataStore = .nonPersistent()
        // A weak hop: the content controller keeps its handlers alive.
        config.userContentController.add(WeakHandler(context.coordinator), name: "reader")
        // The theme is in place before reader.js runs, so the first paint has it (ADR 0091).
        let theme = ThemeStore.shared.theme
        config.userContentController.addUserScript(WKUserScript(
            source: "window.muxaTheme = \(Self.themeJSON(theme));", injectionTime: .atDocumentStart, forMainFrameOnly: true))
        context.coordinator.applied = theme
        let view = ReaderPageView(frame: .zero, configuration: config)
        view.navigationDelegate = context.coordinator
        view.readerLook(identifier: "reader")
        context.coordinator.load(into: view)
        return view
    }

    func updateWebView(_ view: WKWebView, context: Context) {
        context.coordinator.onLink = onLink
        context.coordinator.show(Shown(text: text, path: path, kind: kind))
        context.coordinator.apply(ThemeStore.shared.theme)
    }

    static func dismantleWebView(_ view: WKWebView, coordinator: Coordinator) {
        view.configuration.userContentController.removeScriptMessageHandler(forName: "reader")
    }

    @MainActor
    final class Coordinator: NSObject, WKNavigationDelegate, WKScriptMessageHandler {
        var onLink: (String, String) -> Void = { _, _ in }
        private weak var view: WKWebView?
        private var page: URL?
        private var loaded = false
        private var wanted: Shown?
        private var shown: Shown?
        /// The theme the page has; a different one is sent with `setTheme`.
        var applied: ResolvedTheme?
        private var wantedTheme: ResolvedTheme?

        func load(into view: WKWebView) {
            self.view = view
            guard let page = Bundle.main.url(forResource: "reader", withExtension: "html") else { return }
            self.page = page
            view.loadFileURL(page, allowingReadAccessTo: page.deletingLastPathComponent())
        }

        func apply(_ theme: ResolvedTheme) {
            wantedTheme = theme
            sendTheme()
        }

        private func sendTheme() {
            guard loaded, let view, let theme = wantedTheme, theme != applied else { return }
            applied = theme
            view.evaluateJavaScript("setTheme(\(ReaderWebView.themeJSON(theme)))") { _, error in
                if let error { NSLog("muxa: the Reader did not take the theme: %@", error.localizedDescription) }
            }
            #if canImport(UIKit)
            view.backgroundColor = UIColor(Color(theme[.ground]))
            #endif
        }

        func show(_ content: Shown) {
            wanted = content
            render()
        }

        private func render() {
            guard loaded, let view, let wanted, shown != wanted else { return }
            // A new document starts at the top; an update keeps the reading position.
            let first = shown?.path != wanted.path || shown?.kind != wanted.kind
            shown = wanted
            guard let text = Self.jsonString(wanted.text), let path = Self.jsonString(wanted.path) else { return }
            let function = wanted.kind == .diff ? "renderDiff" : "render"
            view.evaluateJavaScript("\(function)(\(text), \(path), \(first))") { _, _ in }
            #if canImport(UIKit)
            // Keys reach only the first responder, and nothing else makes the page one.
            // Without a hardware keyboard nothing needs it.
            if first, KeyInputView.focusesByItself, view.window != nil { view.becomeFirstResponder() }
            #endif
        }

        /// A JSON string literal is a valid JavaScript string literal.
        private static func jsonString(_ s: String) -> String? {
            guard let data = try? JSONSerialization.data(withJSONObject: [s]),
                let array = String(data: data, encoding: .utf8)
            else { return nil }
            return String(array.dropFirst().dropLast())
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            loaded = true
            sendTheme()
            render()
        }

        func webView(
            _ webView: WKWebView, decidePolicyFor action: WKNavigationAction,
            decisionHandler: @escaping @MainActor (WKNavigationActionPolicy) -> Void
        ) {
            decisionHandler(action.request.url == page && !loaded ? .allow : .cancel)
        }

        func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
            guard let body = message.body as? [String: Any], body["type"] as? String == "link",
                let href = body["href"] as? String
            else { return }
            onLink(href, body["from"] as? String ?? "")
        }
    }

    /// Every token as `#rrggbb`, and the appearance: what reader.js reads.
    static func themeJSON(_ theme: ResolvedTheme) -> String {
        var values = Dictionary(uniqueKeysWithValues: ThemeToken.allCases.map { ($0.rawValue, theme[$0].hex) })
        values["appearance"] = theme.appearance.rawValue
        guard let data = try? JSONSerialization.data(withJSONObject: values, options: [.sortedKeys]),
              let json = String(data: data, encoding: .utf8)
        else { return "{}" }
        return json
    }

    struct Shown: Equatable {
        let text: String
        let path: String
        let kind: Kind
    }

    private final class WeakHandler: NSObject, WKScriptMessageHandler {
        weak var target: (any WKScriptMessageHandler)?

        init(_ target: any WKScriptMessageHandler) { self.target = target }

        func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
            target?.userContentController(controller, didReceive: message)
        }
    }
}
