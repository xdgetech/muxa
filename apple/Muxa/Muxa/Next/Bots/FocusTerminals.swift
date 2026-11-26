import MuxaV2

/// What Focus lists on the Mac: a Bot's talks, runs and members stay on its
/// page (ADR 0086), so Focus leaves them out.
extension NextModel {
    var focusTasks: [Terminal] { (liveTasks + pausedTasks).filter { !$0.belongsToBot } }
    var focusNeedsYou: [Terminal] { needsYou.filter { !$0.belongsToBot } }
    var focusOthers: [Terminal] { otherTerminals.filter { !$0.belongsToBot } }
}
