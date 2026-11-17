import XCTest

/// More → Scrollback, then a finger drag inside it reads older lines.
final class ScrollbackDragTests: XCTestCase {
    @MainActor
    func testADragInsideTheScrollbackReadsOlderLines() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "sessions"]
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any)["composer"].firstMatch.waitForExistence(timeout: 10))
        tapWhenReady(app.buttons["More"])
        tapWhenReady(app.buttons["menu-history"])
        let history = app.descendants(matching: .any)["history"].firstMatch
        XCTAssertTrue(history.waitForExistence(timeout: 10))
        sleep(1)
        let before = history.value as? String ?? ""
        let start = history.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.2))
        start.press(forDuration: 0.1, thenDragTo: history.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.8)),
                    withVelocity: .slow, thenHoldForDuration: 0.3)
        sleep(1)
        let after = history.value as? String ?? ""
        XCTAssertNotEqual(after, before, "the drag did not scroll: \(after.suffix(200))")
    }
}
