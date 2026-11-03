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
import tech.xdge.muxa.protocol.WorkerSelection

@Composable
fun WorkerPicker(workers: List<WorkerInfo>, selected: String?, tag: String, pick: (String?) -> Unit) {
    WorkerMenu(selected ?: "Workspace default", tag) { close ->
        DropdownMenuItem({ Text("Workspace default") }, { pick(null); close() })
        for (worker in workers) {
            DropdownMenuItem({ Text(worker.name) }, { pick(worker.name); close() })
        }
    }
}

@Composable
fun WorkerRestartPicker(workers: List<WorkerInfo>, current: String?, selected: WorkerSelection, pick: (WorkerSelection) -> Unit) {
    val keep = "Keep current (${current ?: "Workspace default"})"
    val label = when (selected) {
        WorkerSelection.Keep -> keep
        WorkerSelection.WorkspaceDefault -> "Workspace default"
        is WorkerSelection.Named -> selected.name
    }
    WorkerMenu(label, "restart-worker") { close ->
        DropdownMenuItem({ Text(keep) }, { pick(WorkerSelection.Keep); close() })
        DropdownMenuItem({ Text("Workspace default") }, { pick(WorkerSelection.WorkspaceDefault); close() })
        for (worker in workers) {
            DropdownMenuItem({ Text(worker.name) }, { pick(WorkerSelection.Named(worker.name)); close() })
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
