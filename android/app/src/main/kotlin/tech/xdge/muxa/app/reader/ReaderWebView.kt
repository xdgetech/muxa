package tech.xdge.muxa.app.reader

import android.annotation.SuppressLint
import android.webkit.JavascriptInterface
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
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * Shows one Markdown document, or one unified diff, with the bundled renderer
 * (`reader.html`, shared with iOS). The page loads nothing else: network loads are
 * blocked, its CSP allows no network, and every navigation but the first is refused.
 * Links come back through [onLink] with their `href` and the document they were in.
 */
@Composable
fun ReaderWebView(text: String, path: String, diff: Boolean, onLink: (String, String) -> Unit, modifier: Modifier = Modifier) {
    val state = remember { ReaderPage() }
    state.onLink = onLink
    AndroidView(
        factory = { context -> state.create(context) },
        update = { state.show(ReaderPage.Shown(text, path, diff)) },
        modifier = modifier.testTag("reader"),
    )
}

private class ReaderPage {
    data class Shown(val text: String, val path: String, val diff: Boolean)

    var onLink: (String, String) -> Unit = { _, _ -> }
    private var view: WebView? = null
    private var loaded = false
    private var wanted: Shown? = null
    private var shown: Shown? = null

    @SuppressLint("SetJavaScriptEnabled")
    fun create(context: android.content.Context): WebView {
        val assets = WebViewAssetLoader.Builder()
            .addPathHandler("/assets/", WebViewAssetLoader.AssetsPathHandler(context))
            .build()
        return WebView(context).apply {
            settings.javaScriptEnabled = true
            settings.blockNetworkLoads = true
            settings.allowFileAccess = false
            settings.allowContentAccess = false
            settings.domStorageEnabled = false
            addJavascriptInterface(Bridge(), "muxaReader")
            webViewClient = object : WebViewClient() {
                override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? =
                    assets.shouldInterceptRequest(request.url)

                override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest) = true

                // A renderer that crashed or was killed for memory takes only this page down.
                override fun onRenderProcessGone(view: WebView, detail: android.webkit.RenderProcessGoneDetail): Boolean {
                    (view.parent as? android.view.ViewGroup)?.removeView(view)
                    view.destroy()
                    this@ReaderPage.view = null
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

    fun show(content: Shown) {
        wanted = content
        render()
    }

    private fun render() {
        val view = view ?: return
        val wanted = wanted ?: return
        if (!loaded || wanted == shown) return
        // A new document starts at the top; an update keeps the reading position.
        val first = shown?.path != wanted.path || shown?.diff != wanted.diff
        shown = wanted
        val function = if (wanted.diff) "renderDiff" else "render"
        // A JSON string literal is a valid JavaScript string literal.
        view.evaluateJavascript("$function(${JsonPrimitive(wanted.text)}, ${JsonPrimitive(wanted.path)}, $first)", null)
    }

    private inner class Bridge {
        @JavascriptInterface
        fun postMessage(json: String) {
            val body = runCatching { Json.parseToJsonElement(json).jsonObject }.getOrNull() ?: return
            if (body["type"]?.jsonPrimitive?.content != "link") return
            val href = body["href"]?.jsonPrimitive?.content ?: return
            val from = body["from"]?.jsonPrimitive?.content ?: ""
            view?.post { onLink(href, from) }
        }
    }

    private companion object {
        const val PAGE = "https://appassets.androidplatform.net/assets/reader.html"
    }
}
