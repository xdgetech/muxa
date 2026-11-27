package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.SecondaryButton
import tech.xdge.muxa.v2.DiskRow
import tech.xdge.muxa.v2.Terminal

/** A live Task's place on disk: its worktree or folder and the build output beside it. */
data class TaskDiskUse(val terminal: Terminal, val bytes: Long, val buildBytes: Long)

/** Rows nothing live holds, summed by what they are. */
data class Leftovers(val kind: LeftoverKind, val count: Int, val bytes: Long, val cleanable: Int)

enum class LeftoverKind(val title: String, val tag: String) {
    CLAUDE_WORKTREES("Claude Code worktrees", "claude-worktree"),
    OTHER_WORKTREES("Other worktrees", "worktree"),
    BUILD_FOLDERS("Build folders with no place", "build-folder"),
    LOOSE_TASK_FOLDERS("Loose Task folders", "task-folder"),
}

data class DiskUse(val tasks: List<TaskDiskUse>, val leftovers: List<Leftovers>)

/**
 * Splits `disk`'s rows into the live Tasks' places, matched by path to each agent's
 * `muxa.places` or `muxa.dir`, and the leftovers by kind.
 */
fun diskUse(rows: List<DiskRow>, terminals: List<Terminal>): DiskUse {
    val agents = terminals.filter { it.live && it.isTaskAgent }
    fun owner(path: String) = agents.firstOrNull { t -> t.taskDir == path || t.places.any { it.path == path } }
    val tasks = mutableListOf<TaskDiskUse>()
    val loose = mutableMapOf<LeftoverKind, List<DiskRow>>()
    for (row in rows) {
        val task = row.kind in TASK_KINDS
        val t = if (task) owner(row.path) else null
        if (t != null) {
            tasks += TaskDiskUse(t, row.bytes ?: 0, row.buildBytes ?: 0)
            continue
        }
        val kind = when (row.kind) {
            "claude-worktree" -> LeftoverKind.CLAUDE_WORKTREES
            "build-folder" -> LeftoverKind.BUILD_FOLDERS
            "task-folder" -> LeftoverKind.LOOSE_TASK_FOLDERS
            else -> LeftoverKind.OTHER_WORKTREES
        }
        loose[kind] = loose[kind].orEmpty() + row
    }
    val leftovers = LeftoverKind.entries.mapNotNull { k ->
        val list = loose[k] ?: return@mapNotNull null
        Leftovers(k, list.size, list.sumOf { (it.bytes ?: 0) + (it.buildBytes ?: 0) }, list.count { it.cleanable })
    }
    return DiskUse(tasks, leftovers)
}

private val TASK_KINDS = setOf("task-worktree", "task-folder")

/** Settings › Host resources' disk part: a scan on request, since `du` over every place is slow. */
@Composable
fun HostDiskSection(model: NextModel) {
    val scope = rememberCoroutineScope()
    var rows by remember { mutableStateOf<List<DiskRow>?>(null) }
    var scanning by remember { mutableStateOf(false) }
    var failure by remember { mutableStateOf<String?>(null) }
    SettingsHeader("Disk use of Task places")
    SettingsCard {
        Row(Modifier.padding(14.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            SecondaryButton(if (rows == null) "Scan disk" else "Scan again", {
                scanning = true
                scope.launch {
                    val scanned = model.disk()
                    if (scanned != null) rows = scanned
                    failure = if (scanned == null) model.requestError ?: "The scan failed. Try again." else null
                    scanning = false
                }
            }, Modifier.testTag("host.scan"), enabled = !scanning)
            if (scanning) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
        }
        rows?.let { DiskRows(diskUse(it, model.terminals)) }
    }
    failure?.let { SettingsWarning(it, "host.scan.error") }
    SettingsNote("Remove leftovers with muxa-cli disk clean.")
}

@Composable
private fun DiskRows(use: DiskUse) {
    if (use.tasks.isEmpty() && use.leftovers.isEmpty()) {
        CardDivider()
        SettingsNote("No Task places or leftovers on this host.", Modifier.padding(vertical = 10.dp))
        return
    }
    for (u in use.tasks) {
        CardDivider()
        val build = if (u.buildBytes > 0) " + ${size(u.buildBytes)} build" else ""
        DiskLine(u.terminal.title, "${size(u.bytes)}$build", "host.disk.task.${u.terminal.id}")
    }
    for (l in use.leftovers) {
        CardDivider()
        val parts = listOfNotNull(count(l.count, "folder"), size(l.bytes), if (l.cleanable > 0) "${l.cleanable} cleanable" else null)
        DiskLine(l.kind.title, parts.joinToString(" · "), "host.disk.leftover.${l.kind.tag}")
    }
}

@Composable
private fun DiskLine(title: String, detail: String, tag: String) {
    Column(Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 14.dp, vertical = 8.dp)
            .semantics(mergeDescendants = true) {}.testTag(tag)) {
        Text(title, fontSize = 15.sp, fontWeight = FontWeight.Medium, maxLines = 1)
        Text(detail, color = Palette.textTertiary, fontSize = 12.5.sp)
    }
}
