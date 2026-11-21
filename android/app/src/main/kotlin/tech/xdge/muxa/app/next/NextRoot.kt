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
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LocalPhoneLayout
import tech.xdge.muxa.app.ui.LocalWindowLayout
import tech.xdge.muxa.app.ui.WindowLayout
import tech.xdge.muxa.app.ui.currentWindowLayout
import tech.xdge.muxa.v2.Terminal

/** The app on protocol v2: the hosts until one is connected, then Home or Focus. */
@OptIn(ExperimentalComposeUiApi::class)
@Composable
fun NextRoot(model: NextModel) {
    val layout = currentWindowLayout()
    var focus by rememberSaveable { mutableStateOf(false) }
    var terminal by rememberSaveable { mutableStateOf<String?>(null) }
    fun open(t: Terminal) {
        terminal = t.id
        focus = true
    }

    // Ctrl+Shift+H and Ctrl+Shift+F, as on the desktop (ADR 0070).
    val modeKeys = Modifier.onPreviewKeyEvent { e ->
        if (e.type != KeyEventType.KeyDown || !e.isCtrlPressed || !e.isShiftPressed) return@onPreviewKeyEvent false
        when (e.key) {
            Key.H -> true.also { focus = false }
            Key.F -> true.also { focus = true }
            else -> false
        }
    }

    CompositionLocalProvider(LocalPhoneLayout provides (layout == WindowLayout.Phone), LocalWindowLayout provides layout) {
        Surface(Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }, color = MaterialTheme.colorScheme.background) {
            Box(Modifier.fillMaxSize().safeDrawingPadding().then(modeKeys)) {
                when (model.link) {
                    NextModel.Link.Connected, is NextModel.Link.Reconnecting ->
                        if (focus) {
                            Focus(model, terminal, { terminal = it }) { focus = false }
                        } else {
                            Home(model, ::open) { focus = true }
                        }
                    else -> Hosts(model)
                }
            }
            model.untrustedKey?.let { TrustHostDialog(model, it) }
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
        Text("muxa", style = MaterialTheme.typography.headlineMedium)
        Text(status(model), Modifier.testTag("next.link"))
        HorizontalDivider()
        Text("Hosts", style = MaterialTheme.typography.titleSmall)
        for (host in hosts.hosts) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                TextButton({ model.connect(host.id) }, Modifier.weight(1f).testTag("host.${host.title}")) {
                    Text(host.title, Modifier.fillMaxWidth())
                }
                IconButton({ editing = HostDraft.of(host) }) { Icon(Icons.edit, "Edit ${host.title}") }
            }
        }
        TextButton({ editing = HostDraft.new() }, Modifier.testTag("next.addhost").semantics { contentDescription = "Add Host" }) {
            Icon(Icons.add, null)
            Text(" Add Host")
        }
        HorizontalDivider()
        OutlinedButton({ model.startDemo() }, Modifier.testTag("next.demo")) { Text("Try the Demo") }
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
