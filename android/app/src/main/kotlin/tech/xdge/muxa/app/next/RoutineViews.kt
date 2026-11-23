package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.PrimaryButton
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.Routine

/** `09:00 Europe/Berlin`, `09:00 Europe/Berlin · 0 9 * * 1-5`, `On review`, `On request` or `Off`. */
val Routine.whenText: String
    get() {
        if (!enabled) return "Off"
        if (on == RoutineDraft.REVIEW) return "On review"
        val at = at ?: return "On request"
        val base = "$at ${timezone.orEmpty()}".trim()
        return schedule?.let { "$base · $it" } ?: base
    }

private enum class Health { RUNNING, FAILED, OK, NEVER }

private val Routine.health: Health
    get() = when {
        runningSince != null -> Health.RUNNING
        last == null -> Health.NEVER
        last?.ok == true -> Health.OK
        else -> Health.FAILED
    }

@Composable
fun RoutineRow(routine: Routine, modifier: Modifier = Modifier) {
    Row(
        modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp).testTag("routine.${routine.name}"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(routine.name)
            Text(
                "${routine.workspace} · ${routine.whenText}",
                style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        when (routine.health) {
            Health.RUNNING -> Text("Running", color = Theme.accent, style = MaterialTheme.typography.labelMedium)
            Health.FAILED -> Text("Failed", color = Theme.failure, style = MaterialTheme.typography.labelMedium)
            Health.OK -> Icon(Icons.done, "OK", tint = Theme.connected)
            Health.NEVER -> {}
        }
    }
}

@Composable
fun RoutinePane(model: NextModel, routine: Routine) {
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(routine.name, style = MaterialTheme.typography.titleLarge)
        Fact("Workspace", routine.workspace)
        routine.bot?.let { Fact("Bot", it) }
        Fact("Runs", routine.whenText)
        routine.nextDue?.let { Fact("Next run", time(it)) }
        routine.runningSince?.let { Fact("Running since", time(it)) }
        routine.last?.let { last ->
            HorizontalDivider()
            Text("Last run", style = MaterialTheme.typography.titleSmall)
            Fact("At", time(last.at))
            Fact("Result", if (last.ok) "OK" else "Failed")
            last.published?.let { Fact("Items", "$it") }
            last.reason?.let { Text(it, color = Theme.failure) }
        }
        HorizontalDivider()
        PrimaryButton(
            "Run Now",
            { model.runRoutine(routine) },
            Modifier.testTag("routine.run"),
            enabled = routine.enabled && routine.runningSince == null,
        )
        model.requestError?.let { Text(it, color = Theme.failure) }
    }
}

@Composable
private fun Fact(label: String, value: String) {
    Row(Modifier.fillMaxWidth()) {
        Text(label, Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(value)
    }
}

private fun time(unix: Long): String =
    DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM, FormatStyle.SHORT)
        .format(Instant.ofEpochSecond(unix).atZone(ZoneId.systemDefault()))

