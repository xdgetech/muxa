package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.MenuAnchorType
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import tech.xdge.muxa.protocol.WorkerInfo
import tech.xdge.muxa.protocol.DEFAULT_WORKER
import tech.xdge.muxa.session.workerLabel

/** Picks a worker name or `default` (ADR 0027); [current] marks the Task's worker on restart. */
@Composable
fun WorkerPicker(workers: List<String>, selected: String, tag: String, current: String? = null, pick: (String) -> Unit) {
    fun label(name: String) = if (name == current) "${workerLabel(name)} (current)" else workerLabel(name)
    WorkerMenu(label(selected), tag) { close ->
        for (name in listOf(DEFAULT_WORKER) + workers) {
            DropdownMenuItem({ Text(label(name)) }, { pick(name); close() })
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun WorkerMenu(value: String, tag: String, items: @Composable (() -> Unit) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    ExposedDropdownMenuBox(expanded, { expanded = it }) {
        OutlinedTextField(
            value, {}, Modifier.fillMaxWidth().menuAnchor(MenuAnchorType.PrimaryNotEditable).testTag(tag),
            readOnly = true, singleLine = true, label = { Text("Worker") },
            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded) },
        )
        ExposedDropdownMenu(expanded, { expanded = false }) { items { expanded = false } }
    }
}
