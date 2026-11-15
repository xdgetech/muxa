import MuxaSession
import MuxaUI
import UIKit

/// Holds the pane in a scroll view. In `.width` mode the grid can be taller than
/// the view, in `.readable` also wider; the scroll view follows the cursor unless
/// the user is reading.
///
/// A drag down past the grid's top, or a trackpad or wheel scroll up, shows the host's
/// earlier lines in place, as a snapshot that output does not move. Its newest line,
/// a tap, Live or a key returns to the live grid.
final class PaneContainer: UIView, UIScrollViewDelegate, UIGestureRecognizerDelegate {
    let terminal = PaneTerminalView()
    let input = KeyInputView()
    private let scroll = UIScrollView()
    private var cursorWasVisible = true
    var focusRequest = 0
    var onPinch: (PaneFit) -> Void = { _ in }
    /// The grid this view shows at the default font changed (ADR 0020).
    var onViewport: (PaneViewport) -> Void = { _ in }
    private var viewport: PaneViewport?

    /// Asks for a newer `snapshot`. Nil when the host cannot send a pane's scrollback.
    var onNeedSnapshot: (() -> Void)?
    /// The latest scrollback the host sent for this pane.
    var snapshot: PaneHistory? {
        didSet { if snapshot != oldValue { snapshotArrived() } }
    }
    private let reader = PaneReader()
    /// The pane printed something since the last snapshot was asked for.
    private var outputSinceSnapshot = true
    /// The pan's translation when the reader took over this drag, while the finger is down.
    private var handoff: CGFloat?
    /// A pull far enough to read back, waiting for a snapshot.
    private var waitingForSnapshot = false
    /// A pull past the grid's top by this many points shows the earlier lines.
    private static let pullDistance: CGFloat = 24
    /// A trackpad or wheel scroll up by this many points shows them.
    private static let wheelDistance: CGFloat = 20

    /// No snapshot to read in place: show the scrollback once it loads (`showsScrollback`).
    var onScrollBack: () -> Void = {}
    /// The scrollback from More or `onScrollBack` shows over this view.
    var showsScrollback = false
    /// Leaves that scrollback.
    var onLive: () -> Void = {}
    /// The user reads earlier lines, here or in that scrollback.
    var isReadingBack: Bool { reader.isReading || showsScrollback }

    var fitOverride: PaneFit? {
        didSet { if fitOverride != oldValue { setNeedsLayout() } }
    }

