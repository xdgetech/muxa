import MuxaV2
import SwiftUI

/// A Bot's page on iOS (ADR 0086): who it is and its status, then Home,
/// Chats, Routines, Files and Settings. Everything about the Bot stays here;
/// its conversations open on this page, not as Tasks.
struct BotPage: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let entry: BotEntry
    @State private var tab = BotTab.home

    var body: some View {
        VStack(spacing: 0) {
            header
            SegmentBar(segments: BotTab.allCases.map {
                Segment(value: $0, title: $0.title, id: "bot.tab.\($0.title.lowercased())")
            }, selection: $tab, ground: Palette.ground)
            .padding(.horizontal, 12)
            .padding(.bottom, 8)
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .background(Palette.ground)
        .navigationTitle(entry.name)
        .inlineTitle()
        .onAppear(perform: land)
        .onChange(of: tab) { _, picked in nav.botTabs[key] = picked }
        .onChange(of: nav.botTabs[key]) { _, saved in if let saved, saved != tab { tab = saved } }
    }

    private var key: String { "\(entry.workspace.name)/\(entry.name)" }

    /// The tab it was left on in this run; the first time, its live talk
    /// on Chats if it has one, else Home (ADR 0086).
    private func land() {
        if let left = nav.botTabs[key] {
            tab = left
        } else if let talk = model.liveTalk(of: entry) {
            nav.botTabs[key] = .chats
            tab = .chats
            nav.taskView = .chat
            nav.botRoute = BotRoute(workspace: entry.workspace.name, bot: entry.name, terminal: talk.id)
        } else {
            nav.botTabs[key] = .home
            tab = .home
        }
    }

    private var header: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "sparkles")
                .font(.system(size: 20, weight: .semibold))
                .foregroundStyle(entry.bot.spec.enabled ? Palette.accent : Palette.textTertiary)
                .frame(width: 48, height: 48)
                .background(Palette.accent.opacity(entry.bot.spec.enabled ? 0.16 : 0.05), in: RoundedRectangle(cornerRadius: 14))
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    let status = entry.work.status
                    Text(status.title)
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(status.badgeForeground)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 2)
                        .background(status.badgeBackground, in: RoundedRectangle(cornerRadius: 6))
                        .accessibilityIdentifier("bot.status")
                    Text("\(entry.workspace.name) · \(entry.bot.spec.dailyItems ?? 3) items a day")
                        .font(.system(size: 13))
                        .foregroundStyle(Palette.textSecondary)
                        .lineLimit(1)
                }
                Text(entry.name).font(.system(size: 22, weight: .bold)).foregroundStyle(Palette.text)
                    .accessibilityIdentifier("bot.title")
                if let d = entry.bot.spec.description {
                    Text(d).font(.system(size: 15)).foregroundStyle(Palette.textSecondary)
                }
                if model.settings.readOnly {
                    Label(SettingsModel.lockedNotice, systemImage: "lock")
                        .font(.caption)
                        .foregroundStyle(Theme.attention)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }

    @ViewBuilder
    private var content: some View {
        switch tab {
        case .home: BotHomeList(entry: entry) { tab = .chats }
        case .chats: BotChatsList(entry: entry)
        case .routines: BotRoutinesList(entry: entry)
        case .files: BotFilesList(entry: entry)
        case .settings: BotSettingsForm(entry: entry)
        }
    }
}

extension NextModel {
    /// A new chat with the Bot, opened on its page.
    func newChat(with e: BotEntry, nav: NextNavigation) async {
        if let t = await startChat(with: e) { nav.open(t) }
    }
}
