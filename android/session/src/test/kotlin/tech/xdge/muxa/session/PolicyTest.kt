package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.Basis
import tech.xdge.muxa.protocol.BasisKind
import tech.xdge.muxa.protocol.Confidence
import tech.xdge.muxa.protocol.Correction
import tech.xdge.muxa.protocol.CorrectionKind
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.Hypothesis
import tech.xdge.muxa.protocol.HypothesisProposal
import tech.xdge.muxa.protocol.HypothesisState
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Minutes
import tech.xdge.muxa.protocol.Plan
import tech.xdge.muxa.protocol.PlanAction
import tech.xdge.muxa.protocol.PlanRow
import tech.xdge.muxa.protocol.Policy
import tech.xdge.muxa.protocol.PolicyAction
import tech.xdge.muxa.protocol.Request
import java.time.LocalDate
import java.time.ZoneId

/** A host at `work` level [level] with one policy, one hypothesis and its item, one correction and one plan. */
private class PolicyBridge(level: Int = 11) : FakeBridge() {
    var policy = Policy("y-00000001", 2, "Recruiting first.", "all", source = "user", createdAt = 1, updatedAt = 1)
    val hypothesis = Hypothesis("h-00000001", "Reviews first.", "all", listOf("x-00000001"), Confidence.LOW, HypothesisState.PROPOSED, "p-00000001", 99, 5)
    val correction = Correction("x-00000001", CorrectionKind.DISMISSED, 4, "p-00000001", "w-00000002", true, "work", reason = "not now")
    val item = InboxItem(
        "i-00000001", ItemKind.HYPOTHESIS, ItemState.OPEN, "Keep as a policy? Reviews first.", "secretary", "Noticed", 5,
        hypothesis = HypothesisProposal(hypothesis.id, hypothesis.text, hypothesis.scope, hypothesis.evidence, hypothesis.confidence),
        answers = listOf(InboxAction.CONFIRM, InboxAction.REJECT, InboxAction.LATER),
    )
    var plan = Plan(
        "p-00000001", 3, 10, 10, Delivery.RESULTED, 11,
        top = listOf("w-00000001", "w-00000002", "i-00000009").map { id ->
            if (id.startsWith("w-")) PlanRow(work = id, why = "w", next = "n", minutes = Minutes(1, 2, 1)) else PlanRow(item = id, why = "w", next = "n", minutes = Minutes(1, 2, 1))
        },
    )
    var refusal: ErrorBody? = null

    init {
        capabilities = listOf("task-records", "work:$level")
    }

    override fun answer(req: Request) {
        refusal?.takeIf { req !is Request.ListInbox && req !is Request.ListRoutines }?.let { return emit(Message.Result(req.id, it)) }
        when (req) {
            is Request.ListInbox -> {
                emit(Message.InboxList(req.id, emptyList(), items = listOf(item), plans = listOf(plan), policies = listOf(policy), hypotheses = listOf(hypothesis), corrections = listOf(correction)))
                emit(Message.Result(req.id))
            }
            is Request.ListRoutines -> {
                emit(Message.RoutineList(req.id, emptyList()))
                emit(Message.Result(req.id))
            }
            is Request.AddPolicy -> {
                emit(Message.PolicySaved(req.id, Policy("y-00000002", 1, req.text, req.scope, req.from, req.until, "user", 20, 20)))
                emit(Message.Result(req.id))
            }
            is Request.UpdatePolicy -> {
                if (req.action == PolicyAction.EDIT) {
                    policy = policy.copy(rev = policy.rev + 1, text = req.text ?: policy.text)
                    emit(Message.PolicySaved(req.id, policy))
                }
                emit(Message.Result(req.id))
            }
            is Request.UpdatePlan -> {
                plan = if (req.action == PlanAction.REORDER) plan.copy(rev = plan.rev + 1, userOrder = req.order) else plan.copy(rev = plan.rev + 1, dismissed = listOfNotNull(req.row))
                emit(Message.PlanSaved(req.id, plan))
                emit(Message.Result(req.id))
            }
            is Request.UpdateInbox -> {
                if (req.action == InboxAction.CONFIRM) emit(Message.PolicySaved(req.id, policy.copy(id = "y-00000003", source = hypothesis.id)))
                emit(Message.InboxSaved(req.id, item = item.copy(state = ItemState.ANSWERED, answer = req.action, answers = emptyList())))
                emit(Message.Result(req.id))
            }
            is Request.Forget -> emit(Message.Result(req.id))
            else -> super.answer(req)
        }
    }
}

