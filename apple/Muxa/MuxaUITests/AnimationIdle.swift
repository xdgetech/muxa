import ObjectiveC
import XCTest

/// Before each step, XCTest waits for the app's animations to end. After a hardware
/// key, iPadOS shows its keyboard assistant bar for the pane (a `UITextInput`), and
/// the bar's glass animates forever: every step then stalls for 60 seconds.
/// This keeps the wait for the event loop and bounds the wait for animations.
/// XCTest has no public switch for it, so it patches a private method.
enum AnimationIdle {
    /// Longer than UIKit's menu, sheet and rotation transitions.
    static let settle: TimeInterval = 0.5

    static let bound: Void = {
        typealias Wait = @convention(c) (AnyObject, Selector, Bool, Bool) -> Void
        let selector = NSSelectorFromString("waitForQuiescenceIncludingAnimationsIdle:isPreEvent:")
        guard let type = NSClassFromString("XCUIApplicationProcess"),
            let method = class_getInstanceMethod(type, selector)
        else {
            print("AnimationIdle: XCTest has no \(selector); steps wait for animations again")
            return
        }
        let original = unsafeBitCast(method_getImplementation(method), to: Wait.self)
        let wait: @convention(block) (AnyObject, Bool, Bool) -> Void = { process, animations, preEvent in
            original(process, selector, false, preEvent)
            if animations { Thread.sleep(forTimeInterval: settle) }
        }
        method_setImplementation(method, imp_implementationWithBlock(wait))
    }()
}
