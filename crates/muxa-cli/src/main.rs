//! `muxa-cli`: a terminal client for the muxa protocol, the reference client.

mod ask;
mod conn;
mod inbox;
mod routines;
mod tasks;
mod view;
mod when;

use std::process::ExitCode;

use muxa_protocol::{
    InboxAction, InboxChange, LoopSpec, RestartRecoveryAction, Role, Session, TaskStatus,
};

const USAGE: &str = "usage: muxa-cli [--bridge CMD] list
       muxa-cli [--bridge CMD] watch TARGET [--read-only] [--seconds N]
       muxa-cli [--bridge CMD] tasks [--json]
       muxa-cli [--bridge CMD] task new --workspace W --title T --goal G
                [--project P] [--repo REPO [--branch B] [--base REF]] [--worker NAME]
                [--check CMD [--max-attempts N] [--check-timeout S]] [--json]
       muxa-cli [--bridge CMD] task (done | drop) ID [--json]
       muxa-cli [--bridge CMD] ask REPO [--workspace W] [--json]
       muxa-cli [--bridge CMD] task (down | requeue) ID [--json]
       muxa-cli [--bridge CMD] task later ID --until TIME [--json]
       muxa-cli [--bridge CMD] task restart ID [--force] [--json] [--worker NAME]
       muxa-cli [--bridge CMD] task recover ID [--json]
                [--operation OP --rev N --action ACTION]
       muxa-cli [--bridge CMD] task loop start ID --check CMD
                [--max-attempts N] [--check-timeout S] [--reset] [--json]
       muxa-cli [--bridge CMD] task loop stop ID [--json]
       muxa-cli [--bridge CMD] task stop-loop (ID | --all) [--json]
       muxa-cli [--bridge CMD] routines [--json]
       muxa-cli [--bridge CMD] routine run [WORKSPACE/]NAME [--json]
       muxa-cli [--bridge CMD] inbox [--all] [--json]
       muxa-cli [--bridge CMD] inbox (approve | dismiss) ENTRY [--json]
       muxa-cli [--bridge CMD] inbox later ENTRY --until TIME [--json]

  --bridge CMD   how to start the bridge (default: muxa-bridge). Over SSH:
                 --bridge 'ssh host ~/.local/bin/muxa-bridge'
  TARGET         a pane id (%12), or SESSION, SESSION:agent or SESSION:shell
  --read-only    show the pane, send no keys
  --seconds N    leave after N seconds (default: on Ctrl-])
  tasks          the host's workspaces and task records
  task new       records a task and starts an agent for it; without --repo
                 it gets a directory in its workspace. It returns once the
                 agent got its start prompt, or after 30 s without it
  --repo REPO    an absolute path, or a repo under the workspace root, such
                 as app or group/app. Without --branch, the host names a new
                 branch after the title (ADR 0031)
  ask REPO       starts the workspace's agent in the repo's main checkout, to
                 ask about its code: no task and no worktree. Prints the
                 session id. Without --workspace, the one that has REPO
  --json         one line of JSON for other tools: the Inbox for tasks, one
                 Inbox item for task new and task restart (docs/agents.md,
                 Desktop tools)
  task done ID   marks a task record done
  task drop ID   marks a task record dropped: given up, not finished.
                 Neither closes the task's session
  task down ID   moves the task to the end of the Inbox queue (Move down).
                 Its status does not change. Needs task-inbox
  task later ID --until TIME
                 hides the task from the Inbox queue until TIME (Later):
                 2026-11-04T09:00Z (UTC), +30m, +2h, +1d or epoch seconds
  task requeue ID
                 clears Move down and Later. A new need of the agent or the
                 loop also clears them
  task restart ID
                 restarts the task's agent in the same pane, after a handoff
                 in .muxa/HANDOFF.md; --force also while the agent runs
  --worker NAME  selects a host-defined worker for new/restart, or `default`
                 for the workspace default agent; requires workers. Without
                 it, restart keeps the current worker
  task recover ID
                 inspects an interrupted restart. To resolve it, supply its
                 displayed --operation and --rev, with --action cancel-prepared,
                 finish-replacement or leave-unconfirmed. Never retries input
  --check CMD    a Task Loop: the host runs CMD after each of the agent's
                 turns, until it exits 0 (ADR 0019). It must be one of the
                 workspace's checks, and the host needs the muxa daemon
  --max-attempts N
                 failed checks before the loop stops: 1 to 10 (default 3)
  --check-timeout S
                 seconds one check may run: 10 to 3600 (default 600)
  task loop start ID
                 starts the task's loop again; --reset counts attempts from 0
  task loop stop ID, task stop-loop ID
                 stops the task's loop
  task stop-loop --all
                 stops every loop that is armed, checking or paused
  routines       the host's Routines: trigger, next run, last success and
                 failure, the open run (ADR 0026)
  routine run NAME
                 starts a run of an enabled Routine now; needs the daemon
  inbox          the Routines' briefings and proposals that are open or
                 later, and Routine runs that wait for an answer; --all
                 also lists approved and dismissed entries
  inbox approve ENTRY
                 creates one Task that performs the proposal; approving
                 again returns the same Task
  inbox dismiss ENTRY, inbox later ENTRY --until TIME
                 closes the entry, or hides it until TIME, as task later";

