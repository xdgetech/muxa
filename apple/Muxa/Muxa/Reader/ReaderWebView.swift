import SwiftUI
import WebKit

/// Shows one Markdown document with the bundled renderer (`reader.html`). The
/// page loads nothing else: its CSP allows no network, and every navigation but
/// the first is refused here. Links come back through `onLink`.
struct ReaderWebView: UIViewRepresentable {
    let text: String
    let path: String
    /// A link's `href` and the document it was in.
    var onLink: (_ href: String, _ from: String) -> Void

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.websiteDataStore = .nonPersistent()
        // A weak hop: the content controller keeps its handlers alive.
        config.userContentController.add(WeakHandler(context.coordinator), name: "reader")
        let view = WKWebView(frame: .zero, configuration: config)
        view.navigationDelegate = context.coordinator
        view.isOpaque = false
        view.backgroundColor = .systemBackground
        view.allowsLinkPreview = false
        view.accessibilityIdentifier = "reader"
        context.coordinator.load(into: view)
        return view
    }

    func updateUIView(_ view: WKWebView, context: Context) {
        context.coordinator.onLink = onLink
        context.coordinator.show(text: text, path: path)
    }

    static func dismantleUIView(_ view: WKWebView, coordinator: Coordinator) {
        view.configuration.userContentController.removeScriptMessageHandler(forName: "reader")
    }

    @MainActor
    final class Coordinator: NSObject, WKNavigationDelegate, WKScriptMessageHandler {
        var onLink: (String, String) -> Void = { _, _ in }
        private weak var view: WKWebView?
        private var page: URL?
        private var loaded = false
        private var wanted: (text: String, path: String)?
        private var shown: (text: String, path: String)?

        func load(into view: WKWebView) {
            self.view = view
            guard let page = Bundle.main.url(forResource: "reader", withExtension: "html") else { return }
            self.page = page
            view.loadFileURL(page, allowingReadAccessTo: page.deletingLastPathComponent())
        }

        func show(text: String, path: String) {
            wanted = (text, path)
            render()
        }

        private func render() {
            guard loaded, let view, let wanted, shown?.text != wanted.text || shown?.path != wanted.path else { return }
            // A new document starts at the top; an update keeps the reading position.
            let first = shown?.path != wanted.path
            shown = wanted
            guard let text = Self.jsonString(wanted.text), let path = Self.jsonString(wanted.path) else { return }
            view.evaluateJavaScript("render(\(text), \(path), \(first))") { _, _ in }
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

    private final class WeakHandler: NSObject, WKScriptMessageHandler {
        weak var target: (any WKScriptMessageHandler)?

        init(_ target: any WKScriptMessageHandler) { self.target = target }

        func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
            target?.userContentController(controller, didReceive: message)
        }
    }
}
