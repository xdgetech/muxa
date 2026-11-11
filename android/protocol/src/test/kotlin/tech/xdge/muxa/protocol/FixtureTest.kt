package tech.xdge.muxa.protocol

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertEquals
import kotlinx.serialization.SerializationException
import kotlinx.serialization.Serializable
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
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
        assertEquals(36, lines.size)
        for (line in lines) {
            assertEquals(line, tree(line), tree(Wire.encode(Wire.decodeRequest(line))))
        }
    }

    @Test
    fun messagesRoundTrip() {
        val lines = lines("messages.jsonl")
        assertEquals(41, lines.size)
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
    fun worktreeEndContractRoundTrips() {
        val requests = lines("worktree-end/requests.jsonl")
        assertEquals(3, requests.size)
        requests.forEach { assertEquals(it, tree(it), tree(Wire.encode(Wire.decodeRequest(it)))) }
        val messages = lines("worktree-end/messages.jsonl")
        assertEquals(4, messages.size)
        messages.forEach {
            val message = Wire.decodeMessage(it)
            assertFalse("fixture type is unknown: $it", message is Message.Unknown)
            assertEquals(it, tree(it), tree(Wire.encode(message)))
        }
        assertEquals(WORK_WORKTREE_END, workLevel((Wire.decodeMessage(messages[0]) as Message.Hello).capabilities))
        assertEquals(
            ErrorBody(ErrorCode.WOULD_LOSE_WORK, "its branch is not merged"),
            (Wire.decodeMessage(messages[1]) as Message.Result).error,
        )
        assertEquals(
            Request.UpdateTask(61, "t-7f3a2c", 3, TaskStatus.DONE, WorktreeEnd.REMOVE),
            Wire.decodeRequest(requests[0]),
        )
        assertEquals(WorktreeEnd.FORCE, (Wire.decodeRequest(requests[1]) as Request.UpdateTask).worktree)
        assertEquals(
            Request.UpdateInbox(63, "i-00000001", InboxAction.DROP, worktree = WorktreeEnd.REMOVE),
            Wire.decodeRequest(requests[2]),
        )
        // An older client's request has no `worktree` key at all.
        assertFalse("worktree" in Wire.encode(Request.UpdateTask(1, "t-1", 1, TaskStatus.DONE)))
    }

    @Test
    fun assistantContractRoundTrips() {
        val requests = lines("assistant/requests.jsonl")
        assertEquals(49, requests.size)
        requests.forEach { assertEquals(it, tree(it), tree(Wire.encode(Wire.decodeRequest(it)))) }
        val messages = lines("assistant/messages.jsonl")
        assertEquals(58, messages.size)
        messages.forEach {
            val message = Wire.decodeMessage(it)
            assertFalse("fixture type is unknown: $it", message is Message.Unknown)
            assertEquals(it, tree(it), tree(Wire.encode(message)))
        }
        // A resent capture carries the same clientId; the host keeps one.
        val (first, resent) = requests.take(2).map { (Wire.decodeRequest(it) as Request.Capture).clientId }
        assertEquals(first, resent)
        val link = Wire.decodeRequest(requests[4]) as Request.SortCapture
        assertEquals(SortAction.LINK to "w-0a1b2c3d", link.action to link.work)
        val merge = Wire.decodeRequest(requests[11]) as Request.UpdateWork
        assertEquals(WorkAction.MERGE to "w-0a1b2c3d", merge.action to merge.into)
        val task = Wire.decodeRequest(requests[15]) as Request.CreateTask
        assertEquals("w-6a7b8c9d" to 3L, task.work to task.workRev)

        assertEquals(WORK_ASSISTANT, workLevel((Wire.decodeMessage(messages[0]) as Message.Hello).capabilities))
        val list = Wire.decodeMessage(messages[6]) as Message.InboxList
        assertEquals(
            listOf(WorkState.WAITING_DECISION, WorkState.IN_PROGRESS, WorkState.OPEN, WorkState.OPEN, WorkState.DONE, WorkState.WITHDRAWN),
            list.work.map { it.state },
        )
        assertTrue(list.work[3].laterAt != null && list.work[5].mergedInto != null)
        assertEquals(1, list.captures.size)
        assertEquals(
            listOf(Delivery.RESULTED, Delivery.PROCESSING, Delivery.UNCONFIRMED, Delivery.TIMED_OUT),
            list.requests.map { it.delivery },
        )
        val results = list.requests[0].results
        assertEquals(listOf(false, true), results.map { it.stale })
        assertTrue(results[0].findings.all { it.sources.isNotEmpty() })
        // A client before level 9 reads an inbox-list without the new arrays.
        val old = Wire.decodeMessage("""{"type":"inbox-list","inbox":1,"entries":[]}""") as Message.InboxList
        assertEquals(Triple(emptyList<WorkItem>(), emptyList<Capture>(), emptyList<AssistantRequest>()), Triple(old.work, old.captures, old.requests))
        // An older client's create-task has no work keys at all.
        val plain = Wire.encode(Request.CreateTask(1, "w", "t", "g"))
        assertFalse("\"work\"" in plain || "workRev" in plain)
        assertEquals(Request.GetWork(416, "w-6a7b8c9d"), Wire.decodeRequest(requests[16]))
        // get-work: every linked Capture, sorted ones included, in the item's order.
        val detail = Wire.decodeMessage(messages[9]) as Message.WorkDetail
        assertEquals(detail.entry.captures, detail.captures.map { it.id })
        assertTrue(detail.captures.all { it.state == CaptureState.SORTED })
        assertEquals(detail.entry.requests, detail.requests.map { it.id })
        assertEquals(ErrorCode.BAD_REQUEST, (Wire.decodeMessage(messages[11]) as Message.Result).error?.code)
    }

    @Test
    fun planContractRoundTrips() {
        val requests = lines("assistant/requests.jsonl").map(Wire::decodeRequest)
        val messages = lines("assistant/messages.jsonl").map(Wire::decodeMessage)
        val fromRequest = requests[18] as Request.ApplyNotes
        val fromPlan = requests[19] as Request.ApplyNotes
        assertTrue(fromRequest.source.startsWith("q-") && fromRequest.version == 1L)
        assertTrue(fromPlan.source.startsWith("p-") && fromPlan.version == 2L)
        assertEquals(Request.PlanNow(420, 45), requests[20])
        assertEquals(Request.PlanNow(421), requests[21])
        val due = requests[22] as Request.UpdateWork
        assertEquals(WorkAction.EDIT to "Asia/Tokyo", due.action to due.due?.timezone)
        assertEquals(listOf(WorkAction.PIN, WorkAction.UNPIN), requests.subList(23, 25).map { (it as Request.UpdateWork).action })

        assertEquals(WORK_PLANS, workLevel((messages[12] as Message.Hello).capabilities))
        val notes = (messages[13] as Message.WorkSaved).request?.results?.first()?.notes
        assertEquals(64, notes?.base?.length)
        assertEquals(ErrorCode.CONFLICT, (messages[14] as Message.Result).error?.code)
        val pending = (messages[15] as Message.PlanSaved).entry
        assertEquals(Delivery.PENDING to 45, pending.delivery to pending.minutes)
        assertTrue(pending.top.isEmpty())
        assertEquals(ErrorCode.UNSUPPORTED, (messages[16] as Message.Result).error?.code)
        val pinned = (messages[17] as Message.WorkSaved).entry
        assertTrue(pinned.pinnedAt != null && pinned.due != null)

        val list = messages[18] as Message.InboxList
        assertEquals(listOf(ItemKind.SORT, ItemKind.DELEGATE), list.items.map { it.kind })
        assertEquals(SortAction.WORK, list.items[0].sort?.action)
        assertTrue(list.items[1].delegate != null && list.items[0].delegate == null)
        assertTrue(list.plans.size == 2 && list.plans[0].at > list.plans[1].at)
        val plan = list.plans[0]
        assertTrue(plan.top.size <= 3)
        assertTrue(list.work[0].pinnedAt != null)
        assertEquals(list.work[0].id, plan.top[0].work)
        // A Later item is never in a plan's top; with a near due it is in dueSoon.
        val later = list.work.filter { it.laterAt != null }.map { it.id }
        assertTrue(plan.top.none { it.work in later })
        assertEquals(later, plan.dueSoon)
        assertTrue(plan.top.all { (it.work != null) != (it.item != null) })
        assertTrue(plan.top.all { it.minutes.low <= it.minutes.estimated && it.minutes.estimated <= it.minutes.high })
        assertEquals(BasisKind.MANUAL, plan.top[0].basis.first().kind)
        assertEquals(listOf(ChangeKind.MOVED, ChangeKind.ENTERED, ChangeKind.ENTERED, ChangeKind.LEFT), plan.changed.map { it.change })
        assertTrue(plan.unanswered.isNotEmpty() && plan.stale.isNotEmpty())
        val assistant = list.assistant!!
        assertEquals("secretary", assistant.overall)
        assertEquals(
            listOf(Share.SUMMARIES to OverallRequests.REVIEW, Share.OUTLINE to OverallRequests.AUTO),
            assistant.delegates.map { it.share to it.overallRequests },
        )
        val answered = (messages[20] as Message.InboxSaved).item!!
        assertTrue(answered.answers.isEmpty() && answered.delegate != null)
        // A plan whose run did not start says why, in host text.
        val failed = (messages[22] as Message.PlanSaved).entry
        assertTrue(failed.delivery == Delivery.FAILED && !failed.reason.isNullOrEmpty())
        assertTrue(list.plans.all { it.reason == null })
        // A client before level 10 reads an inbox-list without plans.
        val old = Wire.decodeMessage("""{"type":"inbox-list","inbox":1,"entries":[]}""") as Message.InboxList
        assertTrue(old.plans.isEmpty() && old.assistant == null)
        // Unknown plan values keep their names.
        val odd = """{"type":"plan-saved","plan":1,"entry":{"id":"p-00000001","rev":1,"at":1,"inputsAt":1,""" +
            """"delivery":"queued","deliveryAt":1,"top":[{"work":"w-1","why":"w","next":"n","minutes":{"low":1,""" +
            """"high":2,"estimated":1},"basis":[{"kind":"habit","text":"t"}],"targetState":"archived"}],""" +
            """"changed":[{"id":"w-1","change":"split"}]}}"""
        val saved = Wire.decodeMessage(odd) as Message.PlanSaved
        assertEquals(BasisKind("habit"), saved.entry.top[0].basis[0].kind)
        assertEquals(ChangeKind("split"), saved.entry.changed[0].change)
        assertEquals(TargetState("archived"), saved.entry.top[0].targetState)
        assertEquals(tree(odd), tree(Wire.encode(saved)))
    }

    @Test
    fun policyContractRoundTrips() {
        val requests = lines("assistant/requests.jsonl").map(Wire::decodeRequest)
        val messages = lines("assistant/messages.jsonl").map(Wire::decodeMessage)
        val added = requests[27] as Request.AddPolicy
        assertTrue(added.scope == "all" && added.from == null && added.until != null)
        assertEquals(PolicyAction.EDIT, (requests[29] as Request.UpdatePolicy).action)
        val deleted = requests[30] as Request.UpdatePolicy
        assertTrue(deleted.action == PolicyAction.DELETE && deleted.text == null)
        val reorder = requests[31] as Request.UpdatePlan
        assertTrue(reorder.action == PlanAction.REORDER && reorder.order.size == 3 && reorder.row == null)
        val dismiss = requests[32] as Request.UpdatePlan
        assertTrue(dismiss.action == PlanAction.DISMISS && dismiss.row != null && dismiss.text != null)
        val reject = requests[34] as Request.UpdateInbox
        assertTrue(reject.action == InboxAction.REJECT && reject.text != null)
        assertEquals(InboxAction.CONFIRM, (requests[35] as Request.UpdateInbox).action)
        assertEquals(Request.Forget(441, "h-3c4d5e6f"), requests[38])

        assertEquals(WORK_POLICIES, workLevel((messages[23] as Message.Hello).capabilities))
        val saved = messages[24] as Message.PolicySaved
        assertTrue(saved.entry.id.startsWith("y-") && saved.entry.source == "user")
        assertEquals(ErrorCode.CONFLICT, (messages[26] as Message.Result).error?.code)
        // A delete answers with the result only.
        assertEquals(Message.Result(433), messages[28])
        val plan = (messages[29] as Message.PlanSaved).entry
        assertTrue(plan.userOrder.size == plan.top.size && plan.dismissed == listOf("i-1a2b3c4d"))
        val kinds = plan.top.flatMap { row -> row.basis.map { it.kind } }
        assertTrue(kinds.containsAll(listOf(BasisKind.POLICY, BasisKind.HYPOTHESIS, BasisKind.CORRECTION)))
        val made = (messages[32] as Message.PolicySaved).entry
        val confirmed = (messages[33] as Message.InboxSaved).item!!
        assertTrue(made.source == confirmed.hypothesis?.hypothesis && confirmed.answer == InboxAction.CONFIRM)
        assertEquals(ErrorCode.BAD_REQUEST, (messages[35] as Message.Result).error?.code)

        val list = messages[36] as Message.InboxList
        assertEquals(ItemKind.HYPOTHESIS, list.items[0].kind)
        assertEquals(listOf(InboxAction.CONFIRM, InboxAction.REJECT, InboxAction.LATER), list.items[0].answers)
        assertEquals(Confidence.LOW, list.items[0].hypothesis?.confidence)
        // The host names each row and says which target is no longer open.
        val answered = list.plans[0].top.first { it.item == "i-1a2b3c4d" }
        assertEquals(TargetState.ANSWERED, answered.targetState)
        assertTrue(answered.title != null)
        assertTrue(list.plans[0].top.filter { it.work != null }.all { it.title != null && it.targetState == null })
        assertEquals(3, list.policies.size)
        assertEquals(listOf(HypothesisState.PROPOSED, HypothesisState.REJECTED), list.hypotheses.map { it.state })
        assertEquals(
            listOf(
                CorrectionKind.DISMISSED, CorrectionKind.REORDERED, CorrectionKind.REJECTED, CorrectionKind.DEFERRED,
                CorrectionKind.PINNED,
            ),
            list.corrections.map { it.kind },
        )
        val reordered = list.corrections[1]
        assertTrue(reordered.row == null && reordered.before.size == reordered.after.size)
        // A client before level 11 reads an inbox-list without the new arrays.
        val old = Wire.decodeMessage("""{"type":"inbox-list","inbox":1,"entries":[]}""") as Message.InboxList
        assertTrue(old.policies.isEmpty() && old.hypotheses.isEmpty() && old.corrections.isEmpty())
        // Unknown values keep their names.
        val odd = """{"type":"inbox-list","inbox":1,"entries":[],"hypotheses":[{"id":"h-1","text":"t","scope":"all",""" +
            """"evidence":["x-1"],"confidence":"high","state":"paused","plan":"p-1","expiresAt":1,"at":1}],""" +
            """"corrections":[{"id":"x-1","kind":"skipped","at":1,"plan":"p-1","inTop":false}]}"""
        val decoded = Wire.decodeMessage(odd) as Message.InboxList
        assertEquals(Confidence("high") to HypothesisState("paused"), decoded.hypotheses[0].confidence to decoded.hypotheses[0].state)
        assertEquals(CorrectionKind("skipped"), decoded.corrections[0].kind)
        assertEquals(tree(odd), tree(Wire.encode(decoded)))
    }

    @Test
    fun staffContractRoundTrips() {
        val requests = lines("assistant/requests.jsonl").map(Wire::decodeRequest)
        val messages = lines("assistant/messages.jsonl").map(Wire::decodeMessage)
        assertEquals("lisk-strategist", (requests[39] as Request.RequestAssistant).worker)
        assertEquals("lisk-architect", (requests[40] as Request.Capture).assist?.worker)
        assertEquals(2, (requests[42] as Request.ApplyNotes).step)
        // Older lines carry no worker and no step.
        assertEquals(null, (requests[18] as Request.ApplyNotes).step)

        assertEquals(WORK_STAFF, workLevel((messages[37] as Message.Hello).capabilities))
        // A named worker is step 1, picked by the user.
        val asked = (messages[38] as Message.WorkSaved).request!!
        assertEquals(Triple(1, StepBy.USER, asked.run), Triple(asked.steps.size, asked.steps[0].by, asked.steps[0].run))
        // Approving a hand-off: the request with its new step, then the answered item.
        val chain = (messages[42] as Message.WorkSaved).request!!
        val newest = chain.steps.last()
        assertEquals(Triple(2, StepBy.HANDOFF, chain.delivery), Triple(newest.step, newest.by, newest.delivery))
        assertEquals(chain.run, newest.run)
        assertEquals(newest.worker, chain.steps[0].handoff?.worker)
        val item = (messages[43] as Message.InboxSaved).item!!
        assertEquals(Triple(ItemKind.HANDOFF, InboxAction.APPROVE, 1), Triple(item.kind, item.answer, item.handoff?.step))
        assertTrue(item.answers.isEmpty())
        // Each result names its worker and step; inside a version nothing is stale.
        val done = (messages[45] as Message.WorkSaved).request!!
        assertEquals(Delivery.RESULTED, done.delivery)
        val current = done.results.filter { it.version == done.version }.map { it.step to it.stale }
        assertEquals(listOf(2 to false, 1 to false), current)
        assertTrue(done.results[0].notes != null && done.results[0].worker != null)
        assertEquals(ErrorCode.BAD_REQUEST, (messages[47] as Message.Result).error?.code)

        val list = messages[48] as Message.InboxList
        assertEquals(listOf(ItemKind.HANDOFF, ItemKind.DELEGATE), list.items.map { it.kind })
        assertEquals(listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.LATER), list.items[0].answers)
        assertEquals("lisk-security", list.items[1].delegate?.worker)
        val staff = list.assistant!!.delegates[0].staff
        assertEquals(1, staff.count { it.lead })
        assertTrue(staff.all { it.description.toByteArray().size <= 1024 })
        // A client before level 12 reads an inbox-list without staff.
        val old = Wire.decodeMessage(
            """{"type":"inbox-list","inbox":1,"entries":[],"assistant":{"delegates":[""" +
                """{"workspace":"w","share":"outline","overallRequests":"review"}]}}""",
        ) as Message.InboxList
        assertTrue(old.assistant!!.delegates[0].staff.isEmpty())
    }

    /** A level 11 client knows no steps, staff or result worker: it reads the newest step as the request. */
    @Test
    fun aLevel11ClientReadsAChainAsOneRequest() {
        val line = lines("assistant/messages.jsonl")[45]
        val old = Json { ignoreUnknownKeys = true }.decodeFromString(Level11Saved.serializer(), line).request
        assertEquals(Delivery.RESULTED to "t-00e004", old.delivery to old.run)
        assertTrue(old.results.all { it.version >= 1 && it.summary.isNotEmpty() })
        // A newer host's `by` is unknown, not a failed list.
        val list = lines("assistant/messages.jsonl")[48].replace(""""by":"lead"""", """"by":"routine"""")
        val requests = (Wire.decodeMessage(list) as Message.InboxList).requests
        assertEquals(StepBy("routine"), requests[0].steps[0].by)
    }

    @Test
    fun writeContractRoundTrips() {
        val requests = lines("assistant/requests.jsonl").map(Wire::decodeRequest)
        val messages = lines("assistant/messages.jsonl").map(Wire::decodeMessage)
        val approve = requests[44] as Request.UpdateInbox
        assertEquals("i-9c0d1e2f" to InboxAction.APPROVE, approve.entry to approve.action)
        assertEquals(InboxAction.REJECT, (requests[46] as Request.UpdateInbox).action)

        assertEquals(WORK_WRITES, workLevel((messages[49] as Message.Hello).capabilities))
        // Approve: the writer's Task, then the answered item that names it.
        val record = (messages[50] as Message.TaskSaved).record
        val item = (messages[51] as Message.InboxSaved).item!!
        val write = item.write!!
        assertEquals(Triple(ItemKind.WRITE, InboxAction.APPROVE, record.id), Triple(item.kind, item.answer, item.task))
        assertEquals("lisk-writer" to write.action, record.worker to record.title)
        // The content is in the goal unchanged, and the marker is the last line.
        assertTrue(record.goal.contains("\n\n${write.content}\n\n"))
        assertEquals("muxa-write: ${item.id} ${write.hash}", record.goal.lines().last { it.isNotEmpty() })
        val refused = (messages[53] as Message.Result).error!!
        assertEquals(ErrorCode.CONFLICT, refused.code)
        assertTrue(refused.message.contains("max_writes_per_day"))
        val rejected = (messages[54] as Message.InboxSaved).item!!
        assertEquals(InboxAction.REJECT to null, rejected.answer to rejected.task)

        val list = messages[56] as Message.InboxList
        assertTrue(list.items.all { it.kind == ItemKind.WRITE && it.state == ItemState.OPEN })
        assertEquals(listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.LATER), list.items[0].answers)
        // An older version's write stays open and names its version.
        val q = list.requests[0]
        assertEquals(q.version - 1, list.items[1].write?.version)
        // A Result's writes carry their hash, the item, and the Task once approved.
        val newest = q.results[0]
        assertEquals(2, newest.writes.size)
        assertEquals(record.id, newest.writes[0].task)
        assertEquals(list.items[0].id to list.items[0].write?.hash, newest.writes[1].item to newest.writes[1].hash)
        val lisk = list.assistant!!.delegates[0]
        assertEquals("lisk-writer" to Writes.REVIEW, lisk.writer?.worker to lisk.writes)
        assertTrue(lisk.staff.none { it.worker == "lisk-writer" })
        val personal = list.assistant!!.delegates[1]
        assertEquals(null to Writes.NONE, personal.writer to personal.writes)
        // A host before level 13 sends neither.
        val older = (messages[48] as Message.InboxList).assistant!!.delegates
        assertTrue(older.all { it.writer == null && it.writes == null })
        // Approve on a write carries the hash of the write the client showed; without it, refused.
        val shown = requests[47] as Request.UpdateInbox
        assertEquals(list.items.single { it.id == shown.entry }.write?.hash, shown.hash)
        assertEquals(null, (requests[48] as Request.UpdateInbox).hash)
        val refusedOld = (messages[57] as Message.Result).error!!
        assertEquals(ErrorCode.BAD_REQUEST, refusedOld.code)
        assertTrue(refusedOld.message.contains("cannot show the whole write"))
    }

    /** A level 12 client knows no writes: it reads the Result and shows a `write` item as a kind it does not know. */
    @Test
    fun aLevel12ClientReadsWritesAsUnknown() {
        val line = lines("assistant/messages.jsonl")[56]
        val old = Json { ignoreUnknownKeys = true }.decodeFromString(Level12List.serializer(), line)
        assertTrue(old.items.all { it.kind !in setOf("ask", "review", "failure", "proposal", "briefing", "sort", "delegate", "hypothesis", "handoff") })
        assertTrue(old.requests[0].results.all { it.version >= 1 && it.summary.isNotEmpty() })
        // A newer host's `writes` value is unknown, not a failed list.
        val list = line.replace(""""writes":"review"""", """"writes":"auto"""")
        assertEquals(Writes("auto"), (Wire.decodeMessage(list) as Message.InboxList).assistant!!.delegates[0].writes)
    }

    /** ADR 0053: `memory` items, approve with the shown hash, and a workspace's `memory` mode, from line 6 of `items/messages.jsonl`. */
    @Test
    fun memoryContractRoundTrips() {
        val requests = lines("items/requests.jsonl").map(Wire::decodeRequest)
        val messages = lines("items/messages.jsonl").map(Wire::decodeMessage)
        assertEquals(WORK_MEMORY, workLevel((messages[5] as Message.Hello).capabilities))
        val workspaces = (messages[6] as Message.TaskList).workspaces
        assertEquals(listOf(MemoryMode.REVIEW, MemoryMode.AUTO, MemoryMode.OFF), workspaces.map { it.memory })
        assertEquals(listOf(1_762_790_000L, null, null), workspaces.map { it.memoryAt })
        val items = (messages[7] as Message.InboxList).items
        items.forEach {
            assertEquals(Triple(ItemKind.MEMORY, ItemState.OPEN, it.memory?.task), Triple(it.kind, it.state, it.task))
            assertEquals(it.memory?.title, it.title)
            assertEquals(listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.LATER), it.answers)
        }
        // Approve carries the hash of the text the client showed.
        val approve = requests[19] as Request.UpdateInbox
        val approved = items.first { it.id == approve.entry }
        assertEquals(InboxAction.APPROVE to approved.memory?.hash, approve.action to approve.hash)
        val saved = (messages[8] as Message.InboxSaved).item!!
        assertEquals(Triple(ItemState.ANSWERED, InboxAction.APPROVE, approved.memory), Triple(saved.state, saved.answer, saved.memory))
        // Over the file's cap: conflict with the sizes, and the item stays open.
        val full = (messages[10] as Message.Result).error!!
        assertEquals(ErrorCode.CONFLICT, full.code)
        assertTrue(full.message.contains("16 KiB"))
        // Without the hash: refused.
        val bare = requests[21] as Request.UpdateInbox
        assertEquals(InboxAction.APPROVE to null, bare.action to bare.hash)
        assertEquals(ErrorCode.BAD_REQUEST, (messages[11] as Message.Result).error!!.code)
        assertEquals(InboxAction.REJECT, (messages[12] as Message.InboxSaved).item!!.answer)
        // A host before level 14 sends no mode; a newer value is unknown.
        val old = Json.decodeFromString(WorkspaceInfo.serializer(), """{"name":"w"}""")
        assertEquals(null to null, old.memory to old.memoryAt)
        assertEquals(MemoryMode("ask"), Json.decodeFromString(WorkspaceInfo.serializer(), """{"name":"w","memory":"ask"}""").memory)
    }

    /** A level 13 client knows no memory: it shows a `memory` item as a kind it does not know, and reads the workspaces. */
    @Test
    fun aLevel13ClientReadsMemoryAsUnknown() {
        val lines = lines("items/messages.jsonl")
        val json = Json { ignoreUnknownKeys = true }
        val old = json.decodeFromString(Level13List.serializer(), lines[7])
        val level13 = setOf("ask", "review", "failure", "proposal", "briefing", "sort", "delegate", "hypothesis", "handoff", "write")
        assertTrue(old.items.all { it.kind !in level13 && it.title.isNotEmpty() })
        val workspaces = json.decodeFromString(Level13TaskList.serializer(), lines[6]).workspaces
        assertTrue(workspaces.all { it.name.isNotEmpty() && it.root != null })
    }

    @Test
    fun unknownWorkStatesAndDeliveriesStillDecode() {
        val line = """{"type":"work-saved","work":1,"entry":{"id":"w-00000001","rev":1,"title":"t","state":"blocked",""" +
            """"createdAt":1,"updatedAt":1},"request":{"id":"q-00000001","work":"w-00000001","version":1,"workspace":"w",""" +
            """"scope":"plan","delivery":"queued","deliveryAt":1,"createdAt":1,"updatedAt":1}}"""
        val saved = Wire.decodeMessage(line) as Message.WorkSaved
        assertEquals(WorkState("blocked"), saved.entry.state)
        assertEquals(WorkState.OPEN, saved.entry.state.shown)
        assertEquals(WorkState.DONE, WorkState.DONE.shown)
        assertEquals(Delivery("queued"), saved.request?.delivery)
        assertEquals(AssistScope("plan"), saved.request?.scope)
        assertEquals(tree(line), tree(Wire.encode(saved)))
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
        assertEquals(23, requests.size)
        requests.forEach { assertEquals(it, tree(it), tree(Wire.encode(Wire.decodeRequest(it)))) }
        val messages = lines("items/messages.jsonl")
        assertEquals(14, messages.size)
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
            listOf(InboxAction.ACCEPT, InboxAction.REPLY, InboxAction.KEEP_OPEN, InboxAction.LATER),
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
        val keepOpen = Wire.decodeRequest(requests[6]) as Request.UpdateInbox
        assertEquals(InboxAction.KEEP_OPEN, keepOpen.action)
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
            listOf(InboxAction.RETRY, InboxAction.REPLY, InboxAction.DROP, InboxAction.DISMISS, InboxAction.LATER),
            items[2].answers,
        )
        val failure = items[2].attachments
        assertEquals(listOf("report.md"), failure.docs)
        assertEquals(listOf(SourceLink("https://example.com/runs/42")), failure.links)
    }

    /** ADR 0046: `protect`, `unprotect` and `delete-inbox`. */
    @Test
    fun deleteContractRoundTrips() {
        val requests = lines("inbox-delete/requests.jsonl")
        assertEquals(5, requests.size)
        requests.forEach { assertEquals(it, tree(it), tree(Wire.encode(Wire.decodeRequest(it)))) }
        val messages = lines("inbox-delete/messages.jsonl")
        assertEquals(6, messages.size)
        messages.forEach {
            val message = Wire.decodeMessage(it)
            assertFalse("fixture type is unknown: $it", message is Message.Unknown)
            assertEquals(it, tree(it), tree(Wire.encode(message)))
        }
        assertEquals(WORK_DELETE_INBOX, workLevel((Wire.decodeMessage(messages[0]) as Message.Hello).capabilities))
        val item = (Wire.decodeMessage(messages[1]) as Message.InboxSaved).item!!
        assertEquals(1_762_160_000L, item.protectedAt)
        assertFalse(InboxAction.PROTECT in item.answers)
        val both = Wire.decodeMessage(messages[2]) as Message.InboxSaved
        assertEquals(both.entry?.protectedAt, both.item?.protectedAt)
        assertEquals(Message.InboxDeleted(inbox = 403, deleted = 12), Wire.decodeMessage(messages[3]))
        assertEquals(
            listOf(InboxAction.PROTECT, InboxAction.PROTECT, InboxAction.UNPROTECT),
            requests.take(3).map { (Wire.decodeRequest(it) as Request.UpdateInbox).action },
        )
        assertEquals(Request.DeleteInbox(id = 403, olderThanDays = 3), Wire.decodeRequest(requests[3]))
    }

    @Test
    fun unknownItemKindsStatesAndAnswersStillDecode() {
        val line = """{"type":"inbox-list","inbox":1,"entries":[],"items":[{"id":"i-00000001","kind":"escalation",""" +
            """"state":"snoozed","question":"q","workspace":"w","title":"t","createdAt":1,"answer":"escalate",""" +
            """"answers":["accept","escalate","later"]}]}"""
        val item = (Wire.decodeMessage(line) as Message.InboxList).items.single()
        assertEquals(ItemKind("escalation"), item.kind)
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

// What a level 11 client knows of a `work-saved` request.
@Serializable private data class Level11Result(val version: Long, val summary: String)

@Serializable
private data class Level11Request(val delivery: Delivery, val run: String? = null, val results: List<Level11Result>)

@Serializable private data class Level11Saved(val request: Level11Request)

// What a level 12 client knows of an `inbox-list` with writes.
@Serializable private data class Level12Item(val kind: String)

@Serializable private data class Level12Request(val results: List<Level11Result>)

@Serializable private data class Level12List(val items: List<Level12Item>, val requests: List<Level12Request>)

// What a level 13 client knows of an `inbox-list` with memory items and a `task-list` with memory modes.
@Serializable private data class Level13Item(val kind: String, val title: String)

@Serializable private data class Level13List(val items: List<Level13Item>)

@Serializable private data class Level13Workspace(val name: String, val root: String? = null)

@Serializable private data class Level13TaskList(val workspaces: List<Level13Workspace>)
