package tech.xdge.muxa.protocol

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertEquals
import kotlinx.serialization.SerializationException
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Test
import java.io.File

/**
 * `protocol/fixtures` is the contract: every line decodes and re-encodes to the same
 * JSON value. Key order may differ, so the trees are compared.
 */
class FixtureTest {
    private val fixtures = File(System.getProperty("muxa.fixtures"))

    private fun lines(name: String) = fixtures.resolve(name).readLines().filter { it.isNotEmpty() }

    private fun tree(line: String): JsonElement = sorted(Json.parseToJsonElement(line))

    private fun sorted(e: JsonElement): JsonElement = when (e) {
        is JsonObject -> JsonObject(e.toSortedMap().mapValues { sorted(it.value) })
        is JsonArray -> JsonArray(e.map(::sorted))
        else -> e
    }

    @Test
    fun requestsRoundTrip() {
        val lines = lines("requests.jsonl")
        assertEquals(34, lines.size)
        for (line in lines) {
            assertEquals(line, tree(line), tree(Wire.encode(Wire.decodeRequest(line))))
        }
    }

    @Test
    fun messagesRoundTrip() {
        val lines = lines("messages.jsonl")
        assertEquals(39, lines.size)
        for (line in lines) {
            val message = Wire.decodeMessage(line)
            assertFalse("fixture type is unknown: $line", message is Message.Unknown)
            assertEquals(line, tree(line), tree(Wire.encode(message)))
        }
    }

    @Test
    fun workerContractRoundTripsAWorkerNameOrNone() {
        val requests = lines("workers/requests.jsonl")
        assertEquals(7, requests.size)
        requests.forEach { assertEquals(it, tree(it), tree(Wire.encode(Wire.decodeRequest(it)))) }
        val messages = lines("workers/messages.jsonl")
        assertEquals(5, messages.size)
        messages.forEach { assertEquals(it, tree(it), tree(Wire.encode(Wire.decodeMessage(it)))) }
        val expected = listOf(null, null, DEFAULT_WORKER, "Reviewer")
        requests.subList(2, 6).zip(expected).forEach { (line, selection) ->
            assertEquals(selection, (Wire.decodeRequest(line) as Request.RestartAgent).worker)
        }
        for (bad in listOf("true", "17", "{}", "[]")) {
            assertThrows(SerializationException::class.java) {
                Wire.decodeRequest("""{"type":"restart-agent","id":1,"task":"t-000001","worker":$bad}""")
            }
        }
        // A legacy run has no worker: unrecorded, not the workspace default.
        val list = Wire.decodeMessage(messages[1]) as Message.TaskList
        assertEquals(listOf(null, DEFAULT_WORKER, "Reviewer"), list.entries[0].runs.map { it.worker })
    }

    @Test
    fun restartRecoveryRoundTrips() {
        for (name in listOf("restart-recovery/requests.jsonl", "restart-recovery/messages.jsonl")) {
            val entries = lines(name)
            assertFalse(entries.isEmpty())
            entries.forEach {
                val encoded = if (name.contains("requests")) Wire.encode(Wire.decodeRequest(it)) else Wire.encode(Wire.decodeMessage(it))
                assertEquals(it, tree(it), tree(encoded))
            }
        }
    }

    @Test
    fun askCloseContractRoundTrips() {
        val requests = lines("ask-close/requests.jsonl")
        assertEquals(2, requests.size)
        requests.forEach { assertEquals(it, tree(it), tree(Wire.encode(Wire.decodeRequest(it)))) }
        val messages = lines("ask-close/messages.jsonl")
        assertEquals(4, messages.size)
        messages.forEach {
            val message = Wire.decodeMessage(it)
            assertFalse("fixture type is unknown: $it", message is Message.Unknown)
            assertEquals(it, tree(it), tree(Wire.encode(message)))
        }
        assertEquals(WORK_CLOSE_ASK, workLevel((Wire.decodeMessage(messages[0]) as Message.Hello).capabilities))
        val listed = (Wire.decodeMessage(messages[1]) as Message.Result).sessions.orEmpty()
        assertEquals(listOf(null, "lisk"), listed.map { it.ask })
    }

    @Test
    fun workspaceContractRoundTrips() {
        val requests = lines("workspaces/requests.jsonl")
        assertEquals(2, requests.size)
        requests.forEach { assertEquals(it, tree(it), tree(Wire.encode(Wire.decodeRequest(it)))) }
        val messages = lines("workspaces/messages.jsonl")
        assertEquals(3, messages.size)
        messages.forEach {
            val message = Wire.decodeMessage(it)
            assertFalse("fixture type is unknown: $it", message is Message.Unknown)
            assertEquals(it, tree(it), tree(Wire.encode(message)))
        }
        assertEquals(WORK_ASK, workLevel((Wire.decodeMessage(messages[0]) as Message.Hello).capabilities))
        val listing = Wire.decodeMessage(messages[1]) as Message.TaskList
        assertEquals("/work/lisk", listing.workspaces[0].root)
        assertEquals(emptyList<String>(), listing.workspaces[1].repos)
        assertEquals(
            Message.SessionStarted(start = 41, session = "$7", name = "ask-cto-copilot-1"),
            Wire.decodeMessage(messages[2]),
        )
        assertEquals(
            Request.StartSession(id = 41, workspace = "lisk", repo = "cto-copilot"),
            Wire.decodeRequest(requests[0]),
        )
    }

