//! `tasks`, `task new`, `task done`, `task restart` and the Task Loop: task
//! records (`task-records`, ADR 0015; `task-handoff`, ADR 0018; `task-loop`,
//! ADR 0019).

use std::time::Duration;

use muxa_protocol::{
    LoopSpec, LoopState, Message, Request, RestartRecoveryAction, TaskLoop, TaskRecord, TaskStatus,
    WorkerInfo, WorkerSelection,
};
use serde_json::json;

use crate::conn::Conn;
use crate::inbox;

/// A launch makes a worktree and starts an agent; the bridge allows 60 s.
const LAUNCH_WAIT: Duration = Duration::from_secs(90);
const WAIT: Duration = Duration::from_secs(10);
/// The bridge types the start prompt once the agent is idle, within 30 s.
const HANDOFF_WAIT: Duration = Duration::from_secs(45);

/// `task new`'s flags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewTask {
    pub workspace: String,
    pub title: String,
    pub goal: String,
    pub project: Option<String>,
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub base: Option<String>,
    pub worker: Option<String>,
    pub r#loop: Option<LoopSpec>,
}

pub struct Recovery {
    pub operation: String,
    pub rev: u64,
    pub action: RestartRecoveryAction,
}

/// Mutation uses only the identity explicitly supplied after inspection.
pub async fn recover(
    conn: &mut Conn,
    task: &str,
    resolution: Option<Recovery>,
    as_json: bool,
) -> Result<String, String> {
    if let Some(resolution) = resolution {
        if !conn.supports("task-restart-recovery") {
            return Err("unsupported: this host does not advertise task-restart-recovery".into());
        }
        let messages = conn
            .call(
                |id| Request::RecoverRestart {
                    id,
                    task: task.into(),
                    rev: resolution.rev,
                    operation: resolution.operation,
                    action: resolution.action,
                },
                WAIT,
            )
            .await?;
        let record = saved(&messages).ok_or("the bridge saved no task")?;
        return print_record(conn, record, as_json).await;
    }
    let (_, records, _, _) = fetch(conn).await?;
    let record = records
        .iter()
        .find(|r| r.id == task)
        .ok_or_else(|| format!("no task {task} in the bridge's task list"))?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string(record).map_err(|e| e.to_string())?
        );
    } else {
        println!("{}", line(record));
        println!("revision: {}", record.rev);
        if let Some(operation) = &record.restart_operation {
            println!("operation: {}", operation.id);
            println!("phase: {} (requested/unconfirmed)", operation.phase);
            let actions = operation
                .actions
                .iter()
                .filter_map(|value| RestartRecoveryAction::from_wire(value))
                .filter(|action| operation.allows(*action))
                .map(RestartRecoveryAction::as_str)
                .collect::<Vec<_>>();
            if conn.supports("task-restart-recovery") && !actions.is_empty() {
                println!("actions: {}", actions.join(", "));
            } else {
                println!("no recovery action is available; open Terminal");
            }
        } else {
            println!("no pending restart operation");
        }
    }
    Ok(String::new())
}

/// `--check`, `--max-attempts` and `--check-timeout`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoopFlags {
    pub check: Option<String>,
    pub max_attempts: Option<u32>,
    pub timeout: Option<u64>,
}

/// ADR 0019's bounds; the host checks them too.
const ATTEMPTS: std::ops::RangeInclusive<u32> = 1..=10;
const TIMEOUT: std::ops::RangeInclusive<u64> = 10..=3600;

impl LoopFlags {
    /// The loop the flags ask for; none without `--check`.
    pub fn spec(&self) -> Result<Option<LoopSpec>, lexopt::Error> {
        let bad = |m: &str| Err(lexopt::Error::Custom(m.into()));
        let Some(check) = &self.check else {
            if self.max_attempts.is_some() || self.timeout.is_some() {
                return bad("--max-attempts and --check-timeout need --check");
            }
            return Ok(None);
        };
        if check.trim().is_empty() {
            return bad("--check needs a command");
        }
        if self.max_attempts.is_some_and(|n| !ATTEMPTS.contains(&n)) {
            return bad("--max-attempts must be 1 to 10");
        }
        if self.timeout.is_some_and(|t| !TIMEOUT.contains(&t)) {
            return bad("--check-timeout must be 10 to 3600 seconds");
        }
        Ok(Some(LoopSpec {
            check: check.clone(),
            max_attempts: self.max_attempts,
            timeout: self.timeout,
        }))
    }
}

