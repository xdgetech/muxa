package tech.xdge.muxa.app.next

import android.content.Intent
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.reader.HtmlWebView
import tech.xdge.muxa.app.reader.ReaderWebView
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.v2.DiffScope
import tech.xdge.muxa.v2.ReaderContent
import tech.xdge.muxa.v2.ReaderEntry
import tech.xdge.muxa.v2.ReaderKind
import tech.xdge.muxa.v2.ReaderList
import tech.xdge.muxa.v2.Terminal

/** A file to open in the Reader. */
private data class ReaderTarget(val kind: ReaderKind, val path: String, val scope: DiffScope)

/**
 * The Reader beside a terminal (plan F2): the documents, changed files and
 * images of a Task's or a Routine run's folder.
 */
@Composable
fun ReaderPanel(model: NextModel, terminal: Terminal, modifier: Modifier) {
    var kind by rememberSaveable(terminal.id) { mutableStateOf(ReaderKind.DOCS) }
    // The documents opened, the shown one last: Back returns to the one a link left.
    var opened by remember(terminal.id) { mutableStateOf(emptyList<ReaderTarget>()) }
    var linkNote by remember(terminal.id) { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    fun back() {
        opened = opened.dropLast(1)
        linkNote = null
    }

    // A link opens only a document the Reader lists; the web view never navigates.
    fun follow(path: String) {
        scope.launch {
            val listed = model.readerList(terminal.id, ReaderKind.DOCS)?.entries.orEmpty().any { it.path == path }
            val next = ReaderTarget(ReaderKind.DOCS, path, DiffScope.UNCOMMITTED)
            // A second tap before the first opened adds nothing.
            if (listed && opened.lastOrNull() == next) return@launch
            if (listed) {
                opened = opened + next
                linkNote = null
            } else {
                linkNote = "$path is not one of the Reader's documents."
            }
        }
    }
    val target = opened.lastOrNull()
    Column(modifier.testTag("focus.readerpanel")) {
        when {
            terminal.taskDir == null -> Empty("Only a Task's agent or a Routine run has files to read.")
            target != null -> {
                BackHandler { back() }
                Row(Modifier.fillMaxWidth().padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    IconButton({ back() }, Modifier.testTag("reader.back")) { Icon(Icons.back, "Back") }
                    Text(target.path.substringAfterLast('/'), Modifier.testTag("reader.path"), style = MaterialTheme.typography.titleSmall,
                        maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                linkNote?.let {
                    Text(it, Modifier.padding(horizontal = 16.dp), style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.error)
                }
                ReaderDocument(model, terminal.id, target, ::follow, Modifier.weight(1f))
            }
            else -> {
                Row(Modifier.padding(8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for ((k, label) in listOf(ReaderKind.DOCS to "Docs", ReaderKind.CHANGES to "Changes", ReaderKind.IMAGES to "Images")) {
                        FilterChip(kind == k, { kind = k }, { Text(label) }, Modifier.testTag("reader.kind.${k.wire}"))
                    }
                }
                ReaderFiles(model, terminal.id, kind, Modifier.weight(1f)) { path, diff -> opened = listOf(ReaderTarget(kind, path, diff)) }
            }
        }
    }
}

@Composable
private fun ReaderFiles(model: NextModel, terminal: String, kind: ReaderKind, modifier: Modifier, open: (String, DiffScope) -> Unit) {
    var list by remember(terminal, kind) { mutableStateOf<ReaderList?>(null) }
    var loading by remember(terminal, kind) { mutableStateOf(true) }
    LaunchedEffect(terminal, kind) {
        list = model.readerList(terminal, kind)
        loading = false
    }
    val shown = list
    if (shown == null) {
        Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            if (loading) CircularProgressIndicator() else Text("No files.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        return
    }
    LazyColumn(modifier) {
        item(key = "root") {
            Text(shown.root ?: "Folder", Modifier.padding(16.dp, 8.dp), style = MaterialTheme.typography.titleSmall)
        }
        if (shown.entries.isEmpty()) {
            item(key = "empty") {
                Text(emptyText(kind), Modifier.padding(16.dp, 4.dp), color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        items(shown.entries, key = { "u/${it.path}" }) { e -> FileRow(e) { open(e.path, DiffScope.UNCOMMITTED) } }
        shown.branch?.let { b ->
            item(key = "branch") {
                Text("${b.commits} commits since ${b.base}", Modifier.padding(16.dp, 12.dp, 16.dp, 4.dp),
                    style = MaterialTheme.typography.titleSmall)
            }
            items(b.entries, key = { "b/${it.path}" }) { e -> FileRow(e) { open(e.path, DiffScope.BRANCH) } }
        }
        if (shown.truncated) {
            item(key = "more") {
                Text("More files than shown.", Modifier.padding(16.dp, 8.dp), style = MaterialTheme.typography.labelSmall)
            }
        }
    }
}

private fun emptyText(kind: ReaderKind) = when (kind) {
    ReaderKind.DOCS -> "No documents."
    ReaderKind.CHANGES -> "No changes."
    ReaderKind.IMAGES -> "No images."
}

@Composable
private fun FileRow(e: ReaderEntry, onClick: () -> Unit) {
    Column(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 8.dp).testTag("reader.file.${e.path}")) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            e.status?.let { Text(it.take(1).uppercase() + " ", color = MaterialTheme.colorScheme.onSurfaceVariant) }
            Text(e.title ?: e.path, Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (e.added != null && e.removed != null) {
                Text("+${e.added} −${e.removed}", style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        if (e.title != null) {
            Text(e.path, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** One open file, kept current while it is shown. */
@Composable
private fun ReaderDocument(model: NextModel, terminal: String, target: ReaderTarget, follow: (String) -> Unit, modifier: Modifier) {
    var reader by remember(target) { mutableStateOf<Int?>(null) }
    var attempt by remember(target) { mutableStateOf(0) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(target, attempt) {
        reader = model.openReader(terminal, target.kind, target.path, target.scope)
    }
    DisposableEffect(target) {
        onDispose { reader?.let(model::closeReader) }
    }
    val n = reader
    val ended = n?.let { model.readerEnded[it] }
    val content = n?.let { model.readerContents[it] }
    Box(modifier.fillMaxSize()) {
        when {
            ended != null -> Column(Modifier.align(Alignment.Center).padding(16.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Text(ended, color = MaterialTheme.colorScheme.onSurfaceVariant)
                TextButton({
                    scope.launch {
                        n.let(model::closeReader)
                        attempt += 1
                    }
                }) { Text("Open again") }
            }
            content != null -> Shown(target, content, follow)
            else -> CircularProgressIndicator(Modifier.align(Alignment.Center))
        }
    }
}

@Composable
private fun Shown(target: ReaderTarget, content: ReaderContent, follow: (String) -> Unit) {
    val context = LocalContext.current
    // Web links open in the browser, and a document's relative links its other documents.
    val onLink: (String, String) -> Unit = { href, from ->
        val uri = Uri.parse(href)
        if (uri.scheme == "http" || uri.scheme == "https") {
            context.startActivity(Intent(Intent.ACTION_VIEW, uri))
        } else if (target.kind == ReaderKind.DOCS) {
            ReaderLinks.resolve(href, from)?.let(follow)
        }
    }
    val html = target.path.substringAfterLast('.').lowercase() in setOf("html", "htm")
    when {
        target.kind == ReaderKind.DOCS && html -> HtmlWebView(content.text.orEmpty(), Modifier.fillMaxSize())
        target.kind == ReaderKind.DOCS -> ReaderWebView(content.text.orEmpty(), content.path, diff = false, onLink, Modifier.fillMaxSize())
        target.kind == ReaderKind.CHANGES && content.binary -> Empty("A binary file: its diff has no text.")
        target.kind == ReaderKind.CHANGES -> ReaderWebView(content.text.orEmpty(), content.path, diff = true, onLink, Modifier.fillMaxSize())
        else -> {
            val bitmap = remember(content.data) { content.bytes?.let { BitmapFactory.decodeByteArray(it, 0, it.size) } }
            if (bitmap == null) {
                Empty("Not an image.")
            } else {
                Image(bitmap.asImageBitmap(), target.path, Modifier.fillMaxSize().testTag("reader.image"), contentScale = ContentScale.Fit)
            }
        }
    }
}
