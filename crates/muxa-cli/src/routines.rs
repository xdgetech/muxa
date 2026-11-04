//! `routines`, `routine run`, and the stored Inbox entries of `inbox`
//! (ADR 0026), behind the host's `work` level 1 capability.

use std::time::Duration;

use muxa_protocol::{
    AgentState, Disposition, InboxAction, InboxEntry, InboxEntryKind, Message, Request,
    RoutineInfo, Session, WORK_ROUTINES, work_level,
};
use serde_json::json;

use crate::conn::Conn;

const WAIT: Duration = Duration::from_secs(10);
/// A run-now or an approval launches an agent before it answers.
const LAUNCH_WAIT: Duration = Duration::from_secs(120);

fn require(conn: &Conn) -> Result<(), String> {
    if work_level(conn.capabilities()) >= WORK_ROUTINES {
        Ok(())
    } else {
        Err("this host has no Routines; update muxa-bridge on the host".into())
    }
}

async fn fetch(conn: &mut Conn) -> Result<Vec<RoutineInfo>, String> {
    require(conn)?;
    let messages = conn.call(|id| Request::ListRoutines { id }, WAIT).await?;
    messages
        .into_iter()
        .find_map(|m| match m {
            Message::RoutineList { entries, .. } => Some(entries),
            _ => None,
        })
        .ok_or_else(|| "the bridge sent no routine list".into())
}

fn line(r: &RoutineInfo) -> String {
    let trigger = r.trigger.as_ref().map_or("run-now only".to_owned(), |t| {
        format!("{} {}", t.at, t.timezone)
    });
    let mut out = format!(
        "{}/{}  {trigger}  {}",
        r.workspace,
        r.name,
        if r.enabled { "enabled" } else { "disabled" }
    );
    if let Some(run) = &r.running {
        out.push_str(&format!(
            "  running {} ({})",
            run.occurrence,
            run.task.as_deref().unwrap_or("starting")
        ));
    } else if let Some(at) = r.next_due {
        out.push_str(&format!("  next {}", crate::when::format(at)));
    }
    if let Some(s) = &r.last_success {
        out.push_str(&format!(
            "  last success {} ({} new)",
            crate::when::format(s.at),
            s.published
        ));
    }
    if let Some(f) = &r.last_failure {
        out.push_str(&format!(
            "  last failure {}: {}",
            crate::when::format(f.at),
            f.reason
        ));
    }
    out
}

pub async fn list(conn: &mut Conn, as_json: bool) -> Result<String, String> {
    let routines = fetch(conn).await?;
    if as_json {
        println!("{}", json!({ "routines": routines }));
    } else if routines.is_empty() {
        println!("no routines; add [[routine]] to workspaces.toml on the host");
    } else {
        for r in &routines {
            println!("{}", line(r));
        }
    }
    Ok(String::new())
}

/// `NAME`, or `WORKSPACE/NAME` when two workspaces have one of that name.
fn pick<'a>(routines: &'a [RoutineInfo], target: &str) -> Result<&'a RoutineInfo, String> {
    if let Some(r) = routines
        .iter()
        .find(|r| format!("{}/{}", r.workspace, r.name) == target)
    {
        return Ok(r);
    }
    let named: Vec<_> = routines.iter().filter(|r| r.name == target).collect();
    match named.as_slice() {
        [one] => Ok(one),
        [] => Err(format!(
            "no routine {target:?}; muxa-cli routines lists them"
        )),
        _ => Err(format!(
            "{target:?} is in several workspaces; use WORKSPACE/NAME"
        )),
    }
}

pub async fn run(conn: &mut Conn, target: &str, as_json: bool) -> Result<String, String> {
    let routines = fetch(conn).await?;
    let r = pick(&routines, target)?;
    let (workspace, routine) = (r.workspace.clone(), r.name.clone());
    let messages = conn
        .call(
            |id| Request::RunRoutine {
                id,
                workspace,
                routine,
            },
            LAUNCH_WAIT,
        )
        .await?;
    let saved = messages
        .into_iter()
        .find_map(|m| match m {
            Message::RoutineSaved { entry, .. } => Some(entry),
            _ => None,
        })
        .ok_or("the bridge saved no routine")?;
    if as_json {
        println!("{}", json!(saved));
    } else {
        println!("{}", line(&saved));
    }
    Ok(String::new())
}

fn entry_line(e: &InboxEntry) -> String {
    let kind = match e.kind {
        InboxEntryKind::Briefing => "briefing",
        InboxEntryKind::Proposal => "proposal",
    };
    let state = match (e.disposition, e.until, &e.task) {
        (Disposition::Later, Some(u), _) => format!("later until {}", crate::when::format(u)),
        (Disposition::Approved, _, Some(t)) => format!("approved: {t}"),
        (d, _, _) => format!("{d:?}").to_lowercase(),
    };
    let mut out = format!(
        "{}  {kind}  {}/{}  {state}  {}",
        e.id, e.workspace, e.routine, e.title
    );
    for link in &e.links {
        out.push_str(&format!("\n    {}", link.url));
    }
    out
}

