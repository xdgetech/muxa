import MuxaProtocol
import MuxaSession
import SwiftUI

/// One folder of the Images tree: its subfolders, then its images. Folders open
/// and close; `expanded` holds the open ones by path.
struct ImageFolderRows: View {
    let folder: ImageFolder
    @Binding var expanded: Set<String>

    var body: some View {
        ForEach(folder.folders) { sub in
            DisclosureGroup(isExpanded: binding(sub.path)) {
                ImageFolderRows(folder: sub, expanded: $expanded)
            } label: {
                HStack {
                    Label(sub.name, systemImage: Icon.repo)
                    Spacer(minLength: 4)
                    Text("\(sub.count)").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("image-folder-\(sub.path)")
            }
        }
        ForEach(folder.images) { image in
            ImageRow(image: image).tag(ReaderItem.image(image.path))
        }
    }

    private func binding(_ path: String) -> Binding<Bool> {
        Binding(
            get: { expanded.contains(path) },
            set: { open in
                if open { expanded.insert(path) } else { expanded.remove(path) }
            })
    }
}

private struct ImageRow: View {
    let image: ImageInfo

    var body: some View {
        Label {
            VStack(alignment: .leading, spacing: 1) {
                Text(ReaderScreen.name(image.path)).lineLimit(1)
                if let detail {
                    Text(detail).font(.caption).foregroundStyle(.secondary)
                }
            }
        } icon: {
            Image(systemName: Icon.image)
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("image-\(image.path)")
    }

    /// When it changed, so the latest run's screenshots stand out.
    private var detail: String? {
        guard let mtime = image.mtime else { return nil }
        return Date(timeIntervalSince1970: TimeInterval(mtime)).formatted(.relative(presentation: .named))
    }
}
