//! `task propose` (ADR 0060 §3): the secretary proposes a Task. The host
//! makes one Inbox item; the user starts it, in plan mode, from the Inbox.
use std::time::Duration;

use muxa_protocol::{InboxItem, Message, Request, WORK_TASK_PROPOSALS, work_level};

use crate::conn::Conn;

const WAIT: Duration = Duration::from_secs(10);
/// The host's limits (`protocol/PROTOCOL.md`, "Task proposals").
const MAX_TITLE: usize = 200;
const MAX_GOAL: usize = 4096;

/// `task propose`'s flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub workspace: String,
    pub repo: Option<String>,
    pub title: String,
    pub goal: String,
    pub work: Option<String>,
}

impl Proposal {
    pub fn checked(self) -> Result<Self, String> {
        if self.workspace.trim().is_empty() {
            return Err("task propose needs --workspace".into());
        }
        let title = self.title.chars().count();
        if self.title.trim().is_empty() || title > MAX_TITLE || self.title.contains('\n') {
            return Err(format!(
                "--title must be one line of 1 to {MAX_TITLE} characters"
            ));
        }
        if self.goal.trim().is_empty() || self.goal.len() > MAX_GOAL {
            return Err(format!("--goal must be 1 to {MAX_GOAL} bytes"));
        }
        Ok(Self {
            repo: self.repo.filter(|r| !r.trim().is_empty()),
            ..self
        })
    }
}

/// Prints the item: its id, then its question; or the item as JSON.
pub async fn propose(mut conn: Conn, p: Proposal, as_json: bool) -> Result<String, String> {
    if work_level(conn.capabilities()) < WORK_TASK_PROPOSALS {
        return Err(
            "this host takes no proposed Tasks (work level 18); update muxa-bridge on the host"
                .into(),
        );
    }
    let messages = conn
        .call(
            |id| Request::ProposeTask {
                id,
                workspace: p.workspace,
                repo: p.repo,
                title: p.title,
                goal: p.goal,
                work: p.work,
            },
            WAIT,
        )
        .await?;
    let item = saved(messages).ok_or("the bridge saved no Inbox item")?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string(&item).map_err(|e| e.to_string())?
        );
    } else {
        println!("{}\t{}", item.id, item.question);
    }
    Ok(String::new())
}

fn saved(messages: Vec<Message>) -> Option<InboxItem> {
    messages.into_iter().find_map(|m| match m {
        Message::InboxSaved { item, .. } => item.map(|i| *i),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(title: &str, goal: &str) -> Proposal {
        Proposal {
            workspace: "muxa".into(),
            repo: Some(String::new()),
            title: title.into(),
            goal: goal.into(),
            work: None,
        }
    }

    #[test]
    fn a_proposal_needs_a_one_line_title_and_a_goal() {
        let ok = p("Fix the crash", "Guard it.\n").checked().unwrap();
        assert_eq!(ok.repo, None, "an empty --repo is no repo");
        assert!(p("two\nlines", "g").checked().is_err());
        assert!(p("t", " ").checked().is_err());
        assert!(p(&"t".repeat(201), "g").checked().is_err());
        assert!(p("t", &"g".repeat(4097)).checked().is_err());
    }
}
