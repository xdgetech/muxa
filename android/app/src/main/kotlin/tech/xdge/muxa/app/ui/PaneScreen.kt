package tech.xdge.muxa.app.ui

import android.content.res.Configuration
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.ime
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.shortcut
import tech.xdge.muxa.app.pane.TerminalPaneView
import tech.xdge.muxa.session.KeyBar
import tech.xdge.muxa.session.PaneFit

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PaneScreen(model: AppModel, onShowList: () -> Unit) = BoxWithConstraints {
    // The pane's own width decides the bar: in the book posture a tablet window has a
    // phone-wide pane, which needs the compact bar too.
    val stack = LocalPhoneLayout.current
    val phone = stack || maxWidth < 600.dp
    val book = LocalWindowLayout.current is WindowLayout.Book
    val pretty = model.showsPretty
    // Leaving Pretty, by the toggle or by opening another pane, returns the keyboard.
    LaunchedEffect(pretty) { if (!pretty) model.refocusPane() }
    Column(Modifier.fillMaxSize().background(if (pretty) Theme.chatBackground else Theme.terminal)) {
        val dark = !pretty
        TopAppBar(
            title = {
                Column {
                    Text(model.currentSession?.name ?: "Sessions", maxLines = 1)
                    Text(subtitle(model), style = MaterialTheme.typography.labelSmall, maxLines = 1)
                }
            },
            navigationIcon = {
                // Back on the stack, Show List beside a hidden list, nothing when the list always shows.
                if (!book) {
                    IconButton(onShowList, Modifier.testTag(if (stack) "back" else "show-list")) {
                        Icon(if (stack) Icons.back else Icons.list, if (stack) "Back" else "Show List")
                    }
                }
            },
            actions = {
                if (!phone) ViewPicker(model, dark)
                if (model.canReadDocs && !phone) {
                    IconButton(model::openReader, Modifier.testTag("documents")) {
                        Icon(Icons.document, if (model.changeCount > 0) "Documents, ${model.changeCount} changed files" else "Documents")
                    }
                }
                MoreMenu(model, phone)
            },
            colors = if (dark) {
                TopAppBarDefaults.topAppBarColors(
                    containerColor = Theme.terminal,
                    titleContentColor = androidx.compose.ui.graphics.Color.White,
                    navigationIconContentColor = androidx.compose.ui.graphics.Color.White,
                    actionIconContentColor = androidx.compose.ui.graphics.Color.White,
                )
            } else {
                TopAppBarDefaults.topAppBarColors()
            },
        )
        // A phone's bar has no room for the picker next to the actions: it gets its own row.
        if (phone && model.paneViews.size > 1) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp), horizontalArrangement = androidx.compose.foundation.layout.Arrangement.Center) {
                ViewPicker(model, !pretty)
            }
        }
        Box(Modifier.weight(1f).fillMaxWidth()) {
            // The terminal stays under the chat so switching back is instant.
            PaneHost(model, focusRequest = if (pretty) null else model.focusRequest)
            if (pretty) ChatView(model, model.chat)
            notice(model)?.let { text ->
                Surface(
                    Modifier.align(Alignment.BottomCenter).padding(bottom = 12.dp),
                    shape = CircleShape,
                    tonalElevation = 4.dp,
                ) { Text(text, Modifier.padding(horizontal = 12.dp, vertical = 6.dp), style = MaterialTheme.typography.bodyMedium) }
            }
        }
    }
}

/** The adapter's state in Pretty, the pane's tags otherwise. */
private fun subtitle(model: AppModel): String {
    val pane = model.currentPane ?: return ""
    val status = model.chat?.status
    return AgentSummary.summary(if (model.showsPretty && status != null) pane.copy(agent = status) else pane)
}

private fun notice(model: AppModel): String? =
    model.state.paneGoneReason?.let { "Pane closed: $it. Ctrl+Shift+K to pick another." } ?: model.connectError

@Composable
private fun ViewPicker(model: AppModel, dark: Boolean = false) {
    val views = model.paneViews
    if (views.isEmpty()) return
    SingleChoiceSegmentedButtonRow(Modifier.testTag("view-mode")) {
        views.forEachIndexed { i, view ->
            SegmentedButton(
                selected = model.paneView == view,
                onClick = { model.show(view) },
                shape = SegmentedButtonDefaults.itemShape(i, views.size),
                modifier = Modifier.testTag("view-${view.title}"),
                icon = {},
                colors = if (dark) {
                    SegmentedButtonDefaults.colors(
                        inactiveContainerColor = Theme.terminal,
                        inactiveContentColor = androidx.compose.ui.graphics.Color.White,
                    )
                } else {
                    SegmentedButtonDefaults.colors()
                },
            ) { Text(view.title) }
        }
    }
}

