package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.Attachments
import tech.xdge.muxa.protocol.ChangeCount
import tech.xdge.muxa.protocol.ChatPrompt
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxEntry
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.SourceLink
import tech.xdge.muxa.protocol.TaskStatus

/**
 * The demo bridge's Inbox items (ADR 0033, `work` level 3), with the host's checks: only an
 * action in `answers`, a text for Send, Redo and Change course, and a valid option. Later moves
 * an item down and ignores `until` (ADR 0040).
 * A Routine's entries are items too; [DemoWork] keeps them. Not thread safe: the bridge holds its lock.
 */
internal class DemoItems(private var items: List<InboxItem>) {
    /** What [answer] did: the saved item, and the status its Task gets, if any. */
    class Answer(val item: InboxItem, val status: TaskStatus?)

    /**
     * `items` as `inbox-list` sends it: open ones not moved down, `ask` before the rest, then newest
     * first; then the moved-down ones in the order they were moved; then answered. [laterAts] are
     * the entries' `laterAt`, by id. [askAgent]: the host offers `ask-agent` (ADR 0041).
     */
    fun list(entries: List<InboxEntry>, laterAts: Map<String, Long>, askAgent: Boolean): List<InboxItem> {
        val all = items + entries.map { fromEntry(it, laterAts[it.id], askAgent) }
        val open = all.filter { it.state == ItemState.OPEN && it.laterAt == null }
            .sortedWith(compareByDescending<InboxItem> { it.kind == ItemKind.ASK }.thenByDescending { it.createdAt })
        val later = all.filter { it.state == ItemState.OPEN && it.laterAt != null }.sortedBy { it.laterAt }
        val closed = all.filter { it.state == ItemState.ANSWERED || it.state == ItemState.RESOLVED }.sortedByDescending { it.createdAt }
        return open + later + closed
    }

    fun has(id: String) = items.any { it.id == id }

    /** The latest `laterAt` of the items. */
    val lastLaterAt: Long? get() = items.mapNotNull { it.laterAt }.maxOrNull()