class PolicyTest {
    private suspend fun connected(bridge: PolicyBridge): PaneSession {
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
        session.loadWork()
        return session
    }

    private suspend fun refusal(block: suspend () -> Unit): PaneSession.Failure {
        try {
            block()
        } catch (e: PaneSession.Failure) {
            return e
        }
        fail("expected a refusal")
        throw IllegalStateException()
    }

    @Test
    fun levelElevenListsTheMemoryAndLevelTenSendsNothing() = runBlocking {
        val memory = connected(PolicyBridge()).state.value.memory
        assertEquals(listOf("y-00000001"), memory.policies.map { it.id })
        assertEquals(listOf("h-00000001"), memory.hypotheses.map { it.id })
        assertEquals(listOf("x-00000001"), memory.corrections.map { it.id })

        val bridge = PolicyBridge(level = 10)
        val old = connected(bridge)
        assertEquals(Memory(), old.state.value.memory)
        val plan = old.state.value.plans.shown()!!
        for (call in listOf<suspend () -> Unit>(
            { old.addPolicy("x", "all") },
            { old.deletePolicy(bridge.policy) },
            { old.reorderPlan(plan, listOf("w-00000002")) },
            { old.dismissRow(plan, "w-00000002") },
            { old.forget("x-00000001") },
        )) {
            assertEquals(PaneSession.POLICIES_UNSUPPORTED, PaneSession.describe(refusal(call)))
        }
        val sent = bridge.received
        assertTrue(sent.none { it is Request.AddPolicy || it is Request.UpdatePolicy || it is Request.UpdatePlan || it is Request.Forget })
    }

    @Test
    fun policyRequestsCarryTheRevSeenAndAConflictIsTheHostsReason() = runBlocking {
        val bridge = PolicyBridge()
        val session = connected(bridge)
        val added = session.addPolicy("Sales after lunch.", "work", from = 100, until = 200)
        assertEquals(Request.AddPolicy(bridge.received.filterIsInstance<Request.AddPolicy>().single().id, "Sales after lunch.", "work", 100, 200), bridge.received.last { it is Request.AddPolicy })
        assertEquals("work", added.scope)
        val edited = session.editPolicy(bridge.policy, text = "Recruiting first, always.")
        assertEquals(3L, edited.rev)
        val edit = bridge.received.filterIsInstance<Request.UpdatePolicy>().single()
        assertEquals(2L, edit.rev)
        assertNull(edit.scope)
        session.deletePolicy(edited)
        assertEquals(PolicyAction.DELETE, bridge.received.filterIsInstance<Request.UpdatePolicy>().last().action)
        bridge.refusal = ErrorBody(ErrorCode.CONFLICT, "50 active policies: delete one first")
        val e = refusal { session.addPolicy("One more.", "all") }
        assertTrue(e is PaneSession.Failure.Refused)
        assertEquals("50 active policies: delete one first", PaneSession.describe(e))
    }

    @Test
    fun reorderAndNotTodayAreSentOnThePlanShown() = runBlocking {
        val bridge = PolicyBridge()
        val session = connected(bridge)
        val plan = session.state.value.plans.shown()!!
        val order = plan.moving("i-00000009", -1)!!
        assertEquals(listOf("w-00000001", "i-00000009", "w-00000002"), order)
        val reordered = session.reorderPlan(plan, order)
        assertEquals(order, reordered.todayRows.map { it.target })
        val dismissed = session.dismissRow(reordered, "w-00000002", "waiting on legal")
        val sent = bridge.received.filterIsInstance<Request.UpdatePlan>()
        assertEquals(listOf(3L, 4L), sent.map { it.rev })
        assertEquals("waiting on legal", sent.last().text)
        assertTrue(dismissed.isDismissed(dismissed.top[1]))
        bridge.refusal = ErrorBody(ErrorCode.CONFLICT, "plan p-00000001 is not the newest resulted plan; list the Inbox again")
        assertTrue(refusal { session.dismissRow(dismissed, "w-00000001") } is PaneSession.Failure.Refused)
    }

