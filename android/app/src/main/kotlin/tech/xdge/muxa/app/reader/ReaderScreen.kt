package tech.xdge.muxa.app.reader

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.ReaderTab
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LocalPhoneLayout
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.protocol.ChangeInfo
import tech.xdge.muxa.protocol.DiffScope
import tech.xdge.muxa.protocol.DocInfo
import tech.xdge.muxa.session.ImageFolder
import java.net.URI
import java.net.URLDecoder

/** What the Reader shows: a document, the diff of a changed file, or an image. */
sealed interface ReaderItem {
    data class Doc(val path: String) : ReaderItem
    data class Diff(val path: String, val scope: DiffScope) : ReaderItem
    data class Image(val path: String) : ReaderItem
}

/**
 * The documents of the shown pane, in three tabs: its agent's plan and its repo's
 * Markdown by directory; its uncommitted changes and its branch's commits; its images
 * as a folder tree. The open one updates when its file changes.
 */
@Composable
fun ReaderScreen(model: AppModel) {
    var selection by remember { mutableStateOf<ReaderItem?>(null) }
    var query by remember { mutableStateOf("") }
    var external by remember { mutableStateOf<String?>(null) }
    // Open folders of the Images tree, by path.
    var expanded by remember { mutableStateOf(emptySet<String>()) }
    // On a phone the Reader is a stack: it opens on a document, the list one Back away.
    var showList by remember { mutableStateOf(false) }

    LaunchedEffect(Unit) {
        model.loadDocs().join()
        selection = model.preferredDoc?.let { ReaderItem.Doc(it) }
        if (selection == null) showList = true
        // The latest run's screenshots are one tap away.
        model.imageListing?.entries?.firstOrNull()?.let { expanded = expanded + ImageFolder.ancestors(it.path) }
    }
    LaunchedEffect(selection) {
        when (val item = selection) {
            is ReaderItem.Doc -> model.openDoc(item.path)
            is ReaderItem.Diff -> model.openDiff(item.path, item.scope)
            is ReaderItem.Image -> model.openImage(item.path)
            null -> {}
        }
    }

    fun select(item: ReaderItem) {
        selection = item
        showList = false
    }

    // A relative link to a listed document opens here; a web link asks first; nothing else is followed.
    fun follow(href: String, from: String) {
        val scheme = runCatching { URI(href).scheme }.getOrNull()
        if (scheme == "http" || scheme == "https") {
            external = href
            return
        }
        val target = resolve(href, from) ?: return
        if (model.docListing?.entries?.any { it.path == target } == true) select(ReaderItem.Doc(target))
    }

    Dialog(onDismissRequest = model::closeReader, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        // A dialog is its own window: its status bar icons follow the theme here, not the activity.
        val dark = androidx.compose.foundation.isSystemInDarkTheme()
        val view = androidx.compose.ui.platform.LocalView.current
        androidx.compose.runtime.SideEffect {
            (view.parent as? androidx.compose.ui.window.DialogWindowProvider)?.window?.let { window ->
                androidx.core.view.WindowCompat.getInsetsController(window, view).isAppearanceLightStatusBars = !dark
            }
        }
        // It also needs the tag setting the main window has (Root).
        Surface(Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }, color = MaterialTheme.colorScheme.background) {
            val phone = LocalPhoneLayout.current
            val list = @Composable { modifier: Modifier ->
                ReaderList(model, query, { query = it }, selection, ::select, expanded, { expanded = it }, modifier)
            }
            val detail = @Composable { modifier: Modifier ->
                ReaderDetail(model, selection, ::select, ::follow, onShowList = { showList = true }, modifier)
            }
            if (phone) {
                if (showList) list(Modifier.fillMaxSize()) else detail(Modifier.fillMaxSize())
                androidx.activity.compose.BackHandler(!showList) { showList = true }
            } else {
                Row(Modifier.fillMaxSize()) {
                    list(Modifier.width(340.dp).fillMaxHeight())
                    VerticalDivider()
                    detail(Modifier.weight(1f).fillMaxHeight())
                }
            }
        }
    }

    external?.let { url ->
        val uri = LocalUriHandler.current
        AlertDialog(
            onDismissRequest = { external = null },
            title = { Text("Open in the browser?") },
            text = { Text(url) },
            confirmButton = { TextButton({ external = null; uri.openUri(url) }) { Text("Open ${runCatching { URI(url).host }.getOrNull() ?: url}") } },
            dismissButton = { TextButton({ external = null }) { Text("Cancel") } },
        )
    }
}

