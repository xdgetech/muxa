use super::evidence::{MAX_PATHS, MAX_SUMMARY, cut, of_changes, reply};
use super::*;
use crate::items::store::tests_item;
use crate::records::LoopSpec;
use muxa_protocol::{ChangeInfo, ChangeStatus, ChatEntry, ChatKind, PromptOption};

const T0: u64 = 1_000;

fn s(state: AgentState, since: u64) -> Option<AgentStatus> {
    Some(AgentStatus {
        state,
        since: Some(since),
    })
}

fn open(agent: Option<AgentStatus>) -> Need {
    need(Status::Open, None, None, agent)
}

fn a_loop(state: LoopState, reason: Option<Reason>, since: u64) -> Loop {
    let spec = LoopSpec {
        check: "make check".into(),
        max_attempts: None,
        timeout: None,
    };
    Loop {
        state,
        reason,
        since,
        ..Loop::start(spec, None, false, T0)
    }
}

fn stored(kind: ItemKind, state: ItemState, key: &str) -> StoredItem {
    let mut item = tests_item("i-00000001", state);
    item.kind = kind;
    StoredItem {
        item,
        key: key.into(),
    }
}

fn file(items: Vec<StoredItem>) -> TaskItems {
    let mut t = TaskItems::new("t-000001");
    t.items = items;
    t
}

#[test]
fn each_adapter_state_asks_its_own_need() {
    assert_eq!(open(s(AgentState::Waiting, T0)), Need::Ask(T0));
    assert_eq!(
        open(s(AgentState::Done, T0)),
        Need::Turn(T0, AgentState::Done)
    );
    assert_eq!(
        open(s(AgentState::Idle, T0)),
        Need::Turn(T0, AgentState::Idle)
    );
    assert_eq!(open(s(AgentState::Error, T0)), Need::Error(T0));
    assert_eq!(open(s(AgentState::Running, T0)), Need::Resolve);
    assert_eq!(
        Need::Turn(T0, AgentState::Idle).key().unwrap(),
        "review:1000"
    );
    assert_eq!(Need::Ask(T0).key().unwrap(), "ask:1000");
    assert_eq!(Need::Error(T0).key().unwrap(), "failure:1000");
}

#[test]
fn no_state_no_since_or_a_finished_task_asks_nothing() {
    assert_eq!(open(None), Need::Nothing, "invariant 6");
    let no_since = Some(AgentStatus {
        state: AgentState::Idle,
        since: None,
    });
    assert_eq!(open(no_since), Need::Nothing);
    for status in [Status::Done, Status::Dropped] {
        assert_eq!(
            need(status, None, None, s(AgentState::Waiting, T0)),
            Need::Nothing
        );
    }
    assert_eq!(
        need(Status::Review, None, None, s(AgentState::Done, T0)),
        Need::Turn(T0, AgentState::Done)
    );
}

#[test]
fn a_state_older_than_the_newest_run_is_ignored() {
    assert_eq!(
        need(Status::Open, None, Some(T0 + 1), s(AgentState::Idle, T0)),
        Need::Nothing
    );
}

#[test]
fn an_active_loop_makes_no_item_per_turn() {
    let lp = a_loop(LoopState::Armed, None, T0);
    let on = |agent| need(Status::Open, Some(&lp), None, agent);
    assert_eq!(on(s(AgentState::Idle, T0 + 5)), Need::Nothing);
    assert_eq!(on(s(AgentState::Error, T0 + 5)), Need::Nothing);
    assert_eq!(on(s(AgentState::Waiting, T0 + 5)), Need::Ask(T0 + 5));
    assert_eq!(on(s(AgentState::Running, T0 + 5)), Need::Resolve);
}

#[test]
fn an_ended_loop_makes_its_result_until_a_newer_state() {
    let passed = a_loop(LoopState::Passed, None, T0 + 20);
    let turn = s(AgentState::Idle, T0 + 10);
    assert_eq!(
        need(Status::Review, Some(&passed), None, turn),
        Need::Passed(T0 + 20)
    );
    assert_eq!(Need::Passed(T0 + 20).key().unwrap(), "review:1020");
    let stopped = a_loop(LoopState::Stopped, Some(Reason::Limit), T0 + 20);
    assert_eq!(
        need(Status::Open, Some(&stopped), None, turn),
        Need::Stopped(T0 + 20, Reason::Limit)
    );
    // The user's next turn after the loop is a turn again.
    assert_eq!(
        need(
            Status::Open,
            Some(&stopped),
            None,
            s(AgentState::Running, T0 + 30)
        ),
        Need::Resolve
    );
    assert_eq!(
        need(
            Status::Open,
            Some(&stopped),
            None,
            s(AgentState::Done, T0 + 40)
        ),
        Need::Turn(T0 + 40, AgentState::Done)
    );
}

#[test]
fn a_loop_the_user_stopped_asks_nothing_of_its_own() {
    for reason in [Reason::User, Reason::Status] {
        let lp = a_loop(LoopState::Stopped, Some(reason), T0 + 20);
        assert_eq!(
            need(Status::Open, Some(&lp), None, s(AgentState::Idle, T0)),
            Need::Turn(T0, AgentState::Idle)
        );
    }
}

#[test]
fn a_stored_key_never_makes_a_second_item() {
    let need = Need::Turn(T0, AgentState::Idle);
    assert_eq!(
        change(&need, &file(vec![])),
        Change::Make("review:1000".into())
    );
    let answered = file(vec![stored(
        ItemKind::Review,
        ItemState::Answered,
        "review:1000",
    )]);
    assert_eq!(change(&need, &answered), Change::None);
    let newer = Need::Turn(T0 + 1, AgentState::Idle);
    assert_eq!(
        change(&newer, &answered),
        Change::Make("review:1001".into())
    );
}

