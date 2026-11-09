package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.AssistResult
import tech.xdge.muxa.protocol.AssistScope
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.Finding
import tech.xdge.muxa.protocol.Notes
import tech.xdge.muxa.protocol.PossibleDuplicate
import tech.xdge.muxa.protocol.ResultSource

/**
 * How a demo assistant run moves on, checked on each `inbox-list`: it waits [pendingMillis] for
 * its prompt, then works [processingMillis], then has its Result.
 */
data class AssistPace(val pendingMillis: Long, val processingMillis: Long) {
    companion object {
        /** Long enough to see each state in the app. */
        val DEMO = AssistPace(1_500, 3_000)

        /** The next list has the Result: for the JVM tests. */
        val AT_ONCE = AssistPace(0, 0)

        /** Never moves on: for tests that cancel a run. */
        val NEVER = AssistPace(Long.MAX_VALUE / 4, Long.MAX_VALUE / 4)
    }
}

/** The demo's assistant Results and the requests the fixtures start with. Nothing here is a real finding. */
internal object DemoResults {
    /** What every demo run returns: findings with sources, an open question, next actions, a possible duplicate. */
    fun canned(q: AssistantRequest, at: Long, duplicateOf: String?) = AssistResult(
        version = q.version,
        at = at,
        summary = "Two options fit. The first is quicker to start; the second costs less per month.",
        findings = listOf(
            Finding(
                "The first option needs no new account.",
                listOf(ResultSource(url = "https://example.com/guide", title = "Getting started", readAt = at - 60), ResultSource(path = "/src/docs/notes.md")),
            ),
            Finding("The second option costs less at our volume.", listOf(ResultSource(url = "https://example.com/pricing", title = "Pricing", readAt = at - 30))),
        ),
        openQuestions = listOf("Which team owns the account?"),
        next = listOf("Ask for a quote.", "Start a Task to try the first option."),
        files = listOf("/src/assistant/${q.scope.raw}-${q.id}-v${q.version}-${q.run ?: "t-demo"}/notes.md"),
        possibleDuplicates = listOfNotNull(duplicateOf?.let { PossibleDuplicate(it, "It names the same vendor.") }),
    )

    /** The SHA-256 of no bytes: the base of notes proposed when the workspace had none (ADR 0047 §1). */
    const val EMPTY_NOTES = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

    const val TESTS_NOTES = "# Personal\n\n- Domains renew at the current registrar; auto-renew is the default.\n"

    const val SHOWCASE_NOTES = "# Work\n\n- Invoices move to Stripe Billing before March.\n- Webhooks live in src/app/payments/webhooks.ts.\n"

    /** The UI tests': w-0000b003 asked twice; v2 has its Result, v1's is stale. */
    fun tests(now: Long) = listOf(
        AssistantRequest(
            "q-0000a001", "w-0000b003", 2, "personal", AssistScope.COMPARE, Delivery.RESULTED, now - 900, now - 7_200, now - 900,
            purpose = "Which registrar to renew with",
            results = listOf(
                AssistResult(
                    2, now - 900, "Stay with the current registrar for one more year; moving costs more than it saves.",
                    findings = listOf(
                        Finding(
                            "The current registrar raised its price by 2 USD a year.",
                            listOf(ResultSource(url = "https://example.com/registrar/pricing", title = "Registrar pricing", readAt = now - 1_000)),
                        ),
                        Finding("A transfer adds a year but needs the auth code.", listOf(ResultSource(path = "/home/dev/notes/domains.md"))),
                    ),
                    openQuestions = listOf("Is auto-renew on?"),
                    next = listOf("Turn on auto-renew."),
                    files = listOf("/home/dev/assistant/compare-q-0000a001-v2-t-run002/registrars.md"),
                    possibleDuplicates = listOf(PossibleDuplicate("w-0000b001", "Both need the company card.")),
                    notes = Notes(TESTS_NOTES, EMPTY_NOTES),
                ),
                AssistResult(1, now - 5_400, "Three registrars fit. Prices differ by less than 5 USD a year.", stale = true),
            ),
        ),
    )

    /** The showcase's: one Work item with its Result. */
    fun showcase(now: Long) = listOf(
        AssistantRequest(
            "q-5c00a001", "w-5c00b001", 1, "work", AssistScope.RESEARCH, Delivery.RESULTED, now - 1_200, now - 4_800, now - 1_200,
            purpose = "What moving invoices to Stripe Billing takes",
            results = listOf(
                AssistResult(
                    1, now - 1_200,
                    "Stripe Billing can send our invoices. It needs the customer list imported and two webhooks changed.",
                    findings = listOf(
                        Finding(
                            "Invoices can be imported with their numbers kept.",
                            listOf(ResultSource(url = "https://docs.stripe.com/invoicing", title = "Invoicing", readAt = now - 1_500)),
                        ),
                        Finding(
                            "Our webhook handler reads invoice.paid only from Checkout.",
                            listOf(ResultSource(path = "/src/app/payments/webhooks.ts")),
                        ),
                    ),
                    openQuestions = listOf("Do customers keep their payment method?"),
                    next = listOf("Start a Task to change the webhooks.", "Export the customer list."),
                    files = listOf("/src/assistant/research-q-5c00a001-v1-t-run001/billing.md"),
                    possibleDuplicates = listOf(PossibleDuplicate("w-5c00b002", "Both change the payment webhooks.")),
                    notes = Notes(SHOWCASE_NOTES, EMPTY_NOTES),
                ),
            ),
        ),
    )
}
