//! Write proposals (ADR 0052), at `work` level 13: `inbox show ITEM` with a
//! write's content whole, the check before `inbox answer ITEM approve` on a
//! `write` item, and `assist writes WORKSPACE`. Contents and targets go to
//! stdout only, as plain text.

use std::time::Duration;

use muxa_protocol::{
    AssistantRequest, InboxAction, InboxItem, ItemKind, ItemState, Message, Request, WORK_WRITES,
    WriteProposal, work_level,
};
use serde_json::json;

use crate::assist::plain;
use crate::conn::Conn;

const WAIT: Duration = Duration::from_secs(10);

pub const NEEDS_WRITES: &str = "this host takes no write proposals; update muxa-bridge on the host";

pub fn has_writes(conn: &Conn) -> bool {
    work_level(conn.capabilities()) >= WORK_WRITES
}

/// Every Inbox item, for `inbox show` and the check before an approve.
async fn items(conn: &mut Conn) -> Result<Vec<InboxItem>, String> {
    if !crate::items::has_items(conn) {
        return Err("this host has no Inbox items; update muxa-bridge on the host".into());
    }
    let messages = conn.call(|id| Request::ListInbox { id }, WAIT).await?;
    messages
        .into_iter()
        .find_map(|m| match m {
            Message::InboxList { items, .. } => Some(items),
            _ => None,
        })
        .ok_or_else(|| "the bridge sent no Inbox list".into())
}

/// The hash an `approve` sends (ADR 0052 §2): `--hash` as given, or, for a
/// `write` item, its hash once its write is printed whole, so the user saw
/// the text it approves. On a host below level 13 nothing is sent.
pub async fn approve_hash(
    conn: &mut Conn,
    item: &str,
    answer: &crate::items::Answer,
    as_json: bool,
) -> Result<Option<String>, String> {
    if answer.action != InboxAction::Approve {
        return Ok(None);
    }
    if let Some(given) = &answer.hash {
        return if has_writes(conn) {
            Ok(Some(given.clone()))
        } else {
            Err(NEEDS_WRITES.into())
        };
    }
    let found = items(conn).await?;
    let Some(w) = found
        .iter()
        .find(|i| i.id == item && i.kind == ItemKind::Write)
        .and_then(|i| i.write.as_ref())
    else {
        return Ok(None);
    };
    if !has_writes(conn) {
        return Err(NEEDS_WRITES.into());
    }
    // With --json, stdout carries the item only.
    if as_json {
        eprint!("{}", write_text(w));
    } else {
        print!("{}", write_text(w));
    }
    Ok(Some(w.hash.clone()))
}

/// `inbox show ITEM`: the item, and a write's target and content whole.
pub async fn show(mut conn: Conn, item: &str, as_json: bool) -> Result<String, String> {
    let all = items(&mut conn).await?;
    let found = all
        .iter()
        .find(|i| i.id == item)
        .ok_or_else(|| format!("no Inbox item {item}; list the Inbox again"))?;
    if as_json {
        println!("{}", json!(found));
    } else {
        print!("{}", item_text(found, crate::when::now()));
    }
    Ok(String::new())
}

fn item_text(i: &InboxItem, now: u64) -> String {
    let mut out = format!("{}\n", crate::items::line(i, now));
    if let Some(s) = &i.summary {
        out.push_str(&format!("{}\n", plain(s)));
    }
    if let Some(w) = &i.write {
        out.push_str(&write_text(w));
    }
    if let Some(t) = &i.task {
        out.push_str(&format!("Task {t}\n"));
    }
    out
}

/// A write as the user approves it: the target once more, then the content
/// whole, as plain text.
fn write_text(w: &WriteProposal) -> String {
    let from = match (&w.worker, w.step) {
        (Some(who), step) => format!("{} · v{} step {}", plain(who), w.version, step.unwrap_or(1)),
        (None, _) => format!("the assistant · v{}", w.version),
    };
    format!(
        "Action  {}\nTarget  {}\nFrom    {from}\nHash    {}\nContent, written exactly as below:\n{}\n",
        plain(&w.action),
        plain(&w.target),
        w.hash,
        plain(&w.content)
    )
}