#[test]
fn running_resolves_only_an_unanswered_task_item() {
    let open = file(vec![stored(ItemKind::Ask, ItemState::Open, "ask:1")]);
    assert_eq!(change(&Need::Resolve, &open), Change::Resolve);
    let later = file(vec![stored(
        ItemKind::Failure,
        ItemState::Later,
        "failure:1",
    )]);
    assert_eq!(change(&Need::Resolve, &later), Change::Resolve);
    let answered = file(vec![stored(
        ItemKind::Review,
        ItemState::Answered,
        "review:1",
    )]);
    assert_eq!(change(&Need::Resolve, &answered), Change::None);
    assert_eq!(change(&Need::Resolve, &file(vec![])), Change::None);
}

#[test]
fn resolving_closes_the_current_item_with_no_answers() {
    let mut t = file(vec![
        stored(ItemKind::Review, ItemState::Answered, "review:1"),
        stored(ItemKind::Ask, ItemState::Later, "ask:2"),
    ]);
    t.items[1].item.until = Some(9);
    t.items[1].item.answers = vec![muxa_protocol::InboxAction::Later];
    assert!(resolve(&mut t));
    let item = &t.items[1].item;
    assert_eq!(item.state, ItemState::Resolved);
    assert_eq!((item.until, item.answers.len()), (None, 0));
    assert_eq!(t.items[0].item.state, ItemState::Answered);
    assert!(!resolve(&mut t), "nothing open is left");
}

fn prompt(kind: PromptKind, question: Option<&str>) -> ChatPrompt {
    ChatPrompt {
        entry: "toolu_1".into(),
        kind,
        question: question.map(str::to_owned),
        options: vec![PromptOption {
            label: "Yes".into(),
            keys: vec!["1".into()],
            cancel: false,
        }],
    }
}

#[test]
fn the_question_says_what_the_agent_waits_on() {
    let ask = Need::Ask(T0);
    let p = prompt(PromptKind::Permission, None);
    assert_eq!(
        question(&ask, "T", Some(&p), Some(("Bash", "cargo test\n--all"))),
        "Allow Bash: cargo test --all"
    );
    let q = prompt(PromptKind::Question, Some("Which one?"));
    assert_eq!(question(&ask, "T", Some(&q), None), "Which one?");
    let plan = prompt(PromptKind::Plan, None);
    assert_eq!(question(&ask, "T", Some(&plan), None), "Approve the plan?");
    assert_eq!(question(&ask, "T", None, None), "The agent waits for you");
    assert_eq!(
        question(&Need::Turn(T0, AgentState::Done), "Fix login", None, None),
        "Ready for review: Fix login"
    );
    assert_eq!(
        question(&Need::Error(T0), "T", None, None),
        "The agent stopped with an error"
    );
}

#[test]
fn a_long_question_is_cut_to_200_characters() {
    let p = prompt(PromptKind::Permission, None);
    let long = "é".repeat(500);
    let q = question(&Need::Ask(T0), "T", Some(&p), Some(("Bash", &long)));
    assert_eq!(q.chars().count(), MAX_QUESTION);
    assert!(q.starts_with("Allow Bash: é") && q.ends_with('…'));
}

fn entry(kind: ChatKind, text: &str) -> ChatEntry {
    ChatEntry {
        id: format!("{kind:?}-{text}"),
        kind,
        text: text.into(),
        tool: None,
    }
}

#[test]
fn the_summary_is_the_last_reply_when_no_prompt_follows_it() {
    let turn = [
        entry(ChatKind::User, "fix it"),
        entry(ChatKind::Assistant, "first"),
        entry(ChatKind::Tool, ""),
        entry(ChatKind::Assistant, "  Fixed.  "),
    ];
    assert_eq!(reply(&turn), (Some("Fixed.".into()), true));
    let asked = [turn[1].clone(), entry(ChatKind::User, "again")];
    assert_eq!(reply(&asked), (Some("first".into()), false));
    assert_eq!(
        reply(&[]),
        (None, false),
        "a fresh agent has replied nothing"
    );
}

#[test]
fn a_summary_is_at_most_4096_bytes_on_a_char_boundary() {
    let text = "あ".repeat(2000);
    let s = cut(&text, MAX_SUMMARY);
    assert!(s.len() <= MAX_SUMMARY && s.len() > MAX_SUMMARY - 3);
    assert!(text.starts_with(&s));
    assert_eq!(cut("short", MAX_SUMMARY), "short");
}

fn change_info(path: &str, status: ChangeStatus, added: u32) -> ChangeInfo {
    ChangeInfo {
        path: path.into(),
        status,
        old_path: None,
        added: Some(added),
        removed: Some(1),
    }
}

#[test]
fn attachments_count_the_changes_and_list_docs_and_images() {
    let mut entries = vec![
        change_info("README.md", ChangeStatus::Modified, 3),
        change_info("shot.PNG", ChangeStatus::Added, 0),
        change_info("gone.md", ChangeStatus::Deleted, 0),
        change_info("src/main.rs", ChangeStatus::Modified, 10),
    ];
    entries[3].added = None;
    let a = of_changes(&entries, Some("branch".into()));
    let c = a.changes.unwrap();
    assert_eq!((c.files, c.added, c.removed), (4, 3, 4));
    assert_eq!(c.scope.as_deref(), Some("branch"));
    assert_eq!(a.docs, ["README.md"]);
    assert_eq!(a.images, ["shot.PNG"]);
    let many: Vec<_> = (0..30)
        .map(|n| change_info(&format!("d{n:02}.md"), ChangeStatus::Added, 1))
        .collect();
    assert_eq!(of_changes(&many, None).docs.len(), MAX_PATHS);
}
