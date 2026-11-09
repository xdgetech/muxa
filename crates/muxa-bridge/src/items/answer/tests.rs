use std::path::PathBuf;
use std::sync::Arc;

use muxa_protocol::{ChatPrompt, PromptKind, PromptOption};

use super::*;
use crate::agents::Registry;
use crate::items::store::{StoredItem, tests_item};
use crate::records::{NewRecord, Status, Store};
use crate::task_records::{ClientFuture, ClientSource, Config, Pending};

/// No tmux: every request that needs it fails with `tmux`.
struct NoTmux;

impl ClientSource for NoTmux {
    fn client(&self) -> ClientFuture<'_> {
        Box::pin(async { Err(muxa_tmux::Error::Exited("no tmux in this test".into())) })
    }
}

struct Fixture {
    dir: PathBuf,
    cx: Context,
    task: String,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A Task record, and `item` stored as its only item.
fn fixture(item: InboxItem) -> Fixture {
    let dir = std::env::temp_dir().join(format!(
        "muxa-item-answer-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let record = Store::open(&dir)
        .unwrap()
        .create(NewRecord {
            workspace: "w".into(),
            title: "Fixture".into(),
            goal: "Fix it.\n".into(),
            project: None,
            repo: None,
            branch: None,
            dir: None,
            session: None,
            note: None,
            worker: None,
            extra: serde_json::Map::new(),
        })
        .unwrap();
    let task = record.id.clone();
    ItemStore::open(&dir)
        .unwrap()
        .update::<_, StateError>(&task, |t| {
            let mut item = item;
            item.task = Some(task.clone());
            t.items.push(StoredItem {
                item,
                key: "review:1".into(),
            });
            Ok(())
        })
        .unwrap();
    let cx = Context {
        tmux: Arc::new(NoTmux),
        tasks: Arc::new(crate::tasks::Config {
            launcher: None,
            command: "true".into(),
            tmux: "tmux".into(),
            socket: None,
        }),
        records: Arc::new(Config {
            state_dir: Some(dir.clone()),
            workspaces: None,
            home: None,
        }),
        agents: Arc::new(Registry::new(Vec::new())),
        pending: Pending::default(),
    };
    Fixture { dir, cx, task }
}

impl Fixture {
    fn stored(&self, id: &str) -> InboxItem {
        let items = ItemStore::open(&self.dir).unwrap().get(&self.task).unwrap();
        items
            .items
            .into_iter()
            .find(|s| s.item.id == id)
            .unwrap()
            .item
    }

    fn status(&self) -> Status {
        Store::open(&self.dir)
            .unwrap()
            .get(&self.task)
            .unwrap()
            .status
    }
}

fn answer(item: &str, action: InboxAction) -> Answer {
    Answer {
        item: item.into(),
        action,
        text: None,
        option: None,
        worktree: None,
    }
}

fn review(id: &str) -> InboxItem {
    tests_item(id, ItemState::Open)
}

fn menu() -> ChatPrompt {
    let option = |label: &str, key: &str, cancel| PromptOption {
        label: label.into(),
        keys: vec![key.into()],
        cancel,
    };
    ChatPrompt {
        entry: "toolu_1".into(),
        kind: PromptKind::Permission,
        question: None,
        options: vec![
            option("No", "Escape", true),
            option("Yes", "1", false),
            option("Always", "2", false),
        ],
    }
}

fn ask(id: &str) -> InboxItem {
    let mut i = tests_item(id, ItemState::Open);
    i.kind = ItemKind::Ask;
    i.pane = Some("%3".into());
    i.prompt = Some(menu());
    i
}

#[tokio::test]
async fn accept_marks_the_task_done_and_answers_the_item() {
    let f = fixture(review("i-00000001"));
    let saved = update(&f.cx, answer("i-00000001", InboxAction::Accept))
        .await
        .unwrap();
    assert_eq!(saved.state, ItemState::Answered);
    assert_eq!(saved.answer, Some(InboxAction::Accept));
    assert!(saved.answered_at.is_some());
    assert!(saved.answers.is_empty());
    assert_eq!(f.stored("i-00000001"), saved);
    assert_eq!(f.status(), Status::Done);
}

#[tokio::test]
async fn closing_the_task_elsewhere_resolves_its_open_item() {
    for status in [
        muxa_protocol::TaskStatus::Done,
        muxa_protocol::TaskStatus::Dropped,
    ] {
        let f = fixture(review("i-00000001"));
        crate::task_records::set_status(&f.cx.records, f.task.clone(), status)
            .await
            .unwrap();
        let item = f.stored("i-00000001");
        assert_eq!(item.state, ItemState::Resolved);
        assert!(item.answers.is_empty());
    }
}

#[tokio::test]
async fn an_unknown_item_or_an_answer_it_does_not_take_is_a_bad_request() {
    let f = fixture(review("i-00000001"));
    let e = update(&f.cx, answer("i-00000009", InboxAction::Dismiss))
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::BadRequest);
    let e = update(&f.cx, answer("i-00000001", InboxAction::Retry))
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::BadRequest);
    assert!(
        e.message
            .contains("it takes accept, reply, keep-open, ask-agent, later, or read"),
        "{}",
        e.message
    );
    assert_eq!(f.stored("i-00000001").state, ItemState::Open);
    assert_eq!(f.status(), Status::Open);
}

#[tokio::test]
async fn read_is_kept_once_and_the_item_stays_open() {
    let mut item = review("i-00000001");
    item.read_at = Some(5);
    let f = fixture(item);
    let saved = update(&f.cx, answer("i-00000001", InboxAction::Read))
        .await
        .unwrap();
    assert_eq!((saved.state, saved.read_at), (ItemState::Open, Some(5)));
    assert!(saved.answers.contains(&InboxAction::Accept));
}

#[tokio::test]
async fn later_moves_the_item_down_and_it_stays_open() {
    let f = fixture(review("i-00000001"));
    let saved = update(&f.cx, answer("i-00000001", InboxAction::Later))
        .await
        .unwrap();
    assert_eq!(saved.state, ItemState::Open);
    assert!(saved.later_at.is_some());
    assert!(saved.answers.contains(&InboxAction::Accept));

    // Stored hidden before ADR 0040, with a time not passed yet: it is open.
    let mut hidden = review("i-00000002");
    hidden.state = ItemState::Later;
    hidden.until = Some(4_000_000_000);
    let f = fixture(hidden);
    let saved = update(&f.cx, answer("i-00000002", InboxAction::KeepOpen))
        .await
        .unwrap();
    assert_eq!(saved.answer, Some(InboxAction::KeepOpen));
}

#[tokio::test]
async fn a_protected_item_stays_open_and_still_takes_done_and_later() {
    let f = fixture(review("i-00000001"));
    let saved = update(&f.cx, answer("i-00000001", InboxAction::Protect))
        .await
        .unwrap();
    assert_eq!(saved.state, ItemState::Open);
    let at = saved.protected_at.expect("protected");
    assert!(
        !saved.answers.contains(&InboxAction::Protect),
        "never listed"
    );
    let saved = update(&f.cx, answer("i-00000001", InboxAction::Later))
        .await
        .unwrap();
    assert_eq!(
        (saved.protected_at, saved.later_at.is_some()),
        (Some(at), true)
    );
    let saved = update(&f.cx, answer("i-00000001", InboxAction::KeepOpen))
        .await
        .unwrap();
    assert_eq!(
        (saved.state, saved.protected_at),
        (ItemState::Answered, Some(at))
    );
    let saved = update(&f.cx, answer("i-00000001", InboxAction::Unprotect))
        .await
        .unwrap();
    assert_eq!(
        (saved.state, saved.protected_at),
        (ItemState::Answered, None)
    );
}

#[tokio::test]
async fn a_failed_answer_leaves_the_item_open() {
    // Approve needs tmux to read the menu again, and there is none.
    let f = fixture(ask("i-00000001"));
    let e = update(&f.cx, answer("i-00000001", InboxAction::Approve))
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Tmux);
    let stored = f.stored("i-00000001");
    assert_eq!((stored.state, stored.answer), (ItemState::Open, None));
}