    @Test
    fun confirmListsTheInboxAgainAndRejectSendsTheReason() = runBlocking {
        val bridge = PolicyBridge()
        val session = connected(bridge)
        val lists = bridge.received.count { it is Request.ListInbox }
        assertEquals(InboxAction.CONFIRM, session.answer(bridge.item, InboxAction.CONFIRM).answer)
        assertTrue(bridge.received.count { it is Request.ListInbox } > lists)
        session.answer(bridge.item, InboxAction.REJECT, text = "only on Mondays")
        assertEquals("only on Mondays", bridge.received.filterIsInstance<Request.UpdateInbox>().last().text)
        session.forget("x-00000001")
        assertEquals("x-00000001", bridge.received.filterIsInstance<Request.Forget>().single().record)
    }

    @Test
    fun todayRowsFollowTheUsersOrderAndKeepRowsItLeftOut() {
        val rows = listOf("a", "b", "c").map { PlanRow(work = it, why = "", next = "", minutes = Minutes(1, 1, 1)) }
        val plan = Plan("p-1", 1, 1, 1, Delivery.RESULTED, 1, top = rows)
        assertEquals(listOf("a", "b", "c"), plan.todayRows.map { it.target })
        assertEquals(listOf("c", "a", "b"), plan.copy(userOrder = listOf("c", "a")).todayRows.map { it.target })
        assertNull(plan.moving("a", -1))
        assertNull(plan.moving("z", 1))
        assertEquals(listOf("b", "a", "c"), plan.moving("a", 1))
    }

    @Test
    fun whyFindsTheCitedRecordOrSaysForgotten() {
        val bridge = PolicyBridge()
        val memory = Memory(listOf(bridge.policy), listOf(bridge.hypothesis), listOf(bridge.correction))
        assertEquals(Cited.PolicyRecord(bridge.policy), memory.cited(Basis(BasisKind.POLICY, "t", "y-00000001")))
        assertEquals(Cited.HypothesisRecord(bridge.hypothesis), memory.cited(Basis(BasisKind.HYPOTHESIS, "t", "h-00000001")))
        assertEquals(Cited.Forgotten("x-00000009"), memory.cited(Basis(BasisKind.CORRECTION, "t", "x-00000009")))
        assertNull(memory.cited(Basis(BasisKind.DUE, "t", "x-00000001")))
        assertEquals("Not today: Book the venue — “not now”", bridge.correction.summary { "Book the venue" })
        assertEquals("Based on 1 correction", bridge.hypothesis.basedOn)
        assertEquals(bridge.item, listOf(bridge.item).askingAbout(bridge.hypothesis))
    }

    @Test
    fun aPolicyDraftEndsThisWeekOnSundayNightAndChecksItsText() {
        val tokyo = ZoneId.of("Asia/Tokyo")
        // Monday 2025-11-10 00:30 in Tokyo, as the CLI's test of `--until week`.
        val monday = LocalDate.of(2025, 11, 10).atTime(0, 30).atZone(tokyo).toEpochSecond()
        assertEquals(1_763_305_199L, PolicyDraft("x", end = PolicyEnd.ThisWeek).until(monday, tokyo))
        assertNull(PolicyDraft("x").until(monday, tokyo))
        assertEquals(null, PolicyDraft("Recruiting first.").problem(monday, tokyo))
        assertTrue(PolicyDraft("").problem(monday, tokyo) != null)
        assertTrue(PolicyDraft("a\nb").problem(monday, tokyo) != null)
        assertTrue(PolicyDraft("x".repeat(501)).problem(monday, tokyo) != null)
        assertTrue(PolicyDraft("x", end = PolicyEnd.Until(LocalDate.of(2025, 11, 9))).problem(monday, tokyo) != null)
        val policy = Policy("y-1", 1, "x", "work", until = 1_763_305_199, source = "user", createdAt = 1, updatedAt = 1)
        assertEquals(PolicyEnd.Until(LocalDate.of(2025, 11, 16)), PolicyDraft.of(policy, tokyo).end)
        assertTrue(policy.isExpired(1_763_305_199))
        assertFalse(policy.isExpired(1_763_305_198))
        assertEquals("work · until Sun 16 Nov · expired", policy.span(1_763_305_200, tokyo))
    }
}