/// A Routine run whose agent waits on the user: its need shows in Inbox,
/// though the run is not a Task (ADR 0026 section 3).
fn waiting(routines: &[RoutineInfo], sessions: &[Session]) -> Vec<(String, String, String)> {
    routines
        .iter()
        .filter_map(|r| {
            let task = r.running.as_ref()?.task.as_deref()?;
            let session = sessions
                .iter()
                .find(|s| s.task_id.as_deref() == Some(task))?;
            let waits = session.panes.iter().any(|p| {
                !p.shell_foreground
                    && p.agent
                        .as_ref()
                        .is_some_and(|a| a.state == AgentState::Waiting)
            });
            waits.then(|| {
                (
                    format!("{}/{}", r.workspace, r.name),
                    task.to_owned(),
                    session.name.clone(),
                )
            })
        })
        .collect()
}

pub async fn inbox(conn: &mut Conn, all: bool, as_json: bool) -> Result<String, String> {
    let routines = fetch(conn).await?;
    let messages = conn.call(|id| Request::ListInbox { id }, WAIT).await?;
    let (entries, items) = messages
        .into_iter()
        .find_map(|m| match m {
            Message::InboxList { entries, items, .. } => Some((entries, items)),
            _ => None,
        })
        .ok_or("the bridge sent no inbox list")?;
    let entries: Vec<InboxEntry> = entries
        .into_iter()
        .filter(|e| all || matches!(e.disposition, Disposition::Open | Disposition::Later))
        .collect();
    let items: Vec<_> = items
        .into_iter()
        .filter(|i| crate::items::shown(i, all))
        .collect();
    let with_items = crate::items::has_items(conn);
    let sessions = conn.list().await?;
    let waits = waiting(&routines, &sessions);
    if as_json {
        let runs: Vec<_> = waits
            .iter()
            .map(|(routine, task, session)| {
                json!({"routine": routine, "task": task, "session": session, "need": "answer"})
            })
            .collect();
        println!(
            "{}",
            json!({"work": work_level(conn.capabilities()), "items": items, "entries": entries, "runs": runs})
        );
        return Ok(String::new());
    }
    for (routine, task, session) in &waits {
        println!("{task}  run of {routine} waits for an answer in session {session}");
    }
    let now = crate::when::now();
    if with_items {
        for i in &items {
            println!("{}", crate::items::line(i, now));
        }
    } else {
        for e in &entries {
            println!("{}", entry_line(e));
        }
    }
    let listed = if with_items {
        items.is_empty()
    } else {
        entries.is_empty()
    };
    if waits.is_empty() && listed {
        println!("nothing in Inbox from Routines");
    }
    Ok(String::new())
}

pub async fn act(
    mut conn: Conn,
    entry: &str,
    action: InboxAction,
    until: Option<u64>,
    as_json: bool,
) -> Result<String, String> {
    require(&conn)?;
    let entry_id = entry.to_owned();
    let wait = if action == InboxAction::Approve {
        LAUNCH_WAIT
    } else {
        WAIT
    };
    let messages = conn
        .call(
            |id| Request::UpdateInbox {
                id,
                entry: entry_id,
                action,
                until,
                text: None,
                option: None,
            },
            wait,
        )
        .await?;
    let saved = messages
        .into_iter()
        .find_map(|m| match m {
            Message::InboxSaved { entry, .. } => entry.map(|e| *e),
            _ => None,
        })
        .ok_or("the bridge saved no entry")?;
    if as_json {
        println!("{}", json!(saved));
    } else {
        println!("{}", entry_line(&saved));
    }
    // An approval's Task gets its start prompt after the answer.
    if action == InboxAction::Approve {
        conn.finish(Duration::from_secs(40)).await;
    }
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_in_two_workspaces_needs_its_workspace() {
        let r = |w: &str| RoutineInfo {
            workspace: w.into(),
            name: "Brief".into(),
            trigger: None,
            enabled: true,
            worker: None,
            project: None,
            next_due: None,
            last_success: None,
            last_failure: None,
            running: None,
        };
        let one = [r("a")];
        assert_eq!(pick(&one, "Brief").unwrap().workspace, "a");
        let two = [r("a"), r("b")];
        assert!(pick(&two, "Brief").is_err());
        assert_eq!(pick(&two, "b/Brief").unwrap().workspace, "b");
        assert!(pick(&two, "Other").is_err());
    }
}
