import XCTest

/// Try the Demo from a fresh install: the in-app bridge, and nothing kept after it.
final class TryDemoTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    @MainActor
    func testTryTheDemoAndLeaveIt() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-no-saved-hosts"]
        app.launch()

        // The sidebar offers it on a phone, the empty detail on an iPad; either one hides.
        firstHittable(app.buttons.matching(identifier: "try-demo")).tap()
        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
        save("try-demo")

        showSidebar(app)
        XCTAssertTrue(app.buttons["host-Demo"].waitForExistence(timeout: 5))
        tapWhenReady(app.buttons["leave-demo"])
        XCTAssertTrue(app.buttons["host-Demo"].waitForNonExistence(timeout: 5))
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
