//! Inbox items of `inbox`, `inbox answer` (ADR 0033) and `inbox delete`
//! (ADR 0046), behind the host's `work` level 3 and 8 capabilities.

use std::time::Duration;

use muxa_protocol::{
    InboxAction, InboxItem, ItemKind, ItemState, Message, Request, WORK_DELETE_INBOX, WORK_ITEMS,
    WorktreeEnd,
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
    /// With `reject`, at level 11: the correction's reason (ADR 0050).
    pub reason: Option<String>,
    pub option: Option<u32>,
    /// Used by the host only when the answer ends the Task (ADR 0043).
    pub worktree: Option<WorktreeEnd>,
    /// With `approve` on a `write` item: the hash of the write the user saw (ADR 0052 §2).
    pub hash: Option<String>,
}

/// The kebab-case name on the wire.
pub fn action(name: &str) -> Option<InboxAction> {
    // A name this build does not know decodes as `Unknown`; the host refuses it.
    serde_json::from_value(json!(name))
        .ok()
        .filter(muxa_protocol::Known::is_known)
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
        // A reason is never required (ADR 0050 §2).
        if let Some(r) = &self.reason {
            if a != InboxAction::Reject {
                return Err("--reason is only for reject".into());
            }
            crate::policy::reason(r)?;
        }
        if self.hash.is_some() && a != InboxAction::Approve {
            return Err("--hash is only for approve".into());
        }
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

fn has_delete(conn: &Conn) -> bool {
    muxa_protocol::work_level(conn.capabilities()) >= WORK_DELETE_INBOX
}

const NEEDS_DELETE: &str =
    "this host cannot protect or delete Inbox items; update muxa-bridge on the host";

/// `inbox delete --older-than DAYS`: removes the unprotected items made
/// more than DAYS days ago and prints how many.
pub async fn delete(mut conn: Conn, days: u32, as_json: bool) -> Result<String, String> {
    if !has_delete(&conn) {
        return Err(NEEDS_DELETE.into());
    }
    let messages = conn
        .call(
            |id| Request::DeleteInbox {
                id,
                older_than_days: days,
            },
            WAIT,
        )
        .await?;
    let deleted = messages
        .into_iter()
        .find_map(|m| match m {
            Message::InboxDeleted { deleted, .. } => Some(deleted),
            _ => None,
        })
        .ok_or("the bridge sent no delete count")?;
    if as_json {
        println!("{}", json!({"deleted": deleted}));
    } else {
        println!("deleted {deleted} Inbox items older than {days} days");
    }
    Ok(String::new())
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
        ItemKind::Sort => "sort",
        ItemKind::Delegate => "delegate",
        ItemKind::Hypothesis => "hypothesis",
        ItemKind::Handoff => "handoff",
        ItemKind::Write => "write",
        ItemKind::Unknown => "unknown",
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
    if item.protected_at.is_some() {
        state.push_str(" protected");
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
    if answer.action == InboxAction::Confirm || answer.reason.is_some() {
        crate::policy::require(&conn)?;
    }
    let protects = matches!(answer.action, InboxAction::Protect | InboxAction::Unprotect);
    if protects && !has_delete(&conn) {
        return Err(NEEDS_DELETE.into());
    }
    // A write is approved only with the hash of the text the user saw (ADR 0052 §2).
    let hash = crate::writes::approve_hash(&mut conn, item, &answer, as_json).await?;
    // Ask Agent may start the workspace's Ask session and wait for its agent.
    let launches = matches!(
        answer.action,
        InboxAction::Approve | InboxAction::Retry | InboxAction::AskAgent
    );
    let entry = item.to_owned();
    let Answer {
        action,
        text,
        reason,
        option,
        worktree,
        ..
    } = answer;
    let text = text.or(reason);
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
                hash,
            },
            match action {
                _ if launches => LAUNCH_WAIT,
                InboxAction::Reply => REPLY_WAIT,
                _ => WAIT,
            },
        )
        .await?;
    // An approved write's Task (ADR 0052 §3). The host never says the write happened.
    let started = messages.iter().find_map(|m| match m {
        Message::TaskSaved { record, .. } => Some(record.id.clone()),
        _ => None,
    });
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
        if let Some(task) = started {
            println!("Task started: {task}");
        }
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
            reason: None,
            option: None,
            worktree: None,
            hash: None,
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
        assert!(answer("protect").checked().is_ok());
        assert!(answer("unprotect").checked().is_ok());
        assert!(super::action("keep-open").is_some());
        assert!(super::action("keep_open").is_none());
        let reason = |a| Answer {
            reason: Some("Sales still matters.".into()),
            ..answer(a)
        };
        assert!(reason("reject").checked().is_ok());
        assert!(reason("confirm").checked().is_err());
        assert!(answer("confirm").checked().is_ok());
        let hash = |a| Answer {
            hash: Some("sha256:00".into()),
            ..answer(a)
        };
        assert!(hash("approve").checked().is_ok());
        assert!(hash("reject").checked().is_err());
    }
}
