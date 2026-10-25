import Foundation
import Testing

@testable import MuxaTransport

struct TransportTests {
    @Test func framerSplitsLinesAcrossChunks() throws {
        var f = LineFramer()
        #expect(try f.push(Data("{\"a\":".utf8)).isEmpty)
        let lines = try f.push(Data("1}\n{\"b\":2}\n{\"c\"".utf8))
        #expect(lines.map { String(decoding: $0, as: UTF8.self) } == [#"{"a":1}"#, #"{"b":2}"#])
        #expect(try f.push(Data(":3}\n".utf8)).map { String(decoding: $0, as: UTF8.self) } == [#"{"c":3}"#])
    }

    @Test func framerRejectsAnEndlessLine() {
        var f = LineFramer()
        #expect(throws: TransportError.lineTooLong) {
            _ = try f.push(Data(count: LineFramer.maxLine + 1))
        }
    }

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

    @Test func processBridgeCarriesLinesBothWays() async throws {
        let bridge = try ProcessBridge(command: "cat")
        try await bridge.send(Data(#"{"type":"ping","id":1}"#.utf8))
        var it = bridge.lines.makeAsyncIterator()
        let line = try await it.next()
        #expect(line.map { String(decoding: $0, as: UTF8.self) } == #"{"type":"ping","id":1}"#)
        await bridge.close()
    }
}
