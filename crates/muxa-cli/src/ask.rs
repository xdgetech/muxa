//! `ask`: an agent session in a repo's main checkout, to ask about its code
//! (ADR 0031). No Task and no worktree; it lists as a manual session.
//! `ask close` closes one (ADR 0039).

use std::time::Duration;

use muxa_protocol::{
    CloseMode, Message, Request, Session, WORK_ASK, WORK_ASK_ROOT, WORK_CLOSE_ASK, WorkspaceInfo,
    work_level,
};
use serde_json::json;

use crate::conn::Conn;

const WAIT: Duration = Duration::from_secs(10);
/// The bridge starts the agent before it answers.
const LAUNCH_WAIT: Duration = Duration::from_secs(60);

pub async fn ask(
    conn: &mut Conn,
    repo: &str,
    workspace: Option<String>,
    as_json: bool,
) -> Result<String, String> {
    if work_level(conn.capabilities()) < WORK_ASK {
        return Err("this host cannot start ask sessions; update muxa-bridge on the host".into());
    }
    let workspace = match workspace {
        Some(w) => w,
        None => pick(&workspaces(conn).await?, repo)?,
    };
    let messages = conn
        .call(
            |id| Request::StartSession {
                id,
                workspace: workspace.clone(),
                repo: Some(repo.to_owned()),
                root: false,
            },
            LAUNCH_WAIT,
        )
        .await?;
    started(messages, &workspace, as_json)
}

/// Opens the workspace's Ask session at its root, or starts it (ADR 0054):
/// the session Ask Agent types into, and the overall assistant's chat.
pub async fn root(conn: &mut Conn, workspace: &str, as_json: bool) -> Result<String, String> {
    if work_level(conn.capabilities()) < WORK_ASK_ROOT {
        return Err(
            "this host cannot open a workspace's Ask session; update muxa-bridge on the host"
                .into(),
        );
    }
    if workspace.trim().is_empty() {
        return Err("ask --root needs --workspace".into());
    }
    let messages = conn
        .call(
            |id| Request::StartSession {
                id,
                workspace: workspace.to_owned(),
                repo: None,
                root: true,
            },
            LAUNCH_WAIT,
        )
        .await?;
    started(messages, workspace, as_json)
}

/// Prints the `session-started` among `messages`.
fn started(messages: Vec<Message>, workspace: &str, as_json: bool) -> Result<String, String> {
    let (session, name) = messages
        .into_iter()
        .find_map(|m| match m {
            Message::SessionStarted { session, name, .. } => Some((session, name)),
            _ => None,
        })
        .ok_or("the bridge started no session")?;
    if as_json {
        println!(
            "{}",
            json!({"session": session, "name": name, "workspace": workspace})
        );
    } else {
        println!("{session}");
    }
    Ok(String::new())
}

/// Closes the ask session `target`, an id or a name, through `close-task`.
/// The bridge checks `@muxa_ask` again; this finds the id and a clear error.
pub async fn close(conn: &mut Conn, target: &str, as_json: bool) -> Result<String, String> {
    if work_level(conn.capabilities()) < WORK_CLOSE_ASK {
        return Err("this host cannot close ask sessions; update muxa-bridge on the host".into());
    }
    let session = find(&conn.list().await?, target)?;
    conn.call(
        |id| Request::CloseTask {
            id,
            session: session.id.clone(),
            mode: CloseMode::Keep,
        },
        WAIT,
    )
    .await?;
    if as_json {
        println!("{}", json!({"session": session.id, "name": session.name}));
    } else {
        println!("{}", session.id);
    }
    Ok(String::new())
}

/// The ask session whose id or name is `target`.
fn find(sessions: &[Session], target: &str) -> Result<Session, String> {
    let found = sessions
        .iter()
        .find(|s| s.id == target || s.name == target)
        .ok_or_else(|| format!("no session {target:?}"))?;
    if found.ask.is_none() {
        return Err(format!(
            "{} is not an ask session; close it in tmux",
            found.name
        ));
    }
    Ok(found.clone())
}

async fn workspaces(conn: &mut Conn) -> Result<Vec<WorkspaceInfo>, String> {
    let messages = conn.call(|id| Request::ListTasks { id }, WAIT).await?;
    messages
        .into_iter()
        .find_map(|m| match m {
            Message::TaskList { workspaces, .. } => Some(workspaces),
            _ => None,
        })
        .ok_or_else(|| "the bridge sent no task list".to_owned())
}

/// The workspace for `repo` without `--workspace`: the only one, or the one
/// whose repos hold it, by absolute path or relative to its root.
pub fn pick(workspaces: &[WorkspaceInfo], repo: &str) -> Result<String, String> {
    if let [only] = workspaces {
        return Ok(only.name.clone());
    }
    let repo = repo.trim_end_matches('/');
    let holds = |w: &WorkspaceInfo| {
        let under = w.root.as_ref().map(|r| format!("{r}/{repo}"));
        w.repos
            .iter()
            .any(|r| r == repo || under.as_deref() == Some(r.as_str()))
    };
    let found: Vec<&WorkspaceInfo> = workspaces.iter().filter(|w| holds(w)).collect();
    match found.as_slice() {
        [one] => Ok(one.name.clone()),
        _ => Err(format!(
            "give --workspace: one of {}",
            workspaces
                .iter()
                .map(|w| w.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(name: &str, root: &str, repos: &[&str]) -> WorkspaceInfo {
        WorkspaceInfo {
            name: name.into(),
            root: Some(root.into()),
            checks: None,
            repos: repos.iter().map(|r| r.to_string()).collect(),
            memory: None,
            memory_at: None,
        }
    }

    fn session(id: &str, name: &str, ask: Option<&str>) -> Session {
        Session {
            id: id.into(),
            name: name.into(),
            repo: None,
            repo_dir: None,
            branch: None,
            worktree: None,
            task: None,
            task_id: None,
            ask: ask.map(str::to_owned),
            panes: Vec::new(),
        }
    }

    #[test]
    fn only_an_ask_session_is_found_to_close() {
        let all = [
            session("$1", "work", None),
            session("$4", "ask-app-1", Some("lisk")),
        ];
        assert_eq!(find(&all, "$4").unwrap().id, "$4");
        assert_eq!(find(&all, "ask-app-1").unwrap().id, "$4");
        assert!(
            find(&all, "work")
                .unwrap_err()
                .contains("not an ask session")
        );
        assert!(find(&all, "$9").is_err());
    }

    #[test]
    fn a_repo_picks_the_workspace_that_holds_it() {
        let all = [
            ws("lisk", "/p/lisk", &["/p/lisk/app", "/p/lisk/fairpull/plan"]),
            ws("home", "/p/home", &["/p/home/app", "/shared/tool"]),
        ];
        assert_eq!(pick(&all, "fairpull/plan"), Ok("lisk".into()));
        assert_eq!(pick(&all, "fairpull/plan/"), Ok("lisk".into()));
        assert_eq!(pick(&all, "/shared/tool"), Ok("home".into()));
        let both = pick(&all, "app").unwrap_err();
        assert!(both.contains("lisk, home"), "{both}");
        assert!(pick(&all, "nope").is_err());
        assert_eq!(pick(&all[..1], "anything"), Ok("lisk".into()));
    }
}
