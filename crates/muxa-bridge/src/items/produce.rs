//! Makes and resolves a Task's Inbox items from the states its adapters
//! supply (ADR 0033 §2). Never from terminal content (invariant 6).
mod evidence;
mod reading;
mod summary;
mod turn;

use muxa_protocol::{Attachments, ChatPrompt, InboxItem, ItemKind, ItemState, PromptKind};
use muxa_tmux::{AgentState, AgentStatus, Pane, Role, Session};

use crate::agents::Registry;
use crate::loops::step::current;
use crate::records::{Loop, LoopState, Reason, Record, Status, now};
use crate::routines::store::StateError;
use crate::task_records::Config;

use super::store::{ItemStore, StoredItem, TaskItems, new_id};

/// Characters in a question.
const MAX_QUESTION: usize = 200;

/// What a Task's agent and loop ask of the Inbox now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Need {
    Nothing,
    /// The need ended: the agent runs again.
    Resolve,
    /// `waiting` since then.
    Ask(u64),
    /// A finished turn. `idle` is one only when the transcript, if any, ends with a reply.
    Turn(u64, AgentState),
    /// `error` since then.
    Error(u64),
    /// The loop passed then.
    Passed(u64),
    /// The loop stopped then.
    Stopped(u64, Reason),
}

impl Need {
    fn kind(&self) -> Option<ItemKind> {
        Some(match self {
            Need::Ask(_) => ItemKind::Ask,
            Need::Turn(..) | Need::Passed(_) => ItemKind::Review,
            Need::Error(_) | Need::Stopped(..) => ItemKind::Failure,
            Need::Nothing | Need::Resolve => return None,
        })
    }

    /// `<kind>:<since>`: the same need never makes a second item.
    fn key(&self) -> Option<String> {
        let since = match self {
            Need::Ask(t) | Need::Turn(t, _) | Need::Error(t) | Need::Passed(t) => *t,
            Need::Stopped(t, _) => *t,
            Need::Nothing | Need::Resolve => return None,
        };
        let kind = match self.kind()? {
            ItemKind::Ask => "ask",
            ItemKind::Review => "review",
            _ => "failure",
        };
        Some(format!("{kind}:{since}"))
    }
}

/// The need of a Task with this status, loop and agent state. `run_started_at`
/// is the newest run's start: an older state survived `respawn-pane`.
pub(crate) fn need(
    status: Status,
    lp: Option<&Loop>,
    run_started_at: Option<u64>,
    agent: Option<AgentStatus>,
) -> Need {
    if !matches!(status, Status::Open | Status::Review) {
        return Need::Nothing;
    }
    let agent = current(agent, run_started_at);
    // An active loop makes no item per turn: its result does (ADR 0019).
    if lp.is_some_and(Loop::active) {
        return match agent.map(|s| s.state) {
            Some(AgentState::Waiting) => agent_need(agent),
            Some(AgentState::Running) => Need::Resolve,
            _ => Need::Nothing,
        };
    }
    let ended = lp.and_then(loop_need);
    let agent_since = agent.and_then(|s| s.since);
    match (ended, agent_since) {
        (Some((at, n)), since) if since.is_none_or(|t| t <= at) => n,
        _ => agent_need(agent),
    }
}

fn agent_need(agent: Option<AgentStatus>) -> Need {
    let Some(s) = agent else {
        return Need::Nothing;
    };
    if s.state == AgentState::Running {
        return Need::Resolve;
    }
    // A state with no `since` is never a new need, as for the loop.
    let Some(t) = s.since else {
        return Need::Nothing;
    };
    match s.state {
        AgentState::Waiting => Need::Ask(t),
        AgentState::Idle | AgentState::Done => Need::Turn(t, s.state),
        AgentState::Error => Need::Error(t),
        AgentState::Running => Need::Resolve,
    }
}

/// An ended loop's result. A stop the user caused, directly or by moving the
/// Task, asks nothing.
fn loop_need(lp: &Loop) -> Option<(u64, Need)> {
    match (&lp.state, &lp.reason) {
        (LoopState::Passed, _) => Some((lp.since, Need::Passed(lp.since))),
        (LoopState::Stopped, Some(Reason::User | Reason::Status)) => None,
        (LoopState::Stopped, r) => Some((
            lp.since,
            Need::Stopped(
                lp.since,
                r.clone().unwrap_or(Reason::Unknown(String::new())),
            ),
        )),
        _ => None,
    }
}

/// What to do to the Task's items for `need`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Change {
    None,
    /// Resolve the open or later item.
    Resolve,
    /// Make an item for this key, resolving the current one first.
    Make(String),
}

