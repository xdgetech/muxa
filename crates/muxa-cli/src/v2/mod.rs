//! `muxa-cli v2 …`: the reference client of protocol v2 (muxa next B5). It
//! talks to `muxa-bridge connect`, locally or over SSH, and resumes a lost
//! connection the way the apps will.

mod chat;
pub mod client;
mod inbox;
mod reader;
mod settings;

use std::ffi::OsString;
use std::io::Write;
use std::process::Stdio;
use std::time::Duration;

use muxa_protocol::v2::{LoopSpec, PlaceSpec};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, ChildStdin, ChildStdout};

use crate::raw::Raw;
use client::{Client, Event, State};

const USAGE: &str = "usage: muxa-cli v2 [--bridge CMD] terminals [--json]
       muxa-cli v2 [--bridge CMD] start-task --workspace W --title T [--goal G] [--folder] [--bot NAME]
                [--branch REPO:BRANCH [--base REF]]... [--read REPO]... [--all-read]
                [--existing PATH | --checkout REPO]
                [--plan]
                [--loop [--check CMD]] [--json]
       muxa-cli v2 [--bridge CMD] watch [--seconds N]
       muxa-cli v2 [--bridge CMD] capture TEXT... [--json]
       muxa-cli v2 [--bridge CMD] inbox [--json]
       muxa-cli v2 [--bridge CMD] inbox done|reopen|delete ITEM
       muxa-cli v2 [--bridge CMD] inbox move ITEM INDEX
       muxa-cli v2 [--bridge CMD] inbox later ITEM --minutes N
       muxa-cli v2 [--bridge CMD] review [TERMINAL] [--note TEXT]
       muxa-cli v2 [--bridge CMD] approve ITEM [--force]
       muxa-cli v2 [--bridge CMD] request-changes ITEM COMMENT...
       muxa-cli v2 [--bridge CMD] retry ITEM
       muxa-cli v2 [--bridge CMD] apply ITEM
       muxa-cli v2 [--bridge CMD] accept|dismiss ITEM
       muxa-cli v2 [--bridge CMD] close-task TERMINAL [--force]
       muxa-cli v2 [--bridge CMD] shell TERMINAL
       muxa-cli v2 [--bridge CMD] close-shell TERMINAL
       muxa-cli v2 [--bridge CMD] upload TERMINAL FILE
       muxa-cli v2 [--bridge CMD] workspaces [--json]
       muxa-cli v2 [--bridge CMD] subscriptions [--json]
       muxa-cli v2 [--bridge CMD] token-usage [--day YYYY-MM-DD] [--json]
       muxa-cli v2 [--bridge CMD] routines [--json]
       muxa-cli v2 [--bridge CMD] settings [--json]
       muxa-cli v2 [--bridge CMD] settings-set [FILE|-] [--revision REV] [--json]
       muxa-cli v2 [--bridge CMD] run-routine WORKSPACE NAME
       muxa-cli v2 [--bridge CMD] migrate-v1 [--dry-run] [--json]
       muxa-cli v2 [--bridge CMD] attach TERMINAL [--seconds N]
       muxa-cli v2 [--bridge CMD] chat TERMINAL [--seconds N] [--json]
       muxa-cli v2 [--bridge CMD] loops [--json]
       muxa-cli v2 [--bridge CMD] loop-start TERMINAL [--check CMD] [--max-attempts N]
                [--timeout SECONDS] [--reset]
       muxa-cli v2 [--bridge CMD] loop-stop TERMINAL
       muxa-cli v2 [--bridge CMD] reader TERMINAL docs|changes|images [--html] [--json]
       muxa-cli v2 [--bridge CMD] read TERMINAL docs|changes|images PATH [--html] [--commits]
                [--seconds N]

  --bridge CMD   runs through sh -c (default: muxa-bridge connect),
                 e.g. 'ssh host ~/.local/bin/muxa-bridge connect'
  attach         Ctrl-] leaves. A lost connection is resumed.
  review         the Task is ready for review; TERMINAL defaults to
                 $HC_TERMINAL_ID, so its agent can say so itself
  approve        closes the Task of a Review item: ends its agent and removes its
                 branch worktrees when clean and merged; --force removes them anyway
  request-changes  types COMMENT into the Task's agent and resolves the item
  retry          runs again what made a Failed item: its Routine, its Task Loop,
                 or its agent
  apply          applies the settings a Bot proposed in a talk; conflict when
                 they changed since
  accept, dismiss  the Capture assistant's suggestion on an item
  --bot NAME     talks to the Workspace's Bot NAME (a new name: to set one up)
  close-task     closes the Task of an agent terminal as approve does, and marks
                 its open items Done
  shell          starts a shell in the working place of the Task of an agent
                 terminal; every client lists it under the Task
  upload         copies FILE (at most 10 MB) into the Task's working place under
                 .muxa/uploads/ and prints its path there
  --existing     adopts a folder or worktree that is there already as the working
                 place; nothing is made, and Claude Code continues its conversation
  migrate-v1     one Inbox item \"Resume: <title>\" per open Task of the old muxa
                 whose folder is still there; --dry-run only lists them. Running it
                 again adds nothing twice. It never changes the old records
  --loop         a Task Loop: after each finished turn the host runs the check;
                 without --check, the Workspace's only allowed one
  subscriptions  each agent login muxa names, with the plan limits its agents last
                 reported: used percent and reset time per window (ADR 0091)
  token-usage    the tokens agents used today, or on --day, by Workspace and by
                 Task folder, from their transcripts: input, cached input, output
  settings       the host's workspaces.toml and routines files as the apps edit them;
                 env values and secrets never show
  settings-set   applies settings ops (protocol/PROTOCOL-v2.md, Settings) from FILE
                 or stdin: a JSON list, or {\"revision\",\"ops\"}. Without a revision
                 it uses the current one, which overwrites a hand edit made since
  chat           the conversation of a Task's agent, from its own records
  reader         the files of a Task's or a Routine run's folder; read prints
                 one (an image as its bytes), and with --seconds each change;
                 --commits: a change's diff over the branch's commits";

/// ADR 0075: every agent of a Workspace gets all its connections and servers.
const CHOICE_DEPRECATED: &str = "muxa-cli: --connection and --mcp are deprecated and ignored: \
every agent of a Workspace gets all its connections and MCP servers (ADR 0075)";

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
    Workspaces {
        json: bool,
    },
    Subscriptions {
        json: bool,
    },
    TokenUsage {
        day: Option<String>,
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
    Settings {
        json: bool,
    },
    SettingsSet(settings::Set),
    RunRoutine {
        workspace: String,
        name: String,
    },
    MigrateV1 {
        dry_run: bool,
        json: bool,
    },
    StartTask {
        workspace: String,
        title: String,
        goal: Option<String>,
        /// In the order given; none is one folder (ADR 0068).
        places: Vec<PlaceSpec>,
        plan: bool,
        task_loop: Option<LoopSpec>,
        /// A talk with this Bot (ADR 0083).
        bot: Option<String>,
        json: bool,
    },
    Loops {
        json: bool,
    },
    Chat {
        terminal: String,
        seconds: Option<u64>,
        json: bool,
    },
    LoopStart {
        terminal: String,
        spec: LoopSpec,
        reset: bool,
    },
    LoopStop {
        terminal: String,
    },
    Upload {
        terminal: String,
        file: String,
    },
    /// `task-shell`, or with `close` `close-terminal`.
    Shell {
        terminal: String,
        close: bool,
    },
    Watch {
        seconds: Option<u64>,
    },
    Attach {
        terminal: String,
        seconds: Option<u64>,
    },
    Reader {
        terminal: String,
        kind: String,
        html: bool,
        json: bool,
    },
    Read(reader::Read),
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
    let (mut html, mut branch) = (false, false);
    let (mut with_loop, mut reset) = (false, false);
    let mut spec = LoopSpec::default();
    let (mut workspace, mut title, mut goal) = (None, None, None);
    let mut bot: Option<String> = None;
    let mut places: Vec<PlaceSpec> = Vec::new();
    let mut chose = false;
    let mut words = Vec::new();
    let mut minutes: Option<u32> = None;
    let mut note: Option<String> = None;
    let (mut force, mut dry_run) = (false, false);
    let mut revision: Option<String> = None;
    let mut day: Option<String> = None;
    while let Some(arg) = p.next()? {
        match arg {
            Long("bridge") => bridge = p.value()?.string()?,
            Long("json") => json = true,
            Long("plan") => plan = true,
            Long("html") => html = true,
            Long("commits") => branch = true,
            Long("loop") => with_loop = true,
            Long("reset") => reset = true,
            Long("check") => spec.check = Some(p.value()?.string()?),
            Long("max-attempts") => spec.max_attempts = Some(p.value()?.parse()?),
            Long("timeout") => spec.timeout = Some(p.value()?.parse()?),
            Long("seconds") => seconds = Some(p.value()?.parse()?),
            Long("workspace") => workspace = Some(p.value()?.string()?),
            Long("title") => title = Some(p.value()?.string()?),
            Long("goal") => goal = Some(p.value()?.string()?),
            Long("bot") => bot = Some(p.value()?.string()?),
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
            Long("connection") | Long("mcp") => {
                p.value()?;
                if !std::mem::replace(&mut chose, true) {
                    eprintln!("{CHOICE_DEPRECATED}");
                }
            }
            Long("read") => places.push(PlaceSpec::Read {
                repo: p.value()?.string()?,
            }),
            Long("minutes") => minutes = Some(p.value()?.parse()?),
            Long("note") => note = Some(p.value()?.string()?),
            Long("force") => force = true,
            Long("dry-run") => dry_run = true,
            Long("existing") => places.push(PlaceSpec::Existing {
                path: p.value()?.string()?,
            }),
            Long("checkout") => places.push(PlaceSpec::Checkout {
                repo: p.value()?.string()?,
            }),
            Long("all-read") => places.push(PlaceSpec::AllRead),
            Long("revision") => revision = Some(p.value()?.string()?),
            Long("day") => day = Some(p.value()?.string()?),
            Long("help") | Short('h') => return Ok(None),
            Value(v) => words.push(v.string()?),
            _ => return Err(arg.unexpected()),
        }
    }
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let cmd = match words.as_slice() {
        ["terminals"] => Cmd::Terminals { json },
        ["workspaces"] => Cmd::Workspaces { json },
        ["subscriptions"] => Cmd::Subscriptions { json },
        ["token-usage"] => Cmd::TokenUsage {
            day: day.clone(),
            json,
        },
        ["approve", i] => Cmd::Inbox {
            action: inbox::Action::Approve {
                item: (*i).to_owned(),
                force,
            },
            json,
        },
        ["request-changes", i, comment @ ..] if !comment.is_empty() => Cmd::Inbox {
            action: inbox::Action::RequestChanges {
                item: (*i).to_owned(),
                comment: comment.join(" "),
            },
            json,
        },
        ["retry", i] => Cmd::Inbox {
            action: inbox::Action::Retry((*i).to_owned()),
            json,
        },
        ["apply", i] => Cmd::Inbox {
            action: inbox::Action::Apply((*i).to_owned()),
            json,
        },
        [which @ ("accept" | "dismiss"), i] => Cmd::Inbox {
            action: inbox::Action::Suggestion {
                item: (*i).to_owned(),
                accept: *which == "accept",
            },
            json,
        },
        ["start-task"] => Cmd::StartTask {
            workspace: workspace.ok_or("start-task needs --workspace")?,
            title: title.ok_or("start-task needs --title")?,
            goal,
            places,
            plan,
            task_loop: (with_loop || spec.check.is_some()).then(|| spec.clone()),
            bot,
            json,
        },
        ["loops"] => Cmd::Loops { json },
        ["chat", t] => Cmd::Chat {
            terminal: (*t).to_owned(),
            seconds,
            json,
        },
        ["loop-start", t] => Cmd::LoopStart {
            terminal: (*t).to_owned(),
            spec,
            reset,
        },
        ["loop-stop", t] => Cmd::LoopStop {
            terminal: (*t).to_owned(),
        },
        ["close-task", t] => Cmd::Inbox {
            action: inbox::Action::CloseTask {
                terminal: (*t).to_owned(),
                force,
            },
            json,
        },
        ["shell", t] => Cmd::Shell {
            terminal: (*t).to_owned(),
            close: false,
        },
        ["upload", t, f] => Cmd::Upload {
            terminal: (*t).to_owned(),
            file: (*f).to_owned(),
        },
        ["close-shell", t] => Cmd::Shell {
            terminal: (*t).to_owned(),
            close: true,
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
        ["settings"] => Cmd::Settings { json },
        ["settings-set", file @ ..] if file.len() <= 1 => Cmd::SettingsSet(settings::Set {
            file: file.first().map(|f| (*f).to_owned()),
            revision,
            json,
        }),
        ["migrate-v1"] => Cmd::MigrateV1 { dry_run, json },
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
        ["reader", t, k] => Cmd::Reader {
            terminal: (*t).to_owned(),
            kind: reader::kind(k)?,
            html,
            json,
        },
        ["read", t, k, path] => Cmd::Read(reader::Read {
            terminal: (*t).to_owned(),
            kind: reader::kind(k)?,
            path: (*path).to_owned(),
            html,
            branch,
            seconds,
        }),
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
            plan,
            task_loop,
            bot,
            json,
        } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            let mut msg = json!({"type": "start-task", "workspace": workspace, "title": title,
                             "goal": goal, "places": places, "plan": plan});
            if let Some(b) = bot {
                msg["bot"] = json!(b);
            }
            if let Some(l) = task_loop {
                msg["loop"] = json!(l);
            }
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
        Cmd::Chat {
            terminal,
            seconds,
            json,
        } => chat::run(bridge, &terminal, seconds, json).await,
        Cmd::Loops { json } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            let reply = c.request(json!({"type": "loops"})).await?;
            if json {
                println!("{}", reply["loops"]);
            } else {
                for l in reply["loops"].as_array().into_iter().flatten() {
                    println!("{}", loop_row(l));
                }
            }
            Ok(())
        }
        Cmd::LoopStart {
            terminal,
            spec,
            reset,
        } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            let msg =
                json!({"type": "loop-start", "terminal": terminal, "loop": spec, "reset": reset});
            let reply = c.request(msg).await?;
            println!("{}", loop_row(&reply["loop"]));
            Ok(())
        }
        Cmd::LoopStop { terminal } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            let reply = c
                .request(json!({"type": "loop-stop", "terminal": terminal}))
                .await?;
            println!("{}", loop_row(&reply["loop"]));
            Ok(())
        }
        Cmd::Upload { terminal, file } => {
            let request = upload_request(&terminal, &file)?;
            let (mut c, _child) = open(bridge, State::default()).await?;
            let reply = c.request(request).await?;
            println!("{}", reply["path"].as_str().unwrap_or("?"));
            Ok(())
        }
        Cmd::Shell { terminal, close } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            if close {
                c.request(json!({"type": "close-terminal", "terminal": terminal}))
                    .await?;
            } else {
                let reply = c
                    .request(json!({"type": "task-shell", "terminal": terminal}))
                    .await?;
                println!("{}", row(&reply["terminal"]));
            }
            Ok(())
        }
        Cmd::Watch { seconds } => watch(bridge, seconds).await,
        Cmd::Reader {
            terminal,
            kind,
            html,
            json,
        } => reader::list(bridge, &terminal, &kind, html, json).await,
        Cmd::Read(r) => reader::read(bridge, &r).await,
        Cmd::Workspaces { json } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            let reply = c.request(json!({"type": "workspaces"})).await?;
            if json {
                println!("{}", reply["workspaces"]);
            } else {
                for w in reply["workspaces"].as_array().into_iter().flatten() {
                    println!("{}", workspace_row(w));
                }
            }
            Ok(())
        }
        Cmd::Subscriptions { json } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            let reply = c.request(json!({"type": "subscriptions"})).await?;
            if json {
                println!("{}", reply["subscriptions"]);
            } else {
                for s in reply["subscriptions"].as_array().into_iter().flatten() {
                    println!("{}", subscription_row(s));
                }
            }
            Ok(())
        }
        Cmd::TokenUsage { day, json } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            let mut req = json!({"type": "token-usage"});
            if let Some(d) = day {
                req["day"] = json!(d);
            }
            let reply = c.request(req).await?;
            let usage = &reply["token_usage"];
            if json {
                println!("{usage}");
                return Ok(());
            }
            println!("{}", usage["day"].as_str().unwrap_or("?"));
            for w in usage["workspaces"].as_array().into_iter().flatten() {
                println!("{}", token_row(w["workspace"].as_str().unwrap_or("?"), w));
            }
            for t in usage["tasks"].as_array().into_iter().flatten() {
                let name = format!(
                    "  {}/{}",
                    t["workspace"].as_str().unwrap_or("?"),
                    t["title"].as_str().unwrap_or("?")
                );
                println!("{}", token_row(&name, t));
            }
            Ok(())
        }
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
        Cmd::Settings { json } => settings::show(bridge, json).await,
        Cmd::SettingsSet(args) => settings::set(bridge, args).await,
        Cmd::RunRoutine { workspace, name } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            c.request(json!({"type": "run-routine", "workspace": workspace, "name": name}))
                .await?;
            println!("started {workspace}/{name}");
            Ok(())
        }
        Cmd::MigrateV1 { dry_run, json } => {
            let (mut c, _child) = open(bridge, State::default()).await?;
            let reply = c
                .request(json!({"type": "migrate-v1", "dry_run": dry_run}))
                .await?;
            if json {
                println!(
                    "{}",
                    json!({"tasks": reply["tasks"], "added": reply["added"]})
                );
                return Ok(());
            }
            for t in reply["tasks"].as_array().into_iter().flatten() {
                println!("{}", v1_row(t));
            }
            println!("added {}", reply["added"].as_u64().unwrap_or(0));
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

/// `name  root  agent  repos  loop  connections`, one line per Workspace.
fn workspace_row(w: &Value) -> String {
    let words = |v: &Value, field: Option<&str>| {
        let list: Vec<String> = v
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| field.map_or(x, |f| &x[f]).as_str().map(str::to_owned))
            .collect();
        if list.is_empty() {
            "-".to_owned()
        } else {
            list.join(",")
        }
    };
    let checks = match &w["loop"]["checks"] {
        Value::String(s) => s.clone(),
        Value::Array(_) => words(&w["loop"]["checks"], None),
        _ => "-".to_owned(),
    };
    format!(
        "{}\t{}\t{}\trepos {}\tloop {checks}\tconnections {}",
        w["name"].as_str().unwrap_or("?"),
        w["root"].as_str().unwrap_or("?"),
        w["agent"].as_str().unwrap_or("-"),
        words(&w["repos"], None),
        words(&w["connections"], Some("name")),
    )
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
/// The `upload` request for a local file: its name and its bytes in base64.
fn upload_request(terminal: &str, file: &str) -> Result<Value, String> {
    use base64::Engine;
    let path = std::path::Path::new(file);
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("{file} has no file name"))?;
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {file}: {e}"))?;
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(json!({"type": "upload", "terminal": terminal, "name": name, "data": data}))
}

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

