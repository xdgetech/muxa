//! Command-line flags, and finding tmux when SSH gives a minimal PATH.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::tasks::Choice;

pub const USAGE: &str = "usage: muxa-bridge [--tmux PATH] [--socket NAME] [--idle-timeout SECONDS]
                   [--agent-dir AGENT=PATH]... [--task-launcher builtin|tmux-agents=PATH]
                   [--task-command COMMAND] [--state-dir DIR] [--workspaces FILE]
                   [--daemon-dir DIR] [--version]
       muxa-bridge daemon run|install|stop|restart|uninstall|status [flags] [--print]

Serves the muxa protocol on stdin/stdout. Run it as an SSH exec command.
  --tmux PATH             tmux binary (default: Homebrew, /usr/local/bin, then PATH)
  --socket NAME           tmux -L socket name (default: the user's default server)
  --idle-timeout SECONDS  exit after this long without a request (default: 60)
  --agent-dir AGENT=PATH  where an agent keeps its data, e.g. claude-code=~/.claude;
                          repeat it for several dirs (default: claude-code=$HOME/.claude,
                          codex=$HOME/.codex, plus each workspace's agent env dirs)
  --task-launcher L       how new tasks start: builtin, or tmux-agents=PATH to its new.sh
                          (default: tmux-agents when it sets @agents_scripts, else builtin)
  --task-command COMMAND  the agent the built-in launcher starts (default: claude)
  --state-dir DIR         where task records are kept
                          (default: $XDG_STATE_HOME/muxa/tasks, else ~/.local/state/muxa/tasks)
  --workspaces FILE       the workspaces config
                          (default: $XDG_CONFIG_HOME/muxa/workspaces.toml, else ~/.config/...)
  --daemon-dir DIR        the daemon's socket, lock and log
                          (default: $XDG_STATE_HOME/muxa, else ~/.local/state/muxa)

The daemon runs the Task Loop on the host (ADR 0019). `daemon install` takes
the flags above, writes a LaunchAgent (macOS) or a systemd user unit (Linux)
and starts it; `--print` shows the file and changes nothing.
Logs go to stderr; MUXA_LOG sets the level (default: warn; the daemon: info).";

/// Non-interactive SSH sessions often lack Homebrew's directories in PATH.
const TMUX_CANDIDATES: [&str; 2] = ["/opt/homebrew/bin/tmux", "/usr/local/bin/tmux"];

const DEFAULT_IDLE: Duration = Duration::from_secs(60);

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Serve(Args),
    Daemon(Daemon),
    Version,
    Help,
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
    pub tmux: Option<PathBuf>,
    pub socket: Option<String>,
    pub idle: Duration,
    /// Agent adapter id -> data directory.
    pub agent_dirs: Vec<(String, PathBuf)>,
    pub task_launcher: Option<Choice>,
    pub task_command: String,
    pub state_dir: Option<PathBuf>,
    pub workspaces: Option<PathBuf>,
    pub daemon_dir: Option<PathBuf>,
}

impl Args {
    /// The flags that give these args again, for the daemon's service file.
    /// `--tmux` and `--idle-timeout` are left to the caller.
    pub fn service_flags(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut push = |flag: &str, value: String| {
            out.push(flag.to_owned());
            out.push(value);
        };
        if let Some(s) = &self.socket {
            push("--socket", s.clone());
        }
        if let Some(d) = &self.state_dir {
            push("--state-dir", d.display().to_string());
        }
        if let Some(w) = &self.workspaces {
            push("--workspaces", w.display().to_string());
        }
        for (agent, dir) in &self.agent_dirs {
            push("--agent-dir", format!("{agent}={}", dir.display()));
        }
        match &self.task_launcher {
            Some(Choice::Builtin) => push("--task-launcher", "builtin".into()),
            Some(Choice::TmuxAgents(p)) => {
                push("--task-launcher", format!("tmux-agents={}", p.display()))
            }
            None => {}
        }
        if self.task_command != DEFAULT_TASK_COMMAND {
            push("--task-command", self.task_command.clone());
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
            Action::Serve(args) => Action::Daemon(Daemon { op, args, print }),
            other => other,
        });
    }
    parse_flags(args, |_| false)
}

