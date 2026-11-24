package tech.xdge.muxa.v2

/**
 * One Bot's work, grouped by the rules of PROTOCOL-v2.md, "A Bot's work" (ADR 0086), from what a
 * client already has: the Inbox, the live terminals and the Routines. Swift's `BotWork` follows the
 * same rules and the same fixture.
 */
data class BotWork(
    /** Its open items, Later ones only once they are back. */
    val items: List<InboxItem>,
    /** Its live terminals: talks and runs (`muxa.bot`), and its members' runs (`muxa.lead`). */
    val terminals: List<Terminal>,
    /** Its items, then its waiting terminals. */
    val needsYou: List<Entry>,
    val inProgress: List<Terminal>,
    /** Enabled Routines with a next run, soonest first, then those on review. */
    val planned: List<Routine>,
    /** Its done items, newest first. */
    val recent: List<InboxItem>,
    val status: Status,
) {
    enum class Status(val wire: String) { NEEDS_YOU("needs-you"), RUNNING("running"), PAUSED("paused"), IDLE("idle") }

    /** A row of Needs you. */
    sealed interface Entry {
        val id: String

        data class Item(val item: InboxItem) : Entry {
            override val id: String get() = item.id
        }

        data class Waiting(val terminal: Terminal) : Entry {
            override val id: String get() = terminal.id
        }
    }

    companion object {
        /** [enabled] is the Bot's own switch. */
        fun of(
            bot: String,
            workspace: String,
            enabled: Boolean,
            inbox: Inbox,
            terminals: List<Terminal>,
            routines: List<Routine>,
        ): BotWork {
            fun mine(i: InboxItem) = i.bot == bot && i.workspace == workspace
            val items = inbox.items.filter(::mine)
            val own = terminals.filter { it.live && it.labels[WORKSPACE] == workspace && (it.bot == bot || it.lead == bot) }
            val waiting = own.filter { it.status?.state in WAITING }
            val routinesOn = routines.filter { it.bot == bot && it.workspace == workspace && it.enabled }
            val planned = routinesOn.filter { it.nextDue != null }.sortedBy { it.nextDue } +
                routinesOn.filter { it.nextDue == null && it.on == REVIEW }
            val needsYou = items.map(Entry::Item) + waiting.map(Entry::Waiting)
            val status = when {
                needsYou.isNotEmpty() -> Status.NEEDS_YOU
                own.isNotEmpty() -> Status.RUNNING
                !enabled -> Status.PAUSED
                else -> Status.IDLE
            }
            return BotWork(items, own, needsYou, own - waiting.toSet(), planned, inbox.done.filter(::mine), status)
        }

        /** The states in which an agent waits on the user. */
        val WAITING = setOf("blocked", "waiting")
        private const val WORKSPACE = "muxa.workspace"
        private const val REVIEW = "review"
    }
}
