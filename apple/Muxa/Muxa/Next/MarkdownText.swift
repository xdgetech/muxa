import MarkdownUI
import SwiftUI

/// An agent's or the user's Markdown (an Inbox item's note, a Chat reply) in the
/// app's palette. Web links open in the browser; other links and images do
/// nothing, so a note never makes the app fetch or open what it names.
struct MarkdownText: View {
    let text: String
    /// Points; nil keeps the platform's body size.
    var size: CGFloat?
    var color: Color = Palette.text

    var body: some View {
        Markdown(text)
            .markdownTheme(MarkdownUI.Theme.muxa(size: size ?? FontProperties.defaultSize, color: color))
            .markdownImageProvider(NoImages())
            .markdownInlineImageProvider(NoImages())
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
            .environment(\.openURL, OpenURLAction { url in
                ["http", "https"].contains(url.scheme?.lowercased()) ? .systemAction : .discarded
            })
            // A container, so its paragraphs and links stay elements of their own.
            .accessibilityElement(children: .contain)
    }
}

private struct NoImages: ImageProvider, InlineImageProvider {
    func makeImage(url: URL?) -> some View { EmptyView() }

    func image(with url: URL, label: String) async throws -> Image {
        throw CancellationError()
    }
}

@MainActor
extension MarkdownUI.Theme {
    /// GitHub's layout, smaller headings for short notes, and the app's dark palette.
    fileprivate static func muxa(size: CGFloat, color: Color) -> MarkdownUI.Theme {
        MarkdownUI.Theme()
            .text {
                ForegroundColor(color)
                FontSize(size)
            }
            .code {
                FontFamilyVariant(.monospaced)
                FontSize(.em(0.88))
                BackgroundColor(Palette.raised)
            }
            .strong { FontWeight(.semibold) }
            .link { ForegroundColor(Palette.accent) }
            .heading1 { heading($0, scale: 1.4) }
            .heading2 { heading($0, scale: 1.25) }
            .heading3 { heading($0, scale: 1.1) }
            .heading4 { heading($0, scale: 1) }
            .heading5 { heading($0, scale: 1) }
            .heading6 { heading($0, scale: 1) }
            .paragraph { configuration in
                configuration.label
                    .fixedSize(horizontal: false, vertical: true)
                    .relativeLineSpacing(.em(0.25))
                    .markdownMargin(top: 0, bottom: 10)
            }
            .blockquote { configuration in
                HStack(spacing: 0) {
                    RoundedRectangle(cornerRadius: 2)
                        .fill(Palette.borderStrong)
                        .relativeFrame(width: .em(0.2))
                    configuration.label
                        .markdownTextStyle { ForegroundColor(Palette.textSecondary) }
                        .relativePadding(.horizontal, length: .em(0.8))
                }
                .fixedSize(horizontal: false, vertical: true)
                .markdownMargin(top: 0, bottom: 10)
            }
            .codeBlock { configuration in
                ScrollView(.horizontal) {
                    configuration.label
                        .fixedSize(horizontal: false, vertical: true)
                        .relativeLineSpacing(.em(0.2))
                        .markdownTextStyle {
                            FontFamilyVariant(.monospaced)
                            FontSize(.em(0.85))
                            ForegroundColor(Palette.terminalText)
                        }
                        .padding(12)
                }
                .background(Palette.terminal)
                .clipShape(RoundedRectangle(cornerRadius: 8))
                .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.border))
                .markdownMargin(top: 0, bottom: 10)
            }
            .listItem { configuration in
                configuration.label.markdownMargin(top: .em(0.2))
            }
            .table { configuration in
                configuration.label
                    .fixedSize(horizontal: false, vertical: true)
                    .markdownTableBorderStyle(.init(color: Palette.border))
                    .markdownTableBackgroundStyle(.alternatingRows(Color.clear, Palette.surface))
                    .markdownMargin(top: 0, bottom: 10)
            }
            .tableCell { configuration in
                configuration.label
                    .markdownTextStyle {
                        if configuration.row == 0 { FontWeight(.semibold) }
                    }
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.vertical, 5)
                    .padding(.horizontal, 10)
            }
            .thematicBreak {
                Divider().overlay(Palette.divider).markdownMargin(top: 12, bottom: 12)
            }
    }

    private static func heading(_ configuration: BlockConfiguration, scale: Double) -> some View {
        configuration.label
            .relativeLineSpacing(.em(0.125))
            .markdownMargin(top: 14, bottom: 8)
            .markdownTextStyle {
                FontWeight(.semibold)
                FontSize(.em(scale))
            }
    }
}