@Composable
private fun MoreMenu(model: AppModel, phone: Boolean) {
    var open by remember { mutableStateOf(false) }
    IconButton({ open = true }, Modifier.testTag("more")) { Icon(Icons.more, "More") }
    DropdownMenu(open, { open = false }) {
        fun pick(action: () -> Unit) {
            open = false
            action()
        }
        if (model.canReadDocs && phone) {
            DropdownMenuItem(
                { Text(if (model.changeCount > 0) "Documents (${model.changeCount} changed)" else "Documents") },
                { pick(model::openReader) },
                leadingIcon = { Icon(Icons.document, null) },
                modifier = Modifier.testTag("menu-documents"),
            )
        }
        Text("Terminal Size", Modifier.padding(horizontal = 16.dp, vertical = 8.dp), style = MaterialTheme.typography.labelMedium)
        for ((label, fit) in listOf("Automatic" to null, "Readable" to PaneFit.READABLE, "Fit Width" to PaneFit.WIDTH, "Fit Whole Pane" to PaneFit.WHOLE)) {
            DropdownMenuItem(
                { Text(label) },
                { pick { model.paneFit = fit } },
                leadingIcon = { RadioButton(model.paneFit == fit, null) },
                modifier = Modifier.testTag("fit-$label"),
            )
        }
        HorizontalDivider()
        DropdownMenuItem({ Text("Sessions…") }, { pick { model.showSwitcher = true } }, leadingIcon = { Icon(Icons.session, null) })
        if (model.canStartTasks) {
            DropdownMenuItem({ Text("New Task…") }, { pick { model.startNewTask() } }, leadingIcon = { Icon(Icons.newTask, null) })
        }
        model.currentSession?.takeIf(model::canClose)?.let { current ->
            DropdownMenuItem({ Text("Close Task…") }, { pick { model.closingTask = current } }, leadingIcon = { Icon(Icons.closeTask, null) })
        }
        HorizontalDivider()
        DropdownMenuItem(
            { Text("Disconnect", color = Theme.failure) },
            { pick { model.disconnect() } },
            leadingIcon = { Icon(Icons.disconnect, null, tint = Theme.failure) },
        )
    }
}

/**
 * The terminal view, wired to the session, with the key bar over the on-screen
 * keyboard. A new [focusRequest] takes the keyboard back when a hardware keyboard is
 * attached; null while another view (Pretty) has the keyboard.
 */
@Composable
private fun PaneHost(model: AppModel, focusRequest: Int?) {
    val keys = LocalPaneKeys.current
    var bar by remember { mutableStateOf(KeyBar()) }
    var view by remember { mutableStateOf<TerminalPaneView?>(null) }
    keys.view = view
    keys.bar = bar
    // Bumped on every screen change, so the semantics below carry the current text.
    var content by remember { mutableIntStateOf(0) }
    val hardware = LocalConfiguration.current.hardKeyboardHidden == Configuration.HARDKEYBOARDHIDDEN_NO
    Column(Modifier.fillMaxSize().navigationBarsPadding().imePadding()) {
        AndroidView(
            factory = { context ->
                TerminalPaneView(context).apply {
                    model.attach(screen)
                    onOutput = model.session::send
                    onPaste = model.session::paste
                    onKeyBar = { bar = it }
                    onShortcut = { e -> model.shortcut(e) }
                    onContent = { content++ }
                    view = this
                }
            },
            update = {
                it.fitOverride = model.paneFit
                it.onPinch = { fit -> model.paneFit = fit }
            },
            modifier = Modifier.weight(1f).fillMaxWidth().testTag("pane").semantics {
                contentDescription = "Pane"
                stateDescription = if (content >= 0) view?.screenText.orEmpty() else ""
            },
        )
        // A hardware keyboard has these keys, and the bar would cover the pane's bottom rows.
        // On a table the keys sit below the hinge instead (Root).
        if (!hardware && !keys.belowHinge && WindowInsets.ime.getBottom(LocalDensity.current) > 0) KeyBarRow(bar) { view?.tap(it) }
    }
    DisposableEffect(Unit) {
        onDispose {
            view?.let { if (model.session.renderer === it.screen) model.session.renderer = null }
            if (keys.view === view) keys.view = null
            view?.screen?.close()
        }
    }
    LaunchedEffect(focusRequest, view) {
        if (focusRequest != null) view?.focusIfKeyboard()
    }
}

/** The keys above the on-screen keyboard, which cannot type them itself. */
@Composable
fun KeyBarRow(bar: KeyBar, tap: (KeyBar.Key) -> Unit) {
    Row(
        Modifier.fillMaxWidth().height(48.dp).background(MaterialTheme.colorScheme.surfaceVariant).padding(horizontal = 6.dp, vertical = 6.dp),
        horizontalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        for (key in KeyBar.Key.entries) {
            val (face, label) = face(key)
            val selected = key == KeyBar.Key.CONTROL && bar.control
            FilledTonalButton(
                onClick = { tap(key) },
                modifier = Modifier.weight(1f).testTag("keybar-$key").semantics { contentDescription = label },
                contentPadding = androidx.compose.foundation.layout.PaddingValues(0.dp),
                colors = if (selected) {
                    androidx.compose.material3.ButtonDefaults.filledTonalButtonColors(
                        containerColor = Theme.accent,
                        contentColor = androidx.compose.ui.graphics.Color.White,
                    )
                } else {
                    androidx.compose.material3.ButtonDefaults.filledTonalButtonColors()
                },
            ) { Text(face, fontFamily = FontFamily.Monospace) }
        }
    }
}

private fun face(key: KeyBar.Key): Pair<String, String> = when (key) {
    KeyBar.Key.ESCAPE -> "esc" to "Escape"
    KeyBar.Key.TAB -> "⇥" to "Tab"
    KeyBar.Key.BACK_TAB -> "⇤" to "Shift Tab"
    KeyBar.Key.CONTROL -> "ctrl" to "Control"
    KeyBar.Key.LEFT -> "←" to "Left"
    KeyBar.Key.DOWN -> "↓" to "Down"
    KeyBar.Key.UP -> "↑" to "Up"
    KeyBar.Key.RIGHT -> "→" to "Right"
    KeyBar.Key.INTERRUPT -> "^C" to "Control C"
}
