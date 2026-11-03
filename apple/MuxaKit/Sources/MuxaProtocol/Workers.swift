import Foundation

/// The reserved worker name for the workspace default agent (ADR 0027).
/// A worker-aware request sends it, or a host worker's name.
public let defaultWorker = "default"

/// Host-owned public metadata; never native args, instruction content or env.
public struct WorkerInfo: Codable, Equatable, Sendable {
    public var name: String
    public var agent: String
    public var description: String?
    public var workspaces: [String]?
    public var loop: LoopSpec?
    public init(name: String, agent: String, description: String? = nil,
                workspaces: [String]? = nil, loop: LoopSpec? = nil) {
        self.name = name
        self.agent = agent
        self.description = description
        self.workspaces = workspaces
        self.loop = loop
    }
}
