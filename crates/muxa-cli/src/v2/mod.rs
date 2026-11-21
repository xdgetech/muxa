//! `muxa-cli v2 …`: the reference client of protocol v2 (muxa next B5). It
//! talks to `muxa-bridge connect`, locally or over SSH, and resumes a lost
//! connection the way the apps will.

pub mod client;
mod inbox;

use std::ffi::OsString;
use std::io::Write;
use std::process::Stdio;
use std::time::Duration;

use muxa_protocol::v2::PlaceSpec;
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, ChildStdin, ChildStdout};

use crate::view::Raw;
use client::{Client, Event, State};

const USAGE: &str = "usage: muxa-cli v2 [--bridge CMD] terminals [--json]
       muxa-cli v2 [--bridge CMD] start-task --workspace W --title T [--goal G] [--folder]
                [--branch REPO:BRANCH [--base REF]]... [--read REPO]... [--connection NAME]...
                [--plan] [--json]
       muxa-cli v2 [--bridge CMD] watch [--seconds N]
       muxa-cli v2 [--bridge CMD] capture TEXT... [--json]
       muxa-cli v2 [--bridge CMD] inbox [--json]
       muxa-cli v2 [--bridge CMD] inbox done|reopen|delete ITEM
       muxa-cli v2 [--bridge CMD] inbox move ITEM INDEX
       muxa-cli v2 [--bridge CMD] inbox later ITEM --minutes N
       muxa-cli v2 [--bridge CMD] review [TERMINAL] [--note TEXT]
       muxa-cli v2 [--bridge CMD] routines [--json]
       muxa-cli v2 [--bridge CMD] run-routine WORKSPACE NAME
       muxa-cli v2 [--bridge CMD] attach TERMINAL [--seconds N]

  --bridge CMD   runs through sh -c (default: muxa-bridge connect),
                 e.g. 'ssh host ~/.local/bin/muxa-bridge connect'
  attach         Ctrl-] leaves. A lost connection is resumed.
  review         the Task is ready for review; TERMINAL defaults to
                 $HC_TERMINAL_ID, so its agent can say so itself";

const NAME: &str = concat!("muxa-cli ", env!("CARGO_PKG_VERSION"));
const QUIT: u8 = 0x1d; // Ctrl-]
const RECONNECTS: u32 = 5;
const RECONNECT_WAIT: Duration = Duration::from_secs(1);
/// How often `attach` looks at the local window size.
const SIZE_EVERY: Duration = Duration::from_millis(500);

#[derive(Debug, PartialEq, Eq)]
enum Cmd {
    Terminals {
        json: bool,
    },
    Inbox {
        action: inbox::Action,
        json: bool,
    },
    Review {
        terminal: String,
        note: Option<String>,
    },
    Routines {
        json: bool,
    },
    RunRoutine {
        workspace: String,
        name: String,
    },
    StartTask {
        workspace: String,
        title: String,
        goal: Option<String>,
        /// In the order given; none is one folder (ADR 0068).
        places: Vec<PlaceSpec>,
        /// Besides the Workspace's default ones (ADR 0069).
        connections: Vec<String>,
        plan: bool,
        json: bool,
    },
    Watch {
        seconds: Option<u64>,
    },
    Attach {
        terminal: String,
        seconds: Option<u64>,
    },
}

