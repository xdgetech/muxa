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
    /// A Form or List on the mocks' ground, or on `formGround` where a pane sets it.
    func groundList() -> some View {
        modifier(GroundList())
    }
}

private struct GroundList: ViewModifier {
    @Environment(\.formGround) private var ground

    func body(content: Content) -> some View {
        content.scrollContentBackground(.hidden).background(ground)
    }
}

private struct FormGroundKey: EnvironmentKey {
    static let defaultValue = Palette.ground
}

extension EnvironmentValues {
    /// What a Form is drawn on: the Mac's Settings pane is lighter than the ground.
    var formGround: Color {
        get { self[FormGroundKey.self] }
        set { self[FormGroundKey.self] = newValue }
    }
}
