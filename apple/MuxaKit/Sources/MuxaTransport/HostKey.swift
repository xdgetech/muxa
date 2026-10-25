import CryptoKit
import Foundation
import NIOSSH

/// An SSH public key in OpenSSH text form (`ssh-ed25519 AAAA…`).
public struct HostKey: Codable, Hashable, Sendable, Identifiable {
    public let openSSH: String
    public var id: String { openSSH }

    public init(openSSH: String) throws {
        let parsed = try NIOSSHPublicKey(openSSHPublicKey: openSSH)
        self.openSSH = String(openSSHPublicKey: parsed)
    }

    init(_ key: NIOSSHPublicKey) {
        openSSH = String(openSSHPublicKey: key)
    }

    /// `SHA256:…`, as `ssh-keygen -lf` prints it.
    public var fingerprint: String {
        let blob = openSSH.split(separator: " ").dropFirst().first.flatMap { Data(base64Encoded: String($0)) }
        let digest = SHA256.hash(data: blob ?? Data())
        let text = Data(digest).base64EncodedString().replacingOccurrences(of: "=", with: "")
        return "SHA256:" + text
    }
}
