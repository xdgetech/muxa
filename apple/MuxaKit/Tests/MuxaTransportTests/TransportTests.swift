import Foundation
import Testing

@testable import MuxaTransport

struct TransportTests {
    @Test func clientKeyRoundTripsAndFormatsForAuthorizedKeys() throws {
        let key = ClientKey()
        let again = try ClientKey(rawRepresentation: key.rawRepresentation)
        #expect(again.authorizedKey() == key.authorizedKey())
        #expect(key.authorizedKey().hasPrefix("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5"))
        #expect(key.authorizedKey().hasSuffix(" muxa"))
    }

    @Test func hostKeyFingerprintMatchesSshKeygen() throws {
        // A throwaway key; the expected value is what `ssh-keygen -lf` printed for it.
        let key = try HostKey(openSSH: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICmVLvUbABi5WZoMHwkY/jy0DxNDGNMw2TRDgtxMPf9x test")
        #expect(key.openSSH == "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICmVLvUbABi5WZoMHwkY/jy0DxNDGNMw2TRDgtxMPf9x")
        #expect(key.fingerprint == "SHA256:r6zfLbrOjsc3OA59oKVabd7VJ5ZMt25hyJkS3EJEuGo")
    }

    #if os(macOS)
    @Test func processBytesCarriesBytesBothWays() async throws {
        let process = try ProcessBytes(command: "cat")
        try await process.send(Data("ping\n".utf8))
        var it = process.bytes.makeAsyncIterator()
        let chunk = try await it.next()
        #expect(chunk.map { String(decoding: $0, as: UTF8.self) } == "ping\n")
        await process.close()
    }
    #endif
}

#if os(macOS)
struct KeySetupTests {
    /// Runs the setup script the way sshd would: through the user's shell, in a fresh HOME.
    private func run(_ script: String, shell: String, home: URL) throws -> String {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: shell)
        p.arguments = ["-c", script]
        p.environment = ["HOME": home.path, "PATH": "/usr/bin:/bin"]
        let out = Pipe()
        p.standardOutput = out
        try p.run()
        p.waitUntilExit()
        #expect(p.terminationStatus == 0)
        return String(decoding: out.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
    }

    private func tempHome() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("muxa-keysetup-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    @Test func lineIsRestrictedToTheBridge() throws {
        let line = try KeySetup.authorizedLine(for: ClientKey(), command: "~/.local/bin/muxa-bridge", comment: "muxa-ipad")
        #expect(line.hasPrefix(#"command="~/.local/bin/muxa-bridge",no-pty,no-port-forwarding,no-agent-forwarding,no-X11-forwarding ssh-ed25519 "#))
        #expect(line.hasSuffix(" muxa-ipad"))
        #expect(throws: TransportError.self) {
            _ = try KeySetup.authorizedLine(for: ClientKey(), command: #"x" ,permitopen="*"#, comment: "c")
        }
    }

    @Test(arguments: ["/bin/sh", "/bin/zsh", "/bin/bash"])
    func scriptAddsOnceAndKeepsExistingLines(shell: String) throws {
        let home = try tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        let line = #"command="it's ~/x",no-pty ssh-ed25519 AAAA muxa-ipad"#
        let script = KeySetup.script(appending: line)

        #expect(try KeySetup.outcome(fromOutput: run(script, shell: shell, home: home)) == .added)
        let ssh = home.appendingPathComponent(".ssh")
        let file = ssh.appendingPathComponent("authorized_keys")
        let fm = FileManager.default
        #expect(try fm.attributesOfItem(atPath: ssh.path)[.posixPermissions] as? Int == 0o700)
        #expect(try fm.attributesOfItem(atPath: file.path)[.posixPermissions] as? Int == 0o600)
        #expect(try String(contentsOf: file, encoding: .utf8) == line + "\n")

        // An existing file without a final newline keeps its last line intact.
        try Data("ssh-ed25519 BBBB other".utf8).write(to: file)
        #expect(try KeySetup.outcome(fromOutput: run(script, shell: shell, home: home)) == .added)
        #expect(try KeySetup.outcome(fromOutput: run(script, shell: shell, home: home)) == .alreadyPresent)
        #expect(try String(contentsOf: file, encoding: .utf8) == "ssh-ed25519 BBBB other\n" + line + "\n")
    }
}
#endif
