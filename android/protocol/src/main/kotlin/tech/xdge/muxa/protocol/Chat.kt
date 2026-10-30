package tech.xdge.muxa.protocol

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** One item of an agent conversation, the same for every agent. */
@Serializable
data class ChatEntry(
    /** Stable within a chat; a later entry with the same id replaces this one. */
    val id: String,
    val kind: Kind,
    /** The prompt, the reply (markdown), or the notice. Empty for a tool call. */
    val text: String = "",
    val tool: ChatTool? = null,
) {
    @Serializable
    enum class Kind {
        @SerialName("user") USER,
        @SerialName("assistant") ASSISTANT,
        @SerialName("tool") TOOL,
        @SerialName("notice") NOTICE,
    }
}

@Serializable
data class ChatTool(
    val name: String,
    /** One line about the call, e.g. the command or the file. */
    val summary: String,
    /** The start of the result, once it arrived. */
    val result: String? = null,
    val error: Boolean = false,
)

/** A choice the agent waits on. Each option carries the keys that pick it. */
@Serializable
data class ChatPrompt(
    /** The id of the tool entry the agent waits on. */
    val entry: String,
    val kind: Kind,
    /** For a question: the question as the agent asks it. */
    val question: String? = null,
    val options: List<Option>,
) {
    /** Unknown kinds stay as their name, so a newer bridge does not break the message. */
    @Serializable
    @JvmInline
    value class Kind(val raw: String) {
        companion object {
            val PERMISSION = Kind("permission")
            val QUESTION = Kind("question")
            val PLAN = Kind("plan")
        }
    }

    @Serializable
    data class Option(
        val label: String,
        /** tmux key names, as in a `keys` request. */
        val keys: List<String>,
        /** Declines or cancels, e.g. a deny. */
        val cancel: Boolean = false,
    )
}
