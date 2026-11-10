package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.MemoryMode
import tech.xdge.muxa.protocol.MemoryProposal
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.protocol.WorkspaceInfo
import java.security.MessageDigest
import java.time.LocalDate

/**
 * The demo bridge's workspace memory (ADR 0053, `work` level 14): each workspace's `memory` mode,
 * a closed Task whose end proposed to the memory, and its `memory` item. Approve needs the hash of
 * the text the client showed, and appends it once. Not thread safe: the bridge holds its lock.
 */
internal class DemoMemory(
    /** Each workspace's mode; a workspace not named here reports none. */
    private val modes: Map<String, MemoryMode>,
    /** The Tasks the fixtures add for their items; the showcase reuses its own. */
    val tasks: List<TaskRecord>,
    val items: List<InboxItem>,
    /** Epoch seconds of each workspace's last WORKSPACE.md change. */
    private var changedAt: Map<String, Long>,
) {
    /** Each workspace's WORKSPACE.md, as approves appended to it. Never shown: the host has no request for it. */
    private var files: Map<String, String> = emptyMap()

    /** [workspaces] as level 14's `task-list` sends them: with `memory` and `memoryAt`. */
    fun listed(workspaces: List<WorkspaceInfo>): List<WorkspaceInfo> =
        workspaces.map { w -> modes[w.name]?.let { w.copy(memory = it, memoryAt = changedAt[w.name]) } ?: w }

    /** The refusal for an approve without the shown text's hash, as the host words it. */
    fun checkHash(item: InboxItem, hash: String?): MemoryProposal {
        val m = item.memory ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "${item.id} proposes no memory")
        if (hash != m.hash) throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, NOT_SHOWN)
        return m
    }

    /** Appends [m] under its heading to [workspace]'s file, within the cap; `conflict` with the sizes past it. */
    fun append(workspace: String, m: MemoryProposal, now: Long) {
        val before = files[workspace].orEmpty()
        val block = "## ${m.title} (${m.task}, ${LocalDate.ofEpochDay(now / DAY)})\n\n${m.text}\n"
        val after = if (before.isEmpty()) block else "$before\n$block"
        val size = after.toByteArray().size
        if (size > MAX_FILE) throw DemoWork.Refusal(ErrorCode.CONFLICT, "WORKSPACE.md would be $size bytes; the cap is 16 KiB ($MAX_FILE bytes)")
        files = files + (workspace to after)
        changedAt = changedAt + (workspace to now)
    }

    /** [workspace]'s WORKSPACE.md as approves left it, for the demo's tests. */
    fun file(workspace: String): String = files[workspace].orEmpty()

    companion object {
        /** The host's words for an approve without the right hash (PROTOCOL.md, "Workspace memory"). */
        const val NOT_SHOWN = "this app cannot show the whole text; update the app or use muxa-cli"

        private const val MAX_FILE = 16_384
        private const val DAY = 86_400L
        private val ANSWERS = listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.LATER)

        /** The UI tests': a done Task whose text is 4 KiB of numbered lines, so a test can find every one. */
        fun tests(now: Long): DemoMemory {
            val task = demoTask(
                "t-memo01", "work", "Cache the search results", "Cache search results for five minutes.",
                TaskStatus.DONE, now - 1_200, project = "App", dir = "/work/tasks/cache-the-search-results-t-memo01",
            )
            return DemoMemory(
                mapOf("work" to MemoryMode.REVIEW, "personal" to MemoryMode.AUTO, "notes" to MemoryMode.OFF),
                listOf(task),
                listOf(item("i-0000e101", task, numberedLines(), now - 1_200)),
                mapOf("work" to now - 3 * DAY),
            )
        }

        /** Try the Demo's: the VAT return taught two facts about the personal workspace. */
        fun showcase(now: Long): DemoMemory {
            val task = demoTask(
                "t-9b8c7d", "personal", "File the VAT return", "Collect the quarter's invoices and file the VAT return.",
                TaskStatus.DONE, now - 21_600,
            )
            val text = "- Invoices live in ~/personal/invoices, one folder per quarter.\n" +
                "- The VAT portal signs in with the authenticator app, not SMS.\n" +
                "- Filing takes about 20 minutes once the invoices are in one folder."
            return DemoMemory(
                mapOf("work" to MemoryMode.REVIEW, "personal" to MemoryMode.REVIEW),
                emptyList(),
                listOf(item("i-5c00e101", task, text, now - 21_600)),
                mapOf("work" to now - 2 * DAY),
            )
        }

        /** `sha256:` and the hex SHA-256 of [task], NUL, [title], NUL, [text] (PROTOCOL.md, "Workspace memory"). */
        fun hash(task: String, title: String, text: String): String {
            val digest = MessageDigest.getInstance("SHA-256").digest("$task\u0000$title\u0000$text".toByteArray())
            return "sha256:" + digest.joinToString("") { "%02x".format(it) }
        }

        /** 50 numbered lines of 80 characters, 4 049 bytes: near the 4 KiB a proposal holds. */
        fun numberedLines(): String =
            (1..LINES).joinToString("\n") { "Line %02d: search results are cached for five minutes per user".format(it).padEnd(LINE, '.') }

        private const val LINES = 50
        private const val LINE = 80

        private fun item(id: String, task: TaskRecord, text: String, at: Long) = InboxItem(
            id, ItemKind.MEMORY, ItemState.OPEN, "${task.title} proposes to add to the workspace memory", task.workspace, task.title, at,
            task = task.id, memory = MemoryProposal(task.id, task.title, text, hash(task.id, task.title, text)), answers = ANSWERS,
        )
    }
}
