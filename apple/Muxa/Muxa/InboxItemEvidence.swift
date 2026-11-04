import MuxaProtocol
import MuxaSession
import SwiftUI

/// The item's summary as Markdown (ADR 0033 §4a.5). Web links open in the browser;
/// a link or file name of an attachment opens it in the Reader, which returns to the item.
struct ItemSummaryText: View {
    @Environment(AppModel.self) private var model
    let item: InboxItem
    let summary: String

    var body: some View {
        Text(ItemSummary.attributed(summary, attachments: item.attachments))
            .textSelection(.enabled)
            .environment(\.openURL, OpenURLAction { url in
                switch SummaryLink(url) {
                case .web?: return .systemAction
                case .doc(let path)?: return open(.doc(path))
                case .image(let path)?: return open(.image(path))
                case nil: return .discarded
                }
            })
            .accessibilityIdentifier("item-summary")
    }

    private func open(_ opening: AppModel.TaskOpening) -> OpenURLAction.Result {
        guard model.canOpenEvidence(item) else { return .discarded }
        model.open(item, as: opening)
        return .handled
    }
}

/// The item's evidence as a list of named rows. Changes, docs and images open in
/// the Reader for the item's pane and return to the item; links open in the browser.
struct ItemEvidence: View {
    @Environment(AppModel.self) private var model
    let item: InboxItem

    var body: some View {
        let a = item.attachments
        if !a.isEmpty {
            let readable = model.canOpenEvidence(item)
            VStack(alignment: .leading, spacing: 6) {
                Text("Attachments").font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
                VStack(spacing: 0) {
                    if let c = a.changes {
                        row("Changes", detail: Self.counts(c), Icon.changes, id: "changes") { model.open(item, as: .changes) }
                            .disabled(!readable || !model.session.canShowChanges)
                    }
                    ForEach(a.docs, id: \.self) { path in
                        row(Self.name(path), detail: Self.folder(path), Icon.document, id: "doc-\(path)") {
                            model.open(item, as: .doc(path))
                        }
                        .disabled(!readable)
                    }
                    ForEach(a.images, id: \.self) { path in
                        row(Self.name(path), detail: Self.folder(path), Icon.image, id: "image-\(path)") {
                            model.open(item, as: .image(path))
                        }
                        .disabled(!readable || !model.session.canShowImages)
                    }
                    ForEach(a.links, id: \.url) { link in
                        if let url = URL(string: link.url) {
                            Link(destination: url) {
                                label(link.title ?? link.url, detail: link.title == nil ? nil : url.host(), "link",
                                      trailing: "arrow.up.right")
                            }
                            .accessibilityIdentifier("item-evidence-link")
                        }
                    }
                }
                .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 10))
            }
            // A container of its own, so the rows keep their identifiers.
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("item-evidence")
        }
    }

    private func row(_ title: String, detail: String?, _ icon: String, id: String, action: @escaping () -> Void) -> some View {
        Button(action: action) { label(title, detail: detail, icon, trailing: "chevron.right") }
            .accessibilityIdentifier("item-evidence-\(id)")
    }

    private func label(_ title: String, detail: String?, _ icon: String, trailing: String) -> some View {
        HStack(spacing: 12) {
            Image(systemName: icon).frame(width: 22).foregroundStyle(.tint)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).foregroundStyle(.primary).lineLimit(1).truncationMode(.middle)
                if let detail, !detail.isEmpty {
                    Text(detail).font(.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.head)
                }
            }
            Spacer(minLength: 0)
            Image(systemName: trailing).font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
    }

    private static func counts(_ c: ChangeCount) -> String {
        "\(c.files) \(c.files == 1 ? "file" : "files") +\(c.added) −\(c.removed)"
    }

    static func name(_ path: String) -> String {
        path.split(separator: "/").last.map(String.init) ?? path
    }

    /// The folder the file is in, relative to the pane's directory.
    private static func folder(_ path: String) -> String? {
        let parts = path.split(separator: "/")
        return parts.count > 1 ? parts.dropLast().joined(separator: "/") : nil
    }
}
