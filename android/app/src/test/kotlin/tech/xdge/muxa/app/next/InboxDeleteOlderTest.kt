package tech.xdge.muxa.app.next

import org.junit.Assert.assertEquals
import org.junit.Test
import tech.xdge.muxa.v2.Inbox
import tech.xdge.muxa.v2.InboxItem
import tech.xdge.muxa.v2.ItemKind

class InboxDeleteOlderTest {
    private val now = 1_764_000_000L
    private val day = 86_400L

    private fun item(id: String, age: Long) = InboxItem(id, ItemKind.TODO, id, created = now - age, source = "capture")

    @Test
    fun theCountTakesOpenAndDoneItemsOlderThanTheDays() {
        val inbox = Inbox(
            1,
            items = listOf(item("new", day - 1), item("old", 2 * day), item("exact", day)),
            done = listOf(item("done-old", 4 * day)),
        )
        // Exactly a day old is not older than a day, as the host's cutoff is strict.
        assertEquals(2, olderCount(inbox, 1, now))
        assertEquals(1, olderCount(inbox, 3, now))
        assertEquals(0, olderCount(inbox, 5, now))
    }

    @Test
    fun daysReadInTheSingularForOne() {
        assertEquals("1 day", daysText(1))
        assertEquals("3 days", daysText(3))
    }
}
