package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.Basis
import tech.xdge.muxa.protocol.BasisKind
import tech.xdge.muxa.protocol.Confidence
import tech.xdge.muxa.protocol.Correction
import tech.xdge.muxa.protocol.CorrectionKind
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.Hypothesis
import tech.xdge.muxa.protocol.HypothesisProposal
import tech.xdge.muxa.protocol.HypothesisState
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.Policy
import tech.xdge.muxa.protocol.PolicyAction
import tech.xdge.muxa.protocol.Request

/**
 * The demo bridge's policies, corrections and hypotheses (ADR 0050, `work` level 11), with the host's
 * checks: text bounds, scope, the limit of 50, `rev`, and Forget's cascade. [cites] are the basis entries
 * the fixtures' plan rows gain at this level, by row. Not thread safe: the bridge holds its lock.
 */
internal class DemoPolicies(
    private var policies: List<Policy>,
    private var hypotheses: List<Hypothesis>,
    private var corrections: List<Correction>,
    val cites: Map<String, List<Basis>>,
) {
    private var made = 0

    /** Active policies, then expired ones. */
    fun listedPolicies(now: Long): List<Policy> = policies.partition { (it.until ?: Long.MAX_VALUE) > now }.let { (a, e) -> a + e }

    /** `proposed` and `rejected`, newest first. */
    val listedHypotheses: List<Hypothesis>
        get() = hypotheses.filter { it.state == HypothesisState.PROPOSED || it.state == HypothesisState.REJECTED }.sortedByDescending { it.at }

    /** Newest first, at most 50. */
    val listedCorrections: List<Correction> get() = corrections.sortedByDescending { it.at }.take(LISTED_CORRECTIONS)

    /** One `hypothesis` item per proposed hypothesis, keyed by it, from the overall [workspace]. */
    fun items(workspace: String): List<InboxItem> = hypotheses.filter { it.state == HypothesisState.PROPOSED }.map { h ->
        InboxItem(
            itemId(h.id), ItemKind.HYPOTHESIS, ItemState.OPEN, "Keep as a policy? ${h.text}", workspace, "What the assistant noticed", h.at,
            hypothesis = HypothesisProposal(h.id, h.text, h.scope, h.evidence, h.confidence),
            answers = listOf(InboxAction.CONFIRM, InboxAction.REJECT, InboxAction.LATER),
        )
    }

    /** `add-policy`. [workspaces] are the ones a scope may name: not the overall assistant's. */
    fun add(req: Request.AddPolicy, now: Long, workspaces: Set<String>): Policy {
        check(req.text, req.scope, req.from, req.until, now, workspaces)
        if (active(now).size >= MAX_ACTIVE) throw DemoWork.Refusal(ErrorCode.CONFLICT, "$MAX_ACTIVE active policies: delete one first")
        return save(Policy(nextId("y-"), 1, req.text, req.scope, req.from, req.until, USER, now, now))
    }

    /** `update-policy`: null for a delete. */
    fun update(req: Request.UpdatePolicy, now: Long, workspaces: Set<String>): Policy? {
        val p = policies.firstOrNull { it.id == req.policy } ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "no policy ${req.policy}")
        if (p.rev != req.rev) throw DemoWork.Refusal(ErrorCode.CONFLICT, "policy ${p.id} is at rev ${p.rev}; list it again")
        if (req.action == PolicyAction.DELETE) {
            policies = policies - p
            return null
        }
        if (req.text == null && req.scope == null && req.from == null && req.until == null) {
            throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "update-policy edit needs a field")
        }
        val edited = p.copy(rev = p.rev + 1, text = req.text ?: p.text, scope = req.scope ?: p.scope, from = req.from ?: p.from, until = req.until ?: p.until, updatedAt = now)
        check(edited.text, edited.scope, edited.from, edited.until, now, workspaces)
        return save(edited)
    }

    /** `confirm` on a `hypothesis` item: a policy of its text; a second confirm returns the first one's. */
    fun confirm(hypothesis: String, now: Long): Policy {
        policies.firstOrNull { it.source == hypothesis }?.let { return it }
        val h = hypotheses.firstOrNull { it.id == hypothesis } ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "no hypothesis $hypothesis")
        if (active(now).size >= MAX_ACTIVE) throw DemoWork.Refusal(ErrorCode.CONFLICT, "$MAX_ACTIVE active policies: delete one first")
        hypotheses = hypotheses.map { if (it.id == h.id) it.copy(state = HypothesisState.CONFIRMED) else it }
        return save(Policy(nextId("y-"), 1, h.text, h.scope, source = h.id, createdAt = now, updatedAt = now))
    }

    /** `reject` on a `hypothesis` item: it stays in the brief as rejected for 90 days. */
    fun reject(hypothesis: String, now: Long) {
        hypotheses = hypotheses.map { if (it.id == hypothesis) it.copy(state = HypothesisState.REJECTED, expiresAt = now + REJECTED_DAYS * DAY) else it }
    }

    /** Records what the user did while [plan] was current. [reason] only when the user gave one. */
    fun record(kind: CorrectionKind, plan: String, row: String?, inTop: Boolean, workspace: String?, now: Long, reason: String? = null,
               before: List<String> = emptyList(), after: List<String> = emptyList()) {
        reason?.let(::checkReason)
        val c = Correction(nextId("x-"), kind, now, plan, row, inTop, workspace, before, after, reason?.takeIf { it.isNotEmpty() })
        corrections = (listOf(c) + corrections).take(MAX_CORRECTIONS)
    }

    /**
     * `forget`: a correction with the `proposed` hypotheses it grounds, or a hypothesis that is not
     * confirmed. Returns the hypotheses removed, whose items go too.
     */
    fun forget(record: String): List<String> {
        if (corrections.any { it.id == record }) {
            val grounded = hypotheses.filter { it.state == HypothesisState.PROPOSED && record in it.evidence }.map { it.id }
            hypotheses = hypotheses.filterNot { it.id in grounded }
            corrections = corrections.filterNot { it.id == record }
            return grounded
        }
        val h = hypotheses.firstOrNull { it.id == record } ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "no correction or hypothesis $record")
        if (h.state == HypothesisState.CONFIRMED) {
            val policy = policies.firstOrNull { it.source == h.id }?.id ?: "its policy"
            throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "${h.id} is confirmed as $policy; delete the policy with update-policy")
        }
        hypotheses = hypotheses - h
        return listOf(h.id)
    }

    private fun active(now: Long) = policies.filter { (it.until ?: Long.MAX_VALUE) > now }

    private fun save(p: Policy): Policy {
        policies = if (policies.any { it.id == p.id }) policies.map { if (it.id == p.id) p else it } else policies + p
        return p
    }

    private fun nextId(prefix: String) = prefix + "%08x".format(0xf0000000L + ++made)

    private fun check(text: String, scope: String, from: Long?, until: Long?, now: Long, workspaces: Set<String>) {
        val bytes = text.toByteArray().size
        if (bytes !in 1..MAX_TEXT || text.any { it.isISOControl() }) {
            throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "text must be 1 to $MAX_TEXT bytes, one paragraph")
        }
        if (scope != "all" && scope !in workspaces) throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "scope must be all or a workspace: $scope")
        if (until != null && (until <= now || (from != null && until <= from))) {
            throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "until must be in the future and after from")
        }
    }

    /** A reason has at most 500 bytes, with line breaks and tabs the only control characters. */
    fun checkReason(reason: String) {
        if (reason.toByteArray().size > MAX_TEXT || reason.any { it.isISOControl() && it != '\n' && it != '\t' }) {
            throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "a reason has at most $MAX_TEXT bytes")
        }
    }

    companion object {
        private const val MAX_TEXT = 500
        private const val MAX_ACTIVE = 50
        private const val MAX_CORRECTIONS = 200
        private const val LISTED_CORRECTIONS = 50
        private const val REJECTED_DAYS = 90
        private const val PROPOSED_DAYS = 14
        private const val DAY = 86_400L
        private const val USER = "user"

        /** A hypothesis's Inbox item, keyed by it as the host keys `hypothesis:<id>`. */
        fun itemId(hypothesis: String) = "i-" + hypothesis.removePrefix("h-")

        private fun hypothesis(id: String, text: String, scope: String, evidence: List<String>, confidence: Confidence, plan: String, at: Long, state: HypothesisState = HypothesisState.PROPOSED) =
            Hypothesis(id, text, scope, evidence, confidence, state, plan, at + (if (state == HypothesisState.REJECTED) REJECTED_DAYS else PROPOSED_DAYS) * DAY, at)

        /**
         * The UI tests': one policy for this week, a proposed hypothesis grounded on two corrections, one
         * rejected, and three corrections. The plan's first row cites the policy, the hypothesis, a
         * correction and a forgotten one.
         */
        fun tests(now: Long) = DemoPolicies(
            listOf(Policy("y-0000f001", 1, "This week, the offsite comes before sales calls.", "all", until = now + 3 * DAY, source = USER, createdAt = now - DAY, updatedAt = now - DAY)),
            listOf(
                hypothesis("h-0000f101", "Reviews come before new work in the morning.", "all", listOf("x-0000f201", "x-0000f202"), Confidence.MEDIUM, "p-0000d001", now - 3_500),
                hypothesis("h-0000f102", "Domain renewals can wait.", "personal", listOf("x-0000f203"), Confidence.LOW, "p-0000d000", now - 2 * DAY, HypothesisState.REJECTED),
            ),
            listOf(
                Correction("x-0000f201", CorrectionKind.PINNED, now - 4_000, "p-0000d000", "w-0000b001", true, "personal"),
                Correction("x-0000f202", CorrectionKind.DISMISSED, now - 5_000, "p-0000d000", "w-0000b003", true, "personal", reason = "waiting on the registrar"),
                Correction("x-0000f203", CorrectionKind.DEFERRED, now - 6_000, "p-0000d000", "w-0000b003", true, "personal"),
            ),
            mapOf(
                "w-0000b001" to listOf(
                    Basis(BasisKind.POLICY, "The offsite comes first this week", "y-0000f001"),
                    Basis(BasisKind.HYPOTHESIS, "Reviews come before new work", "h-0000f101"),
                    Basis(BasisKind.CORRECTION, "You pinned it", "x-0000f201"),
                    Basis(BasisKind.CORRECTION, "You moved a row up", "x-0000f2ff"),
                ),
            ),
        )

        /** Try the Demo's: a policy, a hypothesis to confirm, and the corrections behind it. */
        fun showcase(now: Long) = DemoPolicies(
            listOf(
                Policy("y-5c00f001", 1, "This week, the payment migration comes before everything else.", "work", until = now + 4 * DAY, source = USER, createdAt = now - DAY, updatedAt = now - DAY),
                Policy("y-5c00f002", 1, "Keep mornings free of meetings.", "all", source = USER, createdAt = now - 10 * DAY, updatedAt = now - 10 * DAY),
            ),
            listOf(
                hypothesis("h-5c00f101", "Agent questions come before reading new Results.", "all", listOf("x-5c00f201", "x-5c00f202"), Confidence.MEDIUM, "p-5c00d001", now - 5_300),
            ),
            listOf(
                Correction("x-5c00f201", CorrectionKind.REORDERED, now - 7_000, "p-5c00d000", null, true, null, listOf("w-5c00b002", "i-5c000001"), listOf("i-5c000001", "w-5c00b002")),
                Correction("x-5c00f202", CorrectionKind.PINNED, now - 8_000, "p-5c00d000", "w-5c00b001", true, "work"),
            ),
            mapOf(
                "w-5c00b001" to listOf(Basis(BasisKind.POLICY, "The migration comes first this week", "y-5c00f001")),
                "i-5c000001" to listOf(Basis(BasisKind.HYPOTHESIS, "Agent questions come first", "h-5c00f101")),
            ),
        )
    }
}
