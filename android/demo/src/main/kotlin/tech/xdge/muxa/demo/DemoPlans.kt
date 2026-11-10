package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.AssistScope
import tech.xdge.muxa.protocol.AssistantInfo
import tech.xdge.muxa.protocol.Basis
import tech.xdge.muxa.protocol.BasisKind
import tech.xdge.muxa.protocol.ChangeKind
import tech.xdge.muxa.protocol.DelegateInfo
import tech.xdge.muxa.protocol.DelegateProposal
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.Due
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.Minutes
import tech.xdge.muxa.protocol.Notes
import tech.xdge.muxa.protocol.OverallRequests
import tech.xdge.muxa.protocol.Plan
import tech.xdge.muxa.protocol.PlanChange
import tech.xdge.muxa.protocol.PlanAction
import tech.xdge.muxa.protocol.PlanRow
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.Share
import tech.xdge.muxa.protocol.SortAction
import tech.xdge.muxa.protocol.SortProposal
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkState
import java.security.MessageDigest

/**
 * The demo bridge's overall assistant (ADR 0049, `work` level 10): its plans, its proposal items, and
 * each workspace's live notes as a hash. `plan-now` makes a plan that is `resulted` [planMillis] later,
 * when the Inbox is listed; one plan run at a time. Not thread safe: the bridge holds its lock.
 */
