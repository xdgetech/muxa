//! `muxa-cli v2 capture …` and `muxa-cli v2 inbox …` (ADR 0066).

use serde_json::{Value, json};

/// What to do with the Inbox.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    List,
    Capture(String),
    Done(String),
    Reopen(String),
    Delete(String),
    Move {
        item: String,
        index: usize,
    },
    /// Moves an item to the bottom (ADR 0091).
    Later(String),
    /// Closes the Task of a Review item; `force` removes worktrees that hold work.
    Approve {
        item: String,
        force: bool,
    },
    RequestChanges {
        item: String,
        comment: String,
    },
    Retry(String),
    /// Applies the settings a Bot proposed (ADR 0083).
    Apply(String),
    /// Accepts or dismisses the Capture assistant's suggestion (ADR 0084).
    Suggestion {
        item: String,
        accept: bool,
    },
    /// Closes the Task of an agent terminal without its item (ADR 0079).
    CloseTask {
        terminal: String,
        force: bool,
    },
}

/// The request for `action`; a Capture first needs [`capture_add`].
pub fn request(action: &Action) -> Value {
    match action {
        Action::List => json!({"type": "inbox"}),
        Action::Capture(text) => json!({"type": "parse-capture", "text": text,
                                        "today": jiff::Zoned::now().date().to_string()}),
        Action::Done(item) => json!({"type": "inbox-done", "item": item}),
        Action::Reopen(item) => json!({"type": "inbox-reopen", "item": item}),
        Action::Delete(item) => json!({"type": "inbox-delete", "item": item}),
        Action::Move { item, index } => json!({"type": "inbox-move", "item": item, "index": index}),
        Action::Later(item) => json!({"type": "inbox-later", "item": item}),
        Action::Approve { item, force } => {
            let worktree = if *force { "force" } else { "remove" };
            json!({"type": "inbox-approve", "item": item, "worktree": worktree})
        }
        Action::RequestChanges { item, comment } => {
            json!({"type": "inbox-request-changes", "item": item, "comment": comment})
        }
        Action::Retry(item) => json!({"type": "inbox-retry", "item": item}),
        Action::Apply(item) => json!({"type": "inbox-apply", "item": item}),
        Action::Suggestion { item, accept } => {
            json!({"type": "inbox-suggestion", "item": item, "accept": accept})
        }
        Action::CloseTask { terminal, force } => {
            let worktree = if *force { "force" } else { "remove" };
            json!({"type": "close-task", "terminal": terminal, "worktree": worktree})
        }
    }
}

/// The `inbox-add` for a parsed Capture: the fields as the parser read them.
pub fn capture_add(parsed: &Value) -> Value {
    let mut add = parsed["capture"].clone();
    add["type"] = json!("inbox-add");
    add
}

/// `id  kind  title  [#workspace] [due D]`, open items in order.
pub fn rows(inbox: &Value) -> Vec<String> {
    inbox["items"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|i| {
            let mut row = format!(
                "{}\t{}\t{}",
                i["id"].as_str().unwrap_or("?"),
                i["kind"].as_str().unwrap_or("?"),
                i["title"].as_str().unwrap_or("")
            );
            if let Some(w) = i["workspace"].as_str() {
                row.push_str(&format!("\t#{w}"));
            }
            if let Some(d) = i["due"].as_str() {
                row.push_str(&format!("\tdue {d}"));
            }
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_show_what_the_item_asks() {
        let inbox = json!({"items": [
            {"id": "i2", "kind": "reply", "title": "the bank", "workspace": "Personal", "due": "2026-11-27"},
            {"id": "i1", "kind": "todo", "title": "milk"}]});
        assert_eq!(
            rows(&inbox),
            [
                "i2\treply\tthe bank\t#Personal\tdue 2026-11-27",
                "i1\ttodo\tmilk"
            ]
        );
    }

    #[test]
    fn later_sends_no_time_and_capture_adds_the_parsed_fields() {
        assert_eq!(
            request(&Action::Later("i1".into())),
            json!({"type": "inbox-later", "item": "i1"})
        );
        let add = capture_add(&json!({"capture": {"kind": "read", "title": "RFC"}}));
        assert_eq!(
            add,
            json!({"type": "inbox-add", "kind": "read", "title": "RFC"})
        );
    }
}
