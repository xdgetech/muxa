//! Inbox requests (ADR 0066). Each change replies with the whole Inbox;
//! watchers get it as an `inbox` notification.

use muxa_inbox::{Inbox, InboxError, NewItem};
use muxa_protocol::v2::{ClientMsg, ErrorCode, Reply};

use crate::tasks::Host;

pub(crate) type Answer = (u64, Result<Reply, (ErrorCode, String)>);

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
