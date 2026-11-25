import Foundation
import MuxaV2
import Observation

/// The host's editable settings (ADR 0074) and who uses each connection and
/// MCP server. Secrets and env values only pass through `save`; nothing here
/// keeps or logs them.
@MainActor
@Observable
final class SettingsModel {
    private(set) var settings: Settings?
    /// The `workspaces` reply by name, for `used_by` and `last_used`.
    private(set) var usage: [String: Workspace] = [:]
    /// What each subscription's agents last reported of its plan limits (ADR 0091).
    private(set) var planUsage: [SubscriptionUsage] = []
    /// Today's tokens by Workspace and Task; nil from a host without `token-usage`.
    private(set) var tokenUsage: TokenUsage?
    /// Why the settings could not be read.
    private(set) var loadError: String?
    /// Shown above the settings until the next save: a conflict or a lock.
    var notice: String?
    /// The host answered `read-only` to a save.
    private(set) var locked = false
    private(set) var saving = false

    /// The connected client; set by `NextModel`.
    @ObservationIgnored var client: @MainActor () throws -> V2Client = { throw V2Error.lost("not connected") }

    static let conflictNotice = "The settings changed on the host. They are shown again; make your change again."
    static let lockedNotice = "The host locked its settings. Edit them on the host, or run the daemon without --settings-read-only."

    var readOnly: Bool { locked || settings?.readOnly == true }

    func workspace(_ name: String) -> SettingsWorkspace? {
        settings?.workspaces.first { $0.name == name }
    }

    func load() async {
        do {
            let c = try client()
            settings = try await c.settings()
            loadError = nil
            locked = settings?.readOnly == true
            // Usage is extra: settings still show when it fails.
            if let list = try? await c.workspaces() {
                usage = Dictionary(list.map { ($0.name, $0) }) { _, b in b }
            }
            await loadPlanUsage()
        } catch {
            loadError = NextModel.describe(error)
        }
    }

    /// Reads the plan limits again; an older host answers none.
    func loadPlanUsage() async {
        guard let c = try? client() else { return }
        planUsage = (try? await c.subscriptions()) ?? []
        // An older host refuses `token-usage`; the page then shows the limits only.
        tokenUsage = try? await c.tokenUsage()
    }

    /// The last report of subscription `name`.
    func limits(_ name: String) -> SubscriptionUsage? {
        planUsage.first { $0.name == name }
    }

    /// Sends `ops` with the current revision. Returns nil when they applied,
    /// else what to show beside the form: the server's `bad-request` message,
    /// or the conflict or lock notice after the settings were read again.
    func save(_ ops: [SettingsOp]) async -> String? {
        guard let revision = settings?.revision else { return "The settings are not loaded yet." }
        guard !readOnly else { return Self.lockedNotice }
        saving = true
        defer { saving = false }
        do {
            settings = try await client().settingsUpdate(revision: revision, ops: ops)
            notice = nil
            return nil
        } catch V2Error.refused(let code, _) where code == "conflict" {
            await load()
            notice = Self.conflictNotice
            return Self.conflictNotice
        } catch V2Error.refused(let code, _) where code == "read-only" {
            locked = true
            notice = Self.lockedNotice
            return Self.lockedNotice
        } catch {
            return NextModel.describe(error)
        }
    }

    /// A new host or the demo: nothing of the old settings carries over.
    func reset() {
        settings = nil
        usage = [:]
        planUsage = []
        loadError = nil
        notice = nil
        locked = false
    }
}
