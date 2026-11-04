import XCTest

/// Try the Demo from a fresh install: the in-app bridge, and nothing kept after it.
final class TryDemoTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    /// The demo plays a host with Inbox items (`work:3`), so it starts on the Inbox
    /// list (ADR 0035 §4, §6). The way out is on that list: Leave Demo in the status row.
    @MainActor
    func testTryTheDemoAndLeaveIt() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-no-saved-hosts", "-mode", "inbox"]
        app.launch()

        // The sidebar offers it on a phone, the empty detail on an iPad too.
        firstHittable(app.buttons.matching(identifier: "try-demo")).tap()
        let ask = app.buttons["item-row-i-5c000001"]
        XCTAssertTrue(ask.waitForExistence(timeout: 10), "the showcase's Inbox items")
        XCTAssertFalse(app.descendants(matching: .any)["pane"].firstMatch.exists, "no pane at connect in Inbox mode")
        save("try-demo")

        tapWhenReady(app.buttons["leave-demo"])
        XCTAssertTrue(ask.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.buttons.matching(identifier: "try-demo").firstMatch.waitForExistence(timeout: 5))
    }

    /// Settings has the demo too, and its host while it runs.
    @MainActor
    func testSettingsHoldTheHostsAndTheDemo() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-no-saved-hosts", "-mode", "inbox"]
        app.launch()
        tapWhenReady(app.buttons["open-settings"])
        tapWhenReady(app.buttons["settings-try-demo"])
        XCTAssertTrue(app.buttons["item-row-i-5c000001"].waitForExistence(timeout: 10))
        tapWhenReady(app.buttons["status-row"])
        XCTAssertTrue(app.buttons["host-Demo"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["settings-disconnect"].exists)
        save("settings")
        tapWhenReady(app.buttons["settings-leave-demo"])
        XCTAssertTrue(app.buttons.matching(identifier: "try-demo").firstMatch.waitForExistence(timeout: 5))
    }

    private func firstHittable(_ query: XCUIElementQuery) -> XCUIElement {
        let deadline = Date().addingTimeInterval(10)
        repeat {
            if let element = query.allElementsBoundByIndex.first(where: { $0.exists && $0.isHittable }) { return element }
            usleep(200_000)
        } while Date() < deadline
        return query.firstMatch
    }
}
