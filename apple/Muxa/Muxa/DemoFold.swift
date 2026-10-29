#if DEBUG
import SwiftUI

/// Launch argument `-foldable`: a button folds and unfolds the app as an iPhone Duo
/// does. No simulator folds on command, so the UI tests use this to change the
/// size class under a running app.
struct FoldHarness<Content: View>: View {
    static var enabled: Bool { ProcessInfo.processInfo.arguments.contains("-foldable") }
    /// The iPhone Duo's outer screen in points, from Xcode 27.1's simulator.
    static var outer: CGSize { CGSize(width: 466, height: 678) }

    @ViewBuilder let content: Content
    @State private var folded = false

    var body: some View {
        ZStack(alignment: .leading) {
            Color.black.ignoresSafeArea()
            content
                .frame(width: folded ? Self.outer.width : nil, height: folded ? Self.outer.height : nil)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .environment(\.horizontalSizeClass, folded ? .compact : .regular)
            // A thin strip on the left edge, over the pane's margin. The status bar's
            // strip takes no taps.
            Button { folded.toggle() } label: {
                Color.orange.frame(width: 6, height: 60)
            }
            .accessibilityLabel(folded ? "Unfold" : "Fold")
            .accessibilityIdentifier("demo-fold")
        }
    }
}
#endif
