package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.AssistResult
import tech.xdge.muxa.protocol.AssistScope
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.ResultWrite
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkState
import tech.xdge.muxa.protocol.WriteProposal
import tech.xdge.muxa.protocol.WriterInfo
import java.security.MessageDigest

/**
 * The demo bridge's write proposals (ADR 0052, `work` level 13): work's writer, a Work item whose
 * request proposed one write in its current Result and one in an older one, and their two `write`
 * items. Approve needs the hash of the write the client showed. Nothing here is a real write.
 */
internal class DemoWrites(
    /** Each workspace's writer; a workspace without one takes no writes. */
    val writers: Map<String, WriterInfo>,
    val work: WorkItem,
    val request: AssistantRequest,
    val items: List<InboxItem>,
) {
    /** The refusal for an approve without the shown write's hash, as the host words it. */
    fun checkHash(item: InboxItem, hash: String?): WriteProposal {
        val w = item.write ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "${item.id} proposes no write")
        if (hash != w.hash) throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, NOT_SHOWN)
        return w
    }

    /** The writer that performs [workspace]'s approved writes; `conflict` without one, as the host answers. */
    fun writerOf(workspace: String): WriterInfo =
        writers[workspace] ?: throw DemoWork.Refusal(ErrorCode.CONFLICT, "$workspace has no writer; set writer in workspaces.toml")

    companion object {
        /** The host's words for an approve without the right hash (PROTOCOL.md, "Write proposals"). */
        const val NOT_SHOWN = "this app cannot show the whole write; update the app or use muxa-cli"

        private val ANSWERS = listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.LATER)

        private val SENDER = WriterInfo("work-sender", "Posts approved messages to Slack and comments on Linear. It has write tools and no web access.")

        /** The UI tests': the older write's content is 8 KiB of numbered lines, so a test can find every one. */
        fun tests(now: Long) = make(
            now, work = "w-0000b006", request = "q-0000a003", items = "i-0000d10",
            title = "Announce the 2.4 release", longContent = numberedLines(),
        )

        /** Try the Demo's: the release note and a Linear comment. */
        fun showcase(now: Long) = make(
            now, work = "w-5c00b006", request = "q-5c00a003", items = "i-5c00d10",
            title = "Announce the 2.4 release",
            longContent = "Checked on staging: the sync change keeps retries under the 30 s budget.\n" +
                "The flaky test was the clock, not the client. A fix is in #482.\n\nNo change is needed in this ticket.",
        )

        /** `sha256:` and the hex SHA-256 of [action], NUL, [target], NUL, [content]: the Routine proposal's hash. */
        fun hash(action: String, target: String, content: String): String {
            val digest = MessageDigest.getInstance("SHA-256").digest("$action\u0000$target\u0000$content".toByteArray())
            return "sha256:" + digest.joinToString("") { "%02x".format(it) }
        }

        /** The goal the writer's Task gets: the approved write unchanged, and the marker last (ADR 0052 §3). */
        fun goal(item: InboxItem, w: WriteProposal): String = buildString {
            append("Perform this approved action. Do exactly this and nothing else.\n\n")
            append("Action: ${w.action}\nTarget: ${w.target}\n\nContent:\n${w.content}\n\n")
            append("Approved in Inbox from a result of ${w.worker ?: "the workspace's assistant"}.\nHash: ${w.hash}\n")
            append("muxa-write: ${item.id} ${w.hash}")
        }

        /** 100 numbered lines of 80 characters, 8 099 bytes: near the 8 KiB a write's content holds. */
        fun numberedLines(): String = (1..LINES).joinToString("\n") { "Line %03d: retry budget checked on staging, no change needed".format(it).padEnd(LINE, '.') }

        private const val LINES = 100
        private const val LINE = 80

        private fun make(now: Long, work: String, request: String, items: String, title: String, longContent: String): DemoWrites {
            val post = write(
                "Post to #release", "slack:#release",
                "Release 2.4 is out.\n\n**Faster sync**: retries stay under 30 s.\nNotes: https://example.com/releases/2.4\n- Thanks to everyone who tested it.",
                "The release is tagged and the team reads #release.", "${items}1",
            )
            val comment = write("Comment on ENG-12", "linear:ENG-12", longContent, "The ticket asks whether the retry budget holds.", "${items}2")
            val results = listOf(
                AssistResult(
                    2, now - 900, "The release note is ready, and the ticket has its answer.",
                    next = listOf("Approve the post in the Inbox."), worker = "work-writer", writes = listOf(post),
                ),
                AssistResult(
                    1, now - 5_400, "The retry budget holds on staging.",
                    stale = true, worker = "work-writer", writes = listOf(comment),
                ),
            )
            val q = AssistantRequest(
                request, work, 2, "work", AssistScope.DRAFT, Delivery.RESULTED, now - 900, now - 7_200, now - 900,
                purpose = "Draft the release note and answer ENG-12", results = results,
            )
            val w = WorkItem(work, 3, title, WorkState.OPEN, now - 7_200, now - 7_200, "work", requests = listOf(request))
            return DemoWrites(
                mapOf("work" to SENDER), w, q,
                listOf(item("${items}1", post, q, 2, now - 900), item("${items}2", comment, q, 1, now - 5_400)),
            )
        }

        private fun write(action: String, target: String, content: String, reason: String, item: String) =
            ResultWrite(action, target, content, hash(action, target, content), reason, item)

        private fun item(id: String, w: ResultWrite, q: AssistantRequest, version: Long, at: Long) = InboxItem(
            id, ItemKind.WRITE, ItemState.OPEN, "work-writer proposes to write to ${w.target}", q.workspace, w.action, at,
            summary = w.reason, write = WriteProposal(q.id, version, null, "work-writer", w.action, w.target, w.content, w.hash),
            answers = ANSWERS,
        )
    }
}
