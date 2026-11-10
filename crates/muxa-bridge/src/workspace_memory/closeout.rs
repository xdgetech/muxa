//! At a Task's end (ADR 0053 §3): read "For the workspace" before the
//! session closes, then, by the workspace's `memory`, make one `memory`
//! item in the Task's item file (`review`), append at once (`auto`, with
//! the item as the fallback), or do nothing (`off`). A Routine or an
//! assistant run never proposes. Nothing here stops a Task's end.
use std::path::PathBuf;

use muxa_protocol::{InboxItem, ItemKind, ItemState, MemoryMode, MemoryProposal};

use super::{MAX_SECTION, append, approvable, hash};
use crate::assistant::notes::{Appended, MAX_NOTES};
use crate::items::produce::{MAX_QUESTION, one_line};
use crate::items::store::{ItemStore, StoredItem, new_id};
use crate::records::{Record, Workspaces};
use crate::routines::store::StateError;
use crate::task_records::Config;
use crate::task_records::for_workspace::{self, Section};

/// A Task's proposal, read before its session closed.
#[derive(Debug, Clone)]
pub struct End {
    pub task: String,
    pub title: String,
    pub workspace: String,
    pub root: PathBuf,
    pub mode: MemoryMode,
    pub section: Section,
}

/// The item's key in the Task's item file: one per Task.
pub fn key(task: &str) -> String {
    format!("memory:{task}")
}

/// Blocking. What `r`'s end proposes; None for a background run, a
/// workspace with `memory = "off"`, or no text. Call it before the session
/// closes and the worktree goes.
pub fn read(workspaces: &Workspaces, r: &Record) -> Option<End> {
    if r.is_background_run() {
        return None;
    }
    let ws = workspaces.get(&r.workspace)?;
    if matches!(ws.memory, MemoryMode::Off | MemoryMode::Unknown) {
        return None;
    }
    let dir = r.dir.as_deref()?;
    let section = match for_workspace::read(dir, r) {
        Ok(found) => found?,
        Err(e) => {
            tracing::warn!(task = %r.id, error = %e, "cannot read the task's section for the workspace");
            return None;
        }
    };
    tracing::info!(task = %r.id, size = section.size, "the task proposes to the workspace memory");
    Some(End {
        task: r.id.clone(),
        title: r.title.clone(),
        workspace: r.workspace.clone(),
        root: ws.root.clone(),
        mode: ws.memory,
        section,
    })
}

/// Blocking. After the Task's status is saved and its other items resolved.
pub fn propose(config: &Config, end: &End, at: u64) {
    let p = proposal(end);
    let why = if end.mode == MemoryMode::Auto && approvable(&p.text) {
        match append(config, &end.root, &p, at) {
            Ok(Appended::Added { size }) => {
                tracing::info!(task = %end.task, workspace = %end.workspace, size, hash = %p.hash, "workspace memory appended without review");
                return;
            }
            Ok(Appended::Present) => return,
            Err(e) => {
                tracing::info!(task = %end.task, workspace = %end.workspace, error = %e, "workspace memory append refused; asking for review");
                Some(format!("Not added automatically: {e} Approve to add it."))
            }
        }
    } else {
        None
    };
    let summary = refusal(&end.section).or(why);
    match make(config, end, p, summary, at) {
        Ok(Some(item)) => {
            tracing::info!(task = %end.task, item, size = end.section.size, "memory item made")
        }
        Ok(None) => {}
        Err(e) => tracing::warn!(task = %end.task, error = %e, "cannot make the memory item"),
    }
}

fn proposal(end: &End) -> MemoryProposal {
    let text = end.section.text.clone();
    MemoryProposal {
        hash: hash(&end.task, &end.title, &text),
        task: end.task.clone(),
        title: end.title.clone(),
        text,
    }
}

/// Why the item has no Approve, when it has none.
fn refusal(section: &Section) -> Option<String> {
    if section.size > MAX_SECTION {
        let shown = if section.size > MAX_NOTES {
            " The item shows the first 16 KiB."
        } else {
            ""
        };
        return Some(format!(
            "The text is {} bytes; muxa adds at most 4 KiB. Copy what you need into \
             .muxa/WORKSPACE.md by hand.{shown}",
            section.size
        ));
    }
    (!approvable(&section.text)).then(|| {
        "The text has control characters or is not UTF-8; muxa does not add it. Copy what \
         you need into .muxa/WORKSPACE.md by hand."
            .to_owned()
    })
}

/// The item, unless the Task has made one already, also a deleted one.
fn make(
    config: &Config,
    end: &End,
    p: MemoryProposal,
    summary: Option<String>,
    at: u64,
) -> Result<Option<String>, StateError> {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return Ok(None);
    };
    let question = format!("{} proposes to add to the workspace memory", end.title);
    let mut item = InboxItem {
        id: new_id()?,
        kind: ItemKind::Memory,
        state: ItemState::Open,
        question: one_line(&question, MAX_QUESTION),
        workspace: end.workspace.clone(),
        title: one_line(&end.title, MAX_QUESTION),
        created_at: at,
        read_at: None,
        later_at: None,
        until: None,
        protected_at: None,
        answered_at: None,
        answer: None,
        summary,
        task: Some(end.task.clone()),
        session: None,
        pane: None,
        prompt: None,
        attachments: Default::default(),
        routine: None,
        proposal: None,
        sort: None,
        delegate: None,
        hypothesis: None,
        handoff: None,
        write: None,
        memory: Some(p),
        answers: Vec::new(),
    };
    item.answers = crate::items::answers(&item);
    let id = item.id.clone();
    let key = key(&end.task);
    ItemStore::open(dir)?.update::<_, StateError>(&end.task, |items| {
        if items.has_key(&key) {
            return Ok(None);
        }
        items.items.push(StoredItem { item, key });
        Ok(Some(id))
    })
}