/// The serve flags, plus the switches `extra` accepts.
fn parse_flags(
    args: Vec<std::ffi::OsString>,
    mut extra: impl FnMut(&str) -> bool,
) -> Result<Action, lexopt::Error> {
    use lexopt::prelude::*;
    let mut parser = lexopt::Parser::from_args(args);
    let mut out = Args {
        tmux: None,
        socket: None,
        idle: DEFAULT_IDLE,
        agent_dirs: Vec::new(),
        task_launcher: None,
        task_command: DEFAULT_TASK_COMMAND.into(),
        state_dir: None,
        workspaces: None,
        daemon_dir: None,
    };
    while let Some(arg) = parser.next()? {
        match arg {
            Long("tmux") => out.tmux = Some(parser.value()?.into()),
            Long("socket") => out.socket = Some(parser.value()?.string()?),
            Long("idle-timeout") => out.idle = Duration::from_secs(parser.value()?.parse()?),
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
            Long("task-launcher") => {
                let value = parser.value()?.string()?;
                out.task_launcher = Some(match value.split_once('=') {
                    None if value == "builtin" => Choice::Builtin,
                    Some(("tmux-agents", path)) if !path.is_empty() => {
                        Choice::TmuxAgents(path.into())
                    }
                    _ => {
                        return Err(lexopt::Error::from(format!(
                            "--task-launcher wants builtin or tmux-agents=PATH, got {value:?}"
                        )));
                    }
                });
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
            Long("version") => return Ok(Action::Version),
            Long("help") | Short('h') => return Ok(Action::Help),
            Long(flag) if extra(flag) => {}
            _ => return Err(arg.unexpected()),
        }
    }
    Ok(Action::Serve(out))
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

/// The tmux to run: the flag, then the usual install locations, then PATH.
pub fn find_tmux(flag: Option<PathBuf>, path_var: Option<&str>) -> Option<PathBuf> {
    if flag.is_some() {
        return flag;
    }
    TMUX_CANDIDATES
        .iter()
        .map(PathBuf::from)
        .chain(
            path_var
                .into_iter()
                .flat_map(|p| std::env::split_paths(p).map(|d| d.join("tmux"))),
        )
        .find(|p| is_executable(p))
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Action, lexopt::Error> {
        parse(args.iter().map(std::ffi::OsString::from))
    }

    #[test]
    fn parses_flags() {
        assert_eq!(
            parse_strs(&[
                "--tmux",
                "/x/tmux",
                "--socket",
                "s",
                "--idle-timeout",
                "5",
                "--agent-dir",
                "claude-code=/tmp/c=d",
            ])
            .unwrap(),
            Action::Serve(Args {
                tmux: Some("/x/tmux".into()),
                socket: Some("s".into()),
                idle: Duration::from_secs(5),
                agent_dirs: vec![("claude-code".into(), "/tmp/c=d".into())],
                task_launcher: None,
                task_command: DEFAULT_TASK_COMMAND.into(),
                state_dir: None,
                workspaces: None,
                daemon_dir: None,
            })
        );
        let Action::Serve(a) = parse_strs(&[
            "--agent-dir",
            "claude-code=/a",
            "--agent-dir",
            "claude-code=/b",
        ])
        .unwrap() else {
            panic!("not serve")
        };
        assert_eq!(
            a.agent_dirs,
            [
                ("claude-code".into(), "/a".into()),
                ("claude-code".into(), "/b".into())
            ],
            "a repeated --agent-dir keeps both"
        );
        assert!(parse_strs(&["--agent-dir", "/no/agent"]).is_err());
        assert!(parse_strs(&["--agent-dir", "claude-code="]).is_err());
        let Action::Serve(a) = parse_strs(&[
            "--task-launcher",
            "tmux-agents=/p/new.sh",
            "--task-command",
            "codex",
        ])
        .unwrap() else {
            panic!("not serve")
        };
        assert_eq!(
            a.task_launcher,
            Some(Choice::TmuxAgents("/p/new.sh".into()))
        );
        assert_eq!(a.task_command, "codex");
        assert!(parse_strs(&["--task-launcher", "other"]).is_err());
        let Action::Serve(a) =
            parse_strs(&["--state-dir", "/s/tasks", "--workspaces", "/c/w.toml"]).unwrap()
        else {
            panic!("not serve")
        };
        assert_eq!(a.state_dir, Some("/s/tasks".into()));
        assert_eq!(a.workspaces, Some("/c/w.toml".into()));
        assert!(parse_strs(&["--state-dir", "rel/tasks"]).is_err());
        assert!(parse_strs(&["--workspaces", "w.toml"]).is_err());
        assert!(parse_strs(&["--task-command", " "]).is_err());
        assert_eq!(parse_strs(&["--version"]).unwrap(), Action::Version);
        assert!(parse_strs(&["--bogus"]).is_err());
        assert!(parse_strs(&["--idle-timeout", "x"]).is_err());
    }

    #[test]
    fn parses_daemon_commands() {
        let Action::Daemon(d) = parse_strs(&[
            "daemon",
            "install",
            "--socket",
            "s",
            "--daemon-dir",
            "/d",
            "--task-launcher",
            "tmux-agents=/p/new.sh",
            "--print",
        ])
        .unwrap() else {
            panic!("not daemon")
        };
        assert_eq!((d.op, d.print), (DaemonOp::Install, true));
        assert_eq!(d.args.daemon_dir, Some("/d".into()));
        assert_eq!(
            d.args.service_flags(),
            ["--socket", "s", "--task-launcher", "tmux-agents=/p/new.sh"]
        );
        let Action::Daemon(d) = parse_strs(&["daemon", "run"]).unwrap() else {
            panic!("not daemon")
        };
        assert_eq!((d.op, d.print), (DaemonOp::Run, false));
        assert!(
            parse_strs(&["daemon", "run", "--print"]).is_err(),
            "--print is for install"
        );
        assert!(parse_strs(&["--print"]).is_err());
        assert!(parse_strs(&["daemon"]).is_err());
        assert!(parse_strs(&["daemon", "bogus"]).is_err());
        assert!(parse_strs(&["daemon", "status", "--daemon-dir", "rel"]).is_err());
    }

    #[test]
    fn the_flag_wins_and_path_is_searched() {
        assert_eq!(
            find_tmux(Some("/given".into()), None),
            Some("/given".into())
        );
        let dir = std::env::temp_dir().join(format!("muxa-args-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("tmux");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let found = find_tmux(None, Some(dir.to_str().unwrap())).unwrap();
        // A Homebrew or /usr/local tmux on this machine comes first.
        assert!(found == bin || TMUX_CANDIDATES.iter().any(|c| found == Path::new(c)));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
