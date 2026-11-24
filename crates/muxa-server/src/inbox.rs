//! Inbox requests (ADR 0066). Each change replies with the whole Inbox;
//! watchers get it as an `inbox` notification.

use muxa_inbox::{Inbox, InboxError, NewItem};
use muxa_protocol::v2::{ClientMsg, ErrorCode, Reply};

use crate::tasks::Host;

pub(crate) type Answer = (u64, Result<Reply, (ErrorCode, String)>);

/// `inbox-delete-older` takes 1 to 3650 days (ADR 0095), as ADR 0046 did.
const MAX_DAYS: u32 = 3650;
/// Seconds.
const DAY: i64 = 24 * 60 * 60;

/// Answers an Inbox request; `None` for any other message.
pub(crate) async fn handle<H: Host>(msg: ClientMsg, inbox: &Inbox, host: &H) -> Option<Answer> {
    let (id, result) = match msg {
        ClientMsg::Inbox { id } => (id, Ok(())),
        ClientMsg::ParseCapture { id, text, today } => {
            let workspaces = host.workspaces().await;
            let reply = muxa_inbox::parse(&text, &workspaces, &today)
                .map(|capture| Reply::Capture { capture })
                .map_err(refused);
            return Some((id, reply));
        }
        ClientMsg::InboxAdd {
            id,
            kind,
            title,
            note,
            workspace,
            due,
        } => {
            let new = NewItem {
                kind,
                title,
                note,
                workspace,
                due,
                source: "capture".into(),
                key: None,
                once: false,
                terminal: None,
                routine: None,
                place: None,
                extra: Default::default(),
            };
            (id, inbox.change(|b, now| b.add(new, now).map(drop)))
        }
        ClientMsg::InboxMove { id, item, index } => {
            (id, inbox.change(|b, _| b.move_to(&item, index)))
        }
        ClientMsg::InboxLater { id, item, until } => {
            (id, inbox.change(|b, now| b.later(&item, until, now)))
        }
        ClientMsg::InboxDone { id, item } => (id, inbox.change(|b, _| b.done(&item))),
        ClientMsg::InboxReopen { id, item } => (id, inbox.change(|b, _| b.reopen(&item))),
        ClientMsg::InboxDelete { id, item } => (id, inbox.change(|b, _| b.delete(&item))),
        ClientMsg::InboxDeleteOlder { id, days } => {
            let result = if (1..=MAX_DAYS).contains(&days) {
                inbox.change(|b, now| {
                    b.delete_older(now - i64::from(days) * DAY);
                    Ok(())
                })
            } else {
                Err(InboxError::Invalid(format!("days must be 1 to {MAX_DAYS}")))
            };
            (id, result)
        }
        _ => return None,
    };
    let reply = result
        .map(|()| Reply::Inbox {
            inbox: inbox.view(),
        })
        .map_err(refused);
    Some((id, reply))
}

fn refused(e: InboxError) -> (ErrorCode, String) {
    let code = match e {
        InboxError::Invalid(_) => ErrorCode::BadRequest,
        InboxError::NotFound(_) => ErrorCode::NotFound,
    };
    (code, e.to_string())
}
