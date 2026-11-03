//! The Inbox of ADR 0015, as `--json` prints it for desktop tools such as
//! tmux-agents (ADR 0017, `docs/agents.md`). The same rule as the apps'
//! `Inbox.swift` and `Inbox.kt`: every state comes from an adapter.

use muxa_protocol::{
    Agent, AgentState, LoopReason, LoopState, Pane, Role, Session, TaskInbox, TaskRecord,
    TaskStatus,
};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    NeedsAttention,
    ReadyForReview,
    Running,
    NoAgent,
    Closed,
}

impl Group {
    pub fn wire(self) -> &'static str {
        match self {
            Group::NeedsAttention => "needs-attention",
            Group::ReadyForReview => "ready-for-review",
            Group::Running => "running",
            Group::NoAgent => "no-agent",
            Group::Closed => "closed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    None,
    Answer,
    AgentError,
    Loop,
    Review,
    NoAgent,
    NoSession,
}

impl Need {
    pub fn wire(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Answer => "answer",
            Self::AgentError => "agent-error",
            Self::Loop => "loop",
            Self::Review => "review",
            Self::NoAgent => "no-agent",
            Self::NoSession => "no-session",
        }
    }
}

/// A record with the session that runs it and the state that can be believed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item<'a> {
    pub record: &'a TaskRecord,
    pub session: Option<&'a Session>,
    pub agent: Option<&'a Agent>,
    pub group: Group,
    pub need: Need,
}

pub fn item<'a>(record: &'a TaskRecord, sessions: &'a [Session]) -> Item<'a> {
    let session = sessions
        .iter()
        .find(|s| s.task_id.as_deref() == Some(record.id.as_str()));
    // A shell in the foreground means the state is likely stale: no state.
    let agent = session
        .and_then(agent_pane)
        .filter(|p| !p.shell_foreground)
        .and_then(|p| p.agent.as_ref());
    let group = match (record.status, agent.map(|a| a.state)) {
        (TaskStatus::Done | TaskStatus::Dropped, _) => Group::Closed,
        _ if loop_needs_user(record) => Group::NeedsAttention,
        (_, None) => Group::NoAgent,
        (_, Some(AgentState::Waiting | AgentState::Error)) => Group::NeedsAttention,
        (_, Some(AgentState::Running)) => Group::Running,
        (_, Some(AgentState::Idle | AgentState::Done)) => Group::ReadyForReview,
    };
    let need = match (group, agent.map(|a| a.state)) {
        (Group::Closed, _) => Need::None,
        (_, Some(AgentState::Waiting)) => Need::Answer,
        (_, Some(AgentState::Error)) => Need::AgentError,
        _ if loop_needs_user(record) => Need::Loop,
        (Group::Running, _) => Need::None,
        (Group::ReadyForReview, _) => Need::Review,
        _ if session.is_none() => Need::NoSession,
        _ => Need::NoAgent,
    };
    Item {
        record,
        session,
        agent: agent.filter(|_| group != Group::Closed),
        group,
        need,
    }
}

/// A loop that paused for the user, or stopped for a reason the user did not
/// give (ADR 0019).
fn loop_needs_user(record: &TaskRecord) -> bool {
    record.r#loop.as_ref().is_some_and(|l| match l.state {
        LoopState::Paused => true,
        LoopState::Stopped => !matches!(l.reason, Some(LoopReason::User | LoopReason::Status)),
        LoopState::Armed | LoopState::Checking | LoopState::Passed => false,
    })
}

/// The tagged agent pane, else the first visible pane an adapter reports on.
fn agent_pane(s: &Session) -> Option<&Pane> {
    let visible = || s.panes.iter().filter(|p| p.role != Role::Hidden);
    visible()
        .find(|p| p.role == Role::Agent)
        .or_else(|| visible().find(|p| p.agent.is_some()))
}

/// Every record, in group order, newest change first within a group.
pub fn items<'a>(records: &'a [TaskRecord], sessions: &'a [Session]) -> Vec<Item<'a>> {
    let mut all: Vec<Item> = records.iter().map(|r| item(r, sessions)).collect();
    all.sort_by(|a, b| {
        a.group
            .cmp(&b.group)
            .then(b.record.updated_at.cmp(&a.record.updated_at))
            .then(b.record.id.cmp(&a.record.id))
    });
    all
}

