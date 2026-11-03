import MuxaProtocol

extension WorkerInfo {
    public func allows(workspace: String) -> Bool {
        workspaces?.contains(workspace) ?? true
    }
}

extension WorkerAssignment {
    public var displayName: String {
        guard isKnown else { return "Unknown worker assignment" }
        return kind == "workspace-default" ? "Workspace default" : name ?? "Unknown worker assignment"
    }
}

/// An open sheet may retain a selection after the host changes its definitions.
public func workerProblem(name: String?, workspace: String, workers: [WorkerInfo]) -> String? {
    guard let name else { return nil }
    return workers.contains { $0.name == name && $0.allows(workspace: workspace) } ? nil
        : "This worker is no longer available in this workspace. Pick a worker again."
}