impl NewTask {
    /// The flags the bridge would refuse without, checked before connecting.
    pub fn checked(self) -> Result<Self, lexopt::Error> {
        let missing = [
            ("--workspace", &self.workspace),
            ("--title", &self.title),
            ("--goal", &self.goal),
        ]
        .into_iter()
        .find(|(_, v)| v.trim().is_empty());
        if let Some((flag, _)) = missing {
            return Err(lexopt::Error::Custom(
                format!("task new needs {flag}").into(),
            ));
        }
        if self.repo.is_some() != self.branch.is_some() {
            return Err(lexopt::Error::Custom(
                "task new needs --repo and --branch together".into(),
            ));
        }
        if self.base.is_some() && self.repo.is_none() {
            return Err(lexopt::Error::Custom("--base needs --repo".into()));
        }
        Ok(self)
    }
}

pub async fn list(conn: &mut Conn, as_json: bool) -> Result<String, String> {
    let (workspaces, records, truncated, workers) = fetch(conn).await?;
    if as_json {
        let sessions = conn.list().await?;
        let tasks: Vec<_> = inbox::items(&records, &sessions)
            .iter()
            .map(inbox::to_json)
            .collect();
        let mut out = json!({"workspaces": workspaces, "truncated": truncated, "tasks": tasks});
        if conn.supports("workers") {
            out["workers"] = json!(workers);
        }
        println!("{out}");
        return Ok(String::new());
    }
    println!("workspaces: {}", workspaces.join(", "));
    if conn.supports("workers") {
        println!(
            "workers: {}",
            workers
                .iter()
                .map(|w| w.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    for r in &records {
        println!("{}", line(r));
    }
    if truncated {
        println!("(more tasks exist than the bridge listed)");
    }
    Ok(String::new())
}

/// Prints the record, then lets the bridge finish handing the goal to the
/// agent: it exits once the start prompt is typed or given up.
pub async fn create(mut conn: Conn, new: NewTask, as_json: bool) -> Result<String, String> {
    require_workers(conn.supports("workers"), new.worker.is_some())?;
    let messages = conn
        .call(
            |id| Request::CreateTask {
                worker: new.worker,
                id,
                workspace: new.workspace,
                title: new.title,
                goal: new.goal,
                project: new.project,
                repo: new.repo,
                branch: new.branch,
                base: new.base,
                r#loop: new.r#loop,
            },
            LAUNCH_WAIT,
        )
        .await?;
    let record = saved(&messages).ok_or("the bridge saved no task")?;
    if as_json {
        let sessions = conn.list().await?;
        println!("{}", inbox::to_json(&inbox::item(record, &sessions)));
    } else {
        println!("{}", line(record));
    }
    std::io::Write::flush(&mut std::io::stdout()).map_err(|e| e.to_string())?;
    conn.finish(HANDOFF_WAIT).await;
    Ok(String::new())
}

/// Reads the record's `rev` first; a `conflict` means it changed meanwhile.
pub async fn done(conn: &mut Conn, task: &str) -> Result<String, String> {
    let (_, records, _, _) = fetch(conn).await?;
    let rev = records
        .iter()
        .find(|r| r.id == task)
        .map(|r| r.rev)
        .ok_or_else(|| format!("no task {task}"))?;
    let messages = conn
        .call(
            |id| Request::UpdateTask {
                id,
                task: task.to_owned(),
                rev,
                status: TaskStatus::Done,
            },
            WAIT,
        )
        .await?;
    let record = saved(&messages).ok_or("the bridge saved no task")?;
    println!("{}", line(record));
    Ok(String::new())
}

/// Prints the record with its new run, then, like `create`, lets the bridge
/// finish typing the prompt into the restarted agent.
pub async fn restart(
    mut conn: Conn,
    task: &str,
    force: bool,
    as_json: bool,
    worker: WorkerSelection,
) -> Result<String, String> {
    let supports_workers = conn.supports("workers");
    require_workers(supports_workers, !worker.is_keep())?;
    let rev = if supports_workers {
        let (_, records, _, _) = fetch(&mut conn).await?;
        Some(
            records
                .iter()
                .find(|r| r.id == task)
                .map(|r| r.rev)
                .ok_or_else(|| format!("no task {task} in the bridge's task list"))?,
        )
    } else {
        None
    };
    let messages = conn
        .call(
            |id| Request::RestartAgent {
                worker,
                rev,
                id,
                task: task.to_owned(),
                force,
            },
            LAUNCH_WAIT,
        )
        .await?;
    let record = saved(&messages).ok_or("the bridge saved no task")?;
    if as_json {
        let sessions = conn.list().await?;
        println!("{}", inbox::to_json(&inbox::item(record, &sessions)));
    } else {
        println!("{}", line(record));
        if let Some(run) = record.runs.last() {
            println!("run {}: restarted", run.id);
        }
    }
    std::io::Write::flush(&mut std::io::stdout()).map_err(|e| e.to_string())?;
    conn.finish(HANDOFF_WAIT).await;
    Ok(String::new())
}

pub async fn start_loop(
    conn: &mut Conn,
    task: &str,
    spec: LoopSpec,
    reset: bool,
    as_json: bool,
) -> Result<String, String> {
    let messages = conn
        .call(
            |id| Request::StartLoop {
                id,
                task: task.to_owned(),
                r#loop: spec,
                reset,
            },
            WAIT,
        )
        .await?;
    let record = saved(&messages).ok_or("the bridge saved no task")?;
    print_record(conn, record, as_json).await
}

pub async fn stop_loop(conn: &mut Conn, task: &str, as_json: bool) -> Result<String, String> {
    let record = stop_one(conn, task).await?;
    print_record(conn, &record, as_json).await
}

/// Stops every loop that can still act. A failure does not stop the others;
/// the error names each task that kept its loop.
pub async fn stop_all_loops(conn: &mut Conn, as_json: bool) -> Result<String, String> {
    let (_, records, truncated, _) = fetch(conn).await?;
    let active: Vec<String> = records
        .iter()
        .filter(|r| r.r#loop.as_deref().is_some_and(is_active))
        .map(|r| r.id.clone())
        .collect();
    let (mut stopped, mut failed) = (Vec::new(), Vec::new());
    for task in &active {
        match stop_one(conn, task).await {
            Ok(record) => stopped.push(record),
            Err(e) => failed.push(format!("{task}: {e}")),
        }
    }
    if as_json {
        let sessions = conn.list().await?;
        let tasks: Vec<_> = stopped
            .iter()
            .map(|r| inbox::to_json(&inbox::item(r, &sessions)))
            .collect();
        println!("{}", json!({"tasks": tasks}));
    } else {
        for r in &stopped {
            println!("{}", line(r));
        }
    }
    if truncated {
        failed.push("the bridge listed only some tasks; run it again".to_owned());
    }
    if failed.is_empty() {
        Ok(String::new())
    } else {
        Err(failed.join("; "))
    }
}

async fn stop_one(conn: &mut Conn, task: &str) -> Result<TaskRecord, String> {
    let messages = conn
        .call(
            |id| Request::StopLoop {
                id,
                task: task.to_owned(),
            },
            WAIT,
        )
        .await?;
    saved(&messages)
        .cloned()
        .ok_or_else(|| "the bridge saved no task".to_owned())
}

/// A loop that may still check or type. An unknown state is left alone.
fn is_active(l: &TaskLoop) -> bool {
    matches!(
        l.state,
        LoopState::Armed | LoopState::Checking | LoopState::Paused
    )
}

async fn print_record(
    conn: &mut Conn,
    record: &TaskRecord,
    as_json: bool,
) -> Result<String, String> {
    if as_json {
        let sessions = conn.list().await?;
        println!("{}", inbox::to_json(&inbox::item(record, &sessions)));
    } else {
        println!("{}", line(record));
    }
    Ok(String::new())
}

async fn fetch(
    conn: &mut Conn,
) -> Result<(Vec<String>, Vec<TaskRecord>, bool, Vec<WorkerInfo>), String> {
    let messages = conn.call(|id| Request::ListTasks { id }, WAIT).await?;
    messages
        .into_iter()
        .find_map(|m| match m {
            Message::TaskList {
                workspaces,
                entries,
                truncated,
                workers,
                ..
            } => Some((
                workspaces.into_iter().map(|w| w.name).collect(),
                entries,
                truncated,
                workers,
            )),
            _ => None,
        })
        .ok_or_else(|| "the bridge sent no task list".to_owned())
}

fn require_workers(supported: bool, selected: bool) -> Result<(), String> {
    if selected && !supported {
        Err("unsupported: this host does not advertise workers".into())
    } else {
        Ok(())
    }
}

fn saved(messages: &[Message]) -> Option<&TaskRecord> {
    messages.iter().find_map(|m| match m {
        Message::TaskSaved { record, .. } => Some(record),
        _ => None,
    })
}

/// `t-7f3a2c  open    work  Fix login  [session]  note`
fn line(r: &TaskRecord) -> String {
    let status = format!("{:?}", r.status).to_lowercase();
    let mut out = format!("{}  {status:<7} {}  {}", r.id, r.workspace, r.title);
    if let Some(session) = &r.session {
        out.push_str(&format!("  [{session}]"));
    }
    if let Some(worker) = &r.worker {
        out.push_str(&format!("  worker: {worker}"));
    }
    if let Some(operation) = &r.restart_operation {
        out.push_str(&format!(
            "  restart recovery: {} ({})",
            operation.id, operation.phase
        ));
    }
    if r.runs
        .last()
        .is_some_and(|run| run.outcome.as_deref() == Some("unconfirmed"))
    {
        out.push_str("  latest run: unconfirmed");
    }
    if let Some(l) = &r.r#loop {
        out.push_str(&format!("  {}", loop_text(l)));
    }
    if let Some(note) = &r.note {
        out.push_str(&format!("  ({note})"));
    }
    out
}

/// `loop stopped (limit) 3/3`: the state, its reason, and failed checks of the limit.
fn loop_text(l: &TaskLoop) -> String {
    let wire = |v: serde_json::Result<serde_json::Value>| {
        v.ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    };
    let reason = l
        .reason
        .map(|r| format!(" ({})", wire(serde_json::to_value(r))))
        .unwrap_or_default();
    format!(
        "loop {}{reason} {}/{}",
        wire(serde_json::to_value(l.state)),
        l.attempts,
        l.max_attempts
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new() -> NewTask {
        NewTask {
            workspace: "work".into(),
            title: "Fix login".into(),
            goal: "Make it work.".into(),
            ..NewTask::default()
        }
    }

    #[test]
    fn task_new_needs_its_flags_and_a_branch_with_a_repo() {
        assert!(new().checked().is_ok());
        assert!(
            NewTask {
                goal: " ".into(),
                ..new()
            }
            .checked()
            .is_err()
        );
        let repo = NewTask {
            repo: Some("/src/app".into()),
            ..new()
        };
        assert!(repo.clone().checked().is_err());
        assert!(
            NewTask {
                branch: Some("feat/x".into()),
                ..repo
            }
            .checked()
            .is_ok()
        );
        assert!(
            NewTask {
                base: Some("main".into()),
                ..new()
            }
            .checked()
            .is_err()
        );
    }

    #[test]
    fn a_line_shows_status_session_and_note() {
        let r = TaskRecord {
            restart_operation: None,
            worker: None,
            id: "t-7f3a2c".into(),
            rev: 2,
            workspace: "work".into(),
            title: "Fix login".into(),
            goal: String::new(),
            status: TaskStatus::Review,
            created_at: 0,
            updated_at: 0,
            project: None,
            repo: None,
            branch: None,
            dir: None,
            session: Some("fix-login-t-7f3a2c".into()),
            note: Some("not sent".into()),
            runs: Vec::new(),
            r#loop: None,
            pending_prompt: None,
        };
        assert_eq!(
            line(&r),
            "t-7f3a2c  review  work  Fix login  [fix-login-t-7f3a2c]  (not sent)"
        );
        let looped = TaskRecord {
            r#loop: Some(Box::new(TaskLoop {
                check: "make check".into(),
                max_attempts: 3,
                timeout: 600,
                state: LoopState::Stopped,
                attempts: 3,
                armed_at: 0,
                since: 0,
                reason: Some(muxa_protocol::LoopReason::Limit),
                last_check: None,
            })),
            note: None,
            ..r
        };
        assert_eq!(
            line(&looped),
            "t-7f3a2c  review  work  Fix login  [fix-login-t-7f3a2c]  loop stopped (limit) 3/3"
        );
    }

    #[test]
    fn loop_flags_need_a_check_and_stay_in_bounds() {
        let flags = |check: Option<&str>, n, t| LoopFlags {
            check: check.map(str::to_owned),
            max_attempts: n,
            timeout: t,
        };
        assert_eq!(flags(None, None, None).spec().ok(), Some(None));
        assert!(flags(None, Some(3), None).spec().is_err());
        assert!(flags(Some(" "), None, None).spec().is_err());
        assert!(flags(Some("make check"), Some(0), None).spec().is_err());
        assert!(flags(Some("make check"), Some(11), None).spec().is_err());
        assert!(flags(Some("make check"), None, Some(5)).spec().is_err());
        assert_eq!(
            flags(Some("make check"), Some(10), Some(3600)).spec().ok(),
            Some(Some(LoopSpec {
                check: "make check".into(),
                max_attempts: Some(10),
                timeout: Some(3600),
            }))
        );
    }
}