    /** [laterAt] is what `later` sets: see [DemoWork.nextLaterAt]. */
    fun answer(id: String, action: InboxAction, text: String?, option: Int?, now: Long, laterAt: Long): Answer {
        val item = items.firstOrNull { it.id == id }
            ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "no Inbox item $id; list the Inbox again")
        if (action != InboxAction.READ && action !in item.answers) {
            throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "$action does not answer Inbox item $id now")
        }
        val saved = when (action) {
            InboxAction.READ -> item.copy(readAt = item.readAt ?: now)
            InboxAction.LATER -> item.copy(laterAt = laterAt)
            else -> {
                check(action, item, text, option)
                item.copy(state = ItemState.ANSWERED, answer = action, answeredAt = now, laterAt = null, answers = emptyList())
            }
        }
        items = items.map { if (it.id == id) saved else it }
        val status = when (action) {
            InboxAction.ACCEPT -> TaskStatus.DONE
            InboxAction.DROP -> TaskStatus.DROPPED
            else -> null
        }
        return Answer(saved, status)
    }

    private fun check(action: InboxAction, item: InboxItem, text: String?, option: Int?) {
        if (action == InboxAction.REPLY || action == InboxAction.REDO || action == InboxAction.CHANGE_COURSE) {
            val bytes = text?.toByteArray()?.size ?: 0
            if (bytes !in 1..MAX_TEXT) throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "$action needs a text of 1 to $MAX_TEXT bytes")
        }
        if (action == InboxAction.CHOOSE && (option == null || option !in item.prompt?.options.orEmpty().indices)) {
            throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "choose needs an option of the prompt")
        }
    }

    companion object {
        private const val MAX_TEXT = 4096
        /**
         * A Routine's entry as an item; it keeps its `e-` id, as the host's `from_entry` does.
         * [laterAt] is when Later moved it down. A stored `later` entry is open (ADR 0040).
         * [askAgent]: it offers `ask-agent` (ADR 0041).
         */
        fun fromEntry(entry: InboxEntry, laterAt: Long?, askAgent: Boolean): InboxItem {
            val (state, answer) = when (entry.disposition) {
                "open", "later" -> ItemState.OPEN to null
                "approved" -> ItemState.ANSWERED to InboxAction.APPROVE
                else -> ItemState.ANSWERED to InboxAction.DISMISS
            }
            val kind = if (entry.kind == "proposal") ItemKind.PROPOSAL else ItemKind.BRIEFING
            val answers = when {
                state == ItemState.ANSWERED -> emptyList()
                kind == ItemKind.PROPOSAL -> listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.ASK_AGENT, InboxAction.LATER)
                else -> listOf(InboxAction.DISMISS, InboxAction.ASK_AGENT, InboxAction.LATER)
            }.filter { askAgent || it != InboxAction.ASK_AGENT }
            return InboxItem(
                entry.id, kind, state, entry.title, entry.workspace, entry.title, entry.createdAt,
                laterAt = laterAt.takeIf { state == ItemState.OPEN }, answer = answer, summary = entry.body,
                attachments = Attachments(links = entry.links), routine = entry.routine, proposal = entry.proposal, answers = answers,
            )
        }

        /**
         * Try the Demo's and the store screenshots' items: an agent asking, and two finished turns.
         * The text matches the iOS showcase (`DemoItems.showcaseItems`) byte for byte.
         */
        fun showcase(now: Long) = DemoItems(
            listOf(
                showcaseItem(
                    "i-5c000001", ItemKind.ASK, "Allow Bash: stripe listen --forward-to localhost:4242", "t-a1b2c3",
                    "Handle Stripe invoice.paid webhook", now - 75, session = "$1", pane = "%1",
                    prompt = ChatPrompt(
                        "toolu_demo", ChatPrompt.Kind.PERMISSION,
                        options = listOf(ChatPrompt.Option("Yes", listOf("Enter")), ChatPrompt.Option("No", listOf("Escape"), cancel = true)),
                    ),
                    answers = ASK_ANSWERS,
                ),
                showcaseItem(
                    "i-5c000002", ItemKind.REVIEW, "Ready for review: Dark mode for the dashboard", "t-0a1b2c",
                    "Dark mode for the dashboard", now - 720, session = "$3", pane = "%5",
                    summary = "The dashboard follows the system setting now. The charts use the new palette, and the contrast checks " +
                        "pass. The setting is described in `README.md`; see [PR #318](https://github.com/example/dashboard/pull/318).",
                    attachments = Attachments(
                        changes = ChangeCount(files = 6, added = 212, removed = 48),
                        docs = listOf("README.md"),
                        links = listOf(SourceLink("https://github.com/example/dashboard/pull/318", "PR #318")),
                    ),
                    answers = REVIEW_ANSWERS,
                ),
                showcaseItem(
                    "i-5c000003", ItemKind.REVIEW, "Ready for review: Faster search index", "t-3c4d5e",
                    "Faster search index", now - 1_800, session = "$4", pane = "%6", read = true,
                    summary = "The index builds in 41 s instead of 96 s. Search results are unchanged.",
                    attachments = Attachments(changes = ChangeCount(files = 2, added = 37, removed = 12)),
                    answers = REVIEW_ANSWERS,
                ),
            ),
        )

        /** The host's answers per kind (PROTOCOL.md "Inbox items"), `reply` included (ADR 0033 §4a). */
        private val REVIEW_ANSWERS =
            listOf(InboxAction.ACCEPT, InboxAction.REPLY, InboxAction.REDO, InboxAction.CHANGE_COURSE, InboxAction.KEEP_OPEN, InboxAction.LATER)
        private val FAILURE_ANSWERS =
            listOf(InboxAction.RETRY, InboxAction.REPLY, InboxAction.CHANGE_COURSE, InboxAction.DROP, InboxAction.DISMISS, InboxAction.LATER)
        private val ASK_ANSWERS = listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.REPLY, InboxAction.LATER)

        /** An open item of the showcase's "work" workspace; a read one was opened 30 seconds after it was made. */
        private fun showcaseItem(
            id: String, kind: ItemKind, question: String, task: String, title: String, createdAt: Long,
            session: String, pane: String, read: Boolean = false, summary: String? = null, prompt: ChatPrompt? = null,
            attachments: Attachments = Attachments(), answers: List<InboxAction>,
        ) = InboxItem(
            id, kind, ItemState.OPEN, question, "work", title, createdAt, readAt = if (read) createdAt + 30 else null,
            task = task, session = session, pane = pane, summary = summary, prompt = prompt, attachments = attachments, answers = answers,
        )

        /** The UI tests' items: an ask with a permission prompt, a review with evidence, a failure, and one answered. */
        fun tests(now: Long) = DemoItems(
            listOf(
                InboxItem(
                    "i-0000a001", ItemKind.ASK, ItemState.OPEN, "Allow Bash: rm -rf build", "work", "Add sign-in with passkeys",
                    createdAt = now - 90, task = "t-auth01", session = "$1", pane = "%1",
                    prompt = ChatPrompt(
                        "t:wait1", ChatPrompt.Kind.PERMISSION,
                        options = listOf(ChatPrompt.Option("Allow", listOf("1")), ChatPrompt.Option("Deny", listOf("Escape"), cancel = true)),
                    ),
                    answers = ASK_ANSWERS,
                ),
                InboxItem(
                    "i-0000a002", ItemKind.REVIEW, ItemState.OPEN, "Ready for review: Fix the login redirect", "work", "Fix the login redirect",
                    createdAt = now - 600, task = "t-login1", session = "$4", pane = "%5",
                    summary = "After sign-in, users return to the page they came from. The redirect test passes. " +
                        "The flow is in [the design](docs/design.md), and **login.png** shows the page; see https://github.com/example/app/pull/817.",
                    attachments = Attachments(
                        changes = ChangeCount(files = 3, added = 120, removed = 40, scope = "branch"),
                        docs = listOf("docs/design.md"),
                        images = listOf("build/screenshots/en/login.png"),
                        links = listOf(SourceLink("https://github.com/example/app/pull/817", "PR #817")),
                    ),
                    answers = REVIEW_ANSWERS,
                ),
                InboxItem(
                    "i-0000a003", ItemKind.FAILURE, ItemState.OPEN, "The loop stopped: npm run lint failed 3 times", "personal",
                    "Fix the lint warnings", createdAt = now - 300, readAt = now - 200, task = "t-lint01",
                    summary = "3 warnings are left in src/legacy.js.",
                    answers = FAILURE_ANSWERS,
                ),
                InboxItem(
                    "i-0000a004", ItemKind.REVIEW, ItemState.ANSWERED, "Ready for review: Write the release notes", "work",
                    "Write the release notes", createdAt = now - 86_400, readAt = now - 86_000, answeredAt = now - 85_000,
                    answer = InboxAction.ACCEPT, task = "t-relnot",
                ),
            ),
        )
    }
}
