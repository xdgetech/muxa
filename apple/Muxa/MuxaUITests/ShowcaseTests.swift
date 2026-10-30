import XCTest

/// The App Store and README screenshots, against the demo bridge's showcase data.
/// Runs only when MUXA_SHOWCASE_DIR is set: `make screenshots`.
final class ShowcaseTests: XCTestCase {
    private var dir: URL!
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }


    override func setUpWithError() throws {
        let path = ProcessInfo.processInfo.environment["MUXA_SHOWCASE_DIR"]
        try XCTSkipIf(path == nil, "set MUXA_SHOWCASE_DIR to take the screenshots")
        dir = URL(fileURLWithPath: path!)
        continueAfterFailure = false
    }

    override func tearDown() {
        XCUIDevice.shared.orientation = .portrait
    }

    @MainActor
    func testScreenshots() throws {
        if isPhone { return try phoneScreenshots() }
        let app = launch()
        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["quick-1"].waitForExistence(timeout: 5))
        try save("1-chat", in: app)

        composer.typeKey("r", modifierFlags: .command)
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(waitFor(pane, containing: "? for shortcuts"))
        try save("2-agent", in: app)

        pane.typeKey("k", modifierFlags: .command)
        XCTAssertTrue(app.staticTexts["search-index"].waitForExistence(timeout: 5))
        try save("3-switcher", in: app)
        app.buttons["Close"].tap()

        pane.typeKey("d", modifierFlags: [.command, .shift])
        let web = app.webViews.firstMatch
        // The diagram is drawn: its labels are text, not a code block.
        XCTAssertTrue(web.staticTexts["verify signature"].waitForExistence(timeout: 10))
        try save("4-plan", in: app)

        // Changes have their own tab (ADR 0011).
        tapWhenReady(app.segmentedControls["reader-tab"].buttons["Changes"])
        tapWhenReady(app.descendants(matching: .any)["change-src/webhooks/router.rs"].firstMatch)
        let added = web.staticTexts.containing(NSPredicate(format: "label CONTAINS 'Job::Receipt'")).firstMatch
        XCTAssertTrue(added.waitForExistence(timeout: 10))
        try save("5-diff", in: app)
        app.buttons["Done"].tap()

        pane.typeKey("j", modifierFlags: .command)
        XCTAssertTrue(waitFor(pane, containing: "test result: ok. 14 passed"))
        try save("6-shell", in: app)
    }

    /// The same story on a phone, in portrait and by touch: the simulator's hardware
    /// keyboard comes and goes, so no shot depends on a shortcut.
    @MainActor
    private func phoneScreenshots() throws {
        let app = launch()
        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["quick-1"].waitForExistence(timeout: 5))
        try save("1-chat", in: app)

        showAgentTerminal(app)
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        // A cold simulator can take longer than one wait to draw the first screen.
        XCTAssertTrue(waitFor(pane, containing: "? for shortcuts") || waitFor(pane, containing: "? for shortcuts"),
                      "the pane shows: \(pane.value ?? "nothing")")
        try save("2-agent", in: app)

        // A phone's session switcher is its list: every agent with its state.
        showSidebar(app)
        XCTAssertTrue(app.buttons["host-Mac mini"].waitForExistence(timeout: 5))
        try save("3-switcher", in: app)
        app.buttons.matching(NSPredicate(format: "label CONTAINS 'stripe-webhooks'")).firstMatch.tap()
        XCTAssertTrue(pane.waitForExistence(timeout: 5))

        tapWhenReady(app.buttons["More"])
        tapWhenReady(app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Documents'")).firstMatch)
        let web = app.webViews.firstMatch
        XCTAssertTrue(web.staticTexts["verify signature"].waitForExistence(timeout: 10))
        try save("4-plan", in: app)

        showReaderList(app)
        tapWhenReady(app.segmentedControls["reader-tab"].buttons["Changes"])
        tapWhenReady(app.descendants(matching: .any)["change-src/webhooks/router.rs"].firstMatch)
        let added = web.staticTexts.containing(NSPredicate(format: "label CONTAINS 'Job::Receipt'")).firstMatch
        XCTAssertTrue(added.waitForExistence(timeout: 10))
        try save("5-diff", in: app)
        app.buttons["Done"].tap()

        tapWhenReady(app.segmentedControls["view-mode"].buttons["Shell"])
        XCTAssertTrue(waitFor(pane, containing: "test result: ok. 14 passed"))
        try save("6-shell", in: app)
    }

    /// Another agent starts waiting a few seconds after launch (`-demo-alerts`).
    @MainActor
    func testAlertScreenshot() throws {
        let app = launch(alerts: true)
        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
        if isPhone {
            showAgentTerminal(app)
        } else {
            composer.typeKey("j", modifierFlags: .command)
        }
        hideKeyboard(app)
        // The showcase alert comes 10 seconds in and stays for 4: no time to settle after it.
        XCTAssertTrue(app.buttons["agent-alert"].waitForExistence(timeout: 15))
        // Past the slide-in.
        Thread.sleep(forTimeInterval: 0.6)
        try snap("7-alert")
    }

    @MainActor
    private func launch(alerts: Bool = false) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-showcase"] + (alerts ? ["-demo-alerts"] : [])
        app.launch()
        // After launch, so every shot has the same orientation: the store's iPad slot is
        // landscape, its iPhone slot portrait.
        XCUIDevice.shared.orientation = isPhone ? .portrait : .landscapeLeft
        return app
    }

    @MainActor
    private func save(_ name: String, in app: XCUIApplication) throws {
        hideKeyboard(app)
        // Animations and the cursor blink settle.
        sleep(1)
        try snap(name)
    }

    /// A headless simulator has no hardware keyboard; the on-screen one would cover half the shot.
    @MainActor
    private func hideKeyboard(_ app: XCUIApplication) {
        let hide = app.keyboards.buttons["Hide keyboard"]
        if hide.exists { hide.tap() }
    }

    /// Upright: the raw PNG is the portrait framebuffer, but the image knows its orientation.
    private func snap(_ name: String) throws {
        let image = XCUIScreen.main.screenshot().image
        let format = UIGraphicsImageRendererFormat()
        format.scale = image.scale
        let png = UIGraphicsImageRenderer(size: image.size, format: format).pngData { _ in
            image.draw(at: .zero)
        }
        try png.write(to: dir.appendingPathComponent("\(name).png"))
    }
}
