package tech.xdge.muxa.app.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.movableContentOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.shortcut
import androidx.compose.ui.input.key.onPreviewKeyEvent
import tech.xdge.muxa.app.ConnectFix
import tech.xdge.muxa.app.reader.ReaderScreen
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.CloseMode
import tech.xdge.muxa.session.AgentAlert
import tech.xdge.muxa.session.PaneSession

/**
 * Hosts and sessions in the list, the pane in the detail. On a tablet the list hides
 * once a pane is open, so the pane gets the full width. On a phone the two are a
 * stack, and an open pane is pushed (ADR 0012, 0013).
 */
@Composable
fun Root(model: AppModel) {
    val layout = currentWindowLayout()
    val phone = layout == WindowLayout.Phone
    val paneKeys = remember { PaneKeys() }
    var showList by remember { mutableStateOf(true) }
    var editing by remember { mutableStateOf<HostDraft?>(null) }
    val paneOpen = model.isConnected && model.state.watchedPane != null

    LaunchedEffect(model.state.watchedPane, model.isConnected) {
        if (paneOpen) showList = false
    }
    LaunchedEffect(model.detailRequest) { if (model.detailRequest > 0) showList = false }
    LaunchedEffect(model.isConnected) { if (!model.isConnected) showList = true }

    CompositionLocalProvider(
        LocalPhoneLayout provides phone,
        LocalPaneKeys provides paneKeys.also { it.belowHinge = layout is WindowLayout.Tabletop },
    ) {
        Surface(
            // Before any focused field: the composer and the dialogs never see the app's chords.
            Modifier.fillMaxSize().onPreviewKeyEvent { model.shortcut(it.nativeKeyEvent) },
            color = MaterialTheme.colorScheme.background,
        ) {
            val list = @Composable { modifier: Modifier ->
                Sidebar(model, onEdit = { editing = it }, onHide = { showList = false }.takeIf { paneOpen }, modifier = modifier)
            }
            // Moved, not rebuilt, when a fold or a resize changes the layout: the pane view
            // keeps its screen and the keyboard, as SwiftUI moves it on iOS.
            val detail = remember {
                movableContentOf { modifier: Modifier ->
                    Box(modifier) {
                        Detail(model, onEdit = { editing = it }, onShowList = { showList = true })
                        AlertOverlay(model)
                    }
                }
            }
            val split = @Composable { modifier: Modifier ->
                Row(modifier) {
                    AnimatedVisibility(showList) {
                        Row {
                            list(Modifier.width(320.dp).fillMaxHeight())
                            VerticalDivider()
                        }
                    }
                    detail(Modifier.weight(1f).fillMaxHeight())
                }
            }
            when (layout) {
                WindowLayout.Phone -> {
                    // A stack: the list, or the detail pushed over it.
                    if (showList) list(Modifier.fillMaxSize()) else detail(Modifier.fillMaxSize())
                    BackHandler(enabled = !showList) { showList = true }
                }
                WindowLayout.Tablet -> split(Modifier.fillMaxSize())
                // Half open like a book: the list on one side of the hinge, the pane on the other.
                is WindowLayout.Book -> Row(Modifier.fillMaxSize()) {
                    list(Modifier.width(layout.listWidth.dp).fillMaxHeight())
                    Spacer(Modifier.width(layout.gap.dp))
                    detail(Modifier.weight(1f).fillMaxHeight())
                }
                // On a table: the app above the hinge, the keys below it.
                is WindowLayout.Tabletop -> Column(Modifier.fillMaxSize()) {
                    split(Modifier.height(layout.topHeight.dp).fillMaxWidth())
                    Spacer(Modifier.height(layout.gap.dp))
                    Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.TopCenter) {
                        paneKeys.view?.takeIf { !model.showsPretty }?.let { view -> KeyBarRow(paneKeys.bar) { view.tap(it) } }
                    }
                }
            }
        }

        editing?.let { draft -> HostEditor(model, draft, onDismiss = { editing = null; model.refocusPane() }) }
        model.untrustedKey?.let { key -> TrustHostDialog(model, key) }
        if (model.showSwitcher) {
            SessionSwitcher(model, onDismiss = {
                model.showSwitcher = false
                model.switcherClosed()
            })
        }
        model.newTask?.let { draft -> NewTaskSheet(model, draft) }
        model.closingTask?.let { task -> CloseTaskDialog(model, task) }
        model.closeError?.let { error ->
            AlertDialog(
                onDismissRequest = { model.closeError = null; model.refocusPane() },
                title = { Text("Can't Close the Task") },
                text = { Text(error) },
                confirmButton = { TextButton({ model.closeError = null; model.refocusPane() }) { Text("OK") } },
            )
        }
        if (model.showReader) ReaderScreen(model)
    }
}

