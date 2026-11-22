import XCTest

/// Screenshots of the main screens on the demo server, kept only when
/// `TEST_RUNNER_MUXA_SCREENSHOT_DIR` names a folder. An iPad turns to
/// landscape, as the tablet mock.
final class ScreenshotTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func tearDown() {
        XCUIDevice.shared.orientation = .portrait
        super.tearDown()
    }

    @MainActor
    func testScreens() throws {
        guard let dir = ProcessInfo.processInfo.environment["MUXA_SCREENSHOT_DIR"], !dir.isEmpty else {
            throw XCTSkip("MUXA_SCREENSHOT_DIR is not set")
        }
        let pad = UIDevice.current.userInterfaceIdiom == .pad
        XCUIDevice.shared.orientation = pad ? .landscapeLeft : .portrait
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        let any = app.descendants(matching: .any)
        XCTAssertTrue(any["item.d1"].waitForExistence(timeout: 10))
        save(dir, "inbox")
        any["item.d1"].tap()
        XCTAssertTrue(app.staticTexts["detail.title"].waitForExistence(timeout: 5))
        save(dir, "item")
        app.launch()
        XCTAssertTrue(any["item.d1"].waitForExistence(timeout: 10))
        app.buttons["needsyou.t2"].tap()
        XCTAssertTrue(any["focus.pane"].waitForExistence(timeout: 10))
        save(dir, "terminal")
        app.buttons["focus.chat"].tap()
        XCTAssertTrue(any["chat.prompt"].waitForExistence(timeout: 10))
        save(dir, "chat")
        app.buttons["focus.reader"].tap()
        XCTAssertTrue(any["reader.file.PLAN.md"].waitForExistence(timeout: 10))
        save(dir, "reader")
        if pad {
            app.buttons["home.routines"].tap()
        } else {
            app.buttons["mode.home"].tap()
            app.tabBars.buttons["Workspaces"].tap()
            XCTAssertTrue(any["terminal.t1"].waitForExistence(timeout: 5))
            save(dir, "tasks")
            app.tabBars.buttons["Routines"].tap()
        }
        XCTAssertTrue(any["routine.nightly-backup"].waitForExistence(timeout: 5))
        save(dir, "routines")
    }

    private func save(_ dir: String, _ name: String) {
        Thread.sleep(forTimeInterval: 1)
        let idiom = UIDevice.current.userInterfaceIdiom == .pad ? "ipad" : "iphone"
        let url = URL(fileURLWithPath: dir).appendingPathComponent("\(idiom)-\(name).png")
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        XCTAssertNoThrow(try XCUIScreen.main.screenshot().pngRepresentation.write(to: url))
    }
}
