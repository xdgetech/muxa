import XCTest

/// Helpers shared by the UI tests.
extension XCTestCase {
    /// Set MUXA_SCREENSHOT_DIR (TEST_RUNNER_MUXA_SCREENSHOT_DIR for xcodebuild) to keep screenshots.
    func save(_ name: String) {
        guard let dir = ProcessInfo.processInfo.environment["MUXA_SCREENSHOT_DIR"] else { return }
        sleep(1)
        let png = XCUIScreen.main.screenshot().pngRepresentation
        try? png.write(to: URL(fileURLWithPath: dir).appendingPathComponent("\(name).png"))
    }

    /// The pane's bar: no two controls cover each other.
    @MainActor
    func assertToolbarDoesNotOverlap(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        sleep(1)
        let bar = app.navigationBars.firstMatch
        let segmented = bar.segmentedControls.allElementsBoundByIndex.map { $0.frame }
        // A segment's own buttons sit inside its control; compare the control instead.
        let buttons = bar.buttons.allElementsBoundByIndex.filter { $0.isHittable }.map { $0.frame }
            .filter { b in !segmented.contains { $0.insetBy(dx: -2, dy: -2).contains(CGPoint(x: b.midX, y: b.midY)) } }
        let tops = (buttons + segmented).filter { !$0.isEmpty && $0.width < bar.frame.width / 2 }
        for (i, a) in tops.enumerated() {
            for b in tops[(i + 1)...] where !a.contains(b) && !b.contains(a) {
                XCTAssertFalse(a.insetBy(dx: 1, dy: 1).intersects(b.insetBy(dx: 1, dy: 1)), "\(a) overlaps \(b)", file: file, line: line)
            }
        }
    }

    /// Steps wait a bounded time for animations (`AnimationIdle`): wait until a menu
    /// or a rotation has put the element where a tap reaches it.
    @MainActor
    func tapWhenReady(_ element: XCUIElement, file: StaticString = #filePath, line: UInt = #line) {
        let exp = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == true AND hittable == true"), object: element)
        XCTAssertEqual(XCTWaiter.wait(for: [exp], timeout: 5), .completed, file: file, line: line)
        element.tap()
    }

    func waitForFocus(_ element: XCUIElement) -> Bool {
        let exp = XCTNSPredicateExpectation(predicate: NSPredicate(format: "hasKeyboardFocus == true"), object: element)
        return XCTWaiter.wait(for: [exp], timeout: 5) == .completed
    }

    func waitFor(_ element: XCUIElement, containing text: String) -> Bool {
        let predicate = NSPredicate(format: "value CONTAINS %@", text)
        let exp = XCTNSPredicateExpectation(predicate: predicate, object: element)
        return XCTWaiter.wait(for: [exp], timeout: 5) == .completed
    }

    /// A tap on the pane's left part. A pane wider than the screen has no hit point
    /// for `tap()`, and its left edge is on screen after a fresh screen.
    func tapPane(_ pane: XCUIElement) {
        pane.coordinate(withNormalizedOffset: CGVector(dx: 0.1, dy: 0.3)).tap()
    }

    /// Switches the agent session from Chat to its terminal, and waits until the chat is gone:
    /// the pane lies under the chat, so its text alone does not say which shows.
    @MainActor
    func showAgentTerminal(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        let agent = app.segmentedControls["view-mode"].buttons["Agent"]
        tapWhenReady(agent, file: file, line: line)
        let composer = app.descendants(matching: .any)["composer"].firstMatch
        // A tap while the bar still settles after launch can be lost; one more is enough.
        if !composer.waitForNonExistence(timeout: 3) { agent.tap() }
        XCTAssertTrue(composer.waitForNonExistence(timeout: 5), file: file, line: line)
    }

    var isPhone: Bool { UIDevice.current.userInterfaceIdiom == .phone }

    /// The Reader's list. On a phone the Reader is a stack: it opens on a document,
    /// and the list is one Back away. A Max phone in landscape keeps both columns.
    @MainActor
    func showReaderList(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        let tabs = app.segmentedControls["reader-tab"]
        if tabs.waitForExistence(timeout: 2), tabs.isHittable { return }
        let back = app.navigationBars.buttons["BackButton"].firstMatch
        // The pane's own Show Sidebar is still in the tree behind the Reader; the Reader's comes last.
        let sidebar = app.buttons.matching(identifier: "Show Sidebar").allElementsBoundByIndex.last
        if back.exists {
            tapWhenReady(back, file: file, line: line)
        } else if let sidebar, sidebar.isHittable {
            sidebar.tap()
        }
        XCTAssertTrue(tabs.waitForExistence(timeout: 5), file: file, line: line)
    }

    /// Hosts and sessions: a sidebar button on an iPad, Back on a phone.
    @MainActor
    func showSidebar(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        let button = isPhone ? app.navigationBars.buttons["BackButton"] : app.buttons["Show Sidebar"]
        tapWhenReady(button.firstMatch, file: file, line: line)
    }

    /// A tap on a button XCUITest calls not hittable while it is on screen: in a chat
    /// scroll view under the phone keyboard, its hit test can fail.
    func tapButton(_ button: XCUIElement) {
        if button.isHittable {
            button.tap()
        } else {
            button.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        }
    }

    /// Says no to a popover or a dialog: outside it on an iPad, Cancel on a phone.
    @MainActor
    func dismissPopover(_ app: XCUIApplication) {
        let region = app.otherElements["PopoverDismissRegion"].firstMatch
        let cancel = app.buttons["Cancel"].firstMatch
        if region.exists {
            region.tap()
        } else if cancel.exists {
            cancel.tap()
        } else {
            app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.3)).tap()
        }
    }
}
