package tech.xdge.muxa.app.next

import java.net.URLDecoder

/** Where a link in a Reader document points, inside the listed folder. */
object ReaderLinks {
    /**
     * The folder-relative path [href] names from the document at [from], or null
     * for a link with a scheme or one that leaves the folder. The query and the
     * fragment are dropped; `/` starts at the folder's root.
     */
    fun resolve(href: String, from: String): String? {
        val bare = href.substringBefore('#').substringBefore('?')
        if (bare.isEmpty() || SCHEME.containsMatchIn(bare)) return null
        val decoded = runCatching { URLDecoder.decode(bare.replace("+", "%2B"), "UTF-8") }.getOrNull() ?: return null
        val base = if (decoded.startsWith("/")) emptyList() else from.split('/').dropLast(1)
        val parts = mutableListOf<String>()
        for (part in base + decoded.split('/')) {
            when (part) {
                "", "." -> {}
                ".." -> if (parts.isEmpty()) return null else parts.removeAt(parts.lastIndex)
                else -> parts += part
            }
        }
        return parts.joinToString("/").ifEmpty { null }
    }

    private val SCHEME = Regex("^[A-Za-z][A-Za-z0-9+.-]*:")
}