pub(crate) fn change(need: &Need, items: &TaskItems) -> Change {
    let current = items
        .items
        .iter()
        .rev()
        .find(|s| matches!(s.item.state, ItemState::Open | ItemState::Later));
    match need {
        Need::Nothing => Change::None,
        Need::Resolve => match current {
            Some(c) if is_task_kind(c.item.kind) => Change::Resolve,
            _ => Change::None,
        },
        _ => match need.key() {
            Some(k) if !items.items.iter().any(|s| s.key == k) => Change::Make(k),
            _ => Change::None,
        },
    }
}

fn is_task_kind(kind: ItemKind) -> bool {
    matches!(kind, ItemKind::Ask | ItemKind::Review | ItemKind::Failure)
}

/// Blocking. A Task that is done or dropped asks nothing more: its open
/// item resolves. An Inbox answer that closed it writes `answered` after this.
pub(crate) fn task_closed(config: &Config, task: &str) {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return;
    };
    let resolved = ItemStore::open(dir).and_then(|s| {
        s.update::<_, crate::routines::store::StateError>(task, |items| Ok(resolve(items)))
    });
    match resolved {
        Ok(true) => tracing::info!(task, "inbox item resolved: the task is closed"),
        Ok(false) => {}
        Err(e) => tracing::warn!(task, error = %e, "cannot resolve the task's inbox item"),
    }
}

fn resolve(items: &mut TaskItems) -> bool {
    match items.current_mut() {
        Some(c) if is_task_kind(c.item.kind) => {
            c.item.state = ItemState::Resolved;
            c.item.until = None;
            c.item.answers = Vec::new();
            true
        }
        _ => false,
    }
}

/// The one line the item asks.
pub(crate) fn question(
    need: &Need,
    title: &str,
    prompt: Option<&ChatPrompt>,
    tool: Option<(&str, &str)>,
) -> String {
    let text = match need {
        Need::Ask(_) => match prompt.map(|p| (p.kind, p.question.as_deref())) {
            Some((PromptKind::Permission, _)) => match tool {
                Some((name, "")) => format!("Allow {name}"),
                Some((name, summary)) => format!("Allow {name}: {summary}"),
                None => "The agent asks for a permission".into(),
            },
            Some((PromptKind::Question, Some(q))) => q.to_owned(),
            Some((PromptKind::Question, None)) => "The agent asks a question".into(),
            Some((PromptKind::Plan, _)) => "Approve the plan?".into(),
            None => "The agent waits for you".into(),
        },
        Need::Turn(..) | Need::Passed(_) => format!("Ready for review: {title}"),
        Need::Error(_) => "The agent stopped with an error".into(),
        Need::Stopped(_, reason) => stop_question(reason).into(),
        Need::Nothing | Need::Resolve => String::new(),
    };
    one_line(&text, MAX_QUESTION)
}

fn stop_question(reason: &Reason) -> &'static str {
    match reason {
        Reason::Limit => "The loop stopped: the check still fails",
        Reason::Timeout => "The loop stopped: the check timed out",
        Reason::Error => "The agent stopped with an error",
        Reason::NoAgent => "The loop stopped: the agent is gone",
        Reason::NotSent => "The loop stopped: its prompt could not be typed",
        Reason::Stalled => "The loop stopped: the agent did not take its prompt",
        _ => "The loop stopped",
    }
}

/// Whitespace and control characters become single spaces; at most `max` characters.
fn one_line(text: &str, max: usize) -> String {
    let mut out = String::new();
    for word in text
        .split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|w| !w.is_empty())
    {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.chars().count() > max {
        out = out.chars().take(max - 1).collect();
        out.push('…');
    }
    out
}

/// Blocking. Called after each look at the sessions, by a bridge and by
/// the daemon. Each need makes at most one item, whoever looks first.
/// True when an open Task has a session: the daemon keeps looking then.
pub(crate) fn observe(sessions: &[Session], config: &Config, agents: &Registry) -> bool {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return false;
    };
    let records: Vec<Record> = config
        .records_if_any()
        .into_iter()
        .filter(watched)
        .collect();
    if records.is_empty() {
        return false;
    }
    let store = match ItemStore::open(dir) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "cannot open the inbox items");
            return false;
        }
    };
    let mut linked = false;
    for record in &records {
        let Some(session) = sessions
            .iter()
            .find(|s| s.task_id.as_deref() == Some(record.id.as_str()))
        else {
            continue;
        };
        linked = true;
        let Some(pane) = agent_pane(session) else {
            continue;
        };
        if let Err(e) = one(&store, record, session, pane, agents) {
            tracing::warn!(task = %record.id, error = %e, "cannot update inbox items");
        }
    }
    linked
}

