package tech.xdge.muxa.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.session.RepoChoice

/**
 * The New Task sheet's repository: a field that shows the chosen repo by its short name and
 * opens a searchable list of [choices]. [other] picks "Other path", for a repo not listed.
 */
@Composable
fun RepoChoiceField(choices: List<RepoChoice>, selected: String, otherPath: Boolean, pick: (String) -> Unit, other: () -> Unit) {
    var picking by remember { mutableStateOf(false) }
    val shown = if (otherPath) "Other path" else choices.firstOrNull { it.dir == selected }?.name ?: "Other path"
    Box {
        OutlinedTextField(
            shown, {}, Modifier.fillMaxWidth().testTag("record-repo-choice"),
            readOnly = true, singleLine = true, label = { Text("Repository") },
            trailingIcon = { Icon(Icons.chevron, null) },
        )
        // A read-only field takes no clicks; this layer opens the list.
        Box(Modifier.matchParentSize().clickable { picking = true })
    }
    if (picking) {
        RepoList(choices, onDismiss = { picking = false }, pick = {
            picking = false
            pick(it)
        }, other = {
            picking = false
            other()
        })
    }
}

@Composable
private fun RepoList(choices: List<RepoChoice>, onDismiss: () -> Unit, pick: (String) -> Unit, other: () -> Unit) {
    var query by remember { mutableStateOf("") }
    val shown = choices.filter { query.isBlank() || it.name.contains(query.trim(), ignoreCase = true) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Repository") },
        text = {
            Column {
                OutlinedTextField(
                    query, { query = it }, Modifier.fillMaxWidth().testTag("repo-search"),
                    placeholder = { Text("Search") }, singleLine = true,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
                )
                LazyColumn(Modifier.heightIn(max = 360.dp)) {
                    items(shown, key = { it.dir }) { choice ->
                        Text(
                            choice.name,
                            Modifier.fillMaxWidth().clickable { pick(choice.dir) }.padding(vertical = 12.dp).testTag("repo-${choice.name}"),
                        )
                    }
                    item(key = "other") {
                        Text(
                            "Other path",
                            Modifier.fillMaxWidth().clickable(onClick = other).padding(vertical = 12.dp).testTag("repo-other"),
                            color = MaterialTheme.colorScheme.primary,
                        )
                    }
                }
            }
        },
        confirmButton = { TextButton(onDismiss) { Text("Cancel") } },
    )
}