internal class DemoPlans(
    /** The overall assistant's workspace. */
    val overall: String,
    private var plans: List<Plan>,
    /** The newest plan's `sort` and `delegate` items. */
    val proposals: List<InboxItem>,
    private var delegates: List<DelegateInfo>,
    /** Pins and deadlines on the fixtures' Work items. */
    val pins: Map<String, Long>,
    val dues: Map<String, Due>,
) {
    var planMillis: Long = 0

    /** When the open plan was asked for, in milliseconds. */
    private var started: Long? = null
    private var made = 0

    /** Each workspace's live `NOTES.md`, as its hash; none is the hash of no bytes. */
    private val live = mutableMapOf<String, String>()

    val info: AssistantInfo get() = AssistantInfo(overall, delegates)

    /** `plans` as `inbox-list` sends them: newest first, at most 7; the newest `resulted` one with what is derived now. */
    fun listed(unanswered: List<String>, stale: List<String>): List<Plan> {
        val newest = plans.firstOrNull { it.delivery == Delivery.RESULTED }?.id
        return plans.take(KEPT).map { if (it.id == newest) it.copy(unanswered = unanswered, stale = stale) else it }
    }

    /** `plan-now`: a `pending` plan, while no other is being made. */
    fun planNow(minutes: Int?, now: Long, nowMillis: Long): Plan {
        if (minutes != null && minutes !in 1..MAX_MINUTES) throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "minutes must be 1 to 1440")
        plans.firstOrNull { it.delivery in OPEN }?.let { throw DemoWork.Refusal(ErrorCode.CONFLICT, "plan ${it.id} is being made; wait for it") }
        val plan = Plan("p-%08x".format(0xd1000000L + ++made), 1, now, now, Delivery.PENDING, now, minutes, run = "t-plan%03d".format(made))
        plans = listOf(plan) + plans
        started = nowMillis
        return plan
    }

    /**
     * Ends the open plan once [planMillis] passed: rows from [work] (the host's order) and the open
     * [items], pinned items first, never one moved down; `dueSoon` and `changed` against the previous plan.
     */
    fun advance(now: Long, nowMillis: Long, work: List<WorkItem>, items: List<InboxItem>) {
        val since = started ?: return
        if (nowMillis - since < planMillis) return
        started = null
        val open = plans.first()
        val previous = plans.firstOrNull { it.delivery == Delivery.RESULTED }
        val live = work.filter { it.state != WorkState.DONE && it.state != WorkState.WITHDRAWN }
        val rows = (live.filter { it.laterAt == null }.map(::workRow) + items.filter { it.state == ItemState.OPEN && it.laterAt == null }.map(::itemRow))
            .take(TOP)
        val dueSoon = live.filter { it.laterAt != null && (it.due?.at ?: Long.MAX_VALUE) <= now + DAY }.sortedBy { it.due?.at }.map { it.id }
        val resulted = open.copy(
            rev = open.rev + 1, delivery = Delivery.RESULTED, deliveryAt = now, inputsAt = now,
            workspaces = delegates.map { it.workspace }, top = rows, dueSoon = dueSoon, changed = changes(previous?.top.orEmpty(), rows),
        )
        plans = plans.map { if (it.id == open.id) resulted else it }
    }

    /**
     * `apply-notes` for a request's Result: writes while the live file still hashes to the proposal's
     * base, else `conflict`. [notes] is the Result's proposal; [workspace] the request's.
     */
    fun applyNotes(workspace: String, notes: Notes?, base: String, now: Long) {
        if (notes == null || notes.base != base) throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "this version has no notes with that base")
        if ((live[workspace] ?: EMPTY) != base) throw DemoWork.Refusal(ErrorCode.CONFLICT, "NOTES.md of $workspace changed; ask again")
        live[workspace] = sha256(notes.text)
        delegates = delegates.map { if (it.workspace == workspace) it.copy(notesAt = now) else it }
    }

    /** `apply-notes` for a plan's notes: the overall workspace's. */
    fun applyPlanNotes(id: String, rev: Long, base: String, now: Long): Plan {
        val plan = plans.firstOrNull { it.id == id } ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "no plan $id")
        if (plan.rev != rev) throw DemoWork.Refusal(ErrorCode.CONFLICT, "plan $id is at rev ${plan.rev}")
        applyNotes(overall, plan.notes, base, now)
        return plan
    }

    /** The plan Today shows: the newest `resulted` one. */
    val current: Plan? get() = plans.firstOrNull { it.delivery == Delivery.RESULTED }

    /** Adds [cites] to the current plan's rows (`work` level 11): the records a row's reasons name. */
    fun cite(cites: Map<String, List<Basis>>) {
        val plan = current ?: return
        val cited = plan.copy(top = plan.top.map { row -> row.copy(basis = row.basis + cites[row.work ?: row.item].orEmpty()) })
        plans = plans.map { if (it.id == plan.id) cited else it }
    }

    /**
     * `update-plan` (ADR 0050 §6) on the current plan. Returns the plan, and whether it changed: a
     * reorder to the order shown, or a dismiss of a dismissed row, keeps its `rev`.
     */
    fun update(req: Request.UpdatePlan): Pair<Plan, Boolean> {
        val plan = current?.takeIf { it.id == req.plan }
            ?: throw DemoWork.Refusal(ErrorCode.CONFLICT, "plan ${req.plan} is not the newest resulted plan; list the Inbox again")
        if (plan.rev != req.rev) throw DemoWork.Refusal(ErrorCode.CONFLICT, "plan ${plan.id} is at rev ${plan.rev}; list the Inbox again")
        val rows = plan.top.mapNotNull { it.work ?: it.item }
        val updated = when (req.action) {
            PlanAction.REORDER -> {
                if (req.row != null || req.text != null || req.order.sorted() != rows.sorted()) {
                    throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "update-plan: reorder needs order, every row once")
                }
                plan.copy(userOrder = req.order)
            }
            PlanAction.DISMISS -> {
                val row = req.row
                if (req.order.isNotEmpty() || row == null || row !in rows) throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "update-plan: dismiss needs a row of top")
                if (row in plan.dismissed) plan else plan.copy(dismissed = plan.dismissed + row)
            }
        }
        val shown = plan.userOrder.ifEmpty { rows }
        if (updated == plan || (req.action == PlanAction.REORDER && req.order == shown)) return plan to false
        val saved = updated.copy(rev = plan.rev + 1)
        plans = plans.map { if (it.id == plan.id) saved else it }
        return saved to true
    }

    private fun workRow(w: WorkItem) = if (w.pinnedAt != null) {
        PlanRow(work = w.id, why = "Pinned by you.", next = "Take the next step.", minutes = Minutes(30, 60, 45), basis = listOf(Basis(BasisKind.MANUAL, "Pinned")))
    } else {
        PlanRow(work = w.id, why = "Next in your list.", next = "Read where it stands.", minutes = Minutes(15, 30, 20), basis = listOf(Basis(BasisKind.TIME, "Fits the time you have")))
    }

    private fun itemRow(i: InboxItem) =
        PlanRow(item = i.id, why = "A decision waits on you.", next = "Answer it.", minutes = Minutes(1, 5, 2), basis = listOf(Basis(BasisKind.DEPENDENCY, "An agent waits", i.task)))

    private fun changes(before: List<PlanRow>, after: List<PlanRow>): List<PlanChange> {
        val old = before.mapNotNull { it.work ?: it.item }
        val new = after.mapNotNull { it.work ?: it.item }
        val moved = new.mapIndexedNotNull { i, id ->
            when {
                id !in old -> PlanChange(id, ChangeKind.ENTERED)
                old.indexOf(id) != i -> PlanChange(id, ChangeKind.MOVED)
                else -> null
            }
        }
        return moved + (old - new.toSet()).map { PlanChange(it, ChangeKind.LEFT) }
    }

    companion object {
        private const val KEPT = 7
        private const val TOP = 3
        private const val MAX_MINUTES = 1440
        private const val DAY = 86_400L
        private const val EMPTY = DemoResults.EMPTY_NOTES
        private val OPEN = setOf(Delivery.PENDING, Delivery.RECEIVED, Delivery.PROCESSING)

        private fun sha256(text: String): String =
            MessageDigest.getInstance("SHA-256").digest(text.toByteArray()).joinToString("") { "%02x".format(it) }

        /** The UI tests': a plan with w-0000b001 pinned, w-0000b002 and the review i-0000a002; w-0000b003 due soon; one sort and one delegate. */
        fun tests(now: Long) = DemoPlans(
            "secretary",
            listOf(
                Plan(
                    "p-0000d001", 2, now - 3_600, now - 3_600, Delivery.RESULTED, now - 3_500, run = "t-plan000",
                    workspaces = listOf("personal", "work"),
                    top = listOf(
                        PlanRow(
                            work = "w-0000b001", why = "Pinned by you, and the venue needs booking this week.", next = "Ask two venues for May dates.",
                            minutes = Minutes(30, 60, 45),
                            basis = listOf(Basis(BasisKind.MANUAL, "Pinned"), Basis(BasisKind.GOAL, "Team offsite in May")),
                            missing = listOf("Which dates the team prefers"),
                        ),
                        PlanRow(
                            work = "w-0000b002", why = "Its Task waits on your decision.", next = "Answer the agent's question.",
                            minutes = Minutes(5, 15, 10), basis = listOf(Basis(BasisKind.DEPENDENCY, "Task waits on you", "t-auth01")),
                        ),
                        PlanRow(
                            item = "i-0000a002", why = "A review is ready.", next = "Read the change and accept it.",
                            minutes = Minutes(10, 20, 15), basis = listOf(Basis(BasisKind.TIME, "Short to review")),
                        ),
                    ),
                    dueSoon = listOf("w-0000b003"),
                    changed = listOf("w-0000b001", "w-0000b002", "i-0000a002").map { PlanChange(it, ChangeKind.ENTERED) },
                ),
            ),
            listOf(
                sort("i-0000e001", now - 3_500, SortProposal("c-0000c001", SortAction.WORK, "work", "Look into the flaky upload test")),
                delegate(
                    "i-0000e002", now - 3_500, "Plan the team offsite",
                    DelegateProposal("w-0000b001", "personal", AssistScope.RESEARCH, "Venues near the coast for 12 people in May", "Pinned, and nobody asked yet."),
                ),
            ),
            listOf(
                DelegateInfo("personal", Share.SUMMARIES, OverallRequests.REVIEW, notesAt = now - 86_400),
                DelegateInfo("work", Share.OUTLINE, OverallRequests.REVIEW),
            ),
            pins = mapOf("w-0000b001" to now - 7_200),
            dues = mapOf("w-0000b003" to Due(now + 6 * 3_600, "UTC")),
        )

        /** Try the Demo's: a morning plan over the showcase's work. */
        fun showcase(now: Long) = DemoPlans(
            "secretary",
            listOf(
                Plan(
                    "p-5c00d001", 2, now - 5_400, now - 5_400, Delivery.RESULTED, now - 5_300, run = "t-plan000",
                    workspaces = listOf("personal", "work"),
                    top = listOf(
                        PlanRow(
                            work = "w-5c00b001", why = "Pinned by you. The old provider ends in March.", next = "Export the customer list.",
                            minutes = Minutes(45, 90, 60),
                            basis = listOf(Basis(BasisKind.MANUAL, "Pinned"), Basis(BasisKind.DUE, "The old provider ends in March")),
                            missing = listOf("Whether customers keep their payment method"),
                        ),
                        PlanRow(
                            item = "i-5c000001", why = "The agent waits on your permission.", next = "Allow or deny the command.",
                            minutes = Minutes(1, 5, 2), basis = listOf(Basis(BasisKind.DEPENDENCY, "An agent waits", "t-a1b2c3")),
                        ),
                        PlanRow(
                            work = "w-5c00b002", why = "It shares the webhooks with the invoices.", next = "Read the Task's change.",
                            minutes = Minutes(20, 40, 30), basis = listOf(Basis(BasisKind.DEPENDENCY, "Same webhook handler", "/src/app/payments/webhooks.ts")),
                        ),
                    ),
                    dueSoon = listOf("w-5c00b003"),
                    changed = listOf("w-5c00b001", "i-5c000001", "w-5c00b002").map { PlanChange(it, ChangeKind.ENTERED) },
                ),
            ),
            listOf(
                sort("i-5c00e001", now - 5_300, SortProposal("c-5c00c001", SortAction.WORK, "work", "Compare error trackers")),
                delegate(
                    "i-5c00e002", now - 5_300, "Take payments with Stripe",
                    DelegateProposal(
                        "w-5c00b002", "work", AssistScope.COMPARE, "Stripe Checkout or Payment Links for one-off payments",
                        "The Task needs a choice before it goes on.",
                    ),
                ),
            ),
            listOf(
                DelegateInfo("personal", Share.OUTLINE, OverallRequests.REVIEW),
                DelegateInfo("work", Share.SUMMARIES, OverallRequests.REVIEW, notesAt = now - 172_800),
            ),
            pins = mapOf("w-5c00b001" to now - 9_000),
            dues = mapOf("w-5c00b003" to Due(now + 5 * 3_600, "UTC")),
        )

        private val PROPOSAL_ANSWERS = listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.LATER)

        private fun sort(id: String, at: Long, p: SortProposal) = InboxItem(
            id, ItemKind.SORT, ItemState.OPEN, "Make a Work item in ${p.workspace}: ${p.title}", "secretary", p.title.orEmpty(), at,
            sort = p, answers = PROPOSAL_ANSWERS,
        )

        private fun delegate(id: String, at: Long, title: String, p: DelegateProposal) = InboxItem(
            id, ItemKind.DELEGATE, ItemState.OPEN, "Ask ${p.workspace} to ${p.scope.raw}: $title", "secretary", title, at,
            delegate = p, answers = PROPOSAL_ANSWERS,
        )
    }
}