/// The record's Move down and Later (ADR 0027 §4), unless the Task's current
/// attention began after they were set: a new need surfaces again.
pub fn place<'a>(item: &Item<'a>) -> Option<&'a TaskInbox> {
    let place = item.record.inbox.as_deref()?;
    let agent = item.agent.and_then(|a| a.since);
    let paused = item
        .record
        .r#loop
        .as_ref()
        .filter(|_| loop_needs_user(item.record))
        .map(|l| l.since);
    match agent.max(paused) {
        Some(since) if since > place.since => None,
        _ => Some(place),
    }
}

/// The actionable queue at `now` (epoch seconds): every item with a need, in
/// group order, then the moved-down ones by `order`. Later hides an item
/// until its time.
pub fn queue<'a>(records: &'a [TaskRecord], sessions: &'a [Session], now: u64) -> Vec<Item<'a>> {
    let mut queue: Vec<Item> = items(records, sessions)
        .into_iter()
        .filter(|i| i.need != Need::None)
        .filter(|i| {
            !place(i)
                .and_then(|p| p.until)
                .is_some_and(|until| until > now)
        })
        .collect();
    // Stable: equal orders keep the group order.
    queue.sort_by_key(|i| place(i).and_then(|p| p.order));
    queue
}

pub fn to_json(item: &Item) -> Value {
    let mut out = json!({
        "group": item.group.wire(),
        "need": item.need.wire(),
        "record": item.record,
    });
    if let Some(s) = item.session {
        out["session"] = json!({"id": s.id, "name": s.name});
    }
    if let Some(a) = item.agent {
        out["agent"] = json!(a);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, status: TaskStatus, updated_at: u64) -> TaskRecord {
        TaskRecord {
            restart_operation: None,
            worker: None,
            id: id.into(),
            rev: 1,
            workspace: "work".into(),
            title: id.into(),
            goal: "g".into(),
            status,
            created_at: 0,
            updated_at,
            project: None,
            repo: None,
            branch: None,
            dir: None,
            session: None,
            note: None,
            runs: Vec::new(),
            r#loop: None,
            pending_prompt: None,
            inbox: None,
        }
    }

    fn pane(role: Role, state: Option<AgentState>, shell_foreground: bool) -> Pane {
        Pane {
            id: "%1".into(),
            window: "@1".into(),
            index: 0,
            role,
            cols: 80,
            rows: 24,
            command: "claude".into(),
            path: "/w".into(),
            dead: false,
            shell_foreground,
            agent: state.map(|state| Agent {
                state,
                since: Some(5),
            }),
        }
    }

    fn session(task: &str, panes: Vec<Pane>) -> Session {
        Session {
            id: format!("${task}"),
            name: format!("s-{task}"),
            repo: None,
            repo_dir: None,
            branch: None,
            worktree: None,
            task: None,
            task_id: Some(task.into()),
            panes,
        }
    }

    fn group(status: TaskStatus, panes: Vec<Pane>) -> Group {
        let r = record("t-1", status, 0);
        let sessions = [session("t-1", panes)];
        item(&r, &sessions).group
    }

    #[test]
    fn groups_follow_the_agent_state_and_the_status() {
        use AgentState::*;
        let agent = |s| vec![pane(Role::Agent, Some(s), false)];
        assert_eq!(
            group(TaskStatus::Open, agent(Waiting)),
            Group::NeedsAttention
        );
        assert_eq!(group(TaskStatus::Open, agent(Error)), Group::NeedsAttention);
        assert_eq!(group(TaskStatus::Open, agent(Running)), Group::Running);
        assert_eq!(group(TaskStatus::Open, agent(Idle)), Group::ReadyForReview);
        assert_eq!(
            group(TaskStatus::Review, agent(Done)),
            Group::ReadyForReview
        );
        assert_eq!(group(TaskStatus::Done, agent(Waiting)), Group::Closed);
        assert_eq!(group(TaskStatus::Dropped, vec![]), Group::Closed);
    }

    #[test]
    fn no_believable_state_is_no_agent() {
        assert_eq!(group(TaskStatus::Open, vec![]), Group::NoAgent);
        assert_eq!(
            group(
                TaskStatus::Open,
                vec![pane(Role::Agent, Some(AgentState::Running), true)]
            ),
            Group::NoAgent
        );
        // A hidden pane never stands for the session.
        assert_eq!(
            group(
                TaskStatus::Open,
                vec![pane(Role::Hidden, Some(AgentState::Running), false)]
            ),
            Group::NoAgent
        );
        let r = record("t-2", TaskStatus::Open, 0);
        let other = [session(
            "t-1",
            vec![pane(Role::Agent, Some(AgentState::Running), false)],
        )];
        assert_eq!(item(&r, &other).group, Group::NoAgent);
    }

    #[test]
    fn an_untagged_pane_with_a_state_stands_for_the_session() {
        assert_eq!(
            group(
                TaskStatus::Open,
                vec![
                    pane(Role::Shell, None, true),
                    pane(Role::Unknown, Some(AgentState::Waiting), false)
                ]
            ),
            Group::NeedsAttention
        );
    }

    #[test]
    fn items_come_in_group_order_newest_first() {
        let records = [
            record("t-old", TaskStatus::Open, 1),
            record("t-done", TaskStatus::Done, 9),
            record("t-new", TaskStatus::Open, 5),
            record("t-wait", TaskStatus::Open, 0),
        ];
        let sessions = [session(
            "t-wait",
            vec![pane(Role::Agent, Some(AgentState::Waiting), false)],
        )];
        let ids: Vec<&str> = items(&records, &sessions)
            .iter()
            .map(|i| i.record.id.as_str())
            .collect();
        assert_eq!(ids, ["t-wait", "t-new", "t-old", "t-done"]);
    }

    #[test]
    fn json_names_the_session_by_id_and_the_believed_state() {
        let r = record("t-1", TaskStatus::Open, 3);
        let sessions = [session(
            "t-1",
            vec![pane(Role::Agent, Some(AgentState::Waiting), false)],
        )];
        let v = to_json(&item(&r, &sessions));
        assert_eq!(v["group"], "needs-attention");
        assert_eq!(v["session"], json!({"id": "$t-1", "name": "s-t-1"}));
        assert_eq!(v["agent"], json!({"state": "waiting", "since": 5}));
        assert_eq!(v["record"]["id"], "t-1");
        assert_eq!(v["record"]["updatedAt"], 3);

        let lone = record("t-2", TaskStatus::Open, 0);
        let v = to_json(&item(&lone, &[]));
        assert_eq!(v["group"], "no-agent");
        assert!(v.get("session").is_none() && v.get("agent").is_none());
    }

    /// `protocol/fixtures/inbox` pins the rule shared with `Inbox.swift` and `Inbox.kt`.
    #[test]
    fn groups_match_the_shared_fixture() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../protocol/fixtures/inbox/groups.jsonl"
        );
        let text = std::fs::read_to_string(path).expect("the inbox fixture");
        let mut n = 0;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let case: Value = serde_json::from_str(line).expect("a fixture line");
            let record: TaskRecord =
                serde_json::from_value(case["record"].clone()).expect("a record");
            let sessions: Vec<Session> =
                serde_json::from_value(case["sessions"].clone()).expect("sessions");
            assert_eq!(
                item(&record, &sessions).group.wire(),
                case["group"],
                "{}",
                case["name"]
            );
            assert_eq!(
                item(&record, &sessions).need.wire(),
                case["need"],
                "{}",
                case["name"]
            );
            n += 1;
        }
        assert_eq!(n, 28);
    }

    /// `protocol/fixtures/inbox/queue.jsonl` pins Move down and Later for all three clients.
    #[test]
    fn the_queue_matches_the_shared_fixture() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../protocol/fixtures/inbox/queue.jsonl"
        );
        let text = std::fs::read_to_string(path).expect("the queue fixture");
        let mut n = 0;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let case: Value = serde_json::from_str(line).expect("a fixture line");
            let records: Vec<TaskRecord> =
                serde_json::from_value(case["records"].clone()).expect("records");
            let sessions: Vec<Session> =
                serde_json::from_value(case["sessions"].clone()).expect("sessions");
            let now = case["now"].as_u64().expect("now");
            let ids: Vec<&str> = queue(&records, &sessions, now)
                .iter()
                .map(|i| i.record.id.as_str())
                .collect();
            assert_eq!(json!(ids), case["queue"], "{}", case["name"]);
            n += 1;
        }
        assert_eq!(n, 16);
    }
}
