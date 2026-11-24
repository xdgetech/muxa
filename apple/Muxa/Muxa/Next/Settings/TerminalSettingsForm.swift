import MuxaSession
import SwiftUI
import UniformTypeIdentifiers

/// Settings › Appearance on this device: the theme, which colors the whole app
/// (ADR 0091), then the terminal's font and, on the Mac, its size, with a preview.
struct TerminalSettingsForm: View {
    @State private var importing = false
    @State private var importError: String?
    @State private var editing: ThemeFile?
    private var settings: TerminalSettings { .shared }
    private var themes: ThemeStore { .shared }

    var body: some View {
        Form {
            Section {
                TerminalPreview(settings: settings, theme: themes.theme)
                    .listRowInsets(EdgeInsets())
            }
            Section {
                automaticRow
                ForEach(themes.files) { file in themeRow(themes.resolve(file.id)) }
                Button("Edit Colors…", systemImage: "paintpalette") { editing = themes.editable(themes.theme.id) }
                    .accessibilityIdentifier("appearance.edit")
                Button("Import Theme…", systemImage: "square.and.arrow.down") { importing = true }
                    .accessibilityIdentifier("terminal.import")
                if let importError {
                    Text(importError).font(.callout).foregroundStyle(Palette.late)
                }
            } header: {
                Text("Theme")
            } footer: {
                Text("The theme colors the whole app and its terminals. Automatic follows the system's light or dark mode. Import a muxa theme (.json), an iTerm2 color preset (.itermcolors) or a Ghostty theme file.")
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
        .groundList()
        .sheet(item: $editing) { file in ThemeColorsEditor(file: file) { editing = nil } }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.item]) { result in
            importError = nil
            do {
                let url = try result.get()
                let scoped = url.startAccessingSecurityScopedResource()
                defer { if scoped { url.stopAccessingSecurityScopedResource() } }
                try themes.importTheme(from: url)
            } catch {
                importError = error.localizedDescription
            }
        }
    }

    private var automaticRow: some View {
        let picked = themes.pick == ThemeStore.automatic
        return Button { themes.choose(ThemeStore.automatic) } label: {
            HStack(spacing: 10) {
                HStack(spacing: 2) {
                    Swatch(theme: themes.resolve("muxa-light"), width: 37)
                    Swatch(theme: themes.resolve(ThemeRegistry.defaultID), width: 37)
                }
                Text("Automatic").foregroundStyle(Palette.text)
                Spacer()
                if picked { Image(systemName: "checkmark").foregroundStyle(Palette.accent) }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(picked ? .isSelected : [])
        .accessibilityIdentifier("terminal.theme.automatic")
    }

    private func themeRow(_ theme: ResolvedTheme) -> some View {
        // Under Automatic no single theme is the pick.
        let picked = themes.pick != ThemeStore.automatic && themes.theme.id == theme.id
        return HStack(spacing: 10) {
            Button { themes.pick(theme.id) } label: {
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
            if !themes.isBuiltIn(theme.id) {
                Button("Remove", systemImage: "trash", role: .destructive) { themes.remove(theme.id) }
                    .accessibilityIdentifier("terminal.remove.\(theme.id)")
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

/// The theme's ground with its text, accent and state colors on it.
private struct Swatch: View {
    let theme: ResolvedTheme
    var width: CGFloat = 76

    var body: some View {
        HStack(spacing: 3) {
            RoundedRectangle(cornerRadius: 1.5).fill(Color(theme[.text])).frame(width: 14, height: 3)
            ForEach([ThemeToken.accent, .working, .needsYou, .failure, .decide], id: \.self) { token in
                Circle().fill(Color(theme[token])).frame(width: 6, height: 6)
            }
        }
        .padding(.horizontal, 6)
        .frame(width: width, height: 22, alignment: .leading)
        .clipped()
        .background(Color(theme[.ground]), in: RoundedRectangle(cornerRadius: 5))
        .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(Color(theme[.border])))
        .accessibilityHidden(true)
    }
}

/// A prompt as the terminal draws it: the picked font, its icons and colors.
private struct TerminalPreview: View {
    let settings: TerminalSettings
    let theme: ResolvedTheme

    var body: some View {
        let theme = theme.terminal
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