/// Runs `muxa-cli v2 <args>`; `Ok(None)` printed the usage.
pub fn main(args: Vec<OsString>) -> Result<Option<()>, String> {
    let Some((bridge, cmd)) = parse(args).map_err(|e| e.to_string())? else {
        println!("{USAGE}");
        return Ok(None);
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let result = runtime.block_on(run(&bridge, cmd));
    // tokio's stdin reads on a blocking thread; do not wait for it.
    runtime.shutdown_background();
    result.map(Some)
}

fn parse(args: Vec<OsString>) -> Result<Option<(String, Cmd)>, lexopt::Error> {
    use lexopt::prelude::*;
    let mut p = lexopt::Parser::from_args(args);
    let mut bridge = "muxa-bridge connect".to_owned();
    let (mut json, mut plan, mut seconds) = (false, false, None);
    let (mut workspace, mut title, mut goal) = (None, None, None);
    let mut places: Vec<PlaceSpec> = Vec::new();
    let mut connections: Vec<String> = Vec::new();
    let mut words = Vec::new();
    let mut minutes: Option<u32> = None;
    let mut note: Option<String> = None;
    while let Some(arg) = p.next()? {
        match arg {
            Long("bridge") => bridge = p.value()?.string()?,
            Long("json") => json = true,
            Long("plan") => plan = true,
            Long("seconds") => seconds = Some(p.value()?.parse()?),
            Long("workspace") => workspace = Some(p.value()?.string()?),
            Long("title") => title = Some(p.value()?.string()?),
            Long("goal") => goal = Some(p.value()?.string()?),
            Long("folder") => places.push(PlaceSpec::Folder),
            Long("branch") => {
                let value = p.value()?.string()?;
                let (repo, branch) = value
                    .rsplit_once(':')
                    .filter(|(r, b)| !r.is_empty() && !b.is_empty())
                    .ok_or("--branch wants REPO:BRANCH")?;
                places.push(PlaceSpec::Branch {
                    repo: repo.to_owned(),
                    branch: branch.to_owned(),
                    base: None,
                });
            }
            Long("base") => {
                let value = p.value()?.string()?;
                match places.last_mut() {
                    Some(PlaceSpec::Branch { base, .. }) => *base = Some(value),
                    _ => return Err("--base follows a --branch".into()),
                }
            }
            Long("connection") => connections.push(p.value()?.string()?),
            Long("read") => places.push(PlaceSpec::Read {
                repo: p.value()?.string()?,
            }),
            Long("minutes") => minutes = Some(p.value()?.parse()?),
            Long("note") => note = Some(p.value()?.string()?),
            Long("help") | Short('h') => return Ok(None),
            Value(v) => words.push(v.string()?),
            _ => return Err(arg.unexpected()),
        }
    }
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let cmd = match words.as_slice() {
        ["terminals"] => Cmd::Terminals { json },
        ["start-task"] => Cmd::StartTask {
            workspace: workspace.ok_or("start-task needs --workspace")?,
            title: title.ok_or("start-task needs --title")?,
            goal,
            places,
            connections,
            plan,
            json,
        },
        ["watch"] => Cmd::Watch { seconds },
        ["capture", text @ ..] if !text.is_empty() => Cmd::Inbox {
            action: inbox::Action::Capture(text.join(" ")),
            json,
        },
        ["review"] => Cmd::Review {
            terminal: std::env::var("HC_TERMINAL_ID")
                .map_err(|_| "review needs a TERMINAL outside a herdr-core terminal")?,
            note,
        },
        ["routines"] => Cmd::Routines { json },
        ["run-routine", w, n] => Cmd::RunRoutine {
            workspace: (*w).to_owned(),
            name: (*n).to_owned(),
        },
        ["review", t] => Cmd::Review {
            terminal: (*t).to_owned(),
            note,
        },
        ["inbox", rest @ ..] => Cmd::Inbox {
            action: inbox_action(rest, minutes)?,
            json,
        },
        ["attach", t] => Cmd::Attach {
            terminal: (*t).to_owned(),
            seconds,
        },
        _ => return Ok(None),
    };
    Ok(Some((bridge, cmd)))
}

fn inbox_action(words: &[&str], minutes: Option<u32>) -> Result<inbox::Action, lexopt::Error> {
    use inbox::Action;
    let item = |s: &&str| (*s).to_owned();
    Ok(match words {
        [] => Action::List,
        ["done", i] => Action::Done(item(i)),
        ["reopen", i] => Action::Reopen(item(i)),
        ["delete", i] => Action::Delete(item(i)),
        ["move", i, n] => Action::Move {
            item: item(i),
            index: n
                .parse()
                .map_err(|_| lexopt::Error::from(format!("{n:?} is not an index")))?,
        },
        ["later", i] => Action::Later {
            item: item(i),
            minutes: minutes.ok_or("inbox later needs --minutes")?,
        },
        _ => return Err("unknown inbox command; see muxa-cli v2 --help".into()),
    })
}

type Conn = Client<ChildStdout, ChildStdin>;

/// Runs the bridge command and says hello; the child ends with the client.
async fn open(bridge: &str, state: State) -> Result<(Conn, Child), String> {
    let mut child = tokio::process::Command::new("sh")
        .args(["-c", bridge])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("cannot run {bridge:?}: {e}"))?;
    let stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let client = Client::open(stdout, stdin, NAME, state).await?;
    Ok((client, child))
}

