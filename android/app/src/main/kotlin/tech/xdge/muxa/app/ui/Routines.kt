package tech.xdge.muxa.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.InboxPage
import tech.xdge.muxa.app.routine
import tech.xdge.muxa.app.runNow
import tech.xdge.muxa.app.runSession
import tech.xdge.muxa.protocol.RoutineInfo
import tech.xdge.muxa.session.key
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/**
 * The host's Routines (ADR 0026). They are configured in `workspaces.toml` on the host;
 * the app shows their state and starts a run now.
 */
@Composable
fun RoutineList(model: AppModel, modifier: Modifier) {
    val routines = model.state.routines
    Column(modifier) {
        Row(Modifier.fillMaxWidth().padding(end = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton({ model.inboxPage = InboxPage.TASKS }, Modifier.testTag("routines-back")) { Icon(Icons.back, "All Tasks") }
            Text("Routines", style = MaterialTheme.typography.titleMedium)
        }
        model.state.workError?.let { Text(it, Modifier.padding(horizontal = 16.dp), color = Theme.failure) }
        if (routines.isEmpty() && model.state.workError == null) {
            Column(Modifier.fillMaxWidth().padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Text("No Routines", style = MaterialTheme.typography.titleMedium)
                Text("Add [[routine]] to workspaces.toml on the host.", style = MaterialTheme.typography.bodySmall)
            }
        }
        LazyColumn(Modifier.fillMaxWidth().weight(1f).testTag("routine-list")) {
            items(routines, key = { it.key }) { RoutineRow(model, it) }
        }
    }
}

@Composable
private fun RoutineRow(model: AppModel, routine: RoutineInfo) {
    Column(
        Modifier.fillMaxWidth()
            .clickable {
                model.clearInboxSelection()
                model.selectedRoutine = routine.key
            }
            .padding(horizontal = 16.dp, vertical = 10.dp)
            .testTag("routine-${routine.key}"),
    ) {
        Text(routine.name, fontWeight = FontWeight.SemiBold)
        Text("${routine.workspace} · ${routine.triggerTitle}", style = MaterialTheme.typography.bodySmall)
        Text(routine.stateTitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** One Routine: its configuration, its runs, and Run now. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RoutineDetail(model: AppModel, key: String, modifier: Modifier) {
    val r = model.routine(key)
    Column(modifier) {
        TopAppBar(
            title = { Text(r?.name ?: "", maxLines = 1, overflow = TextOverflow.Ellipsis) },
            navigationIcon = {
                if (LocalPhoneLayout.current) {
                    IconButton({ model.selectedRoutine = null }, Modifier.testTag("routine-back")) { Icon(Icons.back, "Routines") }
                }
            },
        )
        if (r == null) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text("The host no longer lists this Routine.") }
            return@Column
        }
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp).testTag("routine-detail"),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            RoutineField("Workspace", r.workspace, "routine-workspace")
            RoutineField("Trigger", r.triggerTitle, "routine-trigger")
            RoutineField("Enabled", if (r.enabled) "Yes" else "No", "routine-enabled")
            RoutineField("Worker", r.worker ?: "Workspace default", "routine-worker")
            r.project?.let { RoutineField("Project", it, "routine-project") }
            Text("Runs", style = MaterialTheme.typography.titleSmall)
            Runs(model, r)
            RunNow(model, r)
        }
    }
}

@Composable
private fun Runs(model: AppModel, r: RoutineInfo) {
    r.running?.let { run ->
        RoutineField("Running", "since ${shownDate(run.startedAt)}", "routine-running")
        model.runSession(r)?.let { s ->
            OutlinedButton({ model.openTerminal(s) }, Modifier.testTag("routine-open-terminal")) {
                Icon(Icons.agent, null)
                Text("Open Terminal", Modifier.padding(start = 6.dp))
            }
        }
    }
    RoutineField("Next due", r.nextDue?.let(::shownDate) ?: "Not scheduled", "routine-next-due")
    RoutineField("Last success", r.lastSuccess?.let { "${shownDate(it.at)} · ${it.published} new" } ?: "None yet", "routine-last-success")
    val failure = r.lastFailure
    if (failure == null) {
        RoutineField("Last failure", "None", "routine-last-failure")
    } else {
        RoutineField("Last failure", shownDate(failure.at), "routine-last-failure")
        Text(failure.reason, Modifier.testTag("routine-failure-reason"), style = MaterialTheme.typography.bodySmall, color = Theme.failure)
    }
}

@Composable
private fun RunNow(model: AppModel, r: RoutineInfo) {
    var starting by remember(r.key) { mutableStateOf(false) }
    Button(
        {
            starting = true
            model.runNow(r) { starting = false }
        },
        Modifier.testTag("routine-run-now"),
        enabled = r.enabled && r.running == null && !starting,
    ) {
        Icon(Icons.run, null)
        Text("Run Now", Modifier.padding(start = 6.dp))
    }
    val note = when {
        !r.enabled -> "Enable it in workspaces.toml on the host. Nothing runs while it is disabled."
        r.running != null -> "A Routine runs once at a time."
        else -> "Starts a run on the host now. Its briefings and proposals come to Inbox."
    }
    Text(note, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

/** A labeled value, one node for tests and screen readers. */
@Composable
private fun RoutineField(label: String, value: String, tag: String) {
    Column(Modifier.semantics(mergeDescendants = true) {}.testTag(tag)) {
        Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(value)
    }
}

val RoutineInfo.triggerTitle: String get() = trigger?.let { "${it.at} ${it.timezone}" } ?: "Run now only"

/** Running, disabled, or when it runs next. */
val RoutineInfo.stateTitle: String
    get() {
        val next = nextDue
        return when {
            running != null -> "Running"
            !enabled -> "Disabled"
            next != null -> "Next ${shownDate(next)}"
            else -> "Enabled"
        }
    }

/** Epoch seconds as a short local date and time. */
fun shownDate(epochSeconds: Long): String =
    DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM, FormatStyle.SHORT)
        .format(Instant.ofEpochSecond(epochSeconds).atZone(ZoneId.systemDefault()))
