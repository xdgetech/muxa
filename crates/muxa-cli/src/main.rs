//! `muxa-cli`: a terminal client for the muxa protocol, the reference client.

mod conn;
mod view;

use std::process::ExitCode;

use muxa_protocol::{Role, Session};

const USAGE: &str = "usage: muxa-cli [--bridge CMD] list
       muxa-cli [--bridge CMD] watch TARGET [--read-only] [--seconds N]

  --bridge CMD   how to start the bridge (default: muxa-bridge). Over SSH:
                 --bridge 'ssh host ~/.local/bin/muxa-bridge'
  TARGET         a pane id (%12), or SESSION, SESSION:agent or SESSION:shell
  --read-only    show the pane, send no keys
  --seconds N    leave after N seconds (default: on Ctrl-])";

enum Cmd {
    List,
    Watch {
        target: String,
        read_only: bool,
        seconds: Option<u64>,
    },
}

fn parse() -> Result<Option<(String, Cmd)>, lexopt::Error> {
    use lexopt::prelude::*;
    let mut p = lexopt::Parser::from_env();
    let (mut bridge, mut words) = ("muxa-bridge".to_string(), Vec::new());
    let (mut read_only, mut seconds) = (false, None);
    while let Some(arg) = p.next()? {
        match arg {
            Long("bridge") => bridge = p.value()?.string()?,
            Long("read-only") => read_only = true,
            Long("seconds") => seconds = Some(p.value()?.parse()?),
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
    let cmd = match words.as_slice() {
        [w] if w == "list" => Cmd::List,
        [w, target] if w == "watch" => Cmd::Watch {
            target: target.clone(),
            read_only,
            seconds,
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
    let sessions = conn.list().await?;
    match cmd {
        Cmd::List => {
            print_sessions(&tmux, &sessions);
            Ok(String::new())
        }
        Cmd::Watch {
            target,
            read_only,
            seconds,
        } => {
            let pane = resolve(&sessions, &target)?;
            view::run(&mut conn, &pane, view::Options { read_only, seconds }).await
        }
    }
}

fn print_sessions(tmux: &str, sessions: &[Session]) {
    println!("tmux {tmux}");
    for s in sessions {
        let meta: Vec<&str> = [s.repo.as_deref(), s.branch.as_deref()]
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