/// Open Tasks; a Routine run's items are its entries (ADR 0026).
pub(crate) fn watched(r: &Record) -> bool {
    matches!(r.status, Status::Open | Status::Review) && !r.is_routine_run()
}

/// The Task's agent pane whose adapter supplies a state.
fn agent_pane(session: &Session) -> Option<&Pane> {
    session
        .panes
        .iter()
        .find(|p| p.role == Role::Agent && !p.dead && p.agent.is_some())
}

fn one(
    store: &ItemStore,
    record: &Record,
    session: &Session,
    pane: &Pane,
    agents: &Registry,
) -> Result<(), StateError> {
    let lp = record.task_loop().unwrap_or_else(|e| {
        tracing::warn!(task = %record.id, error = %e, "unreadable loop; ignoring it");
        None
    });
    let need = need(
        record.status,
        lp.as_ref(),
        record.runs.last().map(|r| r.started_at),
        pane.agent,
    );
    let run_start = record.runs.last().map(|r| r.started_at);
    let agent = current(pane.agent, run_start);
    turn::note(store, record, agent)?;
    if record.is_read() {
        reading::note_start(store, record, agent)?;
    }
    let items = store.get(&record.id)?;
    match change(&need, &items) {
        Change::None => Ok(()),
        Change::Resolve => {
            let resolved = store.update::<_, StateError>(&record.id, |t| Ok(resolve(t)))?;
            if resolved {
                tracing::info!(task = %record.id, "inbox item resolved");
            }
            Ok(())
        }
        Change::Make(key) => make(store, record, session, pane, agents, &need, key),
    }
}

fn make(
    store: &ItemStore,
    record: &Record,
    session: &Session,
    pane: &Pane,
    agents: &Registry,
    need: &Need,
    key: String,
) -> Result<(), StateError> {
    let Some(kind) = need.kind() else {
        return Ok(());
    };
    let chat = evidence::chat(agents, &session.name, pane);
    if matches!(need, Need::Turn(_, AgentState::Idle)) && chat.replied == Some(false) {
        // Idle before its first reply, e.g. a fresh agent before its prompt.
        return Ok(());
    }
    let prompt = if kind == ItemKind::Ask {
        chat.prompt
    } else {
        None
    };
    let tool = chat.tool.as_ref().map(|(n, s)| (n.as_str(), s.as_str()));
    let mut item = InboxItem {
        id: new_id()?,
        kind,
        state: ItemState::Open,
        question: question(need, &record.title, prompt.as_ref(), tool),
        workspace: record.workspace.clone(),
        title: record.title.clone(),
        created_at: now(),
        read_at: None,
        until: None,
        answered_at: None,
        answer: None,
        summary: chat.summary,
        task: Some(record.id.clone()),
        session: Some(session.id.to_string()),
        pane: Some(pane.id.to_string()),
        prompt,
        attachments: Attachments::default(),
        routine: None,
        proposal: None,
        answers: Vec::new(),
    };
    let since = match need {
        Need::Turn(ended, _) => turn::start(
            &store.get(&record.id)?,
            record.runs.last().map(|r| r.started_at),
            *ended,
        ),
        _ => None,
    };
    item.attachments = evidence::attachments(
        std::path::Path::new(&pane.path),
        item.summary.as_deref(),
        since,
    );
    if let (Need::Turn(ended, _), Some(main)) = (need, record.repo.as_deref()) {
        if record.is_read() {
            // The turn's item becomes the question about the main checkout (ADR 0034 §2).
            if let Some(changed) = reading::take(store, record, *ended)? {
                item.kind = ItemKind::Proposal;
                item.question = "The agent changed the main checkout".into();
                item.summary = Some(changed.summary(main));
                item.attachments = changed.attachments(main);
            }
        }
    }
    item.answers = super::answers(&item);
    let id = item.id.clone();
    let made = store.update::<_, StateError>(&record.id, |t| {
        if t.items.iter().any(|s| s.key == key) {
            return Ok(false);
        }
        resolve(t);
        t.items.push(StoredItem {
            item,
            key: key.clone(),
        });
        Ok(true)
    })?;
    if made {
        tracing::info!(task = %record.id, item = %id, key, "inbox item made");
    }
    Ok(())
}

#[cfg(test)]
#[path = "produce_tests.rs"]
mod tests;
