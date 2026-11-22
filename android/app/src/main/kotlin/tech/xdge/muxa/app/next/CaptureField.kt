package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.Palette
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.InputChip
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.v2.Capture
import tech.xdge.muxa.v2.ItemKind
import tech.xdge.muxa.v2.V2Request

/**
 * The field on top of the Inbox (ADR 0066 decision 6). The server reads the
 * line; the kind, Workspace and due date show as chips the user can change
 * before adding.
 */
@Composable
fun CaptureField(model: NextModel, focus: FocusRequester = remember { FocusRequester() }) {
    var text by remember { mutableStateOf("") }
    var parsed by remember { mutableStateOf<Capture?>(null) }
    val scope = rememberCoroutineScope()

    // Asks the server once typing pauses.
    LaunchedEffect(text) {
        if (text.isBlank()) {
            parsed = null
            return@LaunchedEffect
        }
        delay(PAUSE_MS)
        parsed = model.parseCapture(text)
    }

    fun add() = scope.launch {
        val capture = parsed ?: model.parseCapture(text)
        if (capture == null || capture.title.isEmpty()) return@launch
        model.inboxNow(V2Request.InboxAdd(capture))
        if (model.requestError == null) {
            text = ""
            parsed = null
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .background(Palette.surface, RoundedCornerShape(12.dp))
                .border(1.dp, Palette.borderStrong, RoundedCornerShape(12.dp))
                .padding(start = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(Icons.add, null, tint = Palette.textTertiary)
            BasicTextField(
                text, { text = it },
                Modifier.weight(1f).focusRequester(focus).testTag("capture.field").semantics { contentDescription = "Capture" },
                textStyle = TextStyle(color = Palette.text, fontSize = 15.sp),
                cursorBrush = SolidColor(Palette.accent),
                singleLine = true,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { add() }),
                decorationBox = { field ->
                    Box(contentAlignment = Alignment.CenterStart) {
                        if (text.isEmpty()) Text("Capture something to do", color = Palette.textTertiary, fontSize = 15.sp)
                        field()
                    }
                },
            )
            if (text.isNotEmpty()) {
                IconButton({ add() }, Modifier.testTag("capture.add"), enabled = parsed?.title?.isNotEmpty() == true) {
                    Icon(Icons.check, "Add", tint = if (parsed?.title?.isNotEmpty() == true) Palette.accent else Palette.textTertiary)
                }
            } else {
                Spacer(Modifier.size(4.dp))
            }
        }
        parsed?.takeIf { text.isNotEmpty() }?.let { p -> Chips(p) { parsed = it } }
    }
}

@Composable
private fun Chips(p: Capture, change: (Capture) -> Unit) {
    var kinds by remember { mutableStateOf(false) }
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
        Box {
            Box(Modifier.clip(RoundedCornerShape(6.dp)).clickable { kinds = true }.testTag("capture.kind").semantics { contentDescription = "Kind ${p.kind.title}" }) {
                KindChip(p.kind, Modifier.width(64.dp).padding(vertical = 4.dp))
            }
            DropdownMenu(kinds, { kinds = false }) {
                for (kind in ItemKind.entries.filter { it != ItemKind.REVIEW && it != ItemKind.FAILED }) {
                    DropdownMenuItem({ Text(kind.title) }, {
                        kinds = false
                        change(p.copy(kind = kind))
                    })
                }
            }
        }
        p.workspace?.let { Removable("#$it") { change(p.copy(workspace = null)) } }
        p.due?.let { Removable("due $it") { change(p.copy(due = null)) } }
    }
}

@Composable
private fun Removable(text: String, remove: () -> Unit) {
    InputChip(
        selected = false, onClick = remove, label = { Text(text) },
        trailingIcon = { Icon(Icons.close, null, Modifier.size(14.dp)) },
        modifier = Modifier.semantics { contentDescription = "$text, remove" },
    )
}

private const val PAUSE_MS = 250L
