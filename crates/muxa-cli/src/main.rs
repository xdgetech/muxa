//! `muxa-cli`: a terminal client for the muxa protocol, the reference client.

mod ask;
mod conn;
mod inbox;
mod items;
mod routines;
mod tasks;
mod view;
mod when;

use std::process::ExitCode;

use muxa_protocol::{InboxAction, LoopSpec, RestartRecoveryAction, Role, Session, TaskStatus};

const USAGE: &str = "usage: muxa-cli [--bridge CMD] list
       muxa-cli [--bridge CMD] watch TARGET [--read-only] [--seconds N]
       muxa-cli [--bridge CMD] tasks [--json]
       muxa-cli [--bridge CMD] task new --workspace W --title T --goal G
                [--project P] [--repo REPO [--branch B] [--base REF]] [--worker NAME]
                [--check CMD [--max-attempts N] [--check-timeout S]] [--json]
       muxa-cli [--bridge CMD] task (done | drop) ID [--json]
       muxa-cli [--bridge CMD] ask REPO [--workspace W] [--json]
       muxa-cli [--bridge CMD] ask close SESSION [--json]
       muxa-cli [--bridge CMD] task restart ID [--force] [--json] [--worker NAME]
       muxa-cli [--bridge CMD] task new --read --workspace W --title T --goal G
                --repo REPO [--project P] [--json]
       muxa-cli [--bridge CMD] task branch ID [--branch NAME] [--json]
       muxa-cli [--bridge CMD] task park ID [--json]
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
       muxa-cli [--bridge CMD] inbox later ENTRY [--json]
       muxa-cli [--bridge CMD] inbox answer ITEM ACTION [--text T] [--option N]
                [--json]
       muxa-cli [--bridge CMD] inbox read ITEM [--json]

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
  ask close SESSION
                 closes an ask session, by id ($3) or name. Its agent stops;
                 the main checkout and its files stay as they are
  --json         one line of JSON for other tools: the Inbox for tasks, one
                 Inbox item for task new and task restart (docs/agents.md,
                 Desktop tools)
  task done ID   marks a task record done
  task drop ID   marks a task record dropped: given up, not finished.
                 Neither closes the task's session
  task restart ID
                 restarts the task's agent in the same pane, after a handoff
                 in .muxa/HANDOFF.md; --force also while the agent runs.
                 A parked task resumes: a new session in its directory
  --read         task new: a reading task. The agent works in the task's own
                 folder and reads REPO; no branch and no worktree (ADR 0034)
  task branch ID gives a reading task a worktree, on --branch NAME or a
                 branch named after its title, and restarts its agent there
  task park ID   writes .muxa/HANDOFF.md and closes the task's session; the
                 task, its TASK.md and its directory stay
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
  inbox dismiss ENTRY, inbox later ENTRY
                 closes the entry, or moves it below the others
                 With work level 3, inbox lists Inbox items instead: id,
                 kind, state, age, title and question; --all also lists
                 answered and resolved items
  inbox answer ITEM ACTION
                 answers an item. ACTION is approve, reject, choose
                 (--option N), accept, keep-open, reply, redo or
                 change-course (--text T), retry, drop, dismiss, later
                 (moves it below the others), ask-agent (--text T) or read.
                 reply sends the text as it is; on an ask it denies the
                 permission first. ask-agent sends the item and the text to
                 the workspace's Ask session and leaves the item open
  inbox read ITEM
                 marks the item read, as inbox answer ITEM read";

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
    CloseAsk {
        session: String,
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
    Restart {
        task: String,
        force: bool,
        json: bool,
        worker: Option<String>,
    },
    Branch {
        task: String,
        branch: Option<String>,
        json: bool,
    },
    Park {
        task: String,
        json: bool,
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
    Answer {
        item: String,
        answer: items::Answer,
        json: bool,
    },
    Act {
        entry: String,
        action: InboxAction,
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
    let (mut text, mut option) = (None, None);
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
            Long("read") => new.read = true,
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
            Long("all") => all = true,
            Long("text") => text = Some(p.value()?.string()?),
            Long("option") => option = Some(p.value()?.parse()?),
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
    let is_answer = matches!(words.as_slice(), [w, a, _, _] if w == "inbox" && a == "answer");
    if (text.is_some() || option.is_some()) && !is_answer {
        return Err(lexopt::Error::Custom(
            "--text and --option require inbox answer".into(),
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
        [w, c, session] if w == "ask" && c == "close" => Cmd::CloseAsk {
            session: session.clone(),
            json,
        },
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
        [w] if w == "routines" => Cmd::Routines { json },
        [w, r, name] if w == "routine" && r == "run" => Cmd::RunRoutine {
            target: name.clone(),
            json,
        },
        [w] if w == "inbox" => Cmd::Inbox { all, json },
        [w, a, item, name] if w == "inbox" && a == "answer" => Cmd::Answer {
            item: item.clone(),
            answer: items::Answer {
                action: items::action(name).ok_or_else(|| {
                    lexopt::Error::Custom(format!("unknown answer {name:?}\n{USAGE}").into())
                })?,
                text,
                option,
            }
            .checked()
            .map_err(|e| lexopt::Error::Custom(e.into()))?,
            json,
        },
        [w, r, item] if w == "inbox" && r == "read" => Cmd::Answer {
            item: item.clone(),
            answer: items::Answer {
                action: InboxAction::Read,
                text: None,
                option: None,
            },
            json,
        },
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
        [w, b, id] if w == "task" && b == "branch" => Cmd::Branch {
            task: id.clone(),
            branch: new.branch,
            json,
        },
        [w, b, id] if w == "task" && b == "park" => Cmd::Park {
            task: id.clone(),
            json,
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
        Cmd::CloseAsk { session, json } => ask::close(&mut conn, &session, json).await,
        Cmd::NewTask { new, json } => tasks::create(conn, new, json).await,
        Cmd::Close { task, status, json } => tasks::close(&mut conn, &task, status, json).await,
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
        Cmd::Branch { task, branch, json } => tasks::branch(conn, &task, branch, json).await,
        Cmd::Park { task, json } => tasks::park(&mut conn, &task, json).await,
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
        Cmd::Answer { item, answer, json } => items::answer(conn, &item, answer, json).await,
        Cmd::Act {
            entry,
            action,
            json,
        } => routines::act(conn, &entry, action, json).await,
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

    fn answer(args: &[&str]) -> Result<items::Answer, String> {
        match parse_with(lexopt::Parser::from_args(args.iter().copied())) {
            Ok(Some((_, Cmd::Answer { answer, .. }))) => Ok(answer),
            Ok(_) => Err("another command".into()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[test]
    fn inbox_answer_takes_each_flag_only_with_its_action() {
        let a = answer(&["inbox", "answer", "i-1", "redo", "--text", "again"]).unwrap();
        assert_eq!(a.action, InboxAction::Redo);
        assert_eq!(a.text.as_deref(), Some("again"));
        let a = answer(&[
            "inbox",
            "answer",
            "i-1",
            "reply",
            "--text",
            "Use the cache.",
        ])
        .unwrap();
        assert_eq!(a.action, InboxAction::Reply);
        assert_eq!(a.text.as_deref(), Some("Use the cache."));
        let a = answer(&["inbox", "answer", "i-1", "choose", "--option", "2"]).unwrap();
        assert_eq!(a.option, Some(2));
        let a = answer(&["inbox", "answer", "i-1", "later"]).unwrap();
        assert_eq!(a.action, InboxAction::Later);
        assert_eq!(
            answer(&["inbox", "read", "i-1"]).unwrap().action,
            InboxAction::Read
        );
        for bad in [
            &["inbox", "answer", "i-1", "redo"][..],
            &["inbox", "answer", "i-1", "reply"],
            &["inbox", "answer", "i-1", "choose"],
            &["inbox", "answer", "i-1", "accept", "--text", "x"],
            &["inbox", "answer", "i-1", "later", "--until", "+1h"],
            &["inbox", "answer", "i-1", "shrug"],
            &["inbox", "approve", "e-1", "--text", "x"],
            &["inbox", "read", "i-1", "--option", "1"],
        ] {
            assert!(answer(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn reading_tasks_branch_and_park_parse() {
        let parse = |args: &[&str]| parse_with(lexopt::Parser::from_args(args.iter().copied()));
        let new = &[
            "task",
            "new",
            "--read",
            "--workspace",
            "w",
            "--title",
            "t",
            "--goal",
            "g",
            "--repo",
            "app",
        ];
        assert!(matches!(
            parse(new),
            Ok(Some((_, Cmd::NewTask { new, .. }))) if new.read && new.repo.as_deref() == Some("app")
        ));
        let mut branched = new.to_vec();
        branched.extend(["--branch", "x"]);
        assert!(parse(&branched).is_err(), "a reading task has no branch");
        assert!(parse(&new[..new.len() - 2]).is_err(), "--read needs --repo");
        assert!(matches!(
            parse(&["task", "branch", "t-1", "--branch", "feat/x"]),
            Ok(Some((_, Cmd::Branch { branch: Some(b), .. }))) if b == "feat/x"
        ));
        assert!(matches!(
            parse(&["task", "branch", "t-1"]),
            Ok(Some((_, Cmd::Branch { branch: None, .. })))
        ));
        assert!(matches!(
            parse(&["task", "park", "t-1", "--json"]),
            Ok(Some((_, Cmd::Park { json: true, .. })))
        ));
        assert_eq!(
            answer(&["inbox", "answer", "i-1", "branch"])
                .unwrap()
                .action,
            InboxAction::Branch
        );
    }

    #[test]
    fn ask_close_takes_a_session_and_ask_still_takes_a_repo() {
        let parse = |args: &[&str]| parse_with(lexopt::Parser::from_args(args.iter().copied()));
        assert!(matches!(
            parse(&["ask", "close", "$3", "--json"]),
            Ok(Some((_, Cmd::CloseAsk { session, json: true }))) if session == "$3"
        ));
        assert!(matches!(
            parse(&["ask", "close"]),
            Ok(Some((_, Cmd::Ask { repo, .. }))) if repo == "close"
        ));
    }

    /// ADR 0033 removed Move down and Later on the Task.
    #[test]
    fn down_later_and_requeue_are_gone() {
        for gone in [
            &["task", "down", "t-1"][..],
            &["task", "later", "t-1", "--until", "+1h"],
            &["task", "requeue", "t-1"],
        ] {
            assert!(
                parse_with(lexopt::Parser::from_args(gone.iter().copied())).is_err(),
                "{gone:?}"
            );
        }
    }
}
