package tech.xdge.muxa.app.next

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import tech.xdge.muxa.app.ui.KindPalette
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SettingsWorkspace

/** The agent's env variables. Keys are shown; a value is write-only, so only "set" is. */
@Composable
fun EnvSettings(editor: SettingsEditor, ws: SettingsWorkspace) {
    // The key being edited; "" adds a new one.
    var editing by remember { mutableStateOf<String?>(null) }
    SettingsHeader("Agent env")
    SettingsCard {
        if (ws.agent.env.isEmpty()) SettingsNote("No variables.")
        for (e in ws.agent.env) {
            EntryRow("settings.env.${ws.name}.${e.key}", !editor.readOnly, { editing = e.key }, badge = null) {
                EntryTitle(e.key) { if (e.set) Chip("set", KindPalette.review) }
            }
            CardDivider()
        }
        ActionRow("+ Add variable", "settings.env.add", !editor.readOnly) { editing = "" }
    }
    editing?.let { key -> EnvDialog(editor, ws.name, key) { editing = null } }
}

/** Sets a value; for a key that exists, it replaces the value or removes the variable. */
@Composable
private fun EnvDialog(editor: SettingsEditor, workspace: String, existing: String, close: () -> Unit) {
    val isNew = existing.isEmpty()
    var key by remember { mutableStateOf(existing) }
    var value by remember { mutableStateOf("") }
    suspend fun save(): String? {
        val op = SettingsOp.EnvSet(workspace, key.trim(), value)
        value = ""
        return editor.save(listOf(op))
    }
    FormDialog(
        if (isNew) "New Variable" else existing, "settings.env.form", key.isNotBlank() && value.isNotEmpty(), { save() }, close,
        remove = if (isNew) null else ({ editor.save(listOf(SettingsOp.EnvUnset(workspace, existing))) }),
        removeTitle = "Remove Variable",
    ) {
        Field(key, { key = it }, "Key", "env.key", enabled = isNew, mono = true, placeholder = "API_BASE")
        SecretField(value, { value = it }, if (isNew) "Value" else "New value", "env.value", isSet = !isNew)
    }
}
