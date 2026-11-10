package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.AssistResult
import tech.xdge.muxa.protocol.AssistScope
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.Basis
import tech.xdge.muxa.protocol.BasisKind
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.Finding
import tech.xdge.muxa.protocol.Handoff
import tech.xdge.muxa.protocol.HandoffProposal
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.Minutes
import tech.xdge.muxa.protocol.Notes
import tech.xdge.muxa.protocol.PlanRow
import tech.xdge.muxa.protocol.RequestStep
import tech.xdge.muxa.protocol.ResultSource
import tech.xdge.muxa.protocol.StaffMember
import tech.xdge.muxa.protocol.StepBy
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkState

/**
 * The demo bridge's staff and hand-offs (ADR 0051, `work` level 12): each workspace's staff, a Work item
 * whose request ran two steps and waits on a third, the `handoff` item that asks for it, the members the
 * overall assistant's `delegate` items name, and a Today row for one of them. Nothing here is a real finding.
 */
internal class DemoStaff(
    val members: Map<String, List<StaffMember>>,
    val work: WorkItem,
    val request: AssistantRequest,
    val handoff: InboxItem,
    /** The member each `delegate` item names, by item id. */
    val named: Map<String, String>,
    /** Today's last row at this level: a `delegate` item that names a member. */
    val todayRow: PlanRow,
) {
    companion object {
        private val WORK_STAFF = listOf(
            StaffMember("work-research", "Reads docs, issues and vendor pages, and reports what it found with sources.", lead = true),
            StaffMember("work-writer", "Turns findings into a draft: a table, a plan or a message to the team."),
            StaffMember("work-review", "Checks a draft against the code and the docs, and lists what is wrong."),
        )

        private val ANSWERS = listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.LATER)

        /** The UI tests': three members in work, one in personal; w-0000b005's chain waits on work-review. */
        fun tests(now: Long) = chain(
            now,
            members = mapOf(
                "work" to WORK_STAFF,
                "personal" to listOf(StaffMember("home-planner", "Plans trips, events and errands.", lead = true, notesAt = now - 86_400)),
            ),
            work = "w-0000b005", request = "q-0000a002", item = "i-0000f001", title = "Choose an error tracker",
            purpose = "Which error tracker fits a team of five",
            named = mapOf("i-0000e002" to "home-planner"),
            row = PlanRow(
                item = "i-0000e002", why = "Nobody asked about the offsite yet.", next = "Approve the request.",
                minutes = Minutes(1, 5, 2), basis = listOf(Basis(BasisKind.GOAL, "Team offsite in May")),
            ),
        )

        /** Try the Demo's: the work team on a status page provider. */
        fun showcase(now: Long) = chain(
            now,
            members = mapOf("work" to WORK_STAFF),
            work = "w-5c00b004", request = "q-5c00a002", item = "i-5c00f001", title = "Pick a status page provider",
            purpose = "Which status page fits our uptime checks",
            named = mapOf("i-5c00e002" to "work-research"),
            row = PlanRow(
                item = "i-5c00e002", why = "The Task needs a choice before it goes on.", next = "Approve the request to work-research.",
                minutes = Minutes(1, 5, 2), basis = listOf(Basis(BasisKind.DEPENDENCY, "The Task waits on a choice", "t-a1b2c3")),
            ),
        )

        /** A request in work whose step 1 (the lead) handed on to step 2, which asks for work-review. */
        private fun chain(
            now: Long,
            members: Map<String, List<StaffMember>>,
            work: String,
            request: String,
            item: String,
            title: String,
            purpose: String,
            named: Map<String, String>,
            row: PlanRow,
        ): DemoStaff {
            val toWriter = Handoff("work-writer", "Draft a comparison table of the two that fit.")
            val toReview = Handoff("work-review", "Check the table against what the team needs.")
            val steps = listOf(
                RequestStep(1, "work-research", StepBy.LEAD, Delivery.RESULTED, now - 7_200, "t-chain01", toWriter),
                RequestStep(2, "work-writer", StepBy.HANDOFF, Delivery.RESULTED, now - 1_800, "t-chain02", toReview),
            )
            val results = listOf(
                AssistResult(
                    1, now - 1_800, "The table compares price, alerts and the SDKs we use. The first option wins on alerts.",
                    files = listOf("/src/assistant/research-$request-v1-s2-t-chain02/comparison.md"),
                    next = listOf("Check the table with the team."),
                    notes = Notes("# work-writer\n\n- Comparison tables go in docs/decisions/.\n", DemoResults.EMPTY_NOTES),
                    worker = "work-writer", step = 2,
                ),
                AssistResult(
                    1, now - 7_200, "Two options fit a team of five. Both have the SDKs we use.",
                    findings = listOf(
                        Finding("Both send alerts to chat.", listOf(ResultSource(url = "https://example.com/alerts", title = "Alerts", readAt = now - 7_300))),
                    ),
                    openQuestions = listOf("Do we need on-call schedules?"),
                    worker = "work-research", step = 1,
                ),
            )
            val q = AssistantRequest(
                request, work, 1, "work", AssistScope.RESEARCH, Delivery.RESULTED, now - 1_800, now - 10_800, now - 1_800,
                purpose = purpose, run = "t-chain02", results = results, steps = steps,
            )
            val w = WorkItem(work, 2, title, WorkState.OPEN, now - 10_800, now - 10_800, "work", requests = listOf(request))
            val handoff = InboxItem(
                item, ItemKind.HANDOFF, ItemState.OPEN, "work-writer asks work-review to: ${toReview.purpose}", "work", title, now - 1_800,
                handoff = HandoffProposal(request, 1, 2, work, "work-writer", "work-review", toReview.purpose),
                answers = ANSWERS,
            )
            return DemoStaff(members, w, q, handoff, named, row)
        }
    }
}
