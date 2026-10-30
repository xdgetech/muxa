package tech.xdge.muxa.app.ui

import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.session.Host
import tech.xdge.muxa.transport.HostKey
import tech.xdge.muxa.transport.SshTarget
import java.util.UUID

/** The editable fields of a host, as text. A new host has no saved id yet. */
data class HostDraft(
    val id: String,
    val isNew: Boolean,
    val name: String = "",
    val address: String = "",
    val port: String = "22",
    val user: String = "",
    val command: String = SshTarget.DEFAULT_COMMAND,
    val hostKey: HostKey? = null,
) {
    val trimmedAddress get() = address.trim()
    val trimmedUser get() = user.trim()

    val isValid: Boolean
        get() {
            val p = port.toIntOrNull() ?: return false
            return p in 1..65535 && trimmedAddress.isNotEmpty() && trimmedUser.isNotEmpty() && command.isNotEmpty() &&
                command.none { it == '"' || it == '\n' || it == '\r' }
        }

    /** The host to save. A changed address, port or user drops the trusted key: it belonged to the old server. */
    fun host(old: Host?): Host {
        val target = SshTarget(trimmedAddress, port.toIntOrNull() ?: 22, trimmedUser, command)
        return Host(id, name.trim(), if (old != null && keepsHostKey(old)) target.copy(hostKey = hostKey) else target)
    }

    fun keepsHostKey(old: Host) =
        old.target.host == trimmedAddress && old.target.port == port.toIntOrNull() && old.target.user == trimmedUser

    companion object {
        fun new() = HostDraft(UUID.randomUUID().toString(), isNew = true)

        fun of(h: Host) = HostDraft(
            h.id, false, h.name, h.target.host, h.target.port.toString(), h.target.user, h.target.command, h.target.hostKey,
        )
    }
}

@Composable
fun HostEditor(model: AppModel, initial: HostDraft, onDismiss: () -> Unit) {
    var draft by remember { mutableStateOf(initial) }
    var password by remember { mutableStateOf("") }
    var copied by remember { mutableStateOf(false) }
    val saved = model.hosts.host(draft.id)
    val keepsKey = saved?.let(draft::keepsHostKey) ?: false
    val canSetUp = draft.isValid && password.isNotEmpty()
    val context = LocalContext.current

    fun save() = model.hosts.upsert(draft.host(model.hosts.host(draft.id)))

    // Saves, closes, then runs the one-time key setup. The trust dialog and any error appear on the main screen.
    fun setUp() {
        save()
        val entered = password
        password = ""
        onDismiss()
        model.setUpKey(draft.id, entered)
    }

    // A new host connects right away; with a password, it adds this device's key first.
    fun connect() {
        if (password.isNotEmpty()) return setUp()
        save()
        onDismiss()
        model.connect(draft.id)
    }

    val plain = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false)
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (draft.isNew) "New Host" else "Edit Host") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(draft.name, { draft = draft.copy(name = it) }, Modifier.fillMaxWidth().testTag("host-name"),
                    label = { Text("Name") }, placeholder = { Text("Mac mini") }, singleLine = true)
                OutlinedTextField(draft.address, { draft = draft.copy(address = it) }, Modifier.fillMaxWidth().testTag("host-address"),
                    label = { Text("Address") }, placeholder = { Text("Hostname or IP address") }, singleLine = true,
                    keyboardOptions = plain.copy(keyboardType = KeyboardType.Uri))
                OutlinedTextField(draft.user, { draft = draft.copy(user = it) }, Modifier.fillMaxWidth().testTag("host-user"),
                    label = { Text("User") }, placeholder = { Text("Your login on the host") }, singleLine = true, keyboardOptions = plain)
                OutlinedTextField(draft.port, { draft = draft.copy(port = it) }, Modifier.fillMaxWidth(),
                    label = { Text("Port") }, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))

                Text("First Connection", style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 8.dp))
                OutlinedTextField(password, { password = it }, Modifier.fillMaxWidth(),
                    label = { Text("Password for ${draft.trimmedUser.ifEmpty { "the user" }}") }, singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Go),
                    keyboardActions = KeyboardActions(onGo = { if (canSetUp) setUp() }))
                OutlinedButton(::setUp, enabled = canSetUp) { Text("Add This Device's Key and Connect") }
                Text(
                    "Logs in once with your password and adds this device's key to ~/.ssh/authorized_keys, restricted to the bridge. The password is not stored.",
                    style = MaterialTheme.typography.bodySmall,
                )

                model.authorizedKeyLine(draft.command)?.let { line ->
                    Text("This Device's Key", style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 8.dp))
                    SelectionContainer { Text(line, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall) }
                    TextButton({
                        context.getSystemService(ClipboardManager::class.java)?.setPrimaryClip(ClipData.newPlainText("muxa key", line))
                        copied = true
                    }) { Text(if (copied) "Copied" else "Copy Key Line") }
                    Text("Or add this line to ~/.ssh/authorized_keys on the host yourself.", style = MaterialTheme.typography.bodySmall)
                }

                OutlinedTextField(draft.command, { draft = draft.copy(command = it) }, Modifier.fillMaxWidth(),
                    label = { Text("Bridge command") }, singleLine = true, keyboardOptions = plain,
                    textStyle = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
                    supportingText = { Text("Where muxa-bridge is installed on the host. Change it only if you installed it elsewhere.") })

                val key = draft.hostKey
                if (key != null && keepsKey) {
                    Text("Trusted Host Key", style = MaterialTheme.typography.titleSmall)
                    SelectionContainer { Text(key.fingerprint, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall) }
                    TextButton({ draft = draft.copy(hostKey = null) }) { Text("Forget Host Key", color = Theme.failure) }
                }
                if (!draft.isNew) {
                    TextButton({
                        model.hosts.delete(draft.id)
                        onDismiss()
                    }) { Text("Delete Host", color = Theme.failure) }
                }
            }
        },
        confirmButton = {
            if (draft.isNew) {
                TextButton(::connect, enabled = draft.isValid, modifier = Modifier.testTag("host-connect")) { Text("Connect") }
            } else {
                TextButton({ save(); onDismiss() }, enabled = draft.isValid) { Text("Save") }
            }
        },
        dismissButton = { TextButton(onDismiss) { Text("Cancel") } },
    )
}

@Composable
fun TrustHostDialog(model: AppModel, key: HostKey) {
    // The host had a trusted key, and now presents another one.
    val changed = model.currentHost?.target?.hostKey != null
    AlertDialog(
        // Only Trust or Cancel closes it, so a held setup password is always cleared.
        onDismissRequest = {},
        title = { Text(if (changed) "Host Key Changed" else "New Host Key") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (changed) {
                    Text(
                        "If you did not reinstall or reconfigure the host, do not trust this key. Another machine may be answering at this address.",
                        color = Theme.failure,
                    )
                }
                Text(model.currentHost?.title ?: "", style = MaterialTheme.typography.titleSmall)
                SelectionContainer { Text(key.fingerprint, fontFamily = FontFamily.Monospace) }
                Text(
                    "Check that it matches the output of `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` on the host.",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        },
        confirmButton = {
            TextButton({ model.trust(key) }) { Text(if (changed) "Trust Anyway" else "Trust", color = if (changed) Theme.failure else Theme.accent) }
        },
        dismissButton = { TextButton(model::cancelTrust) { Text("Cancel") } },
    )
}
