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
