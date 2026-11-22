package tech.xdge.muxa.app.next

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ReaderLinksTest {
    @Test
    fun aRelativeLinkResolvesAgainstTheDocumentsFolder() {
        assertEquals(".muxa/TASK.md", ReaderLinks.resolve(".muxa/TASK.md", "PLAN.md"))
        assertEquals("docs/b.md", ReaderLinks.resolve("b.md", "docs/a.md"))
        assertEquals("README.md", ReaderLinks.resolve("../README.md", "docs/a.md"))
        assertEquals("docs/b.md", ReaderLinks.resolve("./b.md#usage", "docs/a.md"))
        assertEquals("README.md", ReaderLinks.resolve("/README.md", "docs/a.md"))
        assertEquals("docs/my notes.md", ReaderLinks.resolve("my%20notes.md", "docs/a.md"))
    }

    @Test
    fun aLinkOutsideTheFolderOrWithASchemeIsNotADocument() {
        assertNull(ReaderLinks.resolve("../../etc/passwd", "docs/a.md"))
        assertNull(ReaderLinks.resolve("https://example.com/a.md", "PLAN.md"))
        assertNull(ReaderLinks.resolve("mailto:me@example.com", "PLAN.md"))
        assertNull(ReaderLinks.resolve("", "PLAN.md"))
    }
}