    init() {
        super.init(frame: .zero)
        backgroundColor = .black
        scroll.backgroundColor = .black
        // A pull down past the top reads back, also when the grid fits.
        scroll.alwaysBounceVertical = true
        scroll.alwaysBounceHorizontal = false
        scroll.isDirectionalLockEnabled = true
        scroll.indicatorStyle = .white
        scroll.contentInsetAdjustmentBehavior = .never
        scroll.keyboardDismissMode = .none
        scroll.delegate = self
        addSubview(scroll)
        // The input view lies over the grid inside the scroll view, so drags still scroll.
        scroll.addSubview(terminal)
        scroll.addSubview(input)
        input.backgroundColor = .clear
        input.accessibilityIdentifier = "pane"
        input.screenText = { [terminal] in terminal.screenText }
        // The input view has the grid's frame, so the terminal's coordinates are its own.
        input.caretFrame = { [terminal] in terminal.cursorFrame }
        input.addGestureRecognizer(UITapGestureRecognizer(target: input, action: #selector(UIResponder.becomeFirstResponder)))
        input.addGestureRecognizer(UIPinchGestureRecognizer(target: self, action: #selector(pinched)))
        // A trackpad or wheel scroll up at the top: it does not bounce far enough for the pull.
        let wheel = UIPanGestureRecognizer(target: self, action: #selector(wheeled))
        wheel.allowedScrollTypesMask = .all
        wheel.allowedTouchTypes = []
        wheel.delegate = self
        input.addGestureRecognizer(wheel)
        terminal.onUpdate = { [weak self] reset in self?.contentChanged(reset: reset) }
        terminal.onGridChange = { [weak self] in self?.layoutContent() }
        // Over the scroll view, not in it: the live grid's bounce stays hidden while reading.
        reader.isHidden = true
        reader.onLive = { [weak self] in self?.showLive() }
        addSubview(reader)
    }

    required init?(coder: NSCoder) { nil }

    private var mode: PaneFit {
        if let fitOverride { return fitOverride }
        if let viewport, viewport.showsWhole(cols: terminal.cols, rows: terminal.rows) { return .whole }
        return terminal.automaticFit(in: bounds.size)
    }

    @objc private func wheeled(_ pan: UIPanGestureRecognizer) {
        if pan.state == .began, outputSinceSnapshot { requestSnapshot() }
        guard pan.state == .changed, !isReadingBack, scroll.contentOffset.y <= 0,
            pan.translation(in: self).y > Self.wheelDistance
        else { return }
        // Cancels the rest of this scroll: it must not reach the earlier lines as they open.
        pan.isEnabled = false
        pan.isEnabled = true
        if onNeedSnapshot != nil, reader.hasSnapshot { reader.open() } else { onScrollBack() }
    }

    /// The wheel recognizer watches; the scroll view still scrolls a grid taller than the view.
    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer
    ) -> Bool { true }

    @objc private func pinched(_ pinch: UIPinchGestureRecognizer) {
        guard pinch.state == .ended, let fit = PaneFit.pinched(scale: pinch.scale) else { return }
        onPinch(fit)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        reader.frame = bounds
        layoutContent()
    }

    /// Sizes the scroll content to the grid in the current mode.
    private func layoutContent() {
        scroll.frame = bounds
        guard bounds.width > 0, bounds.height > 0 else { return }
        // A phone-sized area: compact in either direction, as the layout decides (ADR 0012).
        let compact = traitCollection.horizontalSizeClass == .compact || traitCollection.verticalSizeClass == .compact
        viewport = terminal.viewportGrid(in: bounds.size, fontSize: PaneViewport.fontSize(compact: compact))
        if let viewport { onViewport(viewport) }
        terminal.fitMode = mode
        terminal.viewport = bounds.size
        let grid = terminal.gridSize(in: bounds.size)
        let content = CGRect(
            x: 0, y: 0, width: max(grid.width, bounds.width), height: max(grid.height, bounds.height))
        if terminal.frame != content {
            terminal.frame = content
            input.frame = content
            scroll.contentSize = content.size
            terminal.layoutIfNeeded()
            followCursor()
        }
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window != nil { input.focusIfKeyboard() }
    }

    private func contentChanged(reset: Bool) {
        outputSinceSnapshot = true
        // A new screen starts at its top; the last pane's scroll position means nothing here.
        if reset {
            scroll.contentOffset = .zero
            closeReader()
            requestSnapshot()
        }
        let wasVisible = reset || cursorWasVisible
        // SwiftTerm moves its caret after the feed is processed.
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            if wasVisible { self.followCursor() }
            self.cursorWasVisible = self.cursorIsVisible
        }
    }

    private var cursorIsVisible: Bool {
        scroll.bounds.contains(terminal.cursorFrame)
    }

    /// Brings the cursor cell into view, with a few cells around it, but never
    /// while the user is scrolling.
    private func followCursor() {
        guard !scroll.isDragging, !scroll.isDecelerating else { return }
        let scrolls = scroll.contentSize.height > scroll.bounds.height || scroll.contentSize.width > scroll.bounds.width
        let caret = terminal.cursorFrame
        guard scrolls, caret.height > 0 else { return }
        let around = scroll.contentSize.width > scroll.bounds.width ? caret.width * 4 : 0
        scroll.scrollRectToVisible(caret.insetBy(dx: -around, dy: 0), animated: false)
        cursorWasVisible = true
    }

    // MARK: Reading back

    /// Back to the live grid, for example when the user types.
    func showLive() {
        closeReader()
        if showsScrollback { onLive() }
    }

    private func closeReader() {
        handoff = nil
        waitingForSnapshot = false
        reader.close()
        if let snapshot, snapshot != reader.loaded { reader.load(snapshot) }
    }

    private func snapshotArrived() {
        guard let snapshot else { return closeReader() }
        // A snapshot never changes under the reader: it waits for the next read.
        if reader.isReading { return }
        reader.load(snapshot)
        if waitingForSnapshot, scroll.isDragging { startReading() }
    }

    /// The reader covers the grid at its newest line; the rest of this drag scrolls it.
    private func startReading() {
        waitingForSnapshot = false
        guard reader.hasSnapshot else { return }
        handoff = scroll.panGestureRecognizer.translation(in: self).y
        reader.open()
    }

    func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
        // A drag may become a read: ask for what the pane printed since the last one.
        if outputSinceSnapshot { requestSnapshot() }
    }

    /// The first screen can come before SwiftUI sets `onNeedSnapshot`: then the next drag asks.
    private func requestSnapshot() {
        guard let onNeedSnapshot else { return }
        outputSinceSnapshot = false
        onNeedSnapshot()
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        guard scrollView.isDragging else { return }
        if let handoff {
            let pulled = scrollView.panGestureRecognizer.translation(in: self).y - handoff
            reader.show(pointsAboveNewest: pulled)
            return
        }
        guard onNeedSnapshot != nil, !isReadingBack, scrollView.contentOffset.y < -Self.pullDistance else { return }
        if reader.hasSnapshot { startReading() } else { waitingForSnapshot = true }
    }

    func scrollViewDidEndDragging(_ scrollView: UIScrollView, willDecelerate decelerate: Bool) {
        handoff = nil
        // The pull ended before a snapshot came: show the scrollback once it loads, or why not.
        if waitingForSnapshot { onScrollBack() }
        waitingForSnapshot = false
        if !decelerate { cursorWasVisible = cursorIsVisible }
    }

    func scrollViewDidEndDecelerating(_ scrollView: UIScrollView) {
        cursorWasVisible = cursorIsVisible
    }
}

/// The earlier lines over the live grid, with a Live button back to it.
private final class PaneReader: UIView {
    private var history: HistoryView?
    private let live = UIButton(configuration: .filled())
    /// The reader left its newest line once; coming back to it returns to live.
    private var leftNewest = false
    var onLive: () -> Void = {}

    private(set) var loaded: PaneHistory?
    var hasSnapshot: Bool { loaded != nil }
    var isReading: Bool { !isHidden }

    init() {
        super.init(frame: .zero)
        backgroundColor = .black
        accessibilityElementsHidden = true
        var config = UIButton.Configuration.filled()
        config.title = String(localized: "Live")
        config.image = UIImage(systemName: "arrow.down.to.line")
        config.imagePadding = 4
        config.cornerStyle = .capsule
        config.buttonSize = .small
        live.configuration = config
        live.accessibilityIdentifier = "pane-live"
        live.addAction(UIAction { [weak self] _ in self?.onLive() }, for: .primaryActionTriggered)
        live.translatesAutoresizingMaskIntoConstraints = false
        addSubview(live)
        NSLayoutConstraint.activate([
            live.trailingAnchor.constraint(equalTo: safeAreaLayoutGuide.trailingAnchor, constant: -12),
            live.bottomAnchor.constraint(equalTo: safeAreaLayoutGuide.bottomAnchor, constant: -12),
        ])
        addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(tapped)))
    }

    required init?(coder: NSCoder) { nil }

    func load(_ snapshot: PaneHistory) {
        history?.removeFromSuperview()
        let view = HistoryView(history: snapshot)
        view.accessibilityIdentifier = "pane-history"
        view.frame = bounds
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.onScroll = { [weak self] atNewest in self?.scrolled(atNewest: atNewest) }
        view.onLive = { [weak self] in self?.onLive() }
        insertSubview(view, belowSubview: live)
        history = view
        loaded = snapshot
    }

    func open() {
        leftNewest = false
        history?.show(pointsAboveNewest: 0)
        isHidden = false
        accessibilityElementsHidden = false
    }

    /// Hidden for VoiceOver too: a hidden view's own accessibility elements stay listed.
    func close() {
        isHidden = true
        accessibilityElementsHidden = true
    }

    func show(pointsAboveNewest points: CGFloat) {
        history?.show(pointsAboveNewest: points)
    }

    private func scrolled(atNewest: Bool) {
        guard isReading else { return }
        if !atNewest {
            leftNewest = true
        } else if leftNewest {
            onLive()
        }
    }

    @objc private func tapped() { onLive() }
}
