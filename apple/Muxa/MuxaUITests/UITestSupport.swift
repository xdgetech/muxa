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
}
