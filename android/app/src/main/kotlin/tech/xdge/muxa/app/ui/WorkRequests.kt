package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.askAssistant
import tech.xdge.muxa.app.canCapture
import tech.xdge.muxa.app.cancelRequest
import tech.xdge.muxa.app.openRunSession
import tech.xdge.muxa.app.runSession
import tech.xdge.muxa.app.workItem
import tech.xdge.muxa.protocol.AssistResult
import tech.xdge.muxa.protocol.AssistScope
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.ResultSource
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.session.assistProblem
import tech.xdge.muxa.session.canCancel
import tech.xdge.muxa.session.currentResults
import tech.xdge.muxa.session.earlierResults
import tech.xdge.muxa.session.isActive
import tech.xdge.muxa.session.isClosed
import tech.xdge.muxa.session.label
import tech.xdge.muxa.session.runMayLive

// A Work item's assistant requests and their Results (ADR 0048 §10). Result files are plain paths:
// the Reader needs a pane and cannot open them yet.

/** One request: its scope, version and delivery state with its age, then its Results. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun RequestSection(model: AppModel, item: WorkItem, request: AssistantRequest) {
    var error by remember(request.id) { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val busy = model.changingWork != null
    Column(Modifier.testTag("request-${request.id}"), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        DetailHeader("Assistant · ${request.scope.label} · v${request.version}")
        Text(
            listOf(request.delivery.label, AgentSummary.age(request.deliveryAt), request.workspace).joinToString(" · "),
            Modifier.testTag("request-delivery-${request.id}"),
            style = MaterialTheme.typography.bodySmall,
            color = deliveryColor(request.delivery),
        )
        request.purpose?.let { DetailNote(it) }
        val session = model.runSession(request)?.takeIf { request.runMayLive }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            if (request.canCancel) {
                TextButton({
                    scope.launch { error = model.cancelRequest(item, request) }
                }, Modifier.testTag("request-cancel-${request.id}"), enabled = !busy) { Text("Cancel") }
            }
            if (session != null) {
                TextButton({ model.openRunSession(session) }, Modifier.testTag("request-session-${request.id}")) { Text("Open Session") }
            }
        }
        error?.let { Text(it, Modifier.testTag("request-error-${request.id}"), color = Theme.failure) }
        // A chain (ADR 0051) shows each step with its own Result; a request without steps is one.
        if (request.steps.isEmpty()) {
            for (result in request.currentResults) {
                ResultView(model, request, result)
                ResultWrites(model, request, result)
            }
        } else {
            ChainSection(model, request)
        }
        val earlier = request.earlierResults
        if (earlier.isNotEmpty()) {
            DetailHeader("Earlier")
            for (result in earlier) {
                Column(Modifier.testTag("result-earlier-${request.id}-${result.version}")) {
                    DetailNote("v${result.version} · ${AgentSummary.age(result.at)}")
                    Text(result.summary, style = MaterialTheme.typography.bodySmall)
                }
                // An older Result's write item stays open: its text is fixed (ADR 0052 §2).
                ResultWrites(model, request, result)
            }
        }
    }
}

/** Green with a result, red when it ended without one, orange while it may still come. */
internal fun deliveryColor(d: Delivery) = when (d) {
    Delivery.RESULTED -> Theme.connected
    Delivery.FAILED, Delivery.TIMED_OUT, Delivery.CANCELLED, Delivery.UNCONFIRMED -> Theme.failure
    else -> Theme.attention
}

/** Summary, findings with their sources, open questions, next actions, files and possible duplicates. [tag] names it. */
@Composable
internal fun ResultView(model: AppModel, request: AssistantRequest, result: AssistResult, tag: String = "result-${request.id}-${result.version}") {
    SelectionContainer {
        Column(Modifier.testTag(tag), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            DetailNote("Result · v${result.version} · ${AgentSummary.age(result.at)}")
            Text(result.summary, Modifier.testTag(if (result.step == null) "result-summary-${request.id}" else "$tag-summary"))
            if (result.findings.isNotEmpty()) {
                DetailHeader("Findings")
                for (f in result.findings) {
                    Text("• ${f.text}")
                    for (s in f.sources) DetailNote(sourceLine(s), Modifier.padding(start = 12.dp))
                }
            }
            Bullets("Open questions", result.openQuestions)
            Bullets("Next", result.next)
            Bullets("Files", result.files)
            if (result.possibleDuplicates.isNotEmpty()) {
                DetailHeader("Possible duplicates")
                for (d in result.possibleDuplicates) {
                    val title = model.workItem(d.work)?.title ?: d.work
                    Text("• $title: ${d.reason}", Modifier.testTag("result-duplicate-${d.work}"))
                }
            }
        }
    }
}

@Composable
private fun Bullets(header: String, lines: List<String>) {
    if (lines.isEmpty()) return
    DetailHeader(header)
    for (line in lines) Text("• $line")
}

/** A source by its title, then its URL or path, then when the run read it. */
private fun sourceLine(s: ResultSource): String = listOfNotNull(
    s.title,
    s.url ?: s.path,
    s.readAt?.let { "read ${AgentSummary.age(it)} ago" },
).joinToString(" · ")

/**
 * Ask Assistant while no request of the item is active; Ask again for a new version once one ran.
 * An item without a workspace asks for one. A refusal shows the host's message.
 */
@Composable
internal fun AskPanel(model: AppModel, item: WorkItem, requests: List<AssistantRequest>) {
    if (!model.canCapture || item.isClosed || requests.any { it.isActive }) return
    val newest = requests.maxByOrNull { it.updatedAt }
    var scopeChoice by remember(item.id) { mutableStateOf(newest?.scope ?: AssistScope.RESEARCH) }
    var purpose by remember(item.id) { mutableStateOf("") }
    var picked by remember(item.id) { mutableStateOf<String?>(null) }
    var error by remember(item.id) { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val workspace = picked ?: newest?.workspace ?: item.workspace
    // A member belongs to one workspace's staff: another workspace starts with its lead again.
    var worker by remember(item.id, workspace) { mutableStateOf<String?>(null) }
    val problem = assistProblem(workspace, purpose)
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        DetailHeader(if (newest == null) "Ask the assistant" else "Ask again")
        ScopePicker(scopeChoice, "ask-scope") { scopeChoice = it }
        if (item.workspace == null && newest == null) {
            WorkspacePicker(model, picked, "Workspace", "ask-workspace") { picked = it }
        }
        StaffPicker(model, workspace, worker, "ask-staff") { worker = it }
        WriterNote(model, workspace, "ask-writer")
        MemoryNote(model, workspace, "ask-memory")
        OutlinedTextField(
            purpose, {
                purpose = it
                error = null
            },
            Modifier.fillMaxWidth().testTag("ask-purpose"),
            placeholder = { Text("What to find out (optional)") }, minLines = 2,
        )
        TextButton(
            {
                scope.launch {
                    error = model.askAssistant(item, scopeChoice, purpose.trim().ifEmpty { null }, workspace.takeIf { it != item.workspace }, worker)
                    if (error == null) purpose = ""
                }
            },
            Modifier.testTag("work-ask"),
            enabled = problem == null && model.changingWork == null,
        ) { Text(if (newest == null) "Ask Assistant" else "Ask again") }
        (error ?: problem?.takeIf { purpose.isNotEmpty() || workspace == null })?.let {
            Text(it, Modifier.testTag("ask-error"), color = Theme.failure)
        }
    }
}