enum Cmd {
    List,
    Watch {
        target: String,
        read_only: bool,
        seconds: Option<u64>,
    },
    Tasks {
        json: bool,
    },
    Ask {
        repo: String,
        workspace: Option<String>,
        json: bool,
    },
    NewTask {
        new: tasks::NewTask,
        json: bool,
    },
    Close {
        task: String,
        status: TaskStatus,
        json: bool,
    },
    Place {
        task: String,
        change: InboxChange,
        json: bool,
    },
    Restart {
        task: String,
        force: bool,
        json: bool,
        worker: Option<String>,
    },
    Recover {
        task: String,
        resolution: Option<tasks::Recovery>,
        json: bool,
    },
    StartLoop {
        task: String,
        spec: LoopSpec,
        reset: bool,
        json: bool,
    },
    StopLoop {
        task: String,
        json: bool,
    },
    StopAllLoops {
        json: bool,
    },
    Routines {
        json: bool,
    },
    RunRoutine {
        target: String,
        json: bool,
    },
    Inbox {
        all: bool,
        json: bool,
    },
    Act {
        entry: String,
        action: InboxAction,
        until: Option<u64>,
        json: bool,
    },
}

fn parse() -> Result<Option<(String, Cmd)>, lexopt::Error> {
    parse_with(lexopt::Parser::from_env())
}

