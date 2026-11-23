//! `migrate-v1` (ADR 0076): one To do item "Resume: <title>" per open v1
//! Task whose place the host can adopt. The host reads the records; the
//! server only adds items, each once by its key `v1:<id>`.

use muxa_inbox::{Inbox, InboxError, NewItem};
use muxa_protocol::v2::{ItemKind, ItemPlace, V1Status, V1Task};

const TITLE_PREFIX: &str = "Resume: ";
const SOURCE: &str = "v1";

fn key(id: &str) -> String {
    format!("v1:{id}")
}

/// Each found Task with what was done, and how many items were added.
pub(crate) fn migrate(
    inbox: &Inbox,
    found: Vec<V1Task>,
    dry_run: bool,
) -> Result<(Vec<V1Task>, u32), InboxError> {
    inbox.change(|book, now| {
        let mut added = 0;
        let mut out = Vec::with_capacity(found.len());
        for mut task in found {
            if task.status == V1Status::New {
                task.status = if book.knows(&key(&task.id)) {
                    V1Status::InInbox
                } else if dry_run {
                    V1Status::New
                } else {
                    match book.add(item(&task), now)? {
                        Some(_) => {
                            added += 1;
                            V1Status::Added
                        }
                        None => V1Status::InInbox,
                    }
                };
            }
            out.push(task);
        }
        Ok((out, added))
    })
}

fn item(task: &V1Task) -> NewItem {
    NewItem {
        kind: ItemKind::Todo,
        title: format!("{TITLE_PREFIX}{}", task.title),
        note: Some(format!(
            "From the old muxa (Task {}). Resume Task starts the agent where it worked.",
            task.id
        )),
        workspace: Some(task.workspace.clone()),
        due: None,
        source: SOURCE.into(),
        key: Some(key(&task.id)),
        once: true,
        terminal: None,
        routine: None,
        place: Some(ItemPlace {
            workspace: task.workspace.clone(),
            path: task.path.clone(),
        }),
        extra: Default::default(),
    }
}
