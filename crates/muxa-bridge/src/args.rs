//! Command-line flags.

use std::path::PathBuf;

pub const USAGE: &str = "usage: muxa-bridge connect [--daemon-dir DIR]
       muxa-bridge daemon run|install|stop|restart|uninstall|status [flags] [--print]
       muxa-bridge --version
       muxa-bridge statusline --subscription NAME --state-dir DIR

`statusline` is the Claude Code status line muxa gives its agents: it records
the plan limits for the subscription, then prints the login's own status line.

`connect` relays protocol v2 (ADR 0067) to the daemon's server.sock; apps run
it as their SSH exec command. With no command, muxa-bridge does the same when
SSH_ORIGINAL_COMMAND ends in `connect`, for a key whose authorized_keys line
forces muxa-bridge.

The daemon serves protocol v2 on herdr-core: Tasks, Routines, Task Loops,
the Reader and Chat. `daemon install` takes these flags, writes a LaunchAgent
(macOS) or a systemd user unit (Linux) and starts it; `--print` shows the file
and changes nothing.
  --agent-dir AGENT=PATH  where an agent keeps its data, e.g. claude-code=~/.claude;
                          repeat it for several dirs (default: claude-code=$HOME/.claude,
                          codex=$HOME/.codex, plus each workspace's agent env dirs)
  --task-command COMMAND  the agent a Workspace without one runs (default: claude)
  --state-dir DIR         where Routine and Inbox state is kept
                          (default: $XDG_STATE_HOME/muxa/tasks, else ~/.local/state/muxa/tasks)
  --workspaces FILE       the workspaces config
                          (default: $XDG_CONFIG_HOME/muxa/workspaces.toml, else ~/.config/...)
  --daemon-dir DIR        the daemon's socket, lock and log
                          (default: $XDG_STATE_HOME/muxa, else ~/.local/state/muxa)
  --settings-read-only    the apps may read the settings but not change them (ADR 0074)
Logs go to stderr; MUXA_LOG sets the level (default: warn; the daemon: info).";

/// Flags of muxa for tmux that an older service file still passes: read and
/// ignored, so the daemon starts before `daemon install` writes a new file.
const RETIRED: [&str; 4] = ["tmux", "socket", "idle-timeout", "task-launcher"];

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// No command: relay when SSH asked for `connect`, else print the usage.
    Bare(Args),
    Daemon(Daemon),
    /// `connect [--daemon-dir DIR]`: relay a protocol v2 client to the daemon.
    Connect(Args),
    /// `statusline --subscription NAME --state-dir DIR`: Claude Code's status
    /// line for an agent muxa started (ADR 0091).
    StatusLine {
        subscription: String,
        state_dir: std::path::PathBuf,
    },
    Version,
    /// The usage text to print.
    Help(&'static str),
}

/// A device key may only run `muxa-bridge` (its `authorized_keys` line forces
/// the command). Its client asks for protocol v2 by sending `… connect`, which
/// sshd puts in `SSH_ORIGINAL_COMMAND`.
pub fn asks_for_connect(original: Option<&str>) -> bool {
    original.and_then(|c| c.split_whitespace().last()) == Some("connect")
}

/// `muxa-bridge daemon <op> [flags]`.
#[derive(Debug, PartialEq, Eq)]
pub struct Daemon {
    pub op: DaemonOp,
    pub args: Args,
    /// `install --print`: show the file, change nothing.
    pub print: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonOp {
    Run,
    Install,
    Stop,
    Restart,
    Uninstall,
    Status,
}

impl DaemonOp {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "run" => Self::Run,
            "install" => Self::Install,
            "stop" => Self::Stop,
            "restart" => Self::Restart,
            "uninstall" => Self::Uninstall,
            "status" => Self::Status,
            _ => return None,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Args {
    /// Agent adapter id -> data directory.
    pub agent_dirs: Vec<(String, PathBuf)>,
    pub task_command: String,
    pub state_dir: Option<PathBuf>,
    pub workspaces: Option<PathBuf>,
    pub daemon_dir: Option<PathBuf>,
    /// `settings-update` is refused (ADR 0074).
    pub settings_read_only: bool,
}

impl Args {
    /// The flags that give these args again, for the daemon's service file.
    pub fn service_flags(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut push = |flag: &str, value: String| {
            out.push(flag.to_owned());
            out.push(value);
        };
        if let Some(d) = &self.state_dir {
            push("--state-dir", d.display().to_string());
        }
        if let Some(w) = &self.workspaces {
            push("--workspaces", w.display().to_string());
        }
        for (agent, dir) in &self.agent_dirs {
            push("--agent-dir", format!("{agent}={}", dir.display()));
        }
        if self.task_command != DEFAULT_TASK_COMMAND {
            push("--task-command", self.task_command.clone());
        }
        if self.settings_read_only {
            out.push("--settings-read-only".to_owned());
        }
        out
    }
}

pub const DEFAULT_TASK_COMMAND: &str = "claude";

pub fn parse(args: impl IntoIterator<Item = std::ffi::OsString>) -> Result<Action, lexopt::Error> {
    let mut args: Vec<std::ffi::OsString> = args.into_iter().collect();
    if args.first().is_some_and(|a| a == "daemon") {
        let op = args
            .get(1)
            .and_then(|a| a.to_str())
            .and_then(DaemonOp::parse)
            .ok_or_else(|| {
                lexopt::Error::from(
                    "daemon wants one of run, install, stop, restart, uninstall, status",
                )
            })?;
        let rest = args.split_off(2);
        let mut print = false;
        let parsed = parse_flags(rest, |flag| {
            let allowed = flag == "print" && op == DaemonOp::Install;
            print |= allowed;
            allowed
        })?;
        return Ok(match parsed {
            Action::Bare(args) => Action::Daemon(Daemon { op, args, print }),
            other => other,
        });
    }
    if args.first().is_some_and(|a| a == "statusline") {
        return status_line(&args[1..]);
    }
    if args.first().is_some_and(|a| a == "connect") {
        return Ok(match parse_flags(args.split_off(1), |_| false)? {
            Action::Bare(args) => Action::Connect(args),
            other => other,
        });
    }
    parse_flags(args, |_| false)
}

/// `statusline`'s two flags, both required.
fn status_line(rest: &[std::ffi::OsString]) -> Result<Action, lexopt::Error> {
    use lexopt::ValueExt;
    let (mut subscription, mut state_dir) = (None, None);
    let mut p = lexopt::Parser::from_args(rest.iter().cloned());
    while let Some(arg) = p.next()? {
        match arg {
            lexopt::Arg::Long("subscription") => subscription = Some(p.value()?.string()?),
            lexopt::Arg::Long("state-dir") => {
                state_dir = Some(std::path::PathBuf::from(p.value()?))
            }
            other => return Err(other.unexpected()),
        }
    }
    match (subscription, state_dir) {
        (Some(subscription), Some(state_dir)) if state_dir.is_absolute() => {
            Ok(Action::StatusLine {
                subscription,
                state_dir,
            })
        }
        _ => Err("statusline wants --subscription NAME --state-dir /ABSOLUTE/DIR".into()),
    }
}

/// The flags, plus the switches `extra` accepts.
fn parse_flags(
    args: Vec<std::ffi::OsString>,
    mut extra: impl FnMut(&str) -> bool,
) -> Result<Action, lexopt::Error> {
    use lexopt::prelude::*;
    let mut parser = lexopt::Parser::from_args(args);
    let mut out = Args {
        agent_dirs: Vec::new(),
        task_command: DEFAULT_TASK_COMMAND.into(),
        state_dir: None,
        workspaces: None,
        daemon_dir: None,
        settings_read_only: false,
    };
    while let Some(arg) = parser.next()? {
        match arg {
            Long("agent-dir") => {
                let value = parser.value()?.string()?;
                let Some((agent, dir)) = value
                    .split_once('=')
                    .filter(|(a, d)| !a.is_empty() && !d.is_empty())
                else {
                    return Err(lexopt::Error::from(format!(
                        "--agent-dir wants AGENT=PATH, got {value:?}"
                    )));
                };
                out.agent_dirs.push((agent.to_owned(), dir.into()));
            }
            Long("task-command") => {
                let value = parser.value()?.string()?;
                if value.trim().is_empty() {
                    return Err(lexopt::Error::from("--task-command is empty"));
                }
                out.task_command = value;
            }
            Long("state-dir") => {
                out.state_dir = Some(absolute(parser.value()?.into(), "--state-dir")?)
            }
            Long("workspaces") => {
                out.workspaces = Some(absolute(parser.value()?.into(), "--workspaces")?)
            }
            Long("daemon-dir") => {
                out.daemon_dir = Some(absolute(parser.value()?.into(), "--daemon-dir")?)
            }
            Long("settings-read-only") => out.settings_read_only = true,
            Long(flag) if RETIRED.contains(&flag) => {
                parser.value()?;
            }
            Long("version") => return Ok(Action::Version),
            Long("help") | Short('h') => return Ok(Action::Help(USAGE)),
            Long(flag) if extra(flag) => {}
            _ => return Err(arg.unexpected()),
        }
    }
    Ok(Action::Bare(out))
}

/// Relative paths would depend on where SSH starts the bridge.
fn absolute(path: PathBuf, flag: &str) -> Result<PathBuf, lexopt::Error> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(lexopt::Error::from(format!(
            "{flag} wants an absolute path, got {}",
            path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Action, lexopt::Error> {
        parse(args.iter().map(std::ffi::OsString::from))
    }

    #[test]
    fn daemon_ops_take_the_flags() {
        let Action::Daemon(d) = parse_strs(&[
            "daemon",
            "install",
            "--print",
            "--state-dir",
            "/s",
            "--agent-dir",
            "claude-code=/c",
            "--settings-read-only",
        ])
        .unwrap() else {
            panic!("not a daemon command")
        };
        assert_eq!((d.op, d.print), (DaemonOp::Install, true));
        assert_eq!(
            d.args.service_flags(),
            [
                "--state-dir",
                "/s",
                "--agent-dir",
                "claude-code=/c",
                "--settings-read-only"
            ]
        );
        assert!(
            parse_strs(&["daemon", "run", "--print"]).is_err(),
            "--print is install's"
        );
        assert!(parse_strs(&["daemon", "fly"]).is_err());
    }

    #[test]
    fn an_older_service_files_tmux_flags_are_ignored() {
        let Action::Daemon(d) = parse_strs(&[
            "daemon",
            "run",
            "--tmux",
            "/opt/homebrew/bin/tmux",
            "--socket",
            "work",
            "--task-launcher",
            "builtin",
            "--workspaces",
            "/w.toml",
        ])
        .unwrap() else {
            panic!("not a daemon command")
        };
        assert_eq!(
            d.args.workspaces.as_deref(),
            Some(std::path::Path::new("/w.toml"))
        );
        assert_eq!(d.args.service_flags(), ["--workspaces", "/w.toml"]);
    }

    #[test]
    fn connect_and_bare_and_bad_paths() {
        assert!(matches!(
            parse_strs(&["connect", "--daemon-dir", "/d"]),
            Ok(Action::Connect(_))
        ));
        assert!(matches!(parse_strs(&[]), Ok(Action::Bare(_))));
        assert!(matches!(parse_strs(&["--version"]), Ok(Action::Version)));
        assert!(parse_strs(&["--state-dir", "relative"]).is_err());
        assert!(parse_strs(&["--agent-dir", "nodir"]).is_err());
        assert!(asks_for_connect(Some("~/.local/bin/muxa-bridge connect")));
        assert!(!asks_for_connect(Some("muxa-bridge")));
        assert!(!asks_for_connect(None));
    }
}
