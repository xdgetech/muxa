package tech.xdge.muxa.app.next

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.UriHandler
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.mikepenz.markdown.m3.Markdown
import com.mikepenz.markdown.m3.markdownColor
import com.mikepenz.markdown.m3.markdownTypography
import com.mikepenz.markdown.model.markdownPadding
import tech.xdge.muxa.app.ui.Palette

/**
 * An agent's or the user's Markdown (an Inbox item's note, a Chat reply) in the
 * app's palette. Web links open in the browser; other links do nothing, and
 * images are never loaded.
 */
@Composable
fun MarkdownText(text: String, modifier: Modifier = Modifier, fontSize: TextUnit = 15.sp, color: Color = Palette.text) {
    val context = LocalContext.current
    val webOnly = remember(context) {
        object : UriHandler {
            override fun openUri(uri: String) {
                val parsed = Uri.parse(uri)
                if (parsed.scheme?.lowercase() in WEB_SCHEMES) context.startActivity(Intent(Intent.ACTION_VIEW, parsed))
            }
        }
    }
    val body = TextStyle(color = color, fontSize = fontSize, lineHeight = fontSize * LINE_HEIGHT)
    fun heading(scale: Float) = body.copy(fontSize = fontSize * scale, lineHeight = fontSize * scale * LINE_HEIGHT, fontWeight = FontWeight.SemiBold)
    CompositionLocalProvider(LocalUriHandler provides webOnly) {
        SelectionContainer(modifier) {
            Markdown(
                content = text,
                modifier = Modifier.fillMaxWidth(),
                colors = markdownColor(
                    text = color,
                    codeBackground = Palette.terminal,
                    inlineCodeBackground = Palette.control,
                    dividerColor = Palette.divider,
                    tableBackground = Palette.surface,
                ),
                typography = markdownTypography(
                    h1 = heading(1.4f), h2 = heading(1.25f), h3 = heading(1.1f),
                    h4 = heading(1f), h5 = heading(1f), h6 = heading(1f),
                    text = body,
                    paragraph = body,
                    ordered = body,
                    bullet = body,
                    list = body,
                    table = body,
                    code = body.copy(fontFamily = FontFamily.Monospace, fontSize = fontSize * CODE_SCALE),
                    inlineCode = body.copy(fontFamily = FontFamily.Monospace, fontSize = fontSize * CODE_SCALE),
                    quote = body.copy(color = Palette.textSecondary),
                    textLink = TextLinkStyles(SpanStyle(color = Palette.accent)),
                ),
                padding = markdownPadding(block = 4.dp, codeBlock = PaddingValues(12.dp)),
            )
        }
    }
}

private val WEB_SCHEMES = setOf("http", "https")
private const val LINE_HEIGHT = 1.47f
private const val CODE_SCALE = 0.88f
