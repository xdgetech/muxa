import Testing

@testable import MuxaSession

struct InboxSelectionTests {
    private func next(_ current: String, _ before: [String], _ after: [String], movedDown: Bool = false) -> String? {
        InboxSelection.next(current: current, before: before, after: after, movedDown: movedDown)
    }

    @Test func aChangedItemThatStaysListedStaysSelected() {
        #expect(next("b", ["a", "b", "c"], ["a", "b", "c"]) == "b")
    }

    @Test func anItemThatLeavesFromTheMiddleSelectsTheNext() {
        #expect(next("b", ["a", "b", "c"], ["a", "c"]) == "c")
    }

    @Test func theLastItemLeavingSelectsTheNewLast() {
        #expect(next("c", ["a", "b", "c"], ["a", "b"]) == "b")
    }

    @Test func laterSelectsTheNext() {
        #expect(next("a", ["a", "b", "c"], ["b", "c", "a"], movedDown: true) == "b")
    }

    @Test func laterOnTheLastItemSelectsTheOneAboveIt() {
        #expect(next("c", ["a", "b", "c"], ["a", "b", "c"], movedDown: true) == "b")
    }

    @Test func laterOnTheOnlyItemKeepsIt() {
        #expect(next("a", ["a"], ["a"], movedDown: true) == "a")
    }

    @Test func newItemsAboveKeepTheSelection() {
        #expect(next("b", ["a", "b"], ["x", "y", "a", "b"]) == "b")
    }

    @Test func anEmptyListSelectsNothing() {
        #expect(next("a", ["a"], []) == nil)
    }
}
