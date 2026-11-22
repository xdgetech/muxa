import MuxaSession
import SwiftUI

/// The saved hosts: connect, edit and add. The connect screen and the
/// iPhone's Settings tab show it, and present the editor for `editing`: a
/// sheet on a List's section does not open.
struct HostsSection: View {
    @Environment(NextModel.self) private var model
    @Binding var editing: HostDraft?

    var body: some View {
        Section("Hosts") {
            ForEach(model.hosts.hosts) { host in
                HStack {
                    Button(host.title) { Task { await model.connect(to: host.id) } }
                        .foregroundStyle(Palette.text)
                    Spacer()
                    Button("Edit", systemImage: "info.circle") { editing = HostDraft(host) }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.borderless)
                        .frame(minWidth: minTouch, minHeight: minTouch)
                }
            }
            Button("Add Host", systemImage: "plus") { editing = HostDraft() }
                .accessibilityIdentifier("next.addhost")
        }
        .listRowBackground(Palette.card)
    }
}

extension View {
    /// A Form or List on the mocks' ground.
    func groundList() -> some View {
        scrollContentBackground(.hidden).background(Palette.ground)
    }
}
