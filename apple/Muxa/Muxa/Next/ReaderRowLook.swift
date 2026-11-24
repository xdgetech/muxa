import MuxaV2
import SwiftUI

/// The Reader's file list on the Mac: 40 pt rows (52 with a path), a 12 pt
/// inset, an icon, no lines between rows and a fill under the pointer. The
/// default list was about 24 pt a row there. iOS keeps its own list rows.
enum ReaderRowLook {
    static let iconGap: CGFloat = 10
    static let lineGap: CGFloat = 3
    #if os(macOS)
    static let pathFont = Font.system(size: 11.5)
    #else
    static let pathFont = Font.caption2
    #endif
}

extension View {
    func readerRow(lines: Int) -> some View {
        #if os(macOS)
        modifier(MacReaderRow(height: lines > 1 ? 52 : 40))
        #else
        self
        #endif
    }

    func readerRowName() -> some View {
        #if os(macOS)
        font(.system(size: 13, weight: .medium))
        #else
        self
        #endif
    }

    func readerCount() -> some View {
        #if os(macOS)
        padding(.horizontal, 7)
            .padding(.vertical, 1)
            .background(Palette.pill, in: Capsule())
        #else
        self
        #endif
    }

    /// The rows' card on iOS; on the Mac the rows sit on the panel itself.
    func readerRowBackground() -> some View {
        #if os(macOS)
        listRowBackground(Color.clear)
            .listRowSeparator(.hidden)
            .listRowInsets(EdgeInsets(top: 1, leading: 0, bottom: 1, trailing: 0))
        #else
        listRowBackground(Palette.card)
        #endif
    }
}

/// A Section's title: the folder or the branch, with its file count on the Mac.
struct ReaderSectionTitle: View {
    let title: String
    let count: Int

    var body: some View {
        #if os(macOS)
        Text("\(title) · \(count) \(count == 1 ? "file" : "files")".uppercased())
            .font(.system(size: 11, weight: .semibold))
            .tracking(0.6)
            .foregroundStyle(Palette.textTertiary)
            .padding(.leading, 12)
            .padding(.top, 14)
            .padding(.bottom, 4)
        #else
        Text(title)
        #endif
    }
}

/// A document's or an image's mark before its name; a changed file shows its
/// status letter instead. The Mac only.
struct ReaderFileIcon: View {
    let kind: ReaderKind

    var body: some View {
        #if os(macOS)
        Image(systemName: kind == .images ? "photo" : "doc.text")
            .font(.system(size: 13))
            .foregroundStyle(Palette.textSecondary)
            .frame(width: 16)
        #else
        EmptyView()
        #endif
    }
}

#if os(macOS)
private struct MacReaderRow: ViewModifier {
    let height: CGFloat
    @State private var hovering = false

    func body(content: Content) -> some View {
        content
            .padding(.horizontal, 12)
            .frame(minHeight: height)
            .background(hovering ? Palette.rowSelected : .clear, in: RoundedRectangle(cornerRadius: 8))
            .onHover { hovering = $0 }
    }
}
#endif
