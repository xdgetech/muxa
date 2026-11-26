package tech.xdge.muxa.app.reader

import android.annotation.SuppressLint
import android.content.res.Configuration
import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.viewinterop.AndroidView
import androidx.webkit.ScriptHandler
import androidx.webkit.WebViewAssetLoader
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import tech.xdge.muxa.app.ui.ThemeStore
import tech.xdge.muxa.session.ResolvedTheme
import tech.xdge.muxa.session.ThemeToken

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
    val theme = ThemeStore.current
    val json = remember(theme) { themeJson(theme.resolved) }
    AndroidView(
        factory = { context -> state.create(context, json, theme[ThemeToken.ground].toArgb()) },
        update = {
            state.theme(json, theme[ThemeToken.ground].toArgb())
            state.show(ReaderPage.Shown(text, path, diff))
        },
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
    private var theme = ""
    private var pageTheme: String? = null
    private var injected: ScriptHandler? = null
    private var injectedTheme: String? = null

    @SuppressLint("SetJavaScriptEnabled")
    fun create(context: android.content.Context, json: String, ground: Int): WebView {
        theme = json
        val assets = WebViewAssetLoader.Builder()
            .addPathHandler("/assets/", WebViewAssetLoader.AssetsPathHandler(context))
            .build()
        return WebView(context).apply {
            // The page's own ground, so no white shows before it loads.
            setBackgroundColor(ground)
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
                    injected = null
                    return true
                }

                override fun onPageFinished(view: WebView, url: String) {
                    loaded = true
                    pageTheme = injectedTheme
                    syncTheme()
                    render()
                }
            }
            view = this
            inject(this)
            loadUrl(PAGE)
        }
    }

    /** A new theme: the page gets it now, and a reload gets it before its first paint. */
    fun theme(json: String, ground: Int) {
        if (json == theme) return
        theme = json
        val view = view ?: return
        view.setBackgroundColor(ground)
        inject(view)
        syncTheme()
    }

    /**
     * reader.js reads `window.muxaTheme` as it starts, so the first paint has the theme.
     * A WebView without document-start scripts gets it from [syncTheme] once loaded.
     */
    private fun inject(view: WebView) {
        if (!WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) return
        injected?.remove()
        injected = WebViewCompat.addDocumentStartJavaScript(view, "window.muxaTheme = $theme;", setOf(ORIGIN))
        injectedTheme = theme
    }

    private fun syncTheme() {
        val view = view ?: return
        if (!loaded || pageTheme == theme) return
        pageTheme = theme
        view.evaluateJavascript("setTheme($theme)", null)
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
        if (first) focusIfKeyboard(view)
    }

    /**
     * Space and the arrows scroll the page only while it has focus, and nothing gives
     * it focus by itself. Without a hardware keyboard nothing needs it.
     */
    private fun focusIfKeyboard(view: WebView) {
        if (view.resources.configuration.hardKeyboardHidden == Configuration.HARDKEYBOARDHIDDEN_NO) view.requestFocus()
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
        const val ORIGIN = "https://appassets.androidplatform.net"
        const val PAGE = "$ORIGIN/assets/reader.html"
    }
}

/** Every token as `#rrggbb`, and the appearance: what reader.js reads, as on iOS. */
internal fun themeJson(theme: ResolvedTheme): String = buildJsonObject {
    ThemeToken.entries.forEach { put(it.key, theme[it].hex) }
    put("appearance", theme.appearance.name.lowercase())
}.toString()
