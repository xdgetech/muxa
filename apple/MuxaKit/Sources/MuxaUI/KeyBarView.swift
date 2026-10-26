#if canImport(UIKit)
import MuxaSession
import UIKit

/// The row of `KeyBar` keys shown above the on-screen keyboard.
@MainActor
final class KeyBarView: UIInputView {
    var onTap: (KeyBar.Key) -> Void = { _ in }
    private var buttons: [KeyBar.Key: UIButton] = [:]
    private static let height: CGFloat = 48

    init() {
        super.init(frame: CGRect(x: 0, y: 0, width: 0, height: Self.height), inputViewStyle: .keyboard)
        allowsSelfSizing = true
        let stack = UIStackView()
        stack.axis = .horizontal
        stack.distribution = .fillEqually
        stack.spacing = 6
        stack.translatesAutoresizingMaskIntoConstraints = false
        for key in KeyBar.Key.allCases {
            let button = makeButton(key)
            buttons[key] = button
            stack.addArrangedSubview(button)
        }
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: safeAreaLayoutGuide.leadingAnchor, constant: 8),
            stack.trailingAnchor.constraint(equalTo: safeAreaLayoutGuide.trailingAnchor, constant: -8),
            stack.topAnchor.constraint(equalTo: topAnchor, constant: 6),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -6),
            heightAnchor.constraint(equalToConstant: Self.height),
        ])
    }

    required init?(coder: NSCoder) { nil }

    /// Shows whether Ctrl waits for the next key.
    func show(_ bar: KeyBar) {
        buttons[.control]?.isSelected = bar.control
    }

    private func makeButton(_ key: KeyBar.Key) -> UIButton {
        let (title, symbol, label) = Self.face(key)
        var config = UIButton.Configuration.gray()
        config.title = title
        config.image = symbol.map { UIImage(systemName: $0) } ?? nil
        config.cornerStyle = .medium
        let button = UIButton(configuration: config, primaryAction: UIAction { [weak self] _ in self?.onTap(key) })
        // Ctrl is a toggle: filled while it waits for the next key.
        button.configurationUpdateHandler = { button in
            guard var config = button.configuration else { return }
            config.baseBackgroundColor = button.isSelected ? .tintColor : nil
            config.baseForegroundColor = button.isSelected ? .white : .label
            button.configuration = config
        }
        button.accessibilityLabel = label
        button.accessibilityIdentifier = "keybar-\(key)"
        return button
    }

    private static func face(_ key: KeyBar.Key) -> (title: String?, symbol: String?, label: String) {
        switch key {
        case .escape: ("esc", nil, "Escape")
        case .tab: (nil, "arrow.right.to.line", "Tab")
        case .backTab: (nil, "arrow.left.to.line", "Shift Tab")
        case .control: ("ctrl", nil, "Control")
        case .left: (nil, "arrow.left", "Left")
        case .down: (nil, "arrow.down", "Down")
        case .up: (nil, "arrow.up", "Up")
        case .right: (nil, "arrow.right", "Right")
        case .interrupt: ("^C", nil, "Control C")
        }
    }
}
#endif
