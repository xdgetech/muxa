//! `update-inbox` on a Task's item (ADR 0033 §1): each answer does what the
//! Task's own request would, then closes the item.
use muxa_protocol::{ErrorBody, ErrorCode, InboxAction, InboxItem};

use crate::convert::error_body;
use crate::task_records::Context;

pub struct Answer {
    pub item: String,
    pub action: InboxAction,
    pub until: Option<u64>,
    pub text: Option<String>,
    pub option: Option<u32>,
}

pub async fn update(_cx: &Context, _answer: Answer) -> Result<InboxItem, ErrorBody> {
    Err(error_body(
        ErrorCode::Unsupported,
        "Inbox items are not answered yet",
    ))
}
