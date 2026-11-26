import MuxaSession
import SwiftUI
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

/// Edit Colors (ADR 0091 amendment 1): the picked theme's base colors. The
/// edits go into a user theme that extends it; the other colors follow them.
struct ThemeColorsEditor: View {
    @State var file: ThemeFile
    var onDone: () -> Void
    @State private var typed: [ThemeToken: String] = [:]
    @State private var error: String?

    private static let tokens: [ThemeToken] = ThemeToken.allCases.filter {
        $0.isBase && $0 != .terminalBackground && $0 != .terminalForeground
    }

    var body: some View {
        let theme = ThemeStore.shared.resolve(draft: file)
        NavigationStack {
            Form {
                Section {
                    ForEach(Self.tokens, id: \.self) { row($0, theme: theme) }
                } footer: {
                    Text("Saved as \(file.name). The colors built on these follow them.")
                }
                if let error {
                    Text(error).font(.callout).foregroundStyle(Palette.late)
                }
            }
            .formStyle(.grouped)
            .groundList()
            .navigationTitle("Edit Colors")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel", action: onDone) }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save", action: save).accessibilityIdentifier("edit.save")
                }
            }
        }
        #if os(macOS)
        .frame(minWidth: 480, minHeight: 560)
        #endif
    }

    private func row(_ token: ThemeToken, theme: ResolvedTheme) -> some View {
        let overridden = file.tokens?[token.rawValue] != nil
        return HStack(spacing: 10) {
            Text(Self.title(token)).foregroundStyle(Palette.text)
            Spacer()
            TextField("#rrggbb", text: Binding(
                get: { typed[token] ?? theme[token].hex },
                set: { text in
                    typed[token] = text
                    if let rgb = RGB(hex: text.trimmingCharacters(in: .whitespaces)) { file = file.setting(token, to: rgb) }
                }))
                .font(.system(.body, design: .monospaced))
                .frame(width: 96)
                .autocorrectionDisabled()
                .accessibilityIdentifier("edit.token.\(token.rawValue)")
            ColorPicker("", selection: Binding(
                get: { Color(theme[token]) },
                set: { color in
                    guard let rgb = RGB(color) else { return }
                    typed[token] = nil
                    file = file.setting(token, to: rgb)
                }), supportsOpacity: false)
                .labelsHidden()
            Button("Reset", systemImage: "arrow.uturn.backward") {
                typed[token] = nil
                file = file.setting(token, to: nil)
            }
            .labelStyle(.iconOnly)
            .buttonStyle(.plain)
            .foregroundStyle(Palette.textSecondary)
            .opacity(overridden ? 1 : 0)
            .disabled(!overridden)
        }
    }

    private func save() {
        do {
            try ThemeStore.shared.save(file)
            onDone()
        } catch {
            self.error = error.localizedDescription
        }
    }

    private static func title(_ token: ThemeToken) -> String {
        switch token {
        case .ground: "Background"
        case .surface: "Surface"
        case .card: "Card"
        case .raised: "Raised"
        case .border: "Border"
        case .text: "Text"
        case .textSecondary: "Secondary text"
        case .textTertiary: "Tertiary text"
        case .accent: "Accent"
        case .working: "Working"
        case .needsYou: "Needs you"
        case .done: "Done"
        case .idle: "Idle"
        case .failure: "Failure"
        case .decide: "Decide"
        default: token.rawValue
        }
    }
}

extension RGB {
    /// A picked color in sRGB; components outside 0–1 are clamped.
    init?(_ color: Color) {
        #if canImport(UIKit)
        var (r, g, b, a): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
        guard UIColor(color).getRed(&r, green: &g, blue: &b, alpha: &a) else { return nil }
        #else
        guard let c = NSColor(color).usingColorSpace(.sRGB) else { return nil }
        let (r, g, b) = (c.redComponent, c.greenComponent, c.blueComponent)
        #endif
        func byte(_ x: CGFloat) -> UInt8 { UInt8((min(max(x, 0), 1) * 255).rounded()) }
        self.init(byte(r), byte(g), byte(b))
    }
}
