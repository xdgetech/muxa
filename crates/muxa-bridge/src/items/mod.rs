//! Inbox items (ADR 0033): one stored question for the user each. `produce`
//! makes and resolves a Task's items from adapter states, `answer` carries
//! out `update-inbox` on them, `delete` `delete-inbox`. A Routine's entries
//! are listed as items too.
pub mod answer;
pub mod ask_agent;
pub mod delete;
pub mod produce;
pub mod store;

use muxa_protocol::{
    Attachments, Disposition, ErrorBody, ErrorCode, InboxAction, InboxEntry, InboxEntryKind,
    InboxItem, ItemKind, ItemState, PromptKind,
};

use crate::convert::error_body;
use crate::task_records::Config;
use store::ItemStore;

/// Closed items one `inbox-list` carries, newest first.
const MAX_CLOSED: usize = 100;

/// The answers the host accepts for an item now, in the order to show them.
/// `read` is always accepted and never listed.
pub fn answers(item: &InboxItem) -> Vec<InboxAction> {
    use InboxAction::*;
    if matches!(item.state, ItemState::Answered | ItemState::Resolved) {
        return Vec::new();
    }
    let mut out = match item.kind {
        ItemKind::Ask => match &item.prompt {
            Some(p) => {
                let mut a = Vec::new();
                if p.kind == PromptKind::Question {
                    a.push(Choose);
                } else if p.options.iter().any(|o| !o.cancel) {
                    a.push(Approve);
                }
                if p.options.iter().any(|o| o.cancel) {
                    a.push(Reject);
                    // Claude Code takes a denied permission's turn as ended,
                    // so the text arrives as the next prompt (docs/agents.md).
                    if p.kind == PromptKind::Permission {
                        a.push(Reply);
                    }
                }
                a
            }
            // A menu the adapter cannot read is answered in the terminal.
            None => vec![Dismiss],
        },
        ItemKind::Review => vec![Accept, Reply, KeepOpen],
        // Retry and Reply reach the agent: they need its session.
        ItemKind::Failure if item.session.is_none() => vec![Drop, Dismiss],
        ItemKind::Failure => vec![Retry, Reply, Drop, Dismiss],
        // A reading Task changed its main checkout (ADR 0034 §2).
        ItemKind::Proposal if item.routine.is_none() && item.task.is_some() => vec![Branch, Keep],
        ItemKind::Proposal => vec![Approve, Reject],
        ItemKind::Briefing => vec![Dismiss],
    };
    // An item with a session sends its text to its own agent (ADR 0040 §4).
    if item.session.is_none() {
        out.push(AskAgent);
    }
    out.push(Later);
    out
}

/// A Routine's entry as an item. Its id stays the entry's.
pub fn from_entry(entry: &InboxEntry) -> InboxItem {
    let (state, answer) = match entry.disposition {
        Disposition::Open => (ItemState::Open, None),
        // Stored before ADR 0040; `shown` lists it as moved down.
        Disposition::Later => (ItemState::Open, None),
        Disposition::Approved => (ItemState::Answered, Some(InboxAction::Approve)),
        Disposition::Dismissed => (ItemState::Answered, Some(InboxAction::Dismiss)),
    };
    let mut item = InboxItem {
        id: entry.id.clone(),
        kind: match entry.kind {
            InboxEntryKind::Briefing => ItemKind::Briefing,
            InboxEntryKind::Proposal => ItemKind::Proposal,
        },
        state,
        question: entry.title.clone(),
        workspace: entry.workspace.clone(),
        title: entry.title.clone(),
        created_at: entry.created_at,
        read_at: entry.read_at,
        later_at: entry
            .later_at
            .or((entry.disposition == Disposition::Later).then_some(entry.created_at)),
        until: entry.until,
        protected_at: entry.protected_at,
        answered_at: None,
        answer,
        summary: Some(entry.body.clone()).filter(|b| !b.is_empty()),
        task: entry.task.clone(),
        session: None,
        pane: None,
        prompt: None,
        attachments: Attachments {
            links: entry.links.clone(),
            ..Attachments::default()
        },
        routine: Some(entry.routine.clone()),
        proposal: entry.proposal.clone(),
        answers: Vec::new(),
    };
    item.answers = answers(&item);
    item
}

/// The item as clients see it, with its answers. A `later` item stored before
/// ADR 0040 is open and moved down.
pub fn shown(item: &InboxItem) -> InboxItem {
    let mut i = item.clone();
    if i.state == ItemState::Later {
        i.state = ItemState::Open;
        i.later_at.get_or_insert(i.created_at);
        i.until = None;
    }
    i.answers = answers(&i);
    i
}

/// Blocking. Every item: open ones (`ask` first, then newest), then later
/// ones, then the newest closed ones. `entries` are the Routines' (already shown).
pub fn list(config: &Config, entries: &[InboxEntry]) -> Result<Vec<InboxItem>, ErrorBody> {
    let mut items: Vec<InboxItem> = entries.iter().map(from_entry).collect();
    if let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) {
        let files = ItemStore::open(dir).and_then(|s| s.all()).map_err(|e| {
            tracing::warn!(error = %e, "cannot list inbox items");
            error_body(ErrorCode::Internal, e.to_string())
        })?;
        items.extend(
            files
                .iter()
                .flat_map(|t| t.items.iter().map(|s| shown(&s.item))),
        );
    }
    Ok(order(items))
}

