//! Inbox items of `inbox` and `inbox answer` (ADR 0033), behind the host's
//! `work` level 3 capability.

use std::time::Duration;

use muxa_protocol::{
    InboxAction, InboxItem, ItemKind, ItemState, Message, Request, WORK_ITEMS, WorktreeEnd,
};
use serde_json::json;

use crate::conn::Conn;

const WAIT: Duration = Duration::from_secs(10);
/// A proposal's approval and a retry launch an agent before they answer.
const LAUNCH_WAIT: Duration = Duration::from_secs(120);
/// A reply to an `ask` waits up to 10 s for the agent to take the denial.
const REPLY_WAIT: Duration = Duration::from_secs(30);

/// An answer as `update-inbox` takes it, validated before any request.
#[derive(Debug, PartialEq, Eq)]
pub struct Answer {
    pub action: InboxAction,
    pub text: Option<String>,
    pub option: Option<u32>,
    /// Used by the host only when the answer ends the Task (ADR 0043).
    pub worktree: Option<WorktreeEnd>,
}

/// The kebab-case name on the wire.
pub fn action(name: &str) -> Option<InboxAction> {
    serde_json::from_value(json!(name)).ok()
}

impl Answer {
    pub fn checked(self) -> Result<Self, String> {
        let a = self.action;
        let wants_text = matches!(a, InboxAction::Reply | InboxAction::AskAgent);
        let rule = |name: &str, given: bool, wanted: bool, needs: &str| match (given, wanted) {
            (false, true) => Err(format!("{needs} needs {name}")),
            (true, false) => Err(format!("{name} is only for {needs}")),
            _ => Ok(()),
        };
        rule(
            "--text",
            self.text.is_some(),
            wants_text,
            "reply and ask-agent",
        )?;
        rule(
            "--option",
            self.option.is_some(),
            a == InboxAction::Choose,
            "choose",
        )?;
        if self
            .text
            .as_ref()
            .is_some_and(|t| t.is_empty() || t.len() > 4096)
        {
            return Err("--text must be 1 to 4096 bytes".into());
        }
        Ok(self)
    }
}

pub fn has_items(conn: &Conn) -> bool {
    muxa_protocol::work_level(conn.capabilities()) >= WORK_ITEMS
}

pub fn shown(item: &InboxItem, all: bool) -> bool {
    all || matches!(item.state, ItemState::Open | ItemState::Later)
}

pub(crate) fn wire(value: serde_json::Value) -> String {
    value.as_str().unwrap_or("?").to_owned()
}

pub(crate) fn age(seconds: u64) -> String {
    match seconds {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86_400),
    }
}

pub fn line(item: &InboxItem, now: u64) -> String {
    let kind = match item.kind {
        ItemKind::Ask => "ask",
        ItemKind::Review => "review",
        ItemKind::Failure => "failure",
        ItemKind::Proposal => "proposal",
        ItemKind::Briefing => "briefing",
    };
    let mut state = wire(json!(item.state));
    if item.state == ItemState::Open && item.later_at.is_some() {
        state = "later".into();
    }
    if let (ItemState::Answered, Some(a)) = (item.state, item.answer) {
        state = format!("answered: {}", wire(json!(a)));
    }
    if item.read_at.is_none() && item.state == ItemState::Open {
        state.push_str(" unread");
    }
    format!(
        "{}  {kind}  {state}  {}  {}  {}",
        item.id,
        age(now.saturating_sub(item.created_at)),
        item.title,
        item.question
    )
}

pub async fn answer(
    mut conn: Conn,
    item: &str,
    answer: Answer,
    as_json: bool,
) -> Result<String, String> {
    if !has_items(&conn) {
        return Err("this host has no Inbox items; update muxa-bridge on the host".into());
    }
    // Ask Agent may start the workspace's Ask session and wait for its agent.
    let launches = matches!(
        answer.action,
        InboxAction::Approve | InboxAction::Retry | InboxAction::AskAgent
    );
    let entry = item.to_owned();
    let Answer {
        action,
        text,
        option,
        worktree,
    } = answer;
    let messages = conn
        .call(
            |id| Request::UpdateInbox {
                id,
                entry,
                action,
                until: None,
                text,
                option,
                worktree,
            },
            match action {
                _ if launches => LAUNCH_WAIT,
                InboxAction::Reply => REPLY_WAIT,
                _ => WAIT,
            },
        )
        .await?;
    let saved = messages
        .into_iter()
        .find_map(|m| match m {
            Message::InboxSaved { item, .. } => item.map(|i| *i),
            _ => None,
        })
        .ok_or("the bridge saved no item")?;
    if as_json {
        println!("{}", json!(saved));
    } else {
        println!("{}", line(&saved, crate::when::now()));
    }
    // A new or restarted agent gets its prompt after the answer.
    if launches {
        conn.finish(Duration::from_secs(40)).await;
    }
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(action: &str) -> Answer {
        Answer {
            action: super::action(action).unwrap(),
            text: None,
            option: None,
            worktree: None,
        }
    }

    #[test]
    fn each_flag_belongs_to_its_action() {
        assert!(answer("approve").checked().is_ok());
        assert!(super::action("redo").is_none(), "ADR 0040 removed it");
        assert!(answer("choose").checked().is_err());
        assert!(answer("later").checked().is_ok());
        let text = |a| Answer {
            text: Some("again".into()),
            ..answer(a)
        };
        assert!(text("ask-agent").checked().is_ok());
        assert!(text("reply").checked().is_ok());
        assert!(answer("reply").checked().is_err());
        assert!(text("accept").checked().is_err());
        assert!(
            Answer {
                option: Some(1),
                ..answer("choose")
            }
            .checked()
            .is_ok()
        );
        assert!(
            Answer {
                text: Some(String::new()),
                ..answer("reply")
            }
            .checked()
            .is_err()
        );
        assert!(super::action("keep-open").is_some());
        assert!(super::action("keep_open").is_none());
    }
}