async fn run(bridge: &str, cmd: Cmd) -> Result<(), String> {
    match cmd {
        Cmd::Terminals { json } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            let reply = c.request(json!({"type": "terminals"})).await?;
            let list = reply["terminals"].as_array().cloned().unwrap_or_default();
            if json {
                println!("{}", Value::Array(list));
            } else {
                for t in &list {
                    println!("{}", row(t));
                }
            }
            Ok(())
        }
        Cmd::StartTask {
            workspace,
            title,
            goal,
            places,
            connections,
            plan,
            json,
        } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            let msg = json!({"type": "start-task", "workspace": workspace, "title": title,
                             "goal": goal, "places": places, "connections": connections,
                             "plan": plan});
            let reply = c.request(msg).await?;
            if json {
                println!("{reply}");
            } else {
                let how = if reply["existing"] == true {
                    "running"
                } else {
                    "started"
                };
                println!("{how} {}", row(&reply["terminal"]));
            }
            Ok(())
        }
        Cmd::Watch { seconds } => watch(bridge, seconds).await,
        Cmd::Routines { json } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            let reply = c.request(json!({"type": "routines"})).await?;
            if json {
                println!(
                    "{}",
                    json!({"routines": reply["routines"], "problems": reply["problems"]})
                );
                return Ok(());
            }
            for r in reply["routines"].as_array().into_iter().flatten() {
                println!("{}", routine_row(r));
            }
            for p in reply["problems"].as_array().into_iter().flatten() {
                eprintln!(
                    "muxa-cli: {}: {}",
                    p["workspace"].as_str().unwrap_or("?"),
                    p["reason"].as_str().unwrap_or("")
                );
            }
            Ok(())
        }
        Cmd::RunRoutine { workspace, name } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            c.request(json!({"type": "run-routine", "workspace": workspace, "name": name}))
                .await?;
            println!("started {workspace}/{name}");
            Ok(())
        }
        Cmd::Review { terminal, note } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            let msg = json!({"type": "task-review", "terminal": terminal, "note": note});
            let reply = c.request(msg).await?;
            let now = jiff::Timestamp::now().as_second();
            if let Some(row) = inbox::rows(&reply["inbox"], now).into_iter().next() {
                println!("{row}");
            }
            Ok(())
        }
        Cmd::Inbox { action, json } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            let now = jiff::Timestamp::now().as_second();
            let mut reply = c.request(inbox::request(&action, now)).await?;
            if matches!(action, inbox::Action::Capture(_)) {
                reply = c.request(inbox::capture_add(&reply)).await?;
            }
            if json {
                println!("{}", reply["inbox"]);
            } else {
                for row in inbox::rows(&reply["inbox"], now) {
                    println!("{row}");
                }
            }
            Ok(())
        }
        Cmd::Attach { terminal, seconds } => attach(bridge, &terminal, seconds).await,
    }
}

/// `workspace/name  when  last`, one line per Routine.
fn routine_row(r: &Value) -> String {
    let when = match (r["at"].as_str(), r["enabled"].as_bool()) {
        (_, Some(false)) => "off".to_owned(),
        (Some(at), _) => at.to_owned(),
        (None, _) => "on request".to_owned(),
    };
    let last = if r["running_since"].is_i64() {
        "running".to_owned()
    } else {
        match (r["last"]["ok"].as_bool(), r["last"]["reason"].as_str()) {
            (Some(true), _) => "ok".to_owned(),
            (Some(false), why) => format!("failed: {}", why.unwrap_or("")),
            _ => "-".to_owned(),
        }
    };
    format!(
        "{}/{}\t{when}\t{last}",
        r["workspace"].as_str().unwrap_or("?"),
        r["name"].as_str().unwrap_or("?")
    )
}

/// `id  state  name  dir`, one line per terminal.
fn row(t: &Value) -> String {
    let state = match (&t["live"], t["status"]["state"].as_str()) {
        (Value::Bool(false), _) => "ended",
        (_, Some(s)) => s,
        _ => "-",
    };
    format!(
        "{}\t{state}\t{}\t{}",
        t["id"].as_str().unwrap_or("?"),
        t["name"].as_str().unwrap_or("-"),
        t["labels"]["muxa.dir"].as_str().unwrap_or("-"),
    )
}

async fn watch(bridge: &str, seconds: Option<u64>) -> Result<(), String> {
    let (mut c, _child) = open(bridge, State::default()).await?;
    c.request(json!({"type": "watch"})).await?;
    let deadline = deadline(seconds);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            event = c.next() => {
                if let Event::Notice(msg) = event? {
                    println!("{msg}");
                }
            }
            () = &mut deadline => return Ok(()),
        }
    }
}

fn deadline(seconds: Option<u64>) -> tokio::time::Sleep {
    tokio::time::sleep(Duration::from_secs(seconds.unwrap_or(u64::MAX / 4)))
}