    @Test
    fun routineContractRoundTrips() {
        val requests = lines("routines/requests.jsonl")
        assertEquals(6, requests.size)
        requests.forEach { assertEquals(it, tree(it), tree(Wire.encode(Wire.decodeRequest(it)))) }
        val messages = lines("routines/messages.jsonl")
        assertEquals(6, messages.size)
        messages.forEach {
            val message = Wire.decodeMessage(it)
            assertFalse("fixture type is unknown: $it", message is Message.Unknown)
            assertEquals(it, tree(it), tree(Wire.encode(message)))
        }
        assertEquals(WORK_ROUTINES, workLevel((Wire.decodeMessage(messages[0]) as Message.Hello).capabilities))
        assertEquals(2, workLevel(listOf("work:2", "work:x", "workers")))
        assertEquals(0, workLevel(listOf("task-records", "work")))
        val inbox = Wire.decodeMessage(messages[3]) as Message.InboxList
        assertEquals(listOf("open", "later"), inbox.entries.map { it.disposition })
        assertEquals(InboxAction.LATER, (Wire.decodeRequest(requests[5]) as Request.UpdateInbox).action)
        assertEquals(emptyList<InboxItem>(), inbox.items)
    }

    @Test
    fun itemContractRoundTrips() {
        val requests = lines("items/requests.jsonl")
        assertEquals(16, requests.size)
        requests.forEach { assertEquals(it, tree(it), tree(Wire.encode(Wire.decodeRequest(it)))) }
        val messages = lines("items/messages.jsonl")
        assertEquals(4, messages.size)
        messages.forEach {
            val message = Wire.decodeMessage(it)
            assertFalse("fixture type is unknown: $it", message is Message.Unknown)
            assertEquals(it, tree(it), tree(Wire.encode(message)))
        }
        assertEquals(WORK_ITEMS, workLevel((Wire.decodeMessage(messages[0]) as Message.Hello).capabilities))
        val list = Wire.decodeMessage(messages[1]) as Message.InboxList
        assertEquals(emptyList<InboxEntry>(), list.entries)
        assertEquals(
            listOf(ItemKind.ASK, ItemKind.REVIEW, ItemKind.FAILURE, ItemKind.PROPOSAL, ItemKind.REVIEW, ItemKind.ASK),
            list.items.map { it.kind },
        )
        assertEquals(
            listOf(ItemState.OPEN, ItemState.OPEN, ItemState.OPEN, ItemState.OPEN, ItemState.ANSWERED, ItemState.RESOLVED),
            list.items.map { it.state },
        )
        assertEquals(1_762_160_500L, list.items[2].laterAt)
        assertEquals(
            listOf(InboxAction.ACCEPT, InboxAction.REPLY, InboxAction.REDO, InboxAction.CHANGE_COURSE, InboxAction.KEEP_OPEN, InboxAction.LATER),
            list.items[1].answers,
        )
        assertEquals(ChangeCount(files = 4, added = 120, removed = 40, scope = "branch"), list.items[1].attachments.changes)
        assertEquals(InboxAction.ACCEPT, list.items[4].answer)
        assertEquals(emptyList<InboxAction>(), list.items[4].answers)
        val itemOnly = Wire.decodeMessage(messages[2]) as Message.InboxSaved
        assertEquals(null, itemOnly.entry)
        assertEquals(ItemState.ANSWERED, itemOnly.item?.state)
        val both = Wire.decodeMessage(messages[3]) as Message.InboxSaved
        assertEquals(both.entry?.id, both.item?.id)
        val choose = Wire.decodeRequest(requests[3]) as Request.UpdateInbox
        assertEquals(InboxAction.CHOOSE to 1, choose.action to choose.option)
        val redo = Wire.decodeRequest(requests[6]) as Request.UpdateInbox
        assertEquals(InboxAction.REDO, redo.action)
        assertFalse(redo.text.isNullOrEmpty())
    }

