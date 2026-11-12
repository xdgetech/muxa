import MuxaProtocol
import MuxaSession
import SwiftUI

/// A `result` item's Result (ADR 0058 §7): findings, open questions, next steps and the files
/// the run wrote, then the hand-off it proposes. The summary and the links are the item's
/// own, so the user reviews it without opening the Work item.
struct ResultReviewView: View {
    @Environment(AppModel.self) private var model
    let review: ResultReview

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            lines("Findings", review.findings, id: "item-result-finding")
            lines("Open Questions", review.openQuestions, id: "item-result-question")
            lines("Next Steps", review.next, id: "item-result-next")
            if !review.files.isEmpty {
                section("Files") {
                    // Paths on the host: plain text to copy, as Work detail shows them.
                    ForEach(Array(review.files.enumerated()), id: \.offset) { i, path in
                        Text(path).font(.caption.monospaced()).foregroundStyle(.secondary).textSelection(.enabled)
                            .accessibilityIdentifier("item-result-file-\(i)")
                    }
                }
            }
            if let handoff = review.handoff {
                section("Proposed Hand-off") {
                    LabeledContent("To", value: handoff.worker)
                        .accessibilityIdentifier("item-result-handoff-to")
                    Text(handoff.purpose).textSelection(.enabled)
                        .accessibilityIdentifier("item-result-handoff-purpose")
                }
            }
            if let work = model.workItem(review.work) {
                Button(work.title, systemImage: Icon.work) { model.select(.work(review.work)) }
                    .accessibilityIdentifier("item-result-work")
            }
        }
    }

    @ViewBuilder
    private func lines(_ title: String, _ texts: [String], id: String) -> some View {
        if !texts.isEmpty {
            section(title) {
                ForEach(Array(texts.enumerated()), id: \.offset) { i, text in
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Text(verbatim: "•").foregroundStyle(.secondary).accessibilityHidden(true)
                        Text(text).textSelection(.enabled).accessibilityIdentifier("\(id)-\(i)")
                    }
                }
            }
        }
    }

    private func section(_ title: String, @ViewBuilder content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
            content()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
