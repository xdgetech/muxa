import SwiftUI
import WebKit

/// Shows one HTML document and runs its scripts (ADR 0065). The document runs in a
/// sandboxed frame of `html.html`, in a web view with no message handler: it cannot
/// reach the app. The page's CSP and a content rule list keep it off the network.
struct HtmlWebView: UIViewRepresentable {
    let text: String
    let path: String

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.websiteDataStore = .nonPersistent()
        let view = ReaderPageView(frame: .zero, configuration: config)
        view.navigationDelegate = context.coordinator
        view.isOpaque = false
        view.backgroundColor = .white
        view.allowsLinkPreview = false
        view.accessibilityIdentifier = "reader-html"
        context.coordinator.load(into: view)
        return view
    }

    func updateUIView(_ view: WKWebView, context: Context) {
        context.coordinator.show(Shown(text: text, path: path))
    }

    struct Shown: Equatable {
        let text: String
        let path: String
    }

    @MainActor
    final class Coordinator: NSObject, WKNavigationDelegate {
        private weak var view: WKWebView?
        private var page: URL?
        private var loaded = false
        private var wanted: Shown?
        private var shown: Shown?

        /// Blocks every web request, behind the page's CSP. `url-filter` has no `|`.
        private static let rules = """
            [{"trigger": {"url-filter": "^https?://"}, "action": {"type": "block"}},
             {"trigger": {"url-filter": "^wss?://"}, "action": {"type": "block"}},
             {"trigger": {"url-filter": "^ftp://"}, "action": {"type": "block"}}]
            """

        func load(into view: WKWebView) {
            self.view = view
            guard let page = Bundle.main.url(forResource: "html", withExtension: "html") else { return }
            self.page = page
            WKContentRuleListStore.default().compileContentRuleList(
                forIdentifier: "muxa-reader-html", encodedContentRuleList: Self.rules
            ) { [weak view] list, error in
                MainActor.assumeIsolated {
                    guard let view else { return }
                    if let list {
                        view.configuration.userContentController.add(list)
                    } else {
                        // A constant list compiles; the CSP still blocks the network.
                        assertionFailure("reader rule list: \(String(describing: error))")
                    }
                    view.loadFileURL(page, allowingReadAccessTo: page.deletingLastPathComponent())
                }
            }
        }

        func show(_ content: Shown) {
            wanted = content
            render()
        }

        private func render() {
            guard loaded, let view, let wanted, shown != wanted else { return }
            shown = wanted
            guard let text = Self.jsonString(wanted.text) else { return }
            view.evaluateJavaScript("show(\(text))") { _, _ in }
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

        /// The host page once, then only its frame's documents (`about:srcdoc`).
        /// Links and scripted navigations go nowhere.
        func webView(
            _ webView: WKWebView, decidePolicyFor action: WKNavigationAction,
            decisionHandler: @escaping @MainActor (WKNavigationActionPolicy) -> Void
        ) {
            let url = action.request.url
            let host = url == page && !loaded && action.targetFrame?.isMainFrame == true
            let frame = url?.absoluteString == "about:srcdoc" && action.targetFrame?.isMainFrame == false
            decisionHandler(host || frame ? .allow : .cancel)
        }
    }
}
