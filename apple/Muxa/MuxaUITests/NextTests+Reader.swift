import XCTest

/// Sharing the Reader's files on iOS (ADR 0100): Share offers Save Image for
/// an image and Save to Files for a document.
extension NextTests {
    @MainActor
    func testShareOffersSaveImageForAnImage() throws {
        let app = openReader()
        app.segmentedControls["reader.kind"].buttons["Images"].tap()
        // Folders start closed.
        let folder = app.descendants(matching: .any)["reader.folder.screenshots"]
        XCTAssertTrue(folder.waitForExistence(timeout: 10))
        folder.firstMatch.tap()
        let image = app.descendants(matching: .any)["reader.file.screenshots/login.png"]
        XCTAssertTrue(image.waitForExistence(timeout: 10))
        image.tap()
        share(in: app, offers: "Save Image")
    }

    @MainActor
    func testShareOffersSaveToFilesForADocument() throws {
        let app = openReader()
        let plan = app.descendants(matching: .any)["reader.file.PLAN.md"]
        XCTAssertTrue(plan.waitForExistence(timeout: 10))
        plan.tap()
        share(in: app, offers: "Save to Files")
    }

    @MainActor
    private func openReader() -> XCUIApplication {
        let app = launch()
        app.buttons["needsyou.t2"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        app.buttons["focus.reader"].tap()
        XCTAssertTrue(app.segmentedControls["reader.kind"].waitForExistence(timeout: 10))
        return app
    }

    @MainActor
    private func share(in app: XCUIApplication, offers action: String) {
        let share = app.buttons["reader.share"]
        XCTAssertTrue(share.waitForExistence(timeout: 10))
        share.tap()
        let item = app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", action)).firstMatch
        XCTAssertTrue(item.waitForExistence(timeout: 10), action)
    }
}
