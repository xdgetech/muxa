package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.Theme

/**
 * An editor as a dialog. [save] returns null when the host took the change, else the
 * message shown above the buttons; the dialog stays open on a failure.
 */
@Composable
fun FormDialog(
    title: String,
    tag: String,
    canSave: Boolean,
    save: suspend () -> String?,
    close: () -> Unit,
    remove: (suspend () -> String?)? = null,
    removeTitle: String = "Remove",
    content: @Composable ColumnScope.() -> Unit,
) {
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    fun run(action: suspend () -> String?) = scope.launch {
        busy = true
        error = action()
        busy = false
        if (error == null) close()
    }
    AlertDialog(
        onDismissRequest = close,
        modifier = Modifier.testTag(tag),
        title = { Text(title) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                content()
                error?.let { Text(it, Modifier.testTag("settings.form.error"), color = Theme.failure) }
                if (remove != null) {
                    TextButton({ run(remove) }, Modifier.testTag("settings.form.remove"), enabled = !busy) {
                        Text(removeTitle, color = Theme.failure)
                    }
                }
            }
        },
        confirmButton = {
            TextButton({ run(save) }, Modifier.testTag("settings.form.save"), enabled = canSave && !busy) { Text("Save") }
        },
        dismissButton = { TextButton(close) { Text("Cancel") } },
    )
}

/** Asks before something that cannot be undone from the app. */
@Composable
fun ConfirmDialog(title: String, text: String, action: String, confirm: () -> Unit, close: () -> Unit) {
    AlertDialog(
        onDismissRequest = close,
        title = { Text(title) },
        text = { Text(text) },
        confirmButton = { TextButton(confirm, Modifier.testTag("settings.confirm")) { Text(action, color = Theme.failure) } },
        dismissButton = { TextButton(close) { Text("Cancel") } },
    )
}

private val plain = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false)

@Composable
fun Field(
    value: String,
    change: (String) -> Unit,
    label: String,
    tag: String,
    enabled: Boolean = true,
    mono: Boolean = false,
    lines: Int = 1,
    placeholder: String? = null,
    help: String? = null,
    number: Boolean = false,
) {
    OutlinedTextField(
        value, change, Modifier.fillMaxWidth().testTag(tag),
        enabled = enabled,
        label = { Text(label) },
        placeholder = placeholder?.let { { Text(it) } },
        supportingText = help?.let { { Text(it) } },
        singleLine = lines == 1,
        minLines = lines,
        keyboardOptions = if (number) KeyboardOptions(keyboardType = KeyboardType.Number) else plain,
        textStyle = if (mono) MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace) else MaterialTheme.typography.bodyLarge,
    )
}

/** A write-only value: typed hidden, never filled in from the host. [isSet] says one is there already. */
@Composable
fun SecretField(value: String, change: (String) -> Unit, label: String, tag: String, isSet: Boolean) {
    OutlinedTextField(
        value, change, Modifier.fillMaxWidth().testTag(tag),
        label = { Text(label) },
        placeholder = { Text(if (isSet) "Set. Type to replace it." else "Not set") },
        singleLine = true,
        visualTransformation = PasswordVisualTransformation(),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false),
    )
}

@Composable
fun SwitchField(title: String, tag: String, on: Boolean, enabled: Boolean = true, change: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(title, Modifier.weight(1f))
        Switch(on, change, Modifier.testTag(tag), enabled = enabled)
    }
}

/** Toggles for names, such as the connections an MCP server needs. */
@Composable
fun NameChips(title: String, names: List<String>, on: Set<String>, tag: String, change: (Set<String>) -> Unit) {
    if (names.isEmpty()) return
    Text(title, style = MaterialTheme.typography.titleSmall)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        for (n in names) {
            FilterChip(n in on, { change(if (n in on) on - n else on + n) }, { Text(n) }, Modifier.testTag("$tag.$n"))
        }
    }
}

/** A [Segmented] choice among [values], with its [titles]. */
@Composable
fun <T> Choice(values: List<T>, titles: List<String>, picked: T, tag: String, enabled: Boolean = true, pick: (T) -> Unit) {
    Segmented(
        titles.mapIndexed { i, t -> Segment(t, "$tag.${values[i]}", enabled) },
        values.indexOf(picked).coerceAtLeast(0),
        { pick(values[it]) },
        Modifier.fillMaxWidth(),
    )
}