fn parse_with(mut p: lexopt::Parser) -> Result<Option<(String, Cmd)>, lexopt::Error> {
    use lexopt::prelude::*;
    let (mut bridge, mut words) = ("muxa-bridge".to_string(), Vec::new());
    let (mut read_only, mut seconds, mut json, mut force) = (false, None, false, false);
    let (mut reset, mut all) = (false, false);
    let (mut operation, mut revision, mut action) = (None, None, None);
    let mut new = tasks::NewTask::default();
    let mut check = tasks::LoopFlags::default();
    let mut until = None;
    while let Some(arg) = p.next()? {
        match arg {
            Long("bridge") => bridge = p.value()?.string()?,
            Long("read-only") => read_only = true,
            Long("json") => json = true,
            Long("force") => force = true,
            Long("seconds") => seconds = Some(p.value()?.parse()?),
            Long("workspace") => new.workspace = p.value()?.string()?,
            Long("title") => new.title = p.value()?.string()?,
            Long("goal") => new.goal = p.value()?.string()?,
            Long("project") => new.project = Some(p.value()?.string()?),
            Long("repo") => new.repo = Some(p.value()?.string()?),
            Long("branch") => new.branch = Some(p.value()?.string()?),
            Long("base") => new.base = Some(p.value()?.string()?),
            Long("worker") => new.worker = Some(p.value()?.string()?),
            Long("operation") => operation = Some(p.value()?.string()?),
            Long("rev") => revision = Some(p.value()?.parse()?),
            Long("action") => {
                let value = p.value()?.string()?;
                action = Some(RestartRecoveryAction::from_wire(&value).ok_or_else(|| {
                    lexopt::Error::Custom(
                        "--action must be cancel-prepared, finish-replacement or leave-unconfirmed"
                            .into(),
                    )
                })?);
            }
            Long("check") => check.check = Some(p.value()?.string()?),
            Long("max-attempts") => check.max_attempts = Some(p.value()?.parse()?),
            Long("check-timeout") => check.timeout = Some(p.value()?.parse()?),
            Long("reset") => reset = true,
            Long("until") => until = Some(p.value()?.string()?),
            Long("all") => all = true,
            Long("version") => {
                println!("muxa-cli {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            Long("help") | Short('h') => {
                println!("{USAGE}");
                return Ok(None);
            }
            Value(v) => words.push(v.string()?),
            _ => return Err(arg.unexpected()),
        }
    }
    if new
        .worker
        .as_ref()
        .is_some_and(|name| name.trim().is_empty())
    {
        return Err(lexopt::Error::Custom("--worker needs a name".into()));
    }
    let is_restart = matches!(words.as_slice(), [w, r, _] if w == "task" && r == "restart");
    let is_new = matches!(words.as_slice(), [w, n] if w == "task" && n == "new");
    let is_recover = matches!(words.as_slice(), [w, r, _] if w == "task" && r == "recover");
    let has_recovery_flags = operation.is_some() || revision.is_some() || action.is_some();
    if has_recovery_flags && !is_recover {
        return Err(lexopt::Error::Custom(
            "--operation, --rev and --action require task recover".into(),
        ));
    }
    let resolution = match (operation, revision, action) {
        (None, None, None) => None,
        (Some(operation), Some(rev), Some(action)) if !operation.trim().is_empty() && rev > 0 => {
            Some(tasks::Recovery {
                operation,
                rev,
                action,
            })
        }
        _ => return Err(lexopt::Error::Custom(
            "task recover requires a nonempty --operation, positive --rev and --action together"
                .into(),
        )),
    };
    let is_later =
        matches!(words.as_slice(), [w, l, _] if (w == "task" || w == "inbox") && l == "later");
    if until.is_some() != is_later {
        return Err(lexopt::Error::Custom(
            "task later and inbox later need --until, and only they take it".into(),
        ));
    }
    if new.worker.is_some() && !is_new && !is_restart {
        return Err(lexopt::Error::Custom(
            "--worker requires task new or task restart".into(),
        ));
    }
    let cmd = match words.as_slice() {
        [w] if w == "list" => Cmd::List,
        [w, target] if w == "watch" => Cmd::Watch {
            target: target.clone(),
            read_only,
            seconds,
        },
        [w] if w == "tasks" => Cmd::Tasks { json },
        [w, repo] if w == "ask" => Cmd::Ask {
            repo: repo.clone(),
            workspace: Some(new.workspace).filter(|w| !w.trim().is_empty()),
            json,
        },
        [w, n] if w == "task" && n == "new" => Cmd::NewTask {
            new: tasks::NewTask {
                r#loop: check.spec()?,
                ..new
            }
            .checked()?,
            json,
        },
        [w, l, s, id] if w == "task" && l == "loop" && s == "start" => Cmd::StartLoop {
            task: id.clone(),
            spec: check
                .spec()?
                .ok_or_else(|| lexopt::Error::Custom("task loop start needs --check".into()))?,
            reset,
            json,
        },
        [w, l, s, id] if w == "task" && l == "loop" && s == "stop" => Cmd::StopLoop {
            task: id.clone(),
            json,
        },
        [w, s, id] if w == "task" && s == "stop-loop" && !all => Cmd::StopLoop {
            task: id.clone(),
            json,
        },
        [w, s] if w == "task" && s == "stop-loop" && all => Cmd::StopAllLoops { json },
        [w, d, id] if w == "task" && d == "done" => Cmd::Close {
            task: id.clone(),
            status: TaskStatus::Done,
            json,
        },
        [w, d, id] if w == "task" && d == "drop" => Cmd::Close {
            task: id.clone(),
            status: TaskStatus::Dropped,
            json,
        },
        [w, d, id] if w == "task" && d == "down" => Cmd::Place {
            task: id.clone(),
            change: InboxChange {
                down: true,
                until: None,
            },
            json,
        },
        [w, r, id] if w == "task" && r == "requeue" => Cmd::Place {
            task: id.clone(),
            change: InboxChange::default(),
            json,
        },
        [w, l, id] if w == "task" && l == "later" => Cmd::Place {
            task: id.clone(),
            change: InboxChange {
                down: false,
                until: until
                    .as_deref()
                    .map(|t| when::parse(t, when::now()))
                    .transpose()
                    .map_err(|e| lexopt::Error::Custom(e.into()))?,
            },
            json,
        },
        [w] if w == "routines" => Cmd::Routines { json },
        [w, r, name] if w == "routine" && r == "run" => Cmd::RunRoutine {
            target: name.clone(),
            json,
        },
        [w] if w == "inbox" => Cmd::Inbox { all, json },
        [w, a, entry] if w == "inbox" => {
            let action = match a.as_str() {
                "approve" => InboxAction::Approve,
                "dismiss" => InboxAction::Dismiss,
                "later" => InboxAction::Later,
                _ => return Err(lexopt::Error::Custom(USAGE.into())),
            };
            Cmd::Act {
                entry: entry.clone(),
                action,
                until: until
                    .as_deref()
                    .map(|t| when::parse(t, when::now()))
                    .transpose()
                    .map_err(|e| lexopt::Error::Custom(e.into()))?,
                json,
            }
        }
        [w, r, id] if w == "task" && r == "recover" => Cmd::Recover {
            task: id.clone(),
            resolution,
            json,
        },
        [w, r, id] if w == "task" && r == "restart" => Cmd::Restart {
            task: id.clone(),
            force,
            json,
            worker: new.worker,
        },
        _ => return Err(lexopt::Error::Custom(USAGE.into())),
    };
    Ok(Some((bridge, cmd)))
}

fn main() -> ExitCode {
    let (bridge, cmd) = match parse() {
        Ok(Some(x)) => x,
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("muxa-cli: {e}");
            return ExitCode::from(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("muxa-cli: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = runtime.block_on(run(&bridge, cmd));
    // tokio's stdin reads on a blocking thread; do not wait for it.
    runtime.shutdown_background();
    match result {
        Ok(msg) => {
            if !msg.is_empty() {
                eprintln!("muxa-cli: {msg}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("muxa-cli: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(bridge: &str, cmd: Cmd) -> Result<String, String> {
    let (mut conn, tmux) = conn::Conn::start(bridge).await?;
    match cmd {
        Cmd::List => {
            print_sessions(&tmux, &conn.list().await?);
            Ok(String::new())
        }
        Cmd::Watch {
            target,
            read_only,
            seconds,
        } => {
            let pane = resolve(&conn.list().await?, &target)?;
            view::run(&mut conn, &pane, view::Options { read_only, seconds }).await
        }
        Cmd::Tasks { json } => tasks::list(&mut conn, json).await,
        Cmd::Ask {
            repo,
            workspace,
            json,
        } => ask::ask(&mut conn, &repo, workspace, json).await,
        Cmd::NewTask { new, json } => tasks::create(conn, new, json).await,
        Cmd::Close { task, status, json } => tasks::close(&mut conn, &task, status, json).await,
        Cmd::Place { task, change, json } => tasks::place(&mut conn, &task, change, json).await,
        Cmd::Recover {
            task,
            resolution,
            json,
        } => tasks::recover(&mut conn, &task, resolution, json).await,
        Cmd::Restart {
            task,
            force,
            json,
            worker,
        } => tasks::restart(conn, &task, force, json, worker).await,
        Cmd::StartLoop {
            task,
            spec,
            reset,
            json,
        } => tasks::start_loop(&mut conn, &task, spec, reset, json).await,
        Cmd::StopLoop { task, json } => tasks::stop_loop(&mut conn, &task, json).await,
        Cmd::StopAllLoops { json } => tasks::stop_all_loops(&mut conn, json).await,
        Cmd::Routines { json } => routines::list(&mut conn, json).await,
        Cmd::RunRoutine { target, json } => routines::run(&mut conn, &target, json).await,
        Cmd::Inbox { all, json } => routines::inbox(&mut conn, all, json).await,
        Cmd::Act {
            entry,
            action,
            until,
            json,
        } => routines::act(conn, &entry, action, until, json).await,
    }
}

fn print_sessions(tmux: &str, sessions: &[Session]) {
    println!("tmux {tmux}");
    for s in sessions {
        let meta: Vec<&str> = [s.repo.as_deref(), s.branch.as_deref(), s.task_id.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        println!("{} {}  {}", s.id, s.name, meta.join(" "));
        for p in s.panes.iter().filter(|p| p.role != Role::Hidden) {
            let agent = p
                .agent
                .as_ref()
                .map(|a| {
                    format!(
                        "{:?}{}",
                        a.state,
                        if p.shell_foreground { " (stale?)" } else { "" }
                    )
                })
                .unwrap_or_default();
            println!(
                "  {:<6} {:<8} {:>3}x{:<3} {:<12} {}",
                p.id,
                format!("{:?}", p.role).to_lowercase(),
                p.cols,
                p.rows,
                p.command,
                agent
            );
        }
    }
}

/// A pane id as given, or a session's agent or shell pane.
fn resolve(sessions: &[Session], target: &str) -> Result<String, String> {
    if target.starts_with('%') {
        return Ok(target.to_owned());
    }
    let (name, want) = match target.rsplit_once(':') {
        Some((n, "agent")) => (n, Some(Role::Agent)),
        Some((n, "shell")) => (n, Some(Role::Shell)),
        _ => (target, None),
    };
    let session = sessions
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| format!("no session named {name:?}"))?;
    let visible = || session.panes.iter().filter(|p| p.role != Role::Hidden);
    let pane = match want {
        Some(role) => visible().find(|p| p.role == role),
        None => visible()
            .find(|p| p.role == Role::Agent)
            .or_else(|| visible().next()),
    };
    pane.map(|p| p.id.clone())
        .ok_or_else(|| format!("session {name:?} has no such pane"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(args: &[&str]) -> Result<InboxChange, String> {
        match parse_with(lexopt::Parser::from_args(args.iter().copied())) {
            Ok(Some((_, Cmd::Place { change, .. }))) => Ok(change),
            Ok(_) => Err("another command".into()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[test]
    fn down_later_and_requeue_ask_for_a_place() {
        assert_eq!(
            place(&["task", "down", "t-1"]),
            Ok(InboxChange {
                down: true,
                until: None
            })
        );
        assert_eq!(
            place(&["task", "later", "t-1", "--until", "1767340800"]),
            Ok(InboxChange {
                down: false,
                until: Some(1_767_340_800)
            })
        );
        assert_eq!(
            place(&["task", "requeue", "t-1"]),
            Ok(InboxChange::default())
        );
        assert!(place(&["task", "later", "t-1"]).is_err());
        assert!(place(&["task", "down", "t-1", "--until", "+1h"]).is_err());
        assert!(place(&["task", "later", "t-1", "--until", "soon"]).is_err());
    }
}