/** [href] relative to the folder of [from], without its fragment or query. */
fun resolve(href: String, from: String): String? {
    val bare = href.substringBefore('#').substringBefore('?')
    if (bare.isEmpty() || ':' in bare) return null
    val parts = if (bare.startsWith("/")) mutableListOf() else folder(from).split('/').filter { it.isNotEmpty() }.toMutableList()
    for (part in bare.split('/')) {
        when (part) {
            ".", "" -> continue
            ".." -> if (parts.isEmpty()) return null else parts.removeAt(parts.size - 1)
            else -> parts += runCatching { URLDecoder.decode(part, "UTF-8") }.getOrDefault(part)
        }
    }
    return parts.joinToString("/")
}

fun name(path: String) = path.substringAfterLast('/')

fun folder(path: String) = path.substringBeforeLast('/', "")

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ReaderList(
    model: AppModel,
    query: String,
    onQuery: (String) -> Unit,
    selection: ReaderItem?,
    select: (ReaderItem) -> Unit,
    expanded: Set<String>,
    onExpanded: (Set<String>) -> Unit,
    modifier: Modifier,
) {
    Column(modifier) {
        TopAppBar(
            title = { Text(model.docListing?.root ?: "Documents") },
            navigationIcon = { IconButton(model::closeReader, Modifier.testTag("reader-done")) { Icon(Icons.close, "Done") } },
        )
        if (model.readerTabs.size > 1) {
            SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().padding(horizontal = 16.dp).testTag("reader-tab")) {
                model.readerTabs.forEachIndexed { i, tab ->
                    SegmentedButton(
                        model.readerTab == tab, { model.readerTab = tab },
                        SegmentedButtonDefaults.itemShape(i, model.readerTabs.size),
                        Modifier.testTag("reader-tab-${tab.title}"),
                        icon = {},
                    ) { Text(tab.title) }
                }
            }
        }
        val prompt = when (model.readerTab) {
            ReaderTab.DOCS -> "Find a document"
            ReaderTab.CHANGES -> "Find a changed file"
            ReaderTab.IMAGES -> "Find an image"
        }
        OutlinedTextField(query, onQuery, Modifier.fillMaxWidth().padding(16.dp, 8.dp), placeholder = { Text(prompt) }, singleLine = true)
        model.docError?.let { Text(it, Modifier.padding(horizontal = 16.dp), color = Theme.failure) }
        fun matches(path: String, title: String? = null) =
            query.isEmpty() || path.contains(query, true) || title?.contains(query, true) == true
        Box(Modifier.weight(1f)) {
            LazyColumn(Modifier.fillMaxSize()) {
                when (model.readerTab) {
                    ReaderTab.DOCS -> docRows(model, model.docListing?.entries.orEmpty().filter { matches(it.path, it.title) }, selection, select)
                    ReaderTab.CHANGES -> changeRows(model, ::matches, selection, select)
                    ReaderTab.IMAGES -> {
                        val images = model.imageListing?.entries.orEmpty().filter { matches(it.path) }
                        // While searching, every folder with a match is open.
                        val open = if (query.isEmpty()) expanded else images.flatMap { ImageFolder.ancestors(it.path) }.toSet()
                        imageRows(ImageFolder.tree(images), open, onExpanded.takeIf { query.isEmpty() } ?: {}, selection, select, depth = 0)
                        if (model.imageListing?.truncated == true) item { Note("Only the newest 2000 images are listed.") }
                    }
                }
            }
            EmptyState(model)
        }
    }
}

private fun LazyListScope.docRows(model: AppModel, docs: List<DocInfo>, selection: ReaderItem?, select: (ReaderItem) -> Unit) {
    val plans = docs.filter { it.kind == DocInfo.Kind.PLAN }
    if (plans.isNotEmpty()) {
        item { Header("Plan") }
        items(plans, key = { "plan-${it.path}" }) { doc ->
            Row(doc.title ?: name(doc.path), name(doc.path), selection == ReaderItem.Doc(doc.path), "doc-${doc.path}") { select(ReaderItem.Doc(doc.path)) }
        }
    }
    // Repo documents by folder, the top folder first, in path order.
    val folders = docs.filter { it.kind == DocInfo.Kind.REPO }.groupBy { folder(it.path) }.toSortedMap()
    for ((dir, list) in folders) {
        item(key = "dir-$dir") { Header(dir.ifEmpty { model.docListing?.root ?: "Repository" }) }
        items(list, key = { "doc-${it.path}" }) { doc ->
            Row(name(doc.path), null, selection == ReaderItem.Doc(doc.path), "doc-${doc.path}") { select(ReaderItem.Doc(doc.path)) }
        }
    }
    if (model.docListing?.truncated == true) item { Note("Only the first 2000 documents are listed.") }
}