#[tokio::test]
async fn a_closed_item_takes_no_answer_but_read() {
    let mut item = review("i-00000001");
    item.state = ItemState::Resolved;
    let f = fixture(item);
    let e = update(&f.cx, answer("i-00000001", InboxAction::Accept))
        .await
        .unwrap_err();
    assert!(e.message.contains("already closed"), "{}", e.message);
    let saved = update(&f.cx, answer("i-00000001", InboxAction::Read))
        .await
        .unwrap();
    assert_eq!(saved.state, ItemState::Resolved);
    assert!(saved.read_at.is_some());
}

#[test]
fn an_ask_picks_the_option_its_answer_names() {
    let item = ask("i-00000001");
    let picked = |action, option| match plan(
        &item,
        &Answer {
            option,
            ..answer("i-00000001", action)
        },
    ) {
        Ok(Plan::Pick { option, pane, .. }) => {
            assert_eq!(pane.to_string(), "%3");
            Ok(option)
        }
        Ok(other) => panic!("{other:?}"),
        Err(e) => Err(e.code),
    };
    assert_eq!(picked(InboxAction::Approve, None), Ok(1));
    assert_eq!(picked(InboxAction::Reject, None), Ok(0));

    let mut question = ask("i-00000001");
    question.prompt.as_mut().unwrap().kind = PromptKind::Question;
    let chosen = |option| {
        plan(
            &question,
            &Answer {
                option,
                ..answer("i-00000001", InboxAction::Choose)
            },
        )
    };
    assert!(matches!(chosen(Some(2)), Ok(Plan::Pick { option: 2, .. })));
    assert_eq!(chosen(Some(3)).unwrap_err().code, ErrorCode::BadRequest);
    assert_eq!(chosen(None).unwrap_err().code, ErrorCode::BadRequest);
}

