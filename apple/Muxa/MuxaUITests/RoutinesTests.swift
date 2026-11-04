import XCTest

/// The Routines list and detail against the demo's Routines (ADR 0026). They moved
/// here from the older Inbox list's tests, which went with that list (ADR 0033 §5).
final class RoutinesTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    /// Opens the demo on the Routines list.
    @MainActor
    private func launchOnRoutines() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        let list = app.descendants(matching: .any)["inbox-list"].firstMatch
        if !list.waitForExistence(timeout: 5) {
            showSidebar(app)
            tapWhenReady(app.buttons["open-tasks"])
        }
        tapWhenReady(app.segmentedControls["inbox-mode"].buttons["All Tasks"])
        tapWhenReady(app.buttons["open-routines"])
        return app
    }

    /// Scrolls until `element` can be tapped.
    @MainActor
    private func reveal(_ app: XCUIApplication, _ element: XCUIElement) {
        for _ in 0..<4 where !(element.exists && element.isHittable) {
            app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.7))
                .press(forDuration: 0.05, thenDragTo: app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.35)))
        }
    }

    /// A Routine's detail shows its fields; Run now starts a run, and a disabled one cannot run.
    @MainActor
    func testARoutineShowsItsFieldsAndRunsNow() throws {
        let app = launchOnRoutines()
        tapWhenReady(app.buttons["routine-work/slack-digest"])
        let field = { (id: String) in app.descendants(matching: .any)[id].firstMatch }
        XCTAssertTrue(field("routine-trigger").waitForExistence(timeout: 5))
        XCTAssertTrue(field("routine-workspace").label.contains("work"), field("routine-workspace").label)
        XCTAssertTrue(field("routine-trigger").label.contains("08:00 Europe/Berlin"), field("routine-trigger").label)
        XCTAssertTrue(field("routine-enabled").label.contains("Yes"))
        XCTAssertTrue(field("routine-last-success").label.contains("3 new"), field("routine-last-success").label)
        XCTAssertTrue(field("routine-next-due").exists)
        XCTAssertTrue(app.staticTexts["routine-failure-reason"].label.contains("output.json"))
        XCTAssertFalse(field("routine-running").exists)
        save("routine-detail")
        let run = app.buttons["routine-run-now"]
        reveal(app, run)
        tapWhenReady(run)
        let disabled = XCTNSPredicateExpectation(predicate: NSPredicate(format: "isEnabled == false"), object: run)
        XCTAssertEqual(XCTWaiter.wait(for: [disabled], timeout: 5), .completed, "one run at a time")
        let window = app.windows.firstMatch
        window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.35))
            .press(forDuration: 0.05, thenDragTo: window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.75)))
        XCTAssertTrue(field("routine-running").waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["routine-open-terminal"].exists)

        app.navigationBars.buttons.firstMatch.tap()
        tapWhenReady(app.buttons["routine-personal/weekly-memory"])
        XCTAssertTrue(field("routine-trigger").waitForExistence(timeout: 5))
        XCTAssertTrue(field("routine-trigger").label.contains("Run now only"))
        reveal(app, app.buttons["routine-run-now"])
        XCTAssertFalse(app.buttons["routine-run-now"].isEnabled, "a disabled Routine never runs")
    }

    /// The running run's Open Terminal shows its session.
    @MainActor
    func testARunningRoutineOpensItsTerminal() throws {
        let app = launchOnRoutines()
        tapWhenReady(app.buttons["routine-work/linear-triage"])
        reveal(app, app.buttons["routine-open-terminal"])
        tapWhenReady(app.buttons["routine-open-terminal"])
        XCTAssertTrue(app.navigationBars["linear-triage-t-r00002"].waitForExistence(timeout: 10))
    }
}