private fun LazyListScope.changeRows(model: AppModel, matches: (String) -> Boolean, selection: ReaderItem?, select: (ReaderItem) -> Unit) {
    val listing = model.changeListing ?: return
    val changes = listing.entries.filter { matches(it.path) }
    if (changes.isNotEmpty()) {
        item { Header("Uncommitted") }
        items(changes, key = { "u-${it.path}" }) { c -> ChangeRow(c, DiffScope.UNCOMMITTED, selection, select) }
        if (listing.truncated) item { Note("Only the first 2000 changed files are listed.") }
    }
    val branch = listing.branch ?: return
    val branchChanges = branch.entries.filter { matches(it.path) }
    if (branchChanges.isEmpty()) return
    item {
        Header(if (branch.commits == 1) "Since ${branch.base} · 1 commit" else "Since ${branch.base} · ${branch.commits} commits", "branch-header")
    }
    items(branchChanges, key = { "b-${it.path}" }) { c -> ChangeRow(c, DiffScope.BRANCH, selection, select) }
    if (branch.truncated) item { Note("Only the first 2000 changed files are listed.") }
}

private fun LazyListScope.imageRows(
    folder: ImageFolder,
    expanded: Set<String>,
    onExpanded: (Set<String>) -> Unit,
    selection: ReaderItem?,
    select: (ReaderItem) -> Unit,
    depth: Int,
) {
    for (sub in folder.folders) {
        val open = sub.path in expanded
        item(key = "folder-${sub.path}") {
            androidx.compose.foundation.layout.Row(
                Modifier.fillMaxWidth().clickable { onExpanded(if (open) expanded - sub.path else expanded + sub.path) }
                    .padding(start = (16 + depth * 16).dp, end = 16.dp, top = 10.dp, bottom = 10.dp).testTag("image-folder-${sub.path}"),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(if (open) "▾" else "▸", Modifier.width(16.dp))
                Icon(Icons.repo, null, Modifier.size(18.dp))
                Text(sub.name, Modifier.padding(start = 8.dp).weight(1f))
                Text("${sub.count}", style = MaterialTheme.typography.labelSmall)
            }
        }
        if (open) imageRows(sub, expanded, onExpanded, selection, select, depth + 1)
    }
    items(folder.images, key = { "image-${it.path}" }) { image ->
        val detail = image.mtime?.let { android.text.format.DateUtils.getRelativeTimeSpanString(it * 1000).toString() }
        Row(name(image.path), detail, selection == ReaderItem.Image(image.path), "image-${image.path}", indent = depth) {
            select(ReaderItem.Image(image.path))
        }
    }
}

