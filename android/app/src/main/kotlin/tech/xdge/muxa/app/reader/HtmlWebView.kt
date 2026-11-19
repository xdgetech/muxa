package tech.xdge.muxa.app.reader

import android.annotation.SuppressLint
import android.graphics.Color
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.viewinterop.AndroidView
import androidx.webkit.WebViewAssetLoader
import kotlinx.serialization.json.JsonPrimitive

/**
 * Shows one HTML document and runs its scripts (ADR 0065). The document runs in a
 * sandboxed frame of `html.html` (shared with iOS), in a web view with no JavaScript
 * interface: it cannot reach the app. Network loads are blocked, and so is the page's CSP.
 */
@Composable
fun HtmlWebView(text: String, modifier: Modifier = Modifier) {
    val state = remember { HtmlPage() }
    AndroidView(
        factory = { context -> state.create(context) },
        update = { state.show(text) },
        modifier = modifier.testTag("reader-html"),
    )
}

private class HtmlPage {
    private var view: WebView? = null
    private var loaded = false
    private var wanted: String? = null
    private var shown: String? = null

    @SuppressLint("SetJavaScriptEnabled")
    fun create(context: android.content.Context): WebView {
        val assets = WebViewAssetLoader.Builder()
            .addPathHandler("/assets/", WebViewAssetLoader.AssetsPathHandler(context))
            .build()
        return WebView(context).apply {
            setBackgroundColor(Color.WHITE)
            settings.javaScriptEnabled = true
            settings.blockNetworkLoads = true
            settings.allowFileAccess = false
            settings.allowContentAccess = false
            settings.domStorageEnabled = false
            webViewClient = object : WebViewClient() {
                override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? =
                    assets.shouldInterceptRequest(request.url)

                override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest) = true

                // A renderer that crashed or was killed for memory takes only this page down.
                override fun onRenderProcessGone(view: WebView, detail: android.webkit.RenderProcessGoneDetail): Boolean {
                    (view.parent as? android.view.ViewGroup)?.removeView(view)
                    view.destroy()
                    this@HtmlPage.view = null
                    return true
                }

                override fun onPageFinished(view: WebView, url: String) {
                    loaded = true
                    render()
                }
            }
            loadUrl(PAGE)
            view = this
        }
    }

    fun show(text: String) {
        wanted = text
        render()
    }

    private fun render() {
        val view = view ?: return
        val wanted = wanted ?: return
        if (!loaded || wanted == shown) return
        shown = wanted
        // A JSON string literal is a valid JavaScript string literal.
        view.evaluateJavascript("show(${JsonPrimitive(wanted)})", null)
    }

    private companion object {
        const val PAGE = "https://appassets.androidplatform.net/assets/html.html"
    }
}
