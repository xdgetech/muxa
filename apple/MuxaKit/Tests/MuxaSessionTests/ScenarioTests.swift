import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// Replays recorded bridge streams (`protocol/fixtures/scenarios`) into SwiftTerm.
/// The expected rows and cursor came from tmux itself.
@MainActor
struct ScenarioTests {
    static let dir = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()
        .appendingPathComponent("../../../../protocol/fixtures/scenarios")
        .standardized

    struct Header: Decodable {
        var scenario: String
        var expectRows: [String]
        var expectCursor: [Int]
    }

    static func scenarios() throws -> [URL] {
        try FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)
            .filter { $0.pathExtension == "jsonl" }
            .sorted { $0.lastPathComponent < $1.lastPathComponent }
    }

    @Test func recordedStreamsRebuildTheExpectedScreen() throws {
        let files = try Self.scenarios()
        #expect(!files.isEmpty)
        for file in files {
            let lines = try String(contentsOf: file, encoding: .utf8).split(separator: "\n")
            let header = try JSONDecoder().decode(Header.self, from: Data(lines[0].utf8))
            let screen = HeadlessScreen()
            var watch: UInt64?
            for line in lines.dropFirst() {
                switch try Wire.decodeMessage(Data(line.utf8)) {
                case .screen(let w, _, let cols, let rows, let data):
                    watch = w
                    screen.reset(cols: Int(cols), rows: Int(rows), data: data)
                case .output(let w, _, let data) where w == watch:
                    screen.feed(data)
                default:
                    break
                }
            }
            #expect(screen.rows == header.expectRows, "\(header.scenario)")
            #expect([screen.cursor.row, screen.cursor.col] == header.expectCursor, "\(header.scenario)")
        }
    }
}
