package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.Corners
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LocalPhoneLayout
import tech.xdge.muxa.app.ui.LocalWindowLayout
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.SecondaryButton
import tech.xdge.muxa.app.ui.WindowLayout
import tech.xdge.muxa.app.ui.currentWindowLayout
import tech.xdge.muxa.v2.Terminal

/** The app on protocol v2: the hosts until one is connected, then Home or Focus. */
@OptIn(ExperimentalComposeUiApi::class)
@Composable
fun NextRoot(model: NextModel) {
    val layout = currentWindowLayout()
    var section by rememberSaveable { mutableStateOf(Section.INBOX) }
    var terminal by rememberSaveable { mutableStateOf<String?>(null) }
    var newTask by remember { mutableStateOf<TaskSeed?>(null) }
    val bots = remember(model) { BotNav() }
    // A Bot's talk, run or member opens on its page; any other terminal in Focus (ADR 0086).
    fun open(t: Terminal) {
        if (t.belongsToBot) {
            bots.open(t)
            section = Section.BOTS
        } else {
            terminal = t.id
            section = Section.TASKS
        }
    }

    // Ctrl+Shift+H and Ctrl+Shift+F, as on the desktop (ADR 0070); Ctrl+Shift+N for a new Task,
    // since Ctrl+N is Capture.
    val modeKeys = Modifier.onPreviewKeyEvent { e ->
        if (e.type != KeyEventType.KeyDown || !e.isCtrlPressed || !e.isShiftPressed) return@onPreviewKeyEvent false
        when (e.key) {
            Key.H -> true.also { section = Section.INBOX }
            Key.F -> true.also { section = Section.TASKS }
            Key.N -> true.also { if (model.isConnected) newTask = TaskSeed() }
            else -> false
        }
    }

    CompositionLocalProvider(LocalPhoneLayout provides (layout == WindowLayout.Phone), LocalWindowLayout provides layout) {
        Surface(Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }, color = MaterialTheme.colorScheme.background) {
            Box(Modifier.fillMaxSize().safeDrawingPadding().then(modeKeys)) {
                when (model.link) {
                    NextModel.Link.Connected, is NextModel.Link.Reconnecting -> {
                        // A tablet has no Settings tab: the host in its sidebar's footer opens Settings as a sheet.
                        val shown = if (layout != WindowLayout.Phone && section == Section.SETTINGS) Section.INBOX else section
                        val nav = Nav(shown, { section = it }, { newTask = it }, ::open, bots)
                        when (shown) {
                            Section.INBOX -> Home(model, nav, ::open)
                            Section.BOTS -> BotsScreen(model, nav)
                            Section.TASKS -> Focus(model, nav, terminal) { terminal = it }
                            Section.SETTINGS -> SettingsScreen(model, nav)
                        }
                    }
                    else -> Hosts(model)
                }
            }
            model.untrustedKey?.let { TrustHostDialog(model, it) }
            newTask?.takeIf { model.isConnected }?.let { seed ->
                StartTaskSheet(model, seed, { newTask = null }) { t ->
                    newTask = null
                    open(t)
                }
            }
        }
    }
}

/** Before a connection: the saved hosts, Add Host and Try the Demo. */
@Composable
private fun Hosts(model: NextModel) {
    var editing by remember { mutableStateOf<HostDraft?>(null) }
    val hosts by model.hosts.contents.collectAsState()
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp).widthIn(max = 600.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("muxa", fontSize = 30.sp, fontWeight = FontWeight.Bold)
        Text(status(model), Modifier.testTag("next.link"), color = Palette.textSecondary)
        HorizontalDivider()
        Text("Hosts", style = MaterialTheme.typography.titleSmall)
        for (host in hosts.hosts) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                TextButton({ model.connect(host.id) }, Modifier.weight(1f).testTag("host.${host.title}"), shape = Corners.link) {
                    Text(host.title, Modifier.fillMaxWidth())
                }
                IconButton({ editing = HostDraft.of(host) }) { Icon(Icons.edit, "Edit ${host.title}") }
            }
        }
        TextButton({ editing = HostDraft.new() }, Modifier.testTag("next.addhost").semantics { contentDescription = "Add Host" }, shape = Corners.link) {
            Icon(Icons.add, null)
            Text(" Add Host")
        }
        HorizontalDivider()
        SecondaryButton("Try the Demo", { model.startDemo() }, Modifier.testTag("next.demo"))
        Text("The demo runs in the app. Nothing reaches a host.", style = MaterialTheme.typography.bodySmall)
    }
    editing?.let { HostEditor(model, it) { editing = null } }
}

private fun status(model: NextModel): String = when (val link = model.link) {
    NextModel.Link.Offline -> "Not connected"
    NextModel.Link.Connecting -> if (model.addingKey) "Adding this device's key…" else "Connecting…"
    NextModel.Link.Connected -> if (model.demo) "Connected to the demo" else "Connected"
    is NextModel.Link.Reconnecting -> "Reconnecting: ${link.why}"
    is NextModel.Link.Failed -> link.why
}
