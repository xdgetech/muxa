//! `tasks`, `task new`, `task done` and `task restart`: task records
//! (`task-records`, ADR 0015; `task-handoff`, ADR 0018).

use std::time::Duration;

use muxa_protocol::{Message, Request, TaskRecord, TaskStatus};

use crate::conn::Conn;

/// A launch makes a worktree and starts an agent; the bridge allows 60 s.
const LAUNCH_WAIT: Duration = Duration::from_secs(90);
const WAIT: Duration = Duration::from_secs(10);

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

pub async fn list(conn: &mut Conn) -> Result<String, String> {
    let (workspaces, records, truncated) = fetch(conn).await?;
    println!("workspaces: {}", workspaces.join(", "));
    for r in &records {
        println!("{}", line(r));
    }
    if truncated {
        println!("(more tasks exist than the bridge listed)");
    }
    Ok(String::new())
}

pub async fn create(conn: &mut Conn, new: NewTask) -> Result<String, String> {
    let messages = conn
        .call(
            |id| Request::CreateTask {
                id,
                workspace: new.workspace,
                title: new.title,
                goal: new.goal,
                project: new.project,
                repo: new.repo,
                branch: new.branch,
                base: new.base,
            },
            LAUNCH_WAIT,
        )
        .await?;
    let record = saved(&messages).ok_or("the bridge saved no task")?;
    println!("{}", line(record));
    Ok(String::new())
}

/// Reads the record's `rev` first; a `conflict` means it changed meanwhile.
pub async fn done(conn: &mut Conn, task: &str) -> Result<String, String> {
    let (_, records, _) = fetch(conn).await?;
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

/// The restarted agent gets its prompt later; the record shows the new run.
pub async fn restart(conn: &mut Conn, task: &str, force: bool) -> Result<String, String> {
    let messages = conn
        .call(
            |id| Request::RestartAgent {
                id,
                task: task.to_owned(),
                force,
            },
            LAUNCH_WAIT,
        )
        .await?;
    let record = saved(&messages).ok_or("the bridge saved no task")?;
    println!("{}", line(record));
    if let Some(run) = record.runs.last() {
        println!("run {}: restarted", run.id);
    }
    Ok(String::new())
}

async fn fetch(conn: &mut Conn) -> Result<(Vec<String>, Vec<TaskRecord>, bool), String> {
    let messages = conn.call(|id| Request::ListTasks { id }, WAIT).await?;
    messages
        .into_iter()
        .find_map(|m| match m {
            Message::TaskList {
                workspaces,
                entries,
                truncated,
                ..
            } => Some((
                workspaces.into_iter().map(|w| w.name).collect(),
                entries,
                truncated,
            )),
            _ => None,
        })
        .ok_or_else(|| "the bridge sent no task list".to_owned())
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
    if let Some(note) = &r.note {
        out.push_str(&format!("  ({note})"));
    }
    out
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
        };
        assert_eq!(
            line(&r),
            "t-7f3a2c  review  work  Fix login  [fix-login-t-7f3a2c]  (not sent)"
        );
    }
}
