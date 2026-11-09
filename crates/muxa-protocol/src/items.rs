//! Inbox items: one stored question for the user each (ADR 0033). The host
//! lists them with `list-inbox` and answers them with `update-inbox`, at
//! `work` level 3 or higher.
use serde::{Deserialize, Serialize};

use crate::{ChatPrompt, InboxAction, Proposal, SourceLink};

/// Level 3: Inbox items (`items` in `inbox-list`, the answers of ADR 0033).
pub const WORK_ITEMS: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxItem {
    /// `i-` and 8 hex digits for a Task's item; a Routine's entry keeps its `e-` id.
    pub id: String,
    pub kind: ItemKind,
    pub state: ItemState,
    /// One line: what the user must decide.
    pub question: String,
    pub workspace: String,
    /// The Task's title, or the entry's title for a Routine's item.
    pub title: String,
    /// Epoch seconds.
    pub created_at: u64,
    /// Set by the `read` answer. Absent: unread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<u64>,
    /// Set by `later`: the item is listed below the others, in the order they
    /// were moved down (ADR 0040).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub later_at: Option<u64>,
    /// Only on an item stored before ADR 0040: when a hidden item was open again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<u64>,
    /// Set by `protect`: `delete-inbox` keeps the item (ADR 0046).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_at: Option<u64>,
    /// With `answered`: the answer given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<InboxAction>,
    /// The agent's last message for the turn, or a Routine entry's body.
    /// At most 4096 bytes. Never logged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// The Task the item asks about, or the Task an approved proposal made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// `$n` of the Task's session when the item was made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// `%n` of the agent pane: open Terminal, and attachments through `changes`, `docs` and `images`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<String>,
    /// With kind `ask`: the menu the agent waits on, as Chat reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<ChatPrompt>,
    #[serde(default, skip_serializing_if = "Attachments::is_empty")]
    pub attachments: Attachments,
    /// The Routine of a `proposal` or `briefing`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<Proposal>,
    /// The answers the host accepts now, in the order to show them. Empty
    /// once the item is answered or resolved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<InboxAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    /// The agent waits on a permission, a question or a plan.
    Ask,
    /// The agent finished a turn, or its loop passed.
    Review,
    /// The agent stopped with an error, or its loop stopped.
    Failure,
    Proposal,
    Briefing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemState {
    Open,
    Later,
    Answered,
    /// The need ended without an answer from the Inbox, e.g. the user
    /// answered in the terminal.
    Resolved,
}

/// What the item's evidence is. The client opens each through the pane's
/// `changes`, `docs` and `images` endpoints, or as a link.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachments {
    /// The pane's changes when the item was made: uncommitted, else the branch's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<ChangeCount>,
    /// Markdown paths: among those changes, changed in the turn outside a
    /// repo, or named by the summary. As `doc` takes them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub docs: Vec<String>,
    /// Image paths, chosen as `docs` are. As `image` takes them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// The summary's `http` and `https` links, or a Routine entry's links.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<SourceLink>,
}

impl Attachments {
    pub fn is_empty(&self) -> bool {
        self.changes.is_none()
            && self.docs.is_empty()
            && self.images.is_empty()
            && self.links.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeCount {
    pub files: u32,
    pub added: u32,
    pub removed: u32,
    /// `branch` when the counts are the branch's own commits, as in `diff {scope}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}