/** The pane when one is open; otherwise what to do next. */
@Composable
private fun Detail(model: AppModel, onEdit: (HostDraft) -> Unit, onShowList: () -> Unit) {
    val host = model.currentHost
    val reason = (model.state.phase as? PaneSession.Phase.Disconnected)?.reason?.takeIf { it != "disconnected" }
    when {
        model.isConnected && model.state.watchedPane != null -> PaneScreen(model, onShowList)
        model.addingKey && host != null -> Progress("Adding this device's key to ${host.title}…")
        model.connecting && host != null -> Progress("Connecting to ${host.title}…")
        (model.connectError ?: reason) != null && host != null -> Unavailable(
            title = "Can't Connect to ${host.title}",
            text = model.connectError ?: reason!!,
            onShowList = onShowList,
        ) {
            val uri = LocalUriHandler.current
            when (model.connectFix) {
                ConnectFix.ADD_KEY -> {
                    Button({ onEdit(HostDraft.of(host)) }) { Text("Set Up with Password") }
                    OutlinedButton({ model.connect(host.id) }) { Text("Try Again") }
                }
                ConnectFix.INSTALL_BRIDGE -> {
                    Button({ model.connect(host.id) }) { Text("Try Again") }
                    OutlinedButton({ uri.openUri(HOST_SETUP) }) { Text("How to Install the Bridge") }
                    TextButton({ onEdit(HostDraft.of(host)) }) { Text("Edit Host") }
                }
                null -> {
                    Button({ model.connect(host.id) }) { Text("Try Again") }
                    OutlinedButton({ onEdit(HostDraft.of(host)) }) { Text("Edit Host") }
                }
            }
        }
        model.isConnected -> Unavailable("No Sessions", "No tmux session on this host has a pane to show.", onShowList)
        model.hostList.hosts.isEmpty() -> Unavailable(
            "Add a Host",
            "Muxa connects over SSH to the Mac or Linux machine where your tmux sessions run. Install muxa-bridge there first.",
            onShowList,
        ) {
            val uri = LocalUriHandler.current
            Button({ onEdit(HostDraft.new()) }) { Text("Add Host") }
            OutlinedButton({ uri.openUri(HOST_SETUP) }) { Text("How to Set Up the Host") }
        }
        else -> Unavailable("Not Connected", "Pick a host in the list.", onShowList)
    }
}

/** Setup steps for the host live in the README, next to the bridge they install. */
private const val HOST_SETUP = "https://github.com/xdgetech/muxa#setup"

@Composable
private fun Progress(text: String) {
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
        CircularProgressIndicator()
        Text(text, Modifier.padding(top = 16.dp), style = MaterialTheme.typography.bodyLarge)
    }
}

@Composable
private fun Unavailable(title: String, text: String, onShowList: () -> Unit, actions: @Composable () -> Unit = {}) {
    val phone = LocalPhoneLayout.current
    Column(
        Modifier.fillMaxSize().padding(32.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(title, style = MaterialTheme.typography.headlineSmall, textAlign = TextAlign.Center)
        Text(text, style = MaterialTheme.typography.bodyMedium, textAlign = TextAlign.Center)
        actions()
        if (phone) TextButton(onShowList) { Text("Hosts") }
    }
}

/** "feature-auth: needs your input". Tapping it opens that session. */
@Composable
private fun AlertOverlay(model: AppModel) {
    val alert = model.alert
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        AnimatedVisibility(alert != null, enter = slideInVertically() + fadeIn(), exit = slideOutVertically() + fadeOut()) {
            alert?.let { AlertBanner(it) { model.openAlert() } }
        }
    }
}

@Composable
private fun AlertBanner(alert: AgentAlert, open: () -> Unit) {
    val (icon, color) = when (alert.kind) {
        AgentAlert.Kind.WAITING -> Icons.waiting to Theme.state(AgentState.WAITING)
        AgentAlert.Kind.DONE -> Icons.done to Theme.state(AgentState.DONE)
        AgentAlert.Kind.FAILED -> Icons.warning to Theme.state(AgentState.ERROR)
    }
    Surface(
        onClick = open,
        shape = MaterialTheme.shapes.extraLarge,
        tonalElevation = 6.dp,
        shadowElevation = 6.dp,
        modifier = Modifier.padding(top = 56.dp).testTag("agent-alert"),
    ) {
        Row(Modifier.padding(horizontal = 16.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(icon, null, tint = color)
            Text("${alert.sessionName}: ${alert.kind.message}", Modifier.padding(start = 8.dp), maxLines = 1)
        }
    }
}

@Composable
private fun CloseTaskDialog(model: AppModel, task: tech.xdge.muxa.protocol.Session) {
    AlertDialog(
        onDismissRequest = { model.closingTask = null },
        title = { Text("Close ${task.name}?") },
        text = {
            Text("Close removes the worktree only when the branch is merged. Uncommitted changes are never removed, and the branch is kept.")
        },
        confirmButton = {
            Column(horizontalAlignment = Alignment.End) {
                TextButton({ model.close(task, CloseMode.CLOSE) }) { Text("Close") }
                TextButton({ model.close(task, CloseMode.KEEP) }) { Text("Close and Keep Worktree") }
                TextButton({ model.close(task, CloseMode.FORCE) }) { Text("Remove Worktree Anyway", color = Theme.failure) }
                TextButton({ model.closingTask = null }) { Text("Cancel") }
            }
        },
    )
}
