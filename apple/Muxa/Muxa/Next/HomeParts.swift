import MuxaV2
import SwiftUI

/// The host and its link, with Disconnect: `Demo · connected`.
struct HostMenu: View {
    @Environment(NextModel.self) private var model

    var body: some View {
        Menu {
            Button("Disconnect", systemImage: "xmark.circle") { Task { await model.disconnect() } }
        } label: {
            HStack(spacing: 6) {
                StateDot(color: linked ? Theme.connected : Theme.attention, size: 7)
                Text("\(model.connectionTitle) · \(state)")
            }
            .font(.system(size: 12.5))
            .foregroundStyle(Palette.textTertiary)
            .frame(minHeight: minTouch)
            .contentShape(Rectangle())
        }
        .accessibilityIdentifier("home.host")
    }

    private var linked: Bool { model.link == .connected }

    private var state: String {
        if case .reconnecting = model.link { return "reconnecting" }
        return linked ? "connected" : "offline"
    }
}

/// Shown while the link to the host is being restored.
struct LinkBanner: View {
    @Environment(NextModel.self) private var model

    var body: some View {
        if case .reconnecting(let why) = model.link {
            Label("Reconnecting… \(why)", systemImage: "wifi.exclamationmark")
                .font(.caption)
                .foregroundStyle(Palette.textSecondary)
                .frame(maxWidth: .infinity)
                .padding(6)
                .background(Palette.surface)
        }
    }
}

/// Workspaces whose Routines the host could not read.
struct RoutineProblems: View {
    @Environment(NextModel.self) private var model

    var body: some View {
        if !model.routineProblems.isEmpty {
            Section("Cannot read") {
                ForEach(model.routineProblems, id: \.workspace) { p in
                    VStack(alignment: .leading) {
                        Text(p.workspace)
                        Text(p.reason).font(.caption).foregroundStyle(Palette.late)
                    }
                }
            }
        }
    }
}