#[test]
fn keys_go_only_to_the_menu_the_item_asked_about() {
    let asked = menu();
    assert_eq!(prompt::keys(&asked, Some(&asked), 1).unwrap(), ["1"]);
    let mut other = menu();
    other.entry = "toolu_2".into();
    for current in [None, Some(&other)] {
        assert_eq!(
            prompt::keys(&asked, current, 1).unwrap_err().code,
            ErrorCode::Conflict
        );
    }
}

#[test]
fn a_reply_needs_a_text_of_1_to_max_bytes() {
    let item = review("i-00000001");
    let with = |text: &str| {
        plan(
            &item,
            &Answer {
                text: Some(text.into()),
                ..answer("i-00000001", InboxAction::Reply)
            },
        )
    };
    for text in [String::new(), "x".repeat(MAX_TEXT + 1)] {
        assert_eq!(with(&text).unwrap_err().code, ErrorCode::BadRequest);
    }
    assert!(with(&"x".repeat(MAX_TEXT)).is_ok());
}

#[test]
fn a_reply_sends_the_text_as_it_is() {
    let with = |item: &InboxItem, text: &str| {
        plan(
            item,
            &Answer {
                text: Some(text.into()),
                ..answer(&item.id, InboxAction::Reply)
            },
        )
    };
    let text = "Use the cache.\nKeep the API.";
    assert_eq!(
        with(&review("i-00000001"), text),
        Ok(Plan::Reply(text.into()))
    );
    let mut failure = review("i-00000002");
    failure.kind = ItemKind::Failure;
    failure.session = Some("$1".into());
    assert_eq!(with(&failure, text), Ok(Plan::Reply(text.into())));
    for text in [String::new(), "x".repeat(MAX_TEXT + 1)] {
        let e = with(&review("i-00000001"), &text).unwrap_err();
        assert_eq!(e.code, ErrorCode::BadRequest);
        assert!(e.message.starts_with("reply needs a text"), "{}", e.message);
    }
}

#[test]
fn an_asks_reply_denies_the_menu_then_sends_the_text() {
    let item = ask("i-00000001");
    let planned = plan(
        &item,
        &Answer {
            text: Some("Run the tests first.".into()),
            ..answer("i-00000001", InboxAction::Reply)
        },
    );
    let Ok(Plan::DenyAndReply {
        pane,
        asked,
        option,
        text,
    }) = planned
    else {
        panic!("{planned:?}");
    };
    assert_eq!(pane.to_string(), "%3");
    assert_eq!(asked, menu());
    assert!(asked.options[option].cancel, "the cancel option");
    assert_eq!(text, "Run the tests first.");

    let mut question = ask("i-00000002");
    question.prompt.as_mut().unwrap().kind = PromptKind::Question;
    let e = plan(
        &question,
        &Answer {
            text: Some("x".into()),
            ..answer("i-00000002", InboxAction::Reply)
        },
    )
    .unwrap_err();
    assert_eq!(
        e.code,
        ErrorCode::BadRequest,
        "only a permission takes reply"
    );
}

#[test]
fn a_denied_menu_is_taken_once_gone_and_idle() {
    use muxa_protocol::AgentState::*;
    let asked = menu();
    let mut other = menu();
    other.entry = "toolu_2".into();
    assert!(prompt::taken(&asked, None, Some(Idle)));
    assert!(prompt::taken(&asked, Some(&other), Some(Done)));
    assert!(
        !prompt::taken(&asked, Some(&asked), Some(Idle)),
        "still shown"
    );
    for state in [None, Some(Waiting), Some(Running), Some(Error)] {
        assert!(!prompt::taken(&asked, None, state), "{state:?}");
    }
}
