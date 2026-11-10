import XCTest

/// The Inbox's Work and Unsorted rows on the demo (ADR 0048), shared by the Capture
/// and assistant tests.
extension XCTestCase {
    @MainActor
    func launchWorkInbox(_ arguments: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "inbox"] + arguments
        app.launch()
        XCTAssertTrue(app.buttons["item-row-i-0000a001"].waitForExistence(timeout: 10))
        return app
    }

    @MainActor
    func workRow(_ app: XCUIApplication, _ id: String) -> XCUIElement { app.buttons["work-row-\(id)"] }

    @MainActor
    func inboxSection(_ app: XCUIApplication, _ name: String) -> XCUIElement {
        app.descendants(matching: .any)["inbox-section-\(name)"].firstMatch
    }

    @MainActor
    func captureRow(_ app: XCUIApplication, _ id: String) -> XCUIElement { app.buttons["capture-row-\(id)"] }

    /// Scrolls the sidebar until `element` is clear of the screen's bottom edge, where a
    /// drag would reach the home indicator instead of the row.
    @MainActor
    func revealInSidebar(_ app: XCUIApplication, _ element: XCUIElement) {
        let bottom = app.windows.firstMatch.frame.maxY - 80
        for _ in 0..<4 where !(element.isHittable && element.frame.maxY < bottom) {
            app.collectionViews["sidebar-list"].swipeUp()
        }
    }

    /// Waits for a row that is still to come, scrolling: a phone's list makes rows
    /// below the screen only once they scroll into view.
    @MainActor
    func waitForSidebarRow(_ app: XCUIApplication, _ row: XCUIElement) -> Bool {
        for _ in 0..<5 {
            if row.waitForExistence(timeout: 2) {
                revealInSidebar(app, row)
                return true
            }
            app.collectionViews["sidebar-list"].swipeUp()
        }
        return row.exists
    }

    /// Back to the Inbox list from a detail: a phone pushed the detail, a portrait iPad
    /// may have hidden the sidebar. A shown sidebar scrolled down keeps its mode picker
    /// under the bar, so Hide Sidebar says it is shown.
    @MainActor
    func showInboxList(_ app: XCUIApplication) {
        let modes = app.segmentedControls["sidebar-mode"]
        if modes.exists && (modes.isHittable || app.buttons["Hide Sidebar"].exists) { return }
        showSidebar(app)
    }

    /// Scrolls a Work item's detail until `target` is on screen: its Result can push
    /// the actions below the fold.
    @MainActor
    func scrollWorkDetail(_ app: XCUIApplication, to target: XCUIElement) {
        for _ in 0..<6 where !(target.exists && target.isHittable) {
            app.collectionViews["work-detail"].swipeUp()
        }
    }
}
