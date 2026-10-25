import CryptoKit
import Foundation
import NIOSSH

/// The device's SSH identity: one ed25519 key.
public struct ClientKey: Sendable {
    let key: Curve25519.Signing.PrivateKey

    public init() {
        key = Curve25519.Signing.PrivateKey()
    }

    public init(rawRepresentation: Data) throws {
        key = try Curve25519.Signing.PrivateKey(rawRepresentation: rawRepresentation)
    }

    public var rawRepresentation: Data { key.rawRepresentation }

    /// The line to add to `~/.ssh/authorized_keys` (see SECURITY.md for the restrictions).
    public func authorizedKey(comment: String = "muxa") -> String {
        String(openSSHPublicKey: NIOSSHPrivateKey(ed25519Key: key).publicKey) + " " + comment
    }
}

/// Keeps the client key in the Keychain, readable only on this device while it is unlocked.
/// Reconnects only happen in the foreground, so that is enough.
public enum ClientKeyStore {
    static let service = "muxa"
    static let account = "ssh-ed25519"

    /// The stored key, or a new one that is stored first.
    public static func loadOrCreate() throws -> ClientKey {
        if let data = try read() { return try ClientKey(rawRepresentation: data) }
        let key = ClientKey()
        try write(key.rawRepresentation)
        return key
    }

    private static var query: [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecUseDataProtectionKeychain as String: true,
        ]
    }

    private static func read() throws -> Data? {
        var q = query
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let status = SecItemCopyMatching(q as CFDictionary, &item)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = item as? Data else {
            throw TransportError.failed("keychain read failed (\(status))")
        }
        return data
    }

    private static func write(_ data: Data) throws {
        var q = query
        q[kSecValueData as String] = data
        q[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let status = SecItemAdd(q as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw TransportError.failed("keychain write failed (\(status))")
        }
    }
}