/// `TERMINAL\tSTATE\tATTEMPTS/MAX\tREASON`; the check is not shown.
fn loop_row(l: &Value) -> String {
    format!(
        "{}\t{}\t{}/{}\t{}",
        l["terminal"].as_str().unwrap_or("?"),
        l["state"].as_str().unwrap_or("?"),
        l["attempts"],
        l["max_attempts"],
        l["reason"].as_str().unwrap_or("")
    )
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

/// `id  status  workspace  title  path`.
fn v1_row(t: &Value) -> String {
    let s = |k: &str| t[k].as_str().unwrap_or("-");
    format!(
        "{}\t{}\t{}\t{}\t{}",
        s("id"),
        s("status"),
        s("workspace"),
        s("title"),
        s("path")
    )
}

/// `name agent plan 300m:42%@<resets_at> ...`, or `no report yet`.
/// `name\tin 1200\tcached 9000\tout 84`.
fn token_row(name: &str, t: &Value) -> String {
    let n = |k: &str| t[k].as_u64().unwrap_or(0);
    format!(
        "{name}\tin {}\tcached {}\tout {}",
        n("input"),
        n("cached"),
        n("output")
    )
}

fn subscription_row(s: &Value) -> String {
    let text = |v: &Value| v.as_str().unwrap_or("").to_owned();
    let windows: Vec<String> = s["windows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|w| {
            format!(
                "{}m:{}%@{}",
                w["minutes"], w["used_percent"], w["resets_at"]
            )
        })
        .collect();
    let limits = if windows.is_empty() {
        "no report yet".to_owned()
    } else {
        format!("{} as of {}", windows.join(" "), s["as_of"])
    };
    format!(
        "{}\t{}\t{}\t{limits}",
        text(&s["name"]),
        text(&s["agent"]),
        text(&s["plan"])
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<OsString> {
        s.iter().map(OsString::from).collect()
    }

    #[test]
    fn task_actions_parse_into_inbox_requests() {
        let action = |words: &[&str]| match parse(args(words)).unwrap() {
            Some((_, Cmd::Inbox { action, .. })) => inbox::request(&action, 0),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            action(&["approve", "i3", "--force"]),
            json!({"type": "inbox-approve", "item": "i3", "worktree": "force"})
        );
        assert_eq!(action(&["approve", "i3"])["worktree"], "remove");
        assert_eq!(
            action(&["close-task", "t1", "--force"]),
            json!({"type": "close-task", "terminal": "t1", "worktree": "force"})
        );
        assert_eq!(
            action(&["request-changes", "i3", "Add", "a", "test"]),
            json!({"type": "inbox-request-changes", "item": "i3", "comment": "Add a test"})
        );
        assert_eq!(
            action(&["retry", "i4"]),
            json!({"type": "inbox-retry", "item": "i4"})
        );
        assert_eq!(
            action(&["apply", "i5"]),
            json!({"type": "inbox-apply", "item": "i5"})
        );
        assert_eq!(
            action(&["dismiss", "i6"]),
            json!({"type": "inbox-suggestion", "item": "i6", "accept": false})
        );
        assert_eq!(parse(args(&["request-changes", "i3"])).unwrap(), None);
        assert_eq!(
            parse(args(&["workspaces", "--json"])).unwrap(),
            Some(("muxa-bridge connect".into(), Cmd::Workspaces { json: true }))
        );
    }

    #[test]
    fn migrate_and_resume_parse() {
        assert_eq!(
            parse(args(&["migrate-v1", "--dry-run"])).unwrap(),
            Some((
                "muxa-bridge connect".into(),
                Cmd::MigrateV1 {
                    dry_run: true,
                    json: false
                }
            ))
        );
        let Some((_, Cmd::StartTask { places, .. })) = parse(args(&[
            "start-task",
            "--workspace",
            "w",
            "--title",
            "Fix",
            "--existing",
            "/w/tasks/fix-t-000001",
        ]))
        .unwrap() else {
            panic!("no start-task");
        };
        assert_eq!(
            places,
            [PlaceSpec::Existing {
                path: "/w/tasks/fix-t-000001".into()
            }]
        );
        let t = json!({"id": "t-000001", "workspace": "w", "title": "Fix", "path": "/w/x",
                       "exists": true, "status": "added"});
        assert_eq!(v1_row(&t), "t-000001\tadded\tw\tFix\t/w/x");
    }

    #[test]
    fn a_workspace_row_shows_its_choices() {
        let w = json!({"name": "app", "root": "/src", "repos": ["app", "group/lib"],
                       "agent": "claude", "loop": {"checks": "any"},
                       "connections": [{"name": "github", "default": true}]});
        assert_eq!(
            workspace_row(&w),
            "app\t/src\tclaude\trepos app,group/lib\tloop any\tconnections github"
        );
        let bare = json!({"name": "p", "root": "/p", "repos": [], "loop": null, "connections": []});
        assert_eq!(
            workspace_row(&bare),
            "p\t/p\t-\trepos -\tloop -\tconnections -"
        );
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
            "--mcp",
            "docs",
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
                plan: true,
                task_loop: None,
                bot: None,
                json: false
            }
        );
        assert!(parse(args(&["start-task", "--workspace", "w"])).is_err());
        let (_, looped) = parse(args(&[
            "start-task",
            "--workspace",
            "w",
            "--title",
            "t",
            "--loop",
            "--max-attempts",
            "5",
        ]))
        .unwrap()
        .unwrap();
        let Cmd::StartTask { task_loop, .. } = looped else {
            panic!("{looped:?}")
        };
        assert_eq!(
            task_loop,
            Some(LoopSpec {
                check: None,
                max_attempts: Some(5),
                timeout: None
            })
        );
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
