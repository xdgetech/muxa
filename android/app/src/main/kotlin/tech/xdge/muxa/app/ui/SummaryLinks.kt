package tech.xdge.muxa.app.ui

import tech.xdge.muxa.protocol.Attachments
import java.net.URI

/** Where a link in an item's summary leads (ADR 0033 §4a). */
sealed interface SummaryTarget {
    /** An `http` or `https` link: the browser. */
    data class Web(val url: String) : SummaryTarget

    /** One of the item's docs, as `doc` takes it: the Reader. */
    data class Doc(val path: String) : SummaryTarget

    /** One of the item's images, as `image` takes it: the Reader. */
    data class Image(val path: String) : SummaryTarget
}

/**
 * The summary's links. A link, a code span or a word that names one of the item's attachments
 * opens it; `http` and `https` links open in the browser; nothing else is followed.
 */
object SummaryLinks {
    /** What tapping [href] opens, or null. A relative, absolute or bare file name matches an attachment's path. */
    fun target(href: String, attachments: Attachments): SummaryTarget? {
        val scheme = runCatching { URI(href).scheme }.getOrNull()?.lowercase()
        if (scheme == "http" || scheme == "https") return SummaryTarget.Web(href)
        if (scheme != null && scheme != "file") return null
        val name = decode(href.removePrefix("file://").substringBefore('#').substringBefore('?')).removePrefix("./")
        if (name.isEmpty() || name.endsWith("/")) return null
        attachments.docs.firstOrNull { names(it, name) }?.let { return SummaryTarget.Doc(it) }
        attachments.images.firstOrNull { names(it, name) }?.let { return SummaryTarget.Image(it) }
        return null
    }

    /**
     * [summary] with every bare `http(s)` link and every attachment's name made a Markdown link,
     * outside code blocks and existing links. A code span that names an attachment becomes one too.
     */
    fun linkify(summary: String, attachments: Attachments): String {
        var fenced = false
        return summary.split('\n').joinToString("\n") { line ->
            val fence = line.trimStart().let { it.startsWith("```") || it.startsWith("~~~") }
            if (fence) fenced = !fenced
            if (fence || fenced || line.startsWith("    ") || line.startsWith("\t")) line else linkifyLine(line, attachments)
        }
    }

    private fun linkifyLine(line: String, attachments: Attachments): String {
        val out = StringBuilder()
        var i = 0
        while (i < line.length) {
            val kept = existingLink(line, i)
            if (kept != null) {
                out.append(line, i, kept)
                i = kept
                continue
            }
            when {
                line[i] == '`' -> i = codeSpan(line, i, attachments, out)
                line[i].isWhitespace() -> out.append(line[i++])
                else -> i = word(line, i, attachments, out)
            }
        }
        return out.toString()
    }

    /** The end of a `[text](dest)`, `![alt](dest)` or `<...>` that starts at [i]; it stays as written. */
    private fun existingLink(line: String, i: Int): Int? {
        val match = when (line[i]) {
            '[', '!' -> LINK.matchAt(line, i)
            '<' -> ANGLE.matchAt(line, i)
            else -> null
        }
        return match?.range?.last?.plus(1)
    }

    private fun codeSpan(line: String, start: Int, attachments: Attachments, out: StringBuilder): Int {
        var ticks = start
        while (ticks < line.length && line[ticks] == '`') ticks++
        val fence = line.substring(start, ticks)
        val close = line.indexOf(fence, ticks)
        if (close < 0) {
            out.append(fence)
            return ticks
        }
        val code = line.substring(ticks, close)
        val span = line.substring(start, close + fence.length)
        val name = code.trim()
        val target = target(name, attachments)
        if (target is SummaryTarget.Doc || target is SummaryTarget.Image) out.append("[").append(span).append("](").append(encode(name)).append(")")
        else out.append(span)
        return close + fence.length
    }

    /** A run of non-space characters, without the punctuation around it. */
    private fun word(line: String, start: Int, attachments: Attachments, out: StringBuilder): Int {
        var end = start
        while (end < line.length && !line[end].isWhitespace() && line[end] != '`' && existingLink(line, end) == null) end++
        if (end == start) {
            out.append(line[start])
            return start + 1
        }
        val raw = line.substring(start, end)
        val lead = raw.takeWhile { it in LEADING }
        val rest = raw.substring(lead.length)
        val core = rest.trimEnd { it in TRAILING }
        val trail = rest.substring(core.length)
        val link = when {
            core.startsWith("http://") || core.startsWith("https://") -> "<$core>"
            core.contains('.') && target(core, attachments).let { it is SummaryTarget.Doc || it is SummaryTarget.Image } ->
                "[${escape(core)}](${encode(core)})"
            else -> null
        }
        out.append(if (link == null) raw else lead + link + trail)
        return end
    }

    /** The attachment [path] is what [name] names: the same path, an absolute path ending in it, or its last parts. */
    private fun names(path: String, name: String): Boolean =
        path == name || name.endsWith("/$path") || path.endsWith("/$name")

    private fun encode(name: String) = name.replace("%", "%25").replace(" ", "%20").replace("(", "%28").replace(")", "%29")

    private fun decode(href: String) = href.replace("%20", " ").replace("%28", "(").replace("%29", ")").replace("%25", "%")

    /** Emphasis characters in a file name stay literal inside the link text. */
    private fun escape(name: String) = name.replace("_", "\\_").replace("*", "\\*")

    private val LINK = Regex("""!?\[[^\]]*]\([^)\s]*(?:\s+"[^"]*")?\)""")
    private val ANGLE = Regex("""<[^<>\s]+>""")
    private const val LEADING = "(\"'*_"
    private const val TRAILING = ".,;:!?)\"'*_"
}