/// Blocking. The docs and images the stored items of pane `pane` (`%n`)
/// attach, each once: what a pane outside a repo serves besides a Task folder.
pub fn evidence_paths(config: &Config, pane: &str) -> Vec<String> {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return Vec::new();
    };
    let files = match ItemStore::open(dir).and_then(|s| s.all()) {
        Ok(files) => files,
        Err(e) => {
            tracing::warn!(error = %e, "cannot read inbox items for a pane's files");
            return Vec::new();
        }
    };
    let mut paths: Vec<String> = files
        .iter()
        .flat_map(|t| &t.items)
        .filter(|s| s.item.pane.as_deref() == Some(pane))
        .flat_map(|s| {
            s.item
                .attachments
                .docs
                .iter()
                .chain(&s.item.attachments.images)
        })
        .cloned()
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

fn order(mut items: Vec<InboxItem>) -> Vec<InboxItem> {
    // Open and not moved down: `ask` first, then newest. Moved down: in the
    // order they were moved (ADR 0040). Then the newest closed ones.
    let rank = |i: &InboxItem| match (i.state, i.kind, i.later_at) {
        (ItemState::Open, ItemKind::Ask, None) => (0, 0),
        (ItemState::Open, _, None) => (1, 0),
        (ItemState::Open, _, Some(at)) => (2, at),
        _ => (3, 0),
    };
    items.sort_by(|a, b| (rank(a), b.created_at, &b.id).cmp(&(rank(b), a.created_at, &a.id)));
    let open = items.iter().filter(|i| rank(i).0 < 3).count();
    items.truncate(open + MAX_CLOSED);
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use muxa_protocol::{ChatPrompt, PromptOption};

    fn item(kind: ItemKind, state: ItemState, created_at: u64) -> InboxItem {
        let mut i = store::tests_item(&format!("i-{created_at:08x}"), state);
        i.kind = kind;
        i.created_at = created_at;
        i
    }

    fn moved_down(kind: ItemKind, created_at: u64, later_at: u64) -> InboxItem {
        let mut i = item(kind, ItemState::Open, created_at);
        i.later_at = Some(later_at);
        i
    }

    #[test]
    fn asks_come_first_then_newest_then_moved_down_then_closed() {
        let ordered = order(vec![
            item(ItemKind::Review, ItemState::Open, 1),
            item(ItemKind::Review, ItemState::Answered, 9),
            moved_down(ItemKind::Review, 8, 20),
            item(ItemKind::Ask, ItemState::Open, 2),
            moved_down(ItemKind::Ask, 7, 10),
            item(ItemKind::Failure, ItemState::Open, 3),
        ]);
        let ids: Vec<u64> = ordered.iter().map(|i| i.created_at).collect();
        assert_eq!(
            ids,
            [2, 3, 1, 7, 8, 9],
            "moved down in the order they were moved"
        );
    }

    #[test]
    fn answers_follow_the_kind_and_the_prompt() {
        use InboxAction::*;
        let mut ask = item(ItemKind::Ask, ItemState::Open, 1);
        assert_eq!(answers(&ask), [Dismiss, AskAgent, Later]);
        ask.prompt = Some(ChatPrompt {
            entry: "toolu_1".into(),
            kind: PromptKind::Permission,
            question: None,
            options: vec![
                PromptOption {
                    label: "Yes".into(),
                    keys: vec!["1".into()],
                    cancel: false,
                },
                PromptOption {
                    label: "No".into(),
                    keys: vec!["Escape".into()],
                    cancel: true,
                },
            ],
        });
        assert_eq!(answers(&ask), [Approve, Reject, Reply, AskAgent, Later]);
        if let Some(p) = ask.prompt.as_mut() {
            p.kind = PromptKind::Plan;
        }
        assert_eq!(
            answers(&ask),
            [Approve, Reject, AskAgent, Later],
            "only a permission"
        );
        assert_eq!(
            answers(&item(ItemKind::Review, ItemState::Open, 1)),
            [Accept, Reply, KeepOpen, AskAgent, Later]
        );
        // An agent's failure has its session; a failed start may not.
        let mut failure = item(ItemKind::Failure, ItemState::Open, 1);
        assert_eq!(answers(&failure), [Drop, Dismiss, AskAgent, Later]);
        failure.session = Some("$1".into());
        assert_eq!(answers(&failure), [Retry, Reply, Drop, Dismiss, Later]);
        assert!(answers(&item(ItemKind::Review, ItemState::Resolved, 1)).is_empty());
    }

    #[test]
    fn a_task_proposal_takes_branch_and_keep() {
        use InboxAction::*;
        let mut changed = item(ItemKind::Proposal, ItemState::Open, 1);
        changed.task = Some("t-000001".into());
        assert_eq!(answers(&changed), [Branch, Keep, AskAgent, Later]);
        changed.routine = Some("Morning".into());
        assert_eq!(
            answers(&changed),
            [Approve, Reject, AskAgent, Later],
            "a Routine's proposal"
        );
    }

    #[test]
    fn a_later_item_from_before_adr_0040_is_open_and_moved_down() {
        let mut i = item(ItemKind::Review, ItemState::Later, 5);
        i.until = Some(4_000_000_000);
        let s = shown(&i);
        assert_eq!(
            (s.state, s.later_at, s.until),
            (ItemState::Open, Some(5), None)
        );
        assert!(s.answers.contains(&InboxAction::Later));
    }
}
