import MuxaSession
import SwiftUI
import UniformTypeIdentifiers

/// Settings › Terminal on this device: the colors, imported from iTerm2 or
/// Ghostty or built in, the font and, on the Mac, its size, with a preview.
struct TerminalSettingsForm: View {
    @State private var importing = false
    @State private var importError: String?
    private var settings: TerminalSettings { .shared }

    var body: some View {
        Form {
            Section {
                TerminalPreview(settings: settings)
                    .listRowInsets(EdgeInsets())
            }
            Section {
                ForEach(settings.themes) { theme in themeRow(theme) }
                Button("Import Theme…", systemImage: "square.and.arrow.down") { importing = true }
                    .accessibilityIdentifier("terminal.import")
                if let importError {
                    Text(importError).font(.callout).foregroundStyle(Palette.late)
                }
            } header: {
                Text("Colors")
            } footer: {
                Text("Import an iTerm2 color preset (.itermcolors) or a Ghostty theme file.")
            }
            Section("Font") {
                Picker("Font", selection: Binding(get: { settings.fontName }, set: { settings.setFont($0) })) {
                    ForEach(Self.fonts, id: \.name) { Text($0.title).tag($0.name) }
                }
                .accessibilityIdentifier("terminal.font")
                #if os(macOS)
                Stepper(value: Binding(get: { settings.fontSize }, set: { settings.setSize($0) }),
                        in: TerminalSettings.sizes, step: 1) {
                    LabeledContent("Size", value: "\(Int(settings.fontSize)) pt")
                }
                .accessibilityIdentifier("terminal.size")
                #endif
            }
        }
        .formStyle(.grouped)
        .fileImporter(isPresented: $importing, allowedContentTypes: [.item]) { result in
            importError = nil
            do {
                let url = try result.get()
                let scoped = url.startAccessingSecurityScopedResource()
                defer { if scoped { url.stopAccessingSecurityScopedResource() } }
                try settings.importTheme(from: url)
            } catch {
                importError = error.localizedDescription
            }
        }
    }

    private func themeRow(_ theme: TerminalTheme) -> some View {
        let picked = settings.themeID == theme.id
        return HStack(spacing: 10) {
            Button { settings.pick(theme) } label: {
                HStack(spacing: 10) {
                    Swatch(theme: theme)
                    Text(theme.name).foregroundStyle(Palette.text)
                    Spacer()
                    if picked { Image(systemName: "checkmark").foregroundStyle(Palette.accent) }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityAddTraits(picked ? .isSelected : [])
            .accessibilityIdentifier("terminal.theme.\(theme.id)")
            if !TerminalTheme.builtIn.contains(where: { $0.id == theme.id }) {
                Button("Remove", systemImage: "trash", role: .destructive) { settings.remove(theme) }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.plain)
                    .foregroundStyle(Palette.textSecondary)
            }
        }
    }

    /// The bundled font first: it has the icons. Then, on the Mac, the
    /// monospaced fonts installed here.
    private static var fonts: [(name: String, title: String)] {
        var list = [(TerminalSettings.bundledFont, "JetBrains Mono"), ("", "System Monospace (no icons)")]
        #if os(macOS)
        let manager = NSFontManager.shared
        let families = Set((manager.availableFontNames(with: .fixedPitchFontMask) ?? [])
            .compactMap { NSFont(name: $0, size: 13)?.familyName })
            .filter { !$0.hasPrefix(".") && $0 != "JetBrains Mono" }
        for family in families.sorted() {
            if let regular = manager.font(withFamily: family, traits: [], weight: 5, size: 13) {
                list.append((regular.fontName, family))
            }
        }
        #endif
        return list
    }
}

/// The background with the first eight ANSI colors on it.
private struct Swatch: View {
    let theme: TerminalTheme

    var body: some View {
        HStack(spacing: 2) {
            ForEach(Array((theme.ansi ?? []).prefix(8).enumerated()), id: \.offset) { _, c in
                Circle().fill(Color(c)).frame(width: 6, height: 6)
            }
        }
        .padding(.horizontal, 6)
        .frame(width: 76, height: 22, alignment: .leading)
        .background(Color(theme.background), in: RoundedRectangle(cornerRadius: 5))
        .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(Palette.borderStrong))
        .accessibilityHidden(true)
    }
}

/// A prompt as the terminal draws it: the picked font, its icons and colors.
private struct TerminalPreview: View {
    let settings: TerminalSettings

    var body: some View {
        let theme = settings.theme
        let ansi = theme.ansi ?? []
        func c(_ n: Int) -> Color { ansi.indices.contains(n) ? Color(ansi[n]) : Color(theme.foreground) }
        return VStack(alignment: .leading, spacing: 4) {
            (Text("\u{f09b} ").foregroundColor(c(4)) + Text("muxa ").foregroundColor(c(6))
                + Text("\u{e0a0} main").foregroundColor(c(5)) + Text(" ❯ ").foregroundColor(c(2))
                + Text("git status").foregroundColor(Color(theme.foreground)))
            (Text("modified: ").foregroundColor(c(1)) + Text("src/app.ts").foregroundColor(Color(theme.foreground)))
            (Text("\u{f00c} 12 passed  ").foregroundColor(c(2)) + Text("\u{f071} 1 warning").foregroundColor(c(3)))
        }
        .font(previewFont)
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(theme.background))
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("terminal.preview")
    }

    private var previewFont: Font {
        settings.font(size: 13).map { Font($0) } ?? .system(size: 13, design: .monospaced)
    }
}

private extension Color {
    init(_ c: RGB) {
        self.init(red: Double(c.r) / 255, green: Double(c.g) / 255, blue: Double(c.b) / 255)
    }
}