    /** ADR 0033 §4a: `reply` with a text, and the evidence a summary names (files and links). */
    @Test
    fun replyContractRoundTrips() {
        val replies = lines("items/requests.jsonl").mapNotNull { Wire.decodeRequest(it) as? Request.UpdateInbox }
            .filter { it.action == InboxAction.REPLY }
        assertEquals(3, replies.size)
        assertEquals("Looks good. Also update the README.", replies[0].text)
        val items = (Wire.decodeMessage(lines("items/messages.jsonl")[1]) as Message.InboxList).items
        assertEquals(listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.REPLY, InboxAction.LATER), items[0].answers)
        assertEquals(
            listOf(InboxAction.RETRY, InboxAction.REPLY, InboxAction.CHANGE_COURSE, InboxAction.DROP, InboxAction.DISMISS, InboxAction.LATER),
            items[2].answers,
        )
        val failure = items[2].attachments
        assertEquals(listOf("report.md"), failure.docs)
        assertEquals(listOf(SourceLink("https://example.com/runs/42")), failure.links)
    }

    @Test
    fun unknownItemKindsStatesAndAnswersStillDecode() {
        val line = """{"type":"inbox-list","inbox":1,"entries":[],"items":[{"id":"i-00000001","kind":"handoff",""" +
            """"state":"snoozed","question":"q","workspace":"w","title":"t","createdAt":1,"answer":"escalate",""" +
            """"answers":["accept","escalate","later"]}]}"""
        val item = (Wire.decodeMessage(line) as Message.InboxList).items.single()
        assertEquals(ItemKind("handoff"), item.kind)
        assertEquals(ItemState("snoozed"), item.state)
        assertEquals(null, item.answer)
        assertEquals(listOf(InboxAction.ACCEPT, InboxAction.LATER), item.answers)
        assertEquals(Attachments(), item.attachments)
    }

    @Test
    fun unknownTypesAndFieldsAreIgnored() {
        assertEquals(Message.Unknown("future-thing"), Wire.decodeMessage("""{"type":"future-thing","x":1}"""))
        assertEquals(
            Message.PaneGone(watch = 3, pane = "%1"),
            Wire.decodeMessage("""{"type":"pane-gone","watch":3,"pane":"%1","extra":true}"""),
        )
    }

    @Test
    fun newErrorCodesAndRolesStillDecode() {
        val result = Wire.decodeMessage("""{"type":"result","id":1,"error":{"code":"rate-limited","message":"m"}}""")
        assertEquals(ErrorCode("rate-limited"), (result as Message.Result).error?.code)
        val session = """{"type":"result","id":1,"sessions":[{"id":"$1","name":"s","panes":[{"id":"%1","window":"@1",""" +
            """"index":0,"role":"observer","cols":80,"rows":24,"command":"sh","path":"/","dead":false,"shellForeground":true}]}]}"""
        val pane = (Wire.decodeMessage(session) as Message.Result).sessions!!.single().panes.single()
        assertEquals(Role.UNKNOWN, pane.role)
        val saved = """{"type":"task-saved","task":1,"record":{"id":"t-1","rev":1,"workspace":"w","title":"t",""" +
            """"goal":"g","status":"blocked","createdAt":1,"updatedAt":1}}"""
        assertEquals(TaskStatus("blocked"), (Wire.decodeMessage(saved) as Message.TaskSaved).record.status)
        val run = """{"type":"task-saved","task":1,"record":{"id":"t-1","rev":1,"workspace":"w","title":"t",""" +
            """"goal":"g","status":"open","createdAt":1,"updatedAt":1,"runs":[{"id":1,"startedAt":1,"reason":"resume"}]}}"""
        assertEquals(RunReason("resume"), (Wire.decodeMessage(run) as Message.TaskSaved).record.runs.single().reason)
        val loop = """{"type":"task-saved","task":1,"record":{"id":"t-1","rev":1,"workspace":"w","title":"t",""" +
            """"goal":"g","status":"open","createdAt":1,"updatedAt":1,"loop":{"check":"c","maxAttempts":3,""" +
            """"timeout":600,"state":"resting","attempts":0,"armedAt":1,"since":1,"reason":"quota"}}}"""
        val decoded = (Wire.decodeMessage(loop) as Message.TaskSaved).record.loop!!
        assertEquals(LoopState("resting"), decoded.state)
        assertEquals(LoopReason("quota"), decoded.reason)
    }

    @Test
    fun workspaceChecksAreAnyOrAList() {
        fun checks(json: String) =
            (Wire.decodeMessage("""{"type":"task-list","tasks":1,"workspaces":[$json],"entries":[]}""") as Message.TaskList)
                .workspaces.single().checks
        assertEquals(WorkspaceChecks.Any, checks("""{"name":"w","checks":"any"}"""))
        assertEquals(WorkspaceChecks.Only(listOf("make check")), checks("""{"name":"w","checks":["make check"]}"""))
        assertEquals(null, checks("""{"name":"w"}"""))
        assertThrows(SerializationException::class.java) { checks("""{"name":"w","checks":"all"}""") }
    }

    @Test
    fun bytesAreBase64() {
        val line = Wire.encode(Request.RawBytes(1, "%1", Bytes(byteArrayOf(0x1b, 0xff.toByte()))))
        assertEquals(tree("""{"type":"bytes","id":1,"pane":"%1","data":"G/8="}"""), tree(line))
    }
}
