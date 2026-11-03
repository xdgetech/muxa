package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.InboxPrompt
import tech.xdge.muxa.app.dismiss
import tech.xdge.muxa.app.entry
import tech.xdge.muxa.protocol.InboxEntry
import tech.xdge.muxa.session.InboxRow
import tech.xdge.muxa.session.hasKnownActions
import tech.xdge.muxa.session.isProposal

/**
 * A Routine's briefing or proposal: what it says, its sources, and its actions. Source
 * text is shown as data; it never acts by itself (ADR 0026 section 4).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun InboxEntryDetail(model: AppModel, id: String, modifier: Modifier) {
    val entry = model.entry(id)
    Column(modifier) {
        TopAppBar(
            title = { Text(entry?.title ?: "", maxLines = 1, overflow = TextOverflow.Ellipsis) },
            navigationIcon = {
                if (LocalPhoneLayout.current) {
                    IconButton({ model.selectedEntry = null }, Modifier.testTag("entry-back")) { Icon(Icons.back, "Inbox") }
                }
            },
        )
        if (entry == null) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text("The host no longer lists this entry.") }
            return@Column
        }
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp).testTag("entry-detail"),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(entry.kindTitle, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
            SelectionContainer { Text(entry.body, Modifier.testTag("entry-body")) }
            Links(entry)
            entry.proposal?.let { p ->
                Text("Proposed action", style = MaterialTheme.typography.titleSmall)
                Field("Action", p.action)
                Field("Target", p.target, mono = true)
                Field("Content", p.content, mono = true)
            }
            EntryActions(model, entry)
            Text("From", style = MaterialTheme.typography.titleSmall)
            Field("Routine", "${entry.workspace}/${entry.routine}")
            Field("Published", shownDate(entry.createdAt))
            Field("Occurrence", entry.occurrence)
        }
    }
}

/** The sources, opened in the browser. */
@Composable
private fun Links(entry: InboxEntry) {
    val uri = LocalUriHandler.current
    for (link in entry.links) {
        TextButton({ runCatching { uri.openUri(link.url) } }, Modifier.testTag("entry-link")) {
            Text(link.title ?: link.url, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun EntryActions(model: AppModel, entry: InboxEntry) {
    val task = entry.task
    if (task != null) {
        OutlinedButton(
            {
                model.clearInboxSelection()
                model.selectedTask = task
            },
            Modifier.testTag("entry-open-task"),
        ) { Text("Open the Task") }
        Text("Approved. The Task performs the action.", style = MaterialTheme.typography.bodySmall)
        return
    }
    if (!entry.hasKnownActions) return
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (entry.isProposal) {
            Action("Approve…", Icons.done, "entry-approve") { model.inboxPrompt = InboxPrompt.Approve(entry) }
            Action("Dismiss", Icons.close, "entry-dismiss") { model.dismiss(entry) }
        } else {
            Action("Done", Icons.check, "entry-done") { model.dismiss(entry) }
        }
        Action("Later…", Icons.later, "entry-later") { model.inboxPrompt = InboxPrompt.Later(InboxRow.Entry(entry)) }
    }
}

@Composable
private fun Action(title: String, icon: ImageVector, tag: String, onClick: () -> Unit) {
    OutlinedButton(onClick, Modifier.testTag(tag)) {
        Icon(icon, null)
        Text(title, Modifier.padding(start = 6.dp))
    }
}
