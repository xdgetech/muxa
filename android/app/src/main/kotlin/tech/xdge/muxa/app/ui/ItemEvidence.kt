package tech.xdge.muxa.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.UriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.mikepenz.markdown.m3.Markdown
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.ReaderTab
import tech.xdge.muxa.app.reader.ReaderItem
import tech.xdge.muxa.protocol.InboxItem
import java.net.URI

// An item's evidence (ADR 0033 §4a): the summary as Markdown with its links, and the
// attachments as a list of names. Docs, images and changes open in the Reader of the item's pane.

/** Opens [target]: the browser for a web link, the Reader for an attachment the pane can serve. */
private fun AppModel.openTarget(item: InboxItem, target: SummaryTarget, browser: UriHandler) {
    val reads = item.pane != null && isConnected
    when (target) {
        is SummaryTarget.Web -> runCatching { browser.openUri(target.url) }
        is SummaryTarget.Doc -> if (reads && state.canReadDocs) readItem(item, ReaderTab.DOCS, ReaderItem.Doc(target.path))
        is SummaryTarget.Image -> if (reads && state.canShowImages) readItem(item, ReaderTab.IMAGES, ReaderItem.Image(target.path))
    }
}

/** The summary as Markdown. Its links go through [SummaryLinks], never straight to the system. */
@Composable
fun ItemSummary(model: AppModel, item: InboxItem, summary: String) {
    val browser = LocalUriHandler.current
    val markdown = remember(summary, item.attachments) { SummaryLinks.linkify(summary, item.attachments) }
    val links = remember(item, browser) {
        object : UriHandler {
            override fun openUri(uri: String) {
                SummaryLinks.target(uri, item.attachments)?.let { model.openTarget(item, it, browser) }
            }
        }
    }
    CompositionLocalProvider(LocalUriHandler provides links) {
        SelectionContainer { Markdown(markdown, modifier = Modifier.fillMaxWidth().testTag("item-summary")) }
    }
}

/** The changes, then the docs, the images and the links, one row each with its file name. */
@Composable
fun ItemAttachments(model: AppModel, item: InboxItem) {
    val a = item.attachments
    val reads = item.pane != null && model.isConnected
    val browser = LocalUriHandler.current
    val changes = a.changes?.takeIf { reads && model.state.canShowChanges }
    val docs = a.docs.takeIf { reads && model.state.canReadDocs }.orEmpty()
    val images = a.images.takeIf { reads && model.state.canShowImages }.orEmpty()
    if (changes == null && docs.isEmpty() && images.isEmpty() && a.links.isEmpty()) return
    Column(Modifier.fillMaxWidth().testTag("item-attachments")) {
        Text("Attachments", style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(bottom = 4.dp))
        changes?.let { c ->
            val files = if (c.files == 1) "1 file" else "${c.files} files"
            val detail = if (c.scope == "branch") "$files on the branch" else files
            Attachment(Icons.changes, "Diff +${c.added} −${c.removed}", detail, "item-changes") { model.readItem(item, ReaderTab.CHANGES) }
        }
        for (doc in docs) {
            Attachment(Icons.document, doc.substringAfterLast('/'), doc, "item-doc-$doc") {
                model.openTarget(item, SummaryTarget.Doc(doc), browser)
            }
        }
        for (image in images) {
            Attachment(Icons.image, image.substringAfterLast('/'), image, "item-image-$image") {
                model.openTarget(item, SummaryTarget.Image(image), browser)
            }
        }
        for (link in a.links) {
            val host = runCatching { URI(link.url).host }.getOrNull()
            Attachment(Icons.newWindow, link.title ?: link.url, host ?: link.url, "item-link") {
                model.openTarget(item, SummaryTarget.Web(link.url), browser)
            }
        }
    }
}

/** A row: the name, and below it the path, the host or the counts. */
@Composable
private fun Attachment(icon: ImageVector, name: String, detail: String, tag: String, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().testTag(tag).clickable(onClick = onClick).padding(vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, tint = MaterialTheme.colorScheme.primary)
        Column(Modifier.weight(1f)) {
            Text(name, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (detail != name) {
                Text(
                    detail, maxLines = 1, overflow = TextOverflow.MiddleEllipsis,
                    style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}