/// Shows one terminal; keys go to it. A lost connection is resumed.
async fn attach(bridge: &str, terminal: &str, seconds: Option<u64>) -> Result<(), String> {
    let interactive = std::io::IsTerminal::is_terminal(&std::io::stdin());
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let (mut c, mut child) = open(bridge, State::default()).await?;
    let channel = c.attach(terminal, cols, rows).await?;
    let _raw = if interactive {
        Some(Raw::enter()?)
    } else {
        None
    };
    let (keys_tx, mut keys) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
    tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        let mut buf = [0u8; 4096];
        while let Ok(n) = stdin.read(&mut buf).await {
            if n == 0 || keys_tx.send(buf[..n].to_vec()).await.is_err() {
                return;
            }
        }
    });
    let deadline = deadline(seconds);
    tokio::pin!(deadline);
    let mut size = (cols, rows);
    let mut size_check = tokio::time::interval(SIZE_EVERY);
    loop {
        let step = tokio::select! {
            _ = size_check.tick(), if interactive => {
                match crossterm::terminal::size() {
                    Ok(now) if now != size => {
                        size = now;
                        match c.resize(channel, now.0, now.1).await {
                            Ok(()) => Step::Go,
                            Err(e) => Step::Lost(e),
                        }
                    }
                    _ => Step::Go,
                }
            }
            event = c.next() => match event {
                Ok(event) => show(channel, event),
                Err(e) => Step::Lost(e),
            },
            Some(key) = keys.recv(), if interactive => {
                if key.contains(&QUIT) {
                    return Ok(());
                }
                match c.input(channel, &key).await {
                    Ok(()) => Step::Go,
                    Err(e) => Step::Lost(e),
                }
            }
            () = &mut deadline => return Ok(()),
        };
        match step {
            Step::Go => {}
            Step::Done(msg) => {
                drop(_raw);
                eprintln!("muxa-cli: {msg}");
                return Ok(());
            }
            Step::Lost(why) => {
                drop(child);
                (c, child) = reconnect(bridge, c.into_state(), &why).await?;
            }
        }
    }
}

enum Step {
    Go,
    Done(String),
    Lost(String),
}

fn show(channel: u32, event: Event) -> Step {
    let mut out = std::io::stdout();
    match event {
        Event::Output { channel: ch, bytes } if ch == channel => {
            let _ = out.write_all(&bytes);
            let _ = out.flush();
        }
        Event::Paint { channel: ch } if ch == channel => {
            // A fresh screen follows: reset the local one.
            let _ = out.write_all(b"\x1bc");
        }
        Event::Closed { channel: ch, exit } if ch == channel => {
            let why = exit
                .as_ref()
                .and_then(|e| e["reason"].as_str())
                .unwrap_or("detached");
            return Step::Done(format!("the terminal is closed: {why}"));
        }
        _ => {}
    }
    Step::Go
}

async fn reconnect(bridge: &str, state: State, why: &str) -> Result<(Conn, Child), String> {
    let mut last = why.to_owned();
    for _ in 0..RECONNECTS {
        tokio::time::sleep(RECONNECT_WAIT).await;
        match open(bridge, state.clone()).await {
            Ok(c) => return Ok(c),
            Err(e) => last = e,
        }
    }
    Err(format!("lost the connection: {last}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<OsString> {
        s.iter().map(OsString::from).collect()
    }

    #[test]
    fn commands_parse() {
        assert_eq!(
            parse(args(&["--bridge", "b", "attach", "t7", "--seconds", "2"])).unwrap(),
            Some((
                "b".into(),
                Cmd::Attach {
                    terminal: "t7".into(),
                    seconds: Some(2)
                }
            ))
        );
        let (bridge, cmd) = parse(args(&[
            "start-task",
            "--workspace",
            "w",
            "--title",
            "Fix login",
            "--read",
            "docs",
            "--branch",
            "/r:fix",
            "--base",
            "main",
            "--connection",
            "linear",
            "--plan",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(bridge, "muxa-bridge connect");
        assert_eq!(
            cmd,
            Cmd::StartTask {
                workspace: "w".into(),
                title: "Fix login".into(),
                goal: None,
                places: vec![
                    PlaceSpec::Read {
                        repo: "docs".into()
                    },
                    PlaceSpec::Branch {
                        repo: "/r".into(),
                        branch: "fix".into(),
                        base: Some("main".into()),
                    },
                ],
                connections: vec!["linear".into()],
                plan: true,
                json: false
            }
        );
        assert!(parse(args(&["start-task", "--workspace", "w"])).is_err());
        assert!(
            parse(args(&[
                "start-task",
                "--title",
                "t",
                "--workspace",
                "w",
                "--branch",
                "fix"
            ]))
            .is_err()
        );
        assert!(
            parse(args(&[
                "start-task",
                "--title",
                "t",
                "--workspace",
                "w",
                "--base",
                "main"
            ]))
            .is_err()
        );
        assert_eq!(parse(args(&["nope"])).unwrap(), None);
    }

    #[test]
    fn a_row_shows_the_reported_state() {
        let t = json!({"id": "t1", "live": true, "status": {"state": "blocked"},
                       "labels": {"muxa.dir": "/w/fix"}});
        assert_eq!(row(&t), "t1\tblocked\t-\t/w/fix");
        assert_eq!(row(&json!({"id": "t2", "live": false})), "t2\tended\t-\t-");
    }
}
