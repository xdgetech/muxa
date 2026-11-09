package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.Capture
import tech.xdge.muxa.protocol.CaptureSource
import tech.xdge.muxa.protocol.CaptureState
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.SortAction
import tech.xdge.muxa.protocol.WorkAction
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkState

/**
 * The demo bridge's Captures and Work items (ADR 0046, `work` level 8), with the host's checks:
 * one Capture per `clientId`, a stale `rev` is `conflict`, Later moves an item down in the order
 * moved, and one Task per Work item `rev`. No assistant runs. Not thread safe: the bridge holds its lock.
 */
internal class DemoAssistant(private var captures: List<Capture>, private var work: List<WorkItem>) {
    /** The Task made for each Work item and `rev`, so a second `create-task` returns it. */
    private val tasks = mutableMapOf<Pair<String, Long>, String>()
    private var made = 0

    /** The unsorted Captures, newest first. */
    val unsorted: List<Capture> get() = captures.filter { it.state == CaptureState.RECEIVED }.sortedByDescending { it.createdAt }

    /**
     * `work` as `inbox-list` sends it: open first, newest first; then the ones Later moved
     * down, in the order moved; then the closed ones, newest change first. [derive] gives an
     * open item the state its Tasks make.
     */
    fun listed(derive: (WorkItem) -> WorkState): List<WorkItem> {
        val (closed, live) = work.partition { it.state == WorkState.DONE || it.state == WorkState.WITHDRAWN }
        val open = live.filter { it.laterAt == null }.sortedByDescending { it.createdAt }
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

    /** [laterAt] is what `later` sets; it grows, so the list keeps the order moved. */
    fun update(req: Request.UpdateWork, now: Long, laterAt: Long): WorkItem {
        val w = current(req.work, req.rev)
        val changed = when (req.action) {
            WorkAction.EDIT -> w.copy(title = req.title ?: w.title, workspace = req.workspace ?: w.workspace)
            WorkAction.LATER -> w.copy(laterAt = laterAt)
            WorkAction.COMPLETE -> w.copy(state = WorkState.DONE)
            WorkAction.WITHDRAW -> w.copy(state = WorkState.WITHDRAWN)
            WorkAction.REOPEN -> w.copy(state = WorkState.OPEN, laterAt = null)
            WorkAction.MERGE -> throw DemoWork.Refusal(ErrorCode.UNSUPPORTED, "The demo does not merge Work items.")
        }
        return save(changed, now)
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

        /**
         * The UI tests': one open item, one whose Task (t-auth01) waits on a decision, one
         * moved down and one done; two unsorted Captures.
         */
        fun tests(now: Long) = DemoAssistant(
            listOf(
                capture("c-0000c001", "Look into the flaky upload test\nIt failed twice on CI this week.", now - 120),
                capture("c-0000c002", "Ask Dana about the beta dates", now - 1_800, "work"),
            ),
            listOf(
                WorkItem("w-0000b001", 2, "Plan the team offsite", WorkState.OPEN, now - 3_600, now - 3_600, "personal"),
                WorkItem("w-0000b002", 3, "Ship passkey sign-in", WorkState.OPEN, now - 7_200, now - 90, "work", tasks = listOf("t-auth01")),
                WorkItem("w-0000b003", 4, "Renew the domain", WorkState.OPEN, now - 86_400, now - 600, laterAt = now - 600),
                WorkItem("w-0000b004", 5, "Write the launch post", WorkState.DONE, now - 172_800, now - 86_400, "work"),
            ),
        )

        /** The showcase's, for Try the Demo: what a week of ideas looks like. */
        fun showcase(now: Long) = DemoAssistant(
            listOf(
                capture("c-5c00c001", "Compare error trackers before the renewal\nSentry, Bugsnag, Rollbar.", now - 300),
                capture("c-5c00c002", "Dana: can the beta open a week early?", now - 2_400, "work"),
            ),
            listOf(
                WorkItem("w-5c00b001", 2, "Move invoices to Stripe Billing", WorkState.OPEN, now - 5_400, now - 5_400, "work"),
                WorkItem("w-5c00b002", 3, "Take payments with Stripe", WorkState.OPEN, now - 9_000, now - 600, "work", tasks = listOf("t-a1b2c3")),
                WorkItem("w-5c00b003", 2, "Book the dentist", WorkState.OPEN, now - 172_800, now - 3_600, "personal", laterAt = now - 3_600),
            ),
        )

        private fun capture(id: String, text: String, at: Long, workspace: String? = null) =
            Capture(id, 1, "demo-$id", text, CaptureSource.APP, CaptureState.RECEIVED, at, at, workspace)
    }
}