@Composable
private fun Header(text: String, tag: String? = null) {
    Text(
        text,
        Modifier.padding(start = 16.dp, top = 16.dp, bottom = 4.dp).let { if (tag != null) it.testTag(tag) else it },
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@Composable
private fun Note(text: String) {
    Text(text, Modifier.padding(16.dp), style = MaterialTheme.typography.labelSmall)
}

@Composable
private fun Row(title: String, detail: String?, selected: Boolean, tag: String, indent: Int = 0, onClick: () -> Unit) {
    Column(
        Modifier.fillMaxWidth()
            .background(if (selected) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface)
            .clickable(onClick = onClick)
            .padding(start = (16 + indent * 16).dp, end = 16.dp, top = 10.dp, bottom = 10.dp)
            .testTag(tag),
    ) {
        Text(title, maxLines = 2)
        if (detail != null && detail != title) {
            Text(detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

/** A changed file: its status letter, name and folder, and the lines it adds and removes. */
@Composable
private fun ChangeRow(change: ChangeInfo, scope: DiffScope, selection: ReaderItem?, select: (ReaderItem) -> Unit) {
    val item = ReaderItem.Diff(change.path, scope)
    val (letter, color) = when (change.status) {
        ChangeInfo.Status.MODIFIED -> "M" to Theme.attention
        ChangeInfo.Status.ADDED -> "A" to Theme.added
        ChangeInfo.Status.DELETED -> "D" to Theme.removed
        ChangeInfo.Status.RENAMED -> "R" to Theme.attention
        ChangeInfo.Status.UNTRACKED -> "U" to Theme.added
        ChangeInfo.Status.CONFLICTED -> "!" to Theme.removed
        ChangeInfo.Status.OTHER -> "?" to Theme.attention
    }
    androidx.compose.foundation.layout.Row(
        Modifier.fillMaxWidth()
            .background(if (selection == item) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface)
            .clickable { select(item) }
            .padding(horizontal = 16.dp, vertical = 10.dp)
            .testTag(if (scope == DiffScope.BRANCH) "branch-change-${change.path}" else "change-${change.path}"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(
            letter,
            Modifier.background(color.copy(alpha = 0.15f), RoundedCornerShape(4.dp)).padding(horizontal = 5.dp),
            color = color, fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold,
        )
        Column(Modifier.weight(1f)) {
            Text(name(change.path), maxLines = 1)
            val sub = change.oldPath?.let { "from $it" } ?: folder(change.path).takeIf { it.isNotEmpty() }
            sub?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1) }
        }
        if (change.added != null && change.removed != null) {
            Text("+${change.added}", color = Theme.added, style = MaterialTheme.typography.labelSmall)
            Text("−${change.removed}", color = Theme.removed, style = MaterialTheme.typography.labelSmall)
        }
    }
}

/** Why the open tab is empty, once its list arrived. */
@Composable
private fun EmptyState(model: AppModel) {
    val (title, text) = when {
        model.readerTab == ReaderTab.DOCS && model.docListing?.entries?.isEmpty() == true ->
            "No Documents" to "This pane is not in a git repository with Markdown files."
        model.readerTab == ReaderTab.CHANGES && model.changeListing?.let { it.entries.isEmpty() && it.branch?.entries.isNullOrEmpty() } == true ->
            "No Changes" to "Nothing is uncommitted, and the branch has no commits of its own."
        model.readerTab == ReaderTab.IMAGES && model.imageListing?.entries?.isEmpty() == true ->
            "No Images" to "This pane's git repository has no image files."
        else -> return
    }
    Unavailable(title, text)
}

@Composable
fun Unavailable(title: String, text: String?) {
    Column(Modifier.fillMaxSize().padding(32.dp), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
        Text(title, style = MaterialTheme.typography.titleLarge, textAlign = TextAlign.Center)
        text?.let { Text(it, Modifier.padding(top = 8.dp), textAlign = TextAlign.Center) }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ReaderDetail(
    model: AppModel,
    selection: ReaderItem?,
    select: (ReaderItem) -> Unit,
    follow: (String, String) -> Unit,
    onShowList: () -> Unit,
    modifier: Modifier,
) {
    val phone = LocalPhoneLayout.current
    val title = when (selection) {
        is ReaderItem.Doc -> name(selection.path)
        is ReaderItem.Diff -> name(selection.path)
        is ReaderItem.Image -> name(selection.path)
        null -> ""
    }
    Column(modifier) {
        TopAppBar(
            title = { Text(title, maxLines = 1) },
            navigationIcon = {
                if (phone) IconButton(onShowList, Modifier.testTag("reader-list")) { Icon(Icons.back, "Documents") }
            },
            actions = {
                if (selection is ReaderItem.Image) ImageStepper(model, selection.path, select)
                // Compact, the list's Done is a screen away.
                if (phone) TextButton(model::closeReader, Modifier.testTag("reader-close")) { Text("Done") }
            },
        )
        Box(Modifier.weight(1f).fillMaxWidth()) {
            when (selection) {
                is ReaderItem.Doc -> DocDetail(model, selection.path, follow)
                is ReaderItem.Diff -> DiffDetail(model, selection.path, selection.scope)
                is ReaderItem.Image -> ImageDetail(model, selection.path, select)
                null -> Unavailable("Pick a Document", null)
            }
        }
    }
}

@Composable
private fun DocDetail(model: AppModel, path: String, follow: (String, String) -> Unit) {
    val doc = model.state.doc?.takeIf { it.path == path }
    val text = doc?.text
    when {
        doc == null -> Loading()
        text != null -> Box(Modifier.fillMaxSize()) {
            ReaderWebView(text, doc.path, diff = false, onLink = follow, modifier = Modifier.fillMaxSize())
            doc.ended?.let { EndedNotice(it) }
        }
        doc.ended != null -> Unavailable("Can't Open ${name(path)}", doc.ended)
        else -> Loading()
    }
}

@Composable
private fun DiffDetail(model: AppModel, path: String, scope: DiffScope) {
    val diff = model.state.diff?.takeIf { it.path == path && it.scope == scope }
    val text = diff?.text
    when {
        diff == null -> Loading()
        text != null -> Box(Modifier.fillMaxSize()) {
            ReaderWebView(text, path, diff = true, onLink = { _, _ -> }, modifier = Modifier.fillMaxSize())
            diff.ended?.let { EndedNotice(it) }
        }
        diff.binary -> Unavailable("Binary File", "Changes to this file cannot be shown as text.")
        diff.ended != null -> Unavailable("No Diff for ${name(path)}", diff.ended)
        else -> Loading()
    }
}

@Composable
fun Loading() {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
}

/** Shown over a document, diff or image that stopped updating. */
@Composable
fun EndedNotice(text: String) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.BottomCenter) {
        Surface(Modifier.padding(bottom = 16.dp), shape = CircleShape, tonalElevation = 4.dp) {
            Text("No longer updating: $text", Modifier.padding(horizontal = 12.dp, vertical = 6.dp))
        }
    }
}
