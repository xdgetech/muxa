package tech.xdge.muxa.app.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import tech.xdge.muxa.protocol.Attachments
import tech.xdge.muxa.protocol.SourceLink

/** ADR 0033 §4a: the summary's links open the browser, or the Reader at a named attachment. */
class SummaryLinksTest {
    private val attachments = Attachments(
        docs = listOf("docs/plan.md", "report.md"),
        images = listOf("build/screenshots/en/login.png"),
        links = listOf(SourceLink("https://github.com/example/app/pull/817", "PR #817")),
    )

    private fun linkify(text: String) = SummaryLinks.linkify(text, attachments)

    @Test
    fun webLinksOpenTheBrowser() {
        assertEquals(SummaryTarget.Web("https://example.com/runs/42"), SummaryLinks.target("https://example.com/runs/42", attachments))
        assertEquals(SummaryTarget.Web("http://localhost:4242/"), SummaryLinks.target("http://localhost:4242/", attachments))
        assertNull(SummaryLinks.target("mailto:dev@example.com", attachments))
        assertNull(SummaryLinks.target("javascript:alert(1)", attachments))
    }

    @Test
    fun aFileNameOpensTheAttachmentItNames() {
        assertEquals(SummaryTarget.Doc("docs/plan.md"), SummaryLinks.target("docs/plan.md", attachments))
        assertEquals(SummaryTarget.Doc("docs/plan.md"), SummaryLinks.target("./docs/plan.md#steps", attachments))
        assertEquals(SummaryTarget.Doc("docs/plan.md"), SummaryLinks.target("plan.md", attachments))
        assertEquals(SummaryTarget.Doc("docs/plan.md"), SummaryLinks.target("/home/dev/app/docs/plan.md", attachments))
        assertEquals(SummaryTarget.Doc("docs/plan.md"), SummaryLinks.target("file:///home/dev/app/docs/plan.md", attachments))
        assertEquals(SummaryTarget.Image("build/screenshots/en/login.png"), SummaryLinks.target("login.png", attachments))
        // Not attached: nothing opens.
        assertNull(SummaryLinks.target("docs/other.md", attachments))
        assertNull(SummaryLinks.target("an.md", attachments))
        assertNull(SummaryLinks.target("docs/", attachments))
    }

    @Test
    fun bareLinksAndNamedFilesBecomeLinks() {
        assertEquals(
            "The plan is in [docs/plan.md](docs/plan.md), see <https://example.com/runs/42>.",
            linkify("The plan is in docs/plan.md, see https://example.com/runs/42."),
        )
        assertEquals("(**[login.png](login.png)**)", linkify("(**login.png**)"))
        assertEquals("Read [`report.md`](report.md) first.", linkify("Read `report.md` first."))
    }

    @Test
    fun linksCodeAndOtherWordsStayAsWritten() {
        val kept = listOf(
            "See [the plan](docs/plan.md) and [PR #817](https://github.com/example/app/pull/817).",
            "An ![image](login.png) and <https://example.com>.",
            "Run `npm test` in src/legacy.js and e.g. v1.2.",
            "Emphasis: *done* and **tests pass**.",
        )
        for (line in kept) assertEquals(line, linkify(line))
    }

    @Test
    fun codeBlocksAreNotLinked() {
        val text = "Changed:\n```\ncat docs/plan.md\nhttps://example.com\n```\n    report.md\nDone: report.md"
        assertEquals(
            "Changed:\n```\ncat docs/plan.md\nhttps://example.com\n```\n    report.md\nDone: [report.md](report.md)",
            linkify(text),
        )
    }

    @Test
    fun aNameWithSpacesStaysOneLink() {
        val spaced = Attachments(docs = listOf("notes/My Plan (v2).md"))
        val linked = SummaryLinks.linkify("See `notes/My Plan (v2).md`.", spaced)
        assertEquals("See [`notes/My Plan (v2).md`](notes/My%20Plan%20%28v2%29.md).", linked)
        assertEquals(SummaryTarget.Doc("notes/My Plan (v2).md"), SummaryLinks.target("notes/My%20Plan%20%28v2%29.md", spaced))
    }
}
