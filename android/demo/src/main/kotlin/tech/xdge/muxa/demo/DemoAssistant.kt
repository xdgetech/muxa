package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.AssistSpec
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.Capture
import tech.xdge.muxa.protocol.CaptureSource
import tech.xdge.muxa.protocol.CaptureState
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.Due
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.SortAction
import tech.xdge.muxa.protocol.WorkAction
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkState
import java.time.DateTimeException
import java.time.ZoneId

/**
 * The demo bridge's Captures, Work items and assistant requests (ADR 0048, `work` level 9), with the
 * host's checks: one Capture per `clientId`, a stale `rev` is `conflict`, Later moves an item down in
 * the order moved, one Task per Work item `rev`, one request per `(work, rev)`, and one open run per
 * workspace. A run moves on at [pace] when the Inbox is listed. Not thread safe: the bridge holds its lock.
 */
internal class DemoAssistant(
    private var captures: List<Capture>,
    private var work: List<WorkItem>,
    private var requests: List<AssistantRequest> = emptyList(),
) {
    /** The Task made for each Work item and `rev`, so a second `create-task` returns it. */
    private val tasks = mutableMapOf<Pair<String, Long>, String>()

    /** The request made for each Work item and `rev`, so a second `request-assistant` returns it. */
    private val asked = mutableMapOf<Pair<String, Long>, String>()

    /** When each request's current version started here, in milliseconds; the fixtures' never move. */
    private val started = mutableMapOf<String, Long>()
    private var made = 0

    var pace: AssistPace = AssistPace.DEMO

    /** What `request-assistant` did; [ended] is the older version's run, whose session closes. */
    class Asked(val work: WorkItem, val request: AssistantRequest, val run: String?, val ended: String?)

    /** What `capture {assist}` did. The Capture is saved even with a [refusal]. */
    class Assisted(val capture: Capture, val work: WorkItem?, val asked: Asked?, val refusal: DemoWork.Refusal?)

    /** The unsorted Captures, newest first. */
    val unsorted: List<Capture> get() = captures.filter { it.state == CaptureState.RECEIVED }.sortedByDescending { it.createdAt }

    /**
     * `work` as `inbox-list` sends it: the pinned ones in pin order (level 10), then the other open
     * ones, newest first; then the ones Later moved down, in the order moved; then the closed ones,
     * newest change first. [derive] gives an open item the state its Tasks make.
     */
    fun listed(derive: (WorkItem) -> WorkState): List<WorkItem> {
        val (closed, live) = work.partition { it.isClosed }
        val (pinned, unpinned) = live.filter { it.laterAt == null }.partition { it.pinnedAt != null }
        val open = pinned.sortedBy { it.pinnedAt } + unpinned.sortedByDescending { it.createdAt }
        val later = live.filter { it.laterAt != null }.sortedBy { it.laterAt }
        return (open + later).map { it.copy(state = derive(it)) } + closed.sortedByDescending { it.updatedAt }.take(CLOSED_LISTED)
    }

    fun capture(req: Request.Capture, now: Long): Capture {
        captures.firstOrNull { it.clientId == req.clientId }?.let { return it }
        if (req.clientId.isEmpty() || req.clientId.length > 64 || req.clientId.any { it.code !in 0x21..0x7e }) {
            throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "clientId must be 1 to 64 printable ASCII bytes")
        }
        if (req.text.toByteArray().size !in 1..MAX_TEXT) throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "text must be 1 byte to 16 KiB")
        val c = Capture(nextId("c"), 1, req.clientId, req.text, req.source ?: CaptureSource.APP, CaptureState.RECEIVED, now, now, req.workspace)
        captures = captures + c
        return c
    }

    /** Sorts a received Capture; `work` and `link` return the Work item too. */
    fun sort(req: Request.SortCapture, now: Long): Pair<Capture, WorkItem?> {
        val c = captures.firstOrNull { it.id == req.capture } ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "${req.capture} is not a capture")
        if (c.rev != req.rev) throw DemoWork.Refusal(ErrorCode.CONFLICT, "${c.id} changed: rev ${c.rev} is current")
        if (c.state != CaptureState.RECEIVED) throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "${c.id} is sorted")
        val item = when (req.action) {
            SortAction.WORK -> {
                val title = req.title ?: c.text.lineSequence().map { it.trim() }.firstOrNull { it.isNotEmpty() }.orEmpty().take(200)
                WorkItem(nextId("w"), 1, title, WorkState.OPEN, now, now, req.workspace ?: c.workspace, captures = listOf(c.id))
                    .also { work = work + it }
            }
            SortAction.LINK -> {
                val target = work.firstOrNull { it.id == req.work } ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "link needs a Work item")
                save(target.copy(captures = target.captures + c.id), now)
            }
            SortAction.NOTE -> null
        }
        val sorted = c.copy(rev = c.rev + 1, state = CaptureState.SORTED, updatedAt = now, work = listOfNotNull(item?.id))
        captures = captures.map { if (it.id == c.id) sorted else it }
        return sorted to item
    }

    /**
     * [laterAt] is what `later` sets; it grows, so the list keeps the order moved. `pin` (level 10) sets
     * `pinnedAt` the same way and clears `laterAt`; a pin of a pinned item, or an unpin of an unpinned
     * one, keeps `rev`. `later`, `complete` and `withdraw` clear `pinnedAt`.
     */
    fun update(req: Request.UpdateWork, now: Long, laterAt: Long): WorkItem {
        val w = current(req.work, req.rev)
        val changed = when (req.action) {
            WorkAction.EDIT -> w.copy(title = req.title ?: w.title, workspace = req.workspace ?: w.workspace, due = req.due?.let(::checked) ?: w.due)
            WorkAction.LATER -> w.copy(laterAt = laterAt, pinnedAt = null)
            WorkAction.COMPLETE -> w.copy(state = WorkState.DONE, pinnedAt = null)
            WorkAction.WITHDRAW -> w.copy(state = WorkState.WITHDRAWN, pinnedAt = null)
            WorkAction.REOPEN -> w.copy(state = WorkState.OPEN, laterAt = null)
            WorkAction.MERGE -> throw DemoWork.Refusal(ErrorCode.UNSUPPORTED, "The demo does not merge Work items.")
            WorkAction.PIN -> {
                if (w.isClosed) throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "${w.id} is done or withdrawn; reopen it first")
                if (w.pinnedAt != null) return w
                w.copy(pinnedAt = maxOf(now, (work.mapNotNull { it.pinnedAt }.maxOrNull() ?: 0) + 1), laterAt = null)
            }
            WorkAction.UNPIN -> if (w.pinnedAt == null) return w else w.copy(pinnedAt = null)
        }
        return save(changed, now)
    }

    private fun checked(due: Due): Due {
        val known = try {
            ZoneId.of(due.timezone)
            due.timezone.contains('/') || due.timezone == "UTC"
        } catch (e: DateTimeException) {
            false
        }
        if (due.at <= 0 || !known) throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "due needs at above 0 and a known IANA timezone")
        return due
    }

    /** The plans' fixtures (ADR 0049): pins and deadlines, without a new `rev`. */
    fun seed(pins: Map<String, Long>, dues: Map<String, Due>) {
        work = work.map { w -> w.copy(pinnedAt = pins[w.id] ?: w.pinnedAt, due = dues[w.id] ?: w.due) }
    }

    fun workItem(id: String): WorkItem? = work.firstOrNull { it.id == id }

    fun captureOf(id: String): Capture? = captures.firstOrNull { it.id == id }

    fun request(id: String): AssistantRequest? = requests.firstOrNull { it.id == id }

    /**
     * What a plan's listing derives now: the requests of open Work items whose current version has no
     * result and is not cancelled, and those whose newest result is older than their item's change.
     */
    fun unansweredAndStale(): Pair<List<String>, List<String>> {
        val open = work.filterNot { it.isClosed }.associateBy { it.id }
        val mine = requests.filter { it.work in open }
        val unanswered = mine.filter { q ->
            q.results.none { it.version == q.version } && q.delivery != Delivery.CANCELLED && q.delivery != Delivery.CANCEL_REQUESTED
        }
        val stale = mine.filter { q -> q.results.firstOrNull()?.let { it.at < (open[q.work]?.updatedAt ?: 0) } == true }
        return unanswered.map { it.id } to stale.map { it.id }
    }

    /** The latest `laterAt`, for the next one. */
    val lastLaterAt: Long? get() = work.mapNotNull { it.laterAt }.maxOrNull()

    /** The Task already made for [work] at [rev]; refuses a stale [rev]. */
    fun taskFor(work: String, rev: Long): String? {
        tasks[work to rev]?.let { return it }
        current(work, rev)
        return null
    }

    /** The Work item lists [task], made for it at [rev]. */
    fun started(work: String, rev: Long, task: String, now: Long) {
        val w = this.work.firstOrNull { it.id == work } ?: return
        tasks[work to rev] = task
        save(w.copy(tasks = w.tasks + task), now)
    }

    /** The requests of [items], as `inbox-list` sends them. */
    fun requestsOf(items: List<WorkItem>): List<AssistantRequest> {
        val ids = items.flatMap { it.requests }.toSet()
        return requests.filter { it.id in ids }
    }

    /** A request's run may still work on [item]: the host lists it `in-progress`. */
    fun running(item: WorkItem): Boolean = requests.any { it.work == item.id && it.delivery in OPEN_RUN }

    /** `get-work`: the item, its Captures in its order, and its requests. */
    fun detail(id: String): Triple<WorkItem, List<Capture>, List<AssistantRequest>> {
        val w = work.firstOrNull { it.id == id } ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "no Work item $id")
        return Triple(w, w.captures.mapNotNull { c -> captures.firstOrNull { it.id == c } }, requestsOf(listOf(w)))
    }

    /**
     * `request-assistant`, with the host's checks. Asking again at the item's new `rev` adds a version to
     * that workspace's request and ends the older version's run. [run] starts the new run in a session
     * named after its scope, request and version, and returns the run's id.
     */
    fun ask(req: Request.RequestAssistant, now: Long, nowMillis: Long, workspaces: Set<String>, run: (String) -> String): Asked {
        asked[req.work to req.rev]?.let { id ->
            val q = requests.first { it.id == id }
            return Asked(work.first { it.id == req.work }, q, null, null)
        }
        val w = current(req.work, req.rev)
        if (w.state == WorkState.DONE || w.state == WorkState.WITHDRAWN) {
            throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "Work item ${w.id} is done or withdrawn; reopen it first")
        }
        val ws = req.workspace ?: w.workspace ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "Work item ${w.id} has no workspace; send one")
        if (ws !in workspaces) throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "no workspace \"$ws\"")
        if ((req.purpose?.toByteArray()?.size ?: 0) > MAX_PURPOSE) throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "purpose is over 4096 bytes")
        val existing = requests.firstOrNull { it.work == w.id && it.workspace == ws }
        val open = requests.count { it.workspace == ws && it.id != existing?.id && it.delivery in OPEN_RUN }
        if (open >= MAX_OPEN_RUNS) {
            throw DemoWork.Refusal(
                ErrorCode.CONFLICT,
                "workspace \"$ws\" has $open open assistant runs, its max_open_runs. It frees when a run ends or is cancelled.",
            )
        }
        val id = existing?.id ?: nextId("q")
        val version = (existing?.version ?: 0) + 1
        val runId = run("${req.scope.raw}-$id-v$version")
        val q = existing?.copy(
            version = version, scope = req.scope, purpose = req.purpose, delivery = Delivery.PENDING,
            deliveryAt = now, updatedAt = now, run = runId,
        ) ?: AssistantRequest(id, w.id, version, ws, req.scope, Delivery.PENDING, now, now, now, req.purpose, runId)
        requests = if (existing != null) requests.map { if (it.id == q.id) q else it } else requests + q
        started[q.id] = nowMillis
        asked[w.id to req.rev] = q.id
        val saved = save(w.copy(requests = if (q.id in w.requests) w.requests else w.requests + q.id), now)
        return Asked(saved, q, runId, existing?.run?.takeIf { existing.delivery in OPEN_RUN })
    }

    /** `cancel-request`: `cancel-requested` until the next list, then `cancelled`. Returns the item, the request and the run that ends. */
    fun cancel(req: Request.CancelRequest, now: Long): Triple<WorkItem, AssistantRequest, String?> {
        val q = requests.firstOrNull { it.id == req.request } ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "no request ${req.request}")
        if (q.version != req.version) throw DemoWork.Refusal(ErrorCode.CONFLICT, "request ${q.id} is at version ${q.version}; list it again")
        if (q.results.any { it.version == req.version }) throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "request ${q.id} v${q.version} has its result")
        val item = work.first { it.id == q.work }
        if (q.delivery !in CANCELLABLE) return Triple(item, q, null)
        val c = q.copy(delivery = Delivery.CANCEL_REQUESTED, deliveryAt = now, updatedAt = now)
        requests = requests.map { if (it.id == c.id) c else it }
        started.remove(c.id)
        return Triple(item, c, q.run)
    }

    /** Moves each run on at [pace]. Returns the runs whose Result came, so their sessions close. */
    fun advance(now: Long, nowMillis: Long): List<String> {
        val ended = mutableListOf<String>()
        requests = requests.map { q ->
            val since = started[q.id]
            when {
                q.delivery == Delivery.CANCEL_REQUESTED -> q.copy(delivery = Delivery.CANCELLED, deliveryAt = now, updatedAt = now)
                since == null || q.delivery !in MOVING -> q
                nowMillis - since >= pace.pendingMillis + pace.processingMillis -> {
                    started.remove(q.id)
                    q.run?.let { ended += it }
                    resulted(q, now)
                }
                q.delivery == Delivery.PENDING && nowMillis - since >= pace.pendingMillis ->
                    q.copy(delivery = Delivery.PROCESSING, deliveryAt = now, updatedAt = now)
                else -> q
            }
        }
        return ended
    }

    /** The canned Result; it may name another open item as a possible duplicate. A stored result makes the older ones stale. */
    private fun resulted(q: AssistantRequest, now: Long): AssistantRequest {
        val other = work.firstOrNull { it.id != q.work && it.state != WorkState.DONE && it.state != WorkState.WITHDRAWN }?.id
        val results = listOf(DemoResults.canned(q, now, other)) + q.results.map { it.copy(stale = true) }
        return q.copy(delivery = Delivery.RESULTED, deliveryAt = now, updatedAt = now, results = results.take(MAX_RESULTS))
    }

    /**
     * `capture {assist}`: the Capture becomes a Work item in its workspace and is asked about. When the
     * assistant cannot start the Capture is kept. A resend of a sorted Capture starts nothing.
     */
    fun assist(c: Capture, spec: AssistSpec, now: Long, nowMillis: Long, workspaces: Set<String>, run: (String) -> String): Assisted {
        if (c.state == CaptureState.SORTED) {
            val w = c.work.firstOrNull()?.let { id -> work.firstOrNull { it.id == id } }
            val q = requests.firstOrNull { it.work == w?.id && it.workspace == c.workspace }
            val a = if (w != null && q != null) Asked(w, q, null, null) else null
            return Assisted(c, w, a, null)
        }
        val ws = c.workspace ?: return Assisted(c, null, null, DemoWork.Refusal(ErrorCode.BAD_REQUEST, "capture: assist needs a workspace"))
        if (ws !in workspaces) return Assisted(c, null, null, DemoWork.Refusal(ErrorCode.BAD_REQUEST, "no workspace \"$ws\""))
        val (sorted, w) = sort(Request.SortCapture(0, c.id, c.rev, SortAction.WORK, workspace = ws), now)
        val item = w ?: return Assisted(sorted, null, null, DemoWork.Refusal(ErrorCode.INTERNAL, "the Capture made no Work item"))
        return try {
            val a = ask(Request.RequestAssistant(0, item.id, item.rev, spec.scope, ws, spec.purpose), now, nowMillis, workspaces, run)
            Assisted(sorted, a.work, a, null)
        } catch (e: DemoWork.Refusal) {
            Assisted(sorted, item, null, e)
        }
    }

    private val WorkItem.isClosed get() = state == WorkState.DONE || state == WorkState.WITHDRAWN

    private fun current(id: String, rev: Long): WorkItem {
        val w = work.firstOrNull { it.id == id } ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "$id is not a Work item")
        if (w.rev != rev) throw DemoWork.Refusal(ErrorCode.CONFLICT, "$id changed: rev ${w.rev} is current")
        return w
    }

    private fun save(w: WorkItem, now: Long): WorkItem {
        val saved = w.copy(rev = w.rev + 1, updatedAt = now)
        work = work.map { if (it.id == saved.id) saved else it }
        return saved
    }

    private fun nextId(prefix: String) = "$prefix-%08x".format(0xd0000000L + ++made)

    companion object {
        private const val MAX_TEXT = 16 * 1024
        private const val CLOSED_LISTED = 100
        private const val MAX_PURPOSE = 4096
        private const val MAX_RESULTS = 10

        /** `[workspace.assistant] max_open_runs`' default. */
        private const val MAX_OPEN_RUNS = 1

        /** A run that may still work, and counts against the open runs. */
        private val OPEN_RUN = setOf(Delivery.PENDING, Delivery.RECEIVED, Delivery.PROCESSING, Delivery.UNCONFIRMED, Delivery.CANCEL_REQUESTED)
        private val CANCELLABLE = OPEN_RUN - Delivery.CANCEL_REQUESTED
        private val MOVING = setOf(Delivery.PENDING, Delivery.PROCESSING)

        /**
         * The UI tests': one open item, one whose Task (t-auth01) waits on a decision, one
         * moved down with an assistant Result, and one done; two unsorted Captures and two sorted ones.
         */
        fun tests(now: Long) = DemoAssistant(
            listOf(
                capture("c-0000c001", "Look into the flaky upload test\nIt failed twice on CI this week.", now - 120),
                capture("c-0000c002", "Ask Dana about the beta dates", now - 1_800, "work"),
                capture("c-0000c003", "Plan the team offsite\nTwo days in May, near the coast.", now - 3_600, "personal", "w-0000b001"),
                capture("c-0000c004", "Renew the domain\nThe registrar raised its price.", now - 86_400, null, "w-0000b003"),
            ),
            listOf(
                WorkItem("w-0000b001", 2, "Plan the team offsite", WorkState.OPEN, now - 3_600, now - 3_600, "personal", captures = listOf("c-0000c003")),
                WorkItem("w-0000b002", 3, "Ship passkey sign-in", WorkState.OPEN, now - 7_200, now - 90, "work", tasks = listOf("t-auth01")),
                WorkItem(
                    "w-0000b003", 4, "Renew the domain", WorkState.OPEN, now - 86_400, now - 600, laterAt = now - 600,
                    captures = listOf("c-0000c004"), requests = listOf("q-0000a001"),
                ),
                WorkItem("w-0000b004", 5, "Write the launch post", WorkState.DONE, now - 172_800, now - 86_400, "work"),
            ),
            DemoResults.tests(now),
        )

        /** The showcase's, for Try the Demo: what a week of ideas looks like. */
        fun showcase(now: Long) = DemoAssistant(
            listOf(
                capture("c-5c00c001", "Compare error trackers before the renewal\nSentry, Bugsnag, Rollbar.", now - 300),
                capture("c-5c00c002", "Dana: can the beta open a week early?", now - 2_400, "work"),
                capture(
                    "c-5c00c003", "Move invoices to Stripe Billing\nThe old provider ends in March. What do we need to change?",
                    now - 5_400, "work", "w-5c00b001",
                ),
            ),
            listOf(
                WorkItem(
                    "w-5c00b001", 3, "Move invoices to Stripe Billing", WorkState.OPEN, now - 5_400, now - 1_200, "work",
                    captures = listOf("c-5c00c003"), requests = listOf("q-5c00a001"),
                ),
                WorkItem("w-5c00b002", 3, "Take payments with Stripe", WorkState.OPEN, now - 9_000, now - 600, "work", tasks = listOf("t-a1b2c3")),
                WorkItem("w-5c00b003", 2, "Book the dentist", WorkState.OPEN, now - 172_800, now - 3_600, "personal", laterAt = now - 3_600),
            ),
            DemoResults.showcase(now),
        )

        /** With [work], a Capture sorted into that Work item. */
        private fun capture(id: String, text: String, at: Long, workspace: String? = null, work: String? = null) =
            if (work == null) {
                Capture(id, 1, "demo-$id", text, CaptureSource.APP, CaptureState.RECEIVED, at, at, workspace)
            } else {
                Capture(id, 2, "demo-$id", text, CaptureSource.APP, CaptureState.SORTED, at, at, workspace, listOf(work))
            }
    }
}
