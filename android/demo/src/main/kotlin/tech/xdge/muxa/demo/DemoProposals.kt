package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.ChatPrompt
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.Minutes
import tech.xdge.muxa.protocol.PlanRow
import tech.xdge.muxa.protocol.Basis
import tech.xdge.muxa.protocol.BasisKind
import tech.xdge.muxa.protocol.TaskProposal

/**
 * The demo bridge's proposed Tasks (ADR 0060, `work` level 18): the secretary's `new-task` item, the
 * Tasks the staff's Results propose, and plan items of Tasks that started in plan mode. [stuckPlans]
 * show no "Yes, manually approve edits" on their screen: Approve plan is `conflict` there.
 * Nothing here is real work.
 */
internal class DemoProposals(
    val items: List<InboxItem>,
    /** The Task each staff `result` item proposes, by item id; the item's workspace, step and worker fill it in. */
    val resultTasks: Map<String, TaskProposal>,
    /** Today's row for the `new-task` item. */
    val todayRow: PlanRow,
    val stuckPlans: Set<String>,
) {
    companion object {
        private val NEW_TASK_ANSWERS = listOf(InboxAction.APPROVE, InboxAction.REPLY, InboxAction.DISMISS, InboxAction.LATER)
        private val PLAN_ANSWERS = listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.REPLY, InboxAction.ASK_AGENT, InboxAction.LATER)

        /** The only option the adapter knows on Claude Code's plan menu (PROTOCOL.md, Task proposals item 7). */
        private val PLAN_PROMPT = ChatPrompt(
            "toolu_plan", ChatPrompt.Kind.PLAN,
            options = listOf(ChatPrompt.Option("No, keep planning", listOf("Escape"), cancel = true)),
        )

        /** The UI tests': a proposal in work, two staff Results that propose Tasks, and two plans, one stuck. */
        fun tests(now: Long) = DemoProposals(
            items = listOf(
                newTask(
                    "i-0000f301", now - 400,
                    TaskProposal(
                        "work", "app", "Fix the flaky upload test",
                        "The upload test fails about one run in ten. Find the race in the upload code and fix it, with a test that shows it.",
                    ),
                ),
                plan("i-0000f302", now - 60, "work", "Document the search API", "t-docs01", "$5", "%6"),
                plan("i-0000f303", now - 120, "work", "Pair on the parser", null, "$6", "%7"),
            ),
            resultTasks = mapOf(
                "i-0000f001" to TaskProposal("", "app", "Add the error tracker SDK", "Add the chosen tracker's SDK to the app, report one test error, and document the setup."),
                "i-0000f002" to TaskProposal("", null, "Turn on auto-renew for the domain", "Turn on auto-renew, and put the renewal date in the team calendar."),
            ),
            todayRow = PlanRow(
                item = "i-0000f301", why = "The secretary proposes it, and the test blocks releases.", next = "Start the Task, or tell the secretary what to change.",
                minutes = Minutes(2, 5, 3), basis = listOf(Basis(BasisKind.DEPENDENCY, "Release checks fail on it")),
            ),
            stuckPlans = setOf("i-0000f303"),
        )

        /** Try the Demo's: the secretary proposes a billing fix, the status page Result proposes its setup, and a plan waits. */
        fun showcase(now: Long) = DemoProposals(
            items = listOf(
                newTask(
                    "i-5c00f301", now - 900,
                    TaskProposal(
                        "work", "billing", "Retry failed invoice webhooks",
                        "Stripe retries webhooks for three days, but we drop the ones that fail twice. Keep them in a retry queue and add a test.",
                    ),
                ),
                plan("i-5c00f302", now - 240, "work", "Retry with backoff", "t-d4e5f6", "$2", "%3"),
            ),
            resultTasks = mapOf(
                "i-5c00f001" to TaskProposal("", "dashboard", "Set up the status page", "Add the chosen provider's status page, wire it to the uptime checks, and link it from the dashboard footer."),
                "i-5c00f002" to TaskProposal("", "billing", "Move invoices to Stripe Billing", "Move the invoice job to Stripe Billing, keep the old job behind a flag, and compare one month of totals."),
            ),
            todayRow = PlanRow(
                item = "i-5c00f301", why = "Failed webhooks lose paid invoices.", next = "Start the Task, or tell the secretary what to change.",
                minutes = Minutes(2, 5, 3), basis = listOf(Basis(BasisKind.DEPENDENCY, "Invoices go missing", "t-a1b2c3")),
            ),
            stuckPlans = emptySet(),
        )

        /** [item], a staff `result` item, offering [task]: Start Task is `approve` alone, and `start-task` beside a hand-off. */
        fun withTask(item: InboxItem, task: TaskProposal): InboxItem {
            val r = item.result ?: return item
            val proposal = task.copy(workspace = item.workspace, step = r.step, worker = r.worker)
            val rest = item.answers.filter { it != InboxAction.APPROVE }
            return if (r.handoff != null) {
                item.copy(result = r.copy(task = proposal), answers = listOf(InboxAction.APPROVE, InboxAction.START_TASK) + rest)
            } else {
                val who = r.worker ?: "The assistant"
                item.copy(question = "$who proposes a Task: ${task.title}", result = r.copy(task = proposal), answers = listOf(InboxAction.APPROVE) + rest)
            }
        }

        /** The goal the started Task gets: the proposal's, then who proposed it and "Plan first" (ADR 0060 §2). */
        fun goal(item: InboxItem, task: TaskProposal): String {
            val from = if (item.kind == ItemKind.NEW_TASK) "the secretary" else task.worker ?: "the assistant"
            return "${task.goal}\n\nProposed by $from in Inbox item ${item.id}. Plan first, and wait for the plan's approval."
        }

        /** The item answered with [action], naming the Task it started. */
        fun started(item: InboxItem, action: InboxAction, task: String, title: String, now: Long): InboxItem = item.copy(
            state = ItemState.ANSWERED, answer = action, answeredAt = now, laterAt = null, answers = emptyList(), task = task,
            summary = "Started Task $task: $title" + item.summary?.let { "\n\n$it" }.orEmpty(),
        )

        /** `propose-task`'s item: the host's question, the goal as its summary. */
        fun newTask(id: String, at: Long, task: TaskProposal) = InboxItem(
            id, ItemKind.NEW_TASK, ItemState.OPEN, "The secretary proposes a Task in ${task.workspace}: ${task.title}", task.workspace, task.title, at,
            summary = task.goal, newTask = task, answers = NEW_TASK_ANSWERS,
        )

        private fun plan(id: String, at: Long, workspace: String, title: String, task: String?, session: String, pane: String) = InboxItem(
            id, ItemKind.ASK, ItemState.OPEN, "Approve the plan?", workspace, title, at,
            task = task, session = session, pane = pane, prompt = PLAN_PROMPT,
            summary = "1. Read the current code.\n2. Change it in small steps, each with a test.\n3. Run the checks.",
            answers = PLAN_ANSWERS,
        )
    }
}
