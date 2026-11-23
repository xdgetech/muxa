import MuxaV2
import SwiftUI

/// What a Bot's item points at (ADR 0082): the pages it is about and what
/// a Decide item proposes. muxa never performs the proposal by itself.
struct ItemSources: View {
    @Environment(\.openURL) private var openURL
    let item: InboxItem

    var body: some View {
        if let links = item.links, !links.isEmpty { linkList(links) }
        if let proposal = item.proposal { proposalCard(proposal) }
    }

    private func linkList(_ links: [ItemLink]) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Links").font(.system(size: 13)).foregroundStyle(Palette.textSecondary)
            ForEach(Array(links.enumerated()), id: \.offset) { n, link in
                Button {
                    if let url = URL(string: link.url) { openURL(url) }
                } label: {
                    Label(link.title ?? link.url, systemImage: "link")
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .buttonStyle(.plain)
                .foregroundStyle(Palette.accent)
                .accessibilityIdentifier("detail.link.\(n)")
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(radius: 12, border: true)
    }

    private func proposalCard(_ p: ItemProposal) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Proposal").font(.system(size: 13)).foregroundStyle(Palette.textSecondary)
            LabeledContent("Action", value: p.action)
            LabeledContent("Target", value: p.target)
            Text(p.content)
                .font(.system(.callout, design: .monospaced))
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityIdentifier("detail.proposal")
            Text("muxa does not do this by itself. Start Task to do it, or Done to let it go.")
                .font(.caption)
                .foregroundStyle(Palette.textSecondary)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(radius: 12, border: true)
    }
}

/// "from pr-watch": the Bot that made an item, or the one a talk Task talks with.
struct BotChip: View {
    let bot: String
    var label = "from"
    var id = "item.bot"

    var body: some View {
        Label(bot, systemImage: "sparkles")
            .labelStyle(.titleAndIcon)
            .padding(.horizontal, 7)
            .padding(.vertical, 2)
            .background(Palette.accent.opacity(0.14), in: Capsule())
            .foregroundStyle(Palette.accent)
            .accessibilityLabel("\(label) \(bot)")
            .accessibilityIdentifier(id)
    }
}

extension InboxItem {
    /// Start Task's goal: the proposal first, then the note.
    var taskGoal: String? {
        guard let p = proposal else { return note }
        let proposed = "\(p.action) on \(p.target):\n\n\(p.content)"
        return [proposed, note].compactMap { $0 }.joined(separator: "\n\n")
    }
}
