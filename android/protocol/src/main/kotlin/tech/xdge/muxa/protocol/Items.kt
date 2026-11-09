package tech.xdge.muxa.protocol

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.nullable
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder

/** Level 3: Inbox items (`items` in `inbox-list`, the answers of ADR 0033). */
const val WORK_ITEMS = 3

/** One stored question for the user (ADR 0033), from `inbox-list` or `inbox-saved`. */
@Serializable
data class InboxItem(
    /** `i-` and 8 hex digits for a Task's item; a Routine's entry keeps its `e-` id. */
    val id: String,
    val kind: ItemKind,
    val state: ItemState,
    /** One line: what the user must decide. */
    val question: String,
    val workspace: String,
    /** The Task's title, or the entry's title for a Routine's item. */
    val title: String,
    /** Epoch seconds. */
    val createdAt: Long,
    /** Set by the `read` answer. Null: unread. */
    val readAt: Long? = null,
    /** Set by `later`: the host lists the item below the others (ADR 0040). */
    val laterAt: Long? = null,
    /** Only from a host before ADR 0040, with `later`: when the item is open again. */
    val until: Long? = null,
    /** Set by `protect`: `delete-inbox` keeps the item (ADR 0046). */
    val protectedAt: Long? = null,
    val answeredAt: Long? = null,
    /** With `answered`: the answer given. Null also when a newer host sent an action this client does not know. */
    @Serializable(with = KnownActionSerializer::class)
    val answer: InboxAction? = null,
    /** The agent's last message for the turn, or a Routine entry's body. */
    val summary: String? = null,
    val task: String? = null,
    /** `$n` of the Task's session when the item was made. */
    val session: String? = null,
    /** `%n` of the agent pane. */
    val pane: String? = null,
    /** With kind `ask`: the menu the agent waits on. */
    val prompt: ChatPrompt? = null,
    val attachments: Attachments = Attachments(),
    val routine: String? = null,
    val proposal: Proposal? = null,
    /** The answers the host accepts now, in the order to show them. Unknown actions are left out: PROTOCOL.md says to show them as nothing. */
    @Serializable(with = KnownActionsSerializer::class)
    val answers: List<InboxAction> = emptyList(),
)

/** What the item asks about. Unknown kinds stay as their name, so a newer bridge does not break the list. */
@Serializable
@JvmInline
value class ItemKind(val raw: String) {
    companion object {
        /** The agent waits on a permission, a question or a plan. */
        val ASK = ItemKind("ask")

        /** The agent finished a turn, or its loop passed. */
        val REVIEW = ItemKind("review")

        /** The agent stopped with an error, or its loop stopped. */
        val FAILURE = ItemKind("failure")
        val PROPOSAL = ItemKind("proposal")
        val BRIEFING = ItemKind("briefing")
    }
}

/** Unknown states stay as their name. */
@Serializable
@JvmInline
value class ItemState(val raw: String) {
    companion object {
        val OPEN = ItemState("open")
        val LATER = ItemState("later")
        val ANSWERED = ItemState("answered")

        /** The need ended without an answer from the Inbox, e.g. the user answered in the terminal. */
        val RESOLVED = ItemState("resolved")
    }
}

/** An item's evidence. The client opens each through the pane's `changes`, `docs` and `images` requests, or as a link. */
@Serializable
data class Attachments(
    /** The pane's changes when the item was made. */
    val changes: ChangeCount? = null,
    /** Markdown paths among those changes, as `doc` takes them. */
    val docs: List<String> = emptyList(),
    /** Image paths among those changes, as `image` takes them. */
    val images: List<String> = emptyList(),
    /** The PR, or a Routine entry's links. */
    val links: List<SourceLink> = emptyList(),
)

/** [scope] is `branch` when the counts are the branch's own commits, as in `diff {scope}`. */
@Serializable
data class ChangeCount(val files: Int, val added: Int, val removed: Int, val scope: String? = null)

private fun knownAction(raw: String): InboxAction? {
    val names = InboxAction.serializer().descriptor
    return InboxAction.entries.firstOrNull { names.getElementName(it.ordinal) == raw }
}

/** Drops the actions this client does not know. */
internal object KnownActionsSerializer : KSerializer<List<InboxAction>> {
    private val names = ListSerializer(String.serializer())
    private val actions = ListSerializer(InboxAction.serializer())
    override val descriptor: SerialDescriptor = names.descriptor

    override fun deserialize(decoder: Decoder): List<InboxAction> = names.deserialize(decoder).mapNotNull(::knownAction)

    override fun serialize(encoder: Encoder, value: List<InboxAction>) = actions.serialize(encoder, value)
}

/** An action this client does not know reads as null. */
internal object KnownActionSerializer : KSerializer<InboxAction?> {
    private val name = String.serializer().nullable
    private val action = InboxAction.serializer().nullable
    override val descriptor: SerialDescriptor = name.descriptor

    override fun deserialize(decoder: Decoder): InboxAction? = name.deserialize(decoder)?.let(::knownAction)

    override fun serialize(encoder: Encoder, value: InboxAction?) = action.serialize(encoder, value)
}
