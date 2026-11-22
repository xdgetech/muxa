#if canImport(UIKit)
import MuxaSession
import UIKit

/// The row of `KeyBar` keys shown above the on-screen keyboard.
@MainActor
final class KeyBarView: UIInputView {
    var onTap: (KeyBar.Key) -> Void = { _ in }
    private var buttons: [KeyBar.Key: UIButton] = [:]
    private static let height: CGFloat = 48
    // The design mocks' key row: raised keys with a thin border on the terminal's ground.
    private static let keyBack = UIColor(red: 0x1A / 255, green: 0x1E / 255, blue: 0x25 / 255, alpha: 1)
    private static let keyBorder = UIColor(red: 0x2C / 255, green: 0x32 / 255, blue: 0x3C / 255, alpha: 1)
    private static let keyText = UIColor(red: 0xE7 / 255, green: 0xE9 / 255, blue: 0xEC / 255, alpha: 1)

    init() {
        super.init(frame: CGRect(x: 0, y: 0, width: 0, height: Self.height), inputViewStyle: .default)
        allowsSelfSizing = true
        backgroundColor = PaneTerminalView.background
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
        var config = UIButton.Configuration.filled()
        config.title = title
        config.image = symbol.map { UIImage(systemName: $0) } ?? nil
        config.background.cornerRadius = 8
        config.background.strokeColor = Self.keyBorder
        config.background.strokeWidth = 1
        config.titleTextAttributesTransformer = UIConfigurationTextAttributesTransformer { attributes in
            var attributes = attributes
            attributes.font = .monospacedSystemFont(ofSize: 13, weight: .medium)
            return attributes
        }
        let button = UIButton(configuration: config, primaryAction: UIAction { [weak self] _ in self?.onTap(key) })
        // Ctrl is a toggle: filled while it waits for the next key.
        button.configurationUpdateHandler = { button in
            guard var config = button.configuration else { return }
            config.baseBackgroundColor = button.isSelected ? .tintColor : Self.keyBack
            config.baseForegroundColor = button.isSelected ? PaneTerminalView.background : Self.keyText
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