/// `assist writes WORKSPACE`: the writer, then each write the workspace's
/// Results proposed, with its state: proposed, rejected, or its Task.
pub async fn list(conn: &mut Conn, workspace: &str, as_json: bool) -> Result<String, String> {
    if !has_writes(conn) {
        return Err(NEEDS_WRITES.into());
    }
    let listing = crate::work::fetch(conn).await?;
    let delegate = listing
        .assistant
        .as_ref()
        .and_then(|a| a.delegates.iter().find(|d| d.workspace == workspace))
        .ok_or_else(|| format!("workspace {workspace} takes no assistant requests"))?;
    let mine: Vec<&AssistantRequest> = listing
        .requests
        .iter()
        .filter(|q| q.workspace == workspace)
        .collect();
    if as_json {
        let writes: Vec<_> = mine
            .iter()
            .flat_map(|q| q.results.iter().map(move |r| (q, r)))
            .flat_map(|(q, r)| {
                r.writes.iter().map(move |w| {
                    json!({"request": q.id, "version": r.version, "step": r.step,
                           "worker": r.worker, "write": w})
                })
            })
            .collect();
        println!(
            "{}",
            json!({"writer": delegate.writer, "writes": delegate.writes, "proposed": writes})
        );
        return Ok(String::new());
    }
    let mut out = match &delegate.writer {
        Some(w) => format!(
            "writer {}  {}\n  {}\n",
            plain(&w.worker),
            crate::items::wire(json!(delegate.writes)),
            plain(&w.description)
        ),
        None => format!("{workspace} has no writer: its results suggest changes in next\n"),
    };
    for q in mine {
        for r in &q.results {
            for w in &r.writes {
                let state = state_of(w.item.as_deref(), w.task.as_deref(), &listing.items);
                out.push_str(&format!(
                    "{} v{}  {}  {} → {}  {state}\n",
                    q.id,
                    r.version,
                    w.item.as_deref().unwrap_or("-"),
                    plain(&w.action),
                    plain(&w.target)
                ));
            }
        }
    }
    print!("{out}");
    Ok(String::new())
}

/// What became of a write: its Task, or its item's answer.
fn state_of(item: Option<&str>, task: Option<&str>, items: &[InboxItem]) -> String {
    if let Some(t) = task {
        return format!("Task {t}");
    }
    let found = item.and_then(|id| items.iter().find(|i| i.id == id));
    match found {
        None if item.is_none() => "proposed, no item yet".into(),
        None => "item deleted".into(),
        Some(i) => match (i.state, i.answer) {
            (ItemState::Answered, Some(InboxAction::Reject)) => "rejected".into(),
            (ItemState::Open, _) if i.later_at.is_some() => "proposed, later".into(),
            (ItemState::Open, _) => "proposed".into(),
            (ItemState::Resolved, _) => "resolved".into(),
            _ => crate::items::wire(json!(i.state)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_shows_its_target_and_its_content_whole_as_plain_text() {
        let w = WriteProposal {
            request: "q-0000abcd".into(),
            version: 2,
            step: Some(1),
            worker: Some("lisk-delivery".into()),
            action: "Post to #release".into(),
            target: "slack:#release".into(),
            content: "Release \u{1b}[31m2.4\u{1b}[0m.\n\n\t- Faster sync".into(),
            hash: "sha256:00".into(),
        };
        assert_eq!(
            write_text(&w),
            "Action  Post to #release\nTarget  slack:#release\n\
             From    lisk-delivery · v2 step 1\nHash    sha256:00\n\
             Content, written exactly as below:\nRelease [31m2.4[0m.\n\n\t- Faster sync\n"
        );
    }
}
