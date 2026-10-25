//! Command-line flags, and finding tmux when SSH gives a minimal PATH.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::tasks::Choice;

pub const USAGE: &str = "usage: muxa-bridge [--tmux PATH] [--socket NAME] [--idle-timeout SECONDS]
                   [--agent-dir AGENT=PATH]... [--task-launcher builtin|tmux-agents=PATH]
                   [--task-command COMMAND] [--version]

Serves the muxa protocol on stdin/stdout. Run it as an SSH exec command.
  --tmux PATH             tmux binary (default: Homebrew, /usr/local/bin, then PATH)
  --socket NAME           tmux -L socket name (default: the user's default server)
  --idle-timeout SECONDS  exit after this long without a request (default: 60)
  --agent-dir AGENT=PATH  where an agent keeps its data, e.g. claude-code=~/.claude
                          (default: claude-code=$HOME/.claude)
  --task-launcher L       how new tasks start: builtin, or tmux-agents=PATH to its new.sh
                          (default: tmux-agents when it sets @agents_scripts, else builtin)
  --task-command COMMAND  the agent the built-in launcher starts (default: claude)
Logs go to stderr; MUXA_LOG sets the level (default: warn).";

/// Non-interactive SSH sessions often lack Homebrew's directories in PATH.
const TMUX_CANDIDATES: [&str; 2] = ["/opt/homebrew/bin/tmux", "/usr/local/bin/tmux"];

const DEFAULT_IDLE: Duration = Duration::from_secs(60);

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Serve(Args),
    Version,
    Help,
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
}

pub const DEFAULT_TASK_COMMAND: &str = "claude";

pub fn parse(args: impl IntoIterator<Item = std::ffi::OsString>) -> Result<Action, lexopt::Error> {
    use lexopt::prelude::*;
    let mut parser = lexopt::Parser::from_args(args);
    let mut out = Args {
        tmux: None,
        socket: None,
        idle: DEFAULT_IDLE,
        agent_dirs: Vec::new(),
        task_launcher: None,
        task_command: DEFAULT_TASK_COMMAND.into(),
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
            Long("version") => return Ok(Action::Version),
            Long("help") | Short('h') => return Ok(Action::Help),
            _ => return Err(arg.unexpected()),
        }
    }
    Ok(Action::Serve(out))
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
            })
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
        assert!(parse_strs(&["--task-command", " "]).is_err());
        assert_eq!(parse_strs(&["--version"]).unwrap(), Action::Version);
        assert!(parse_strs(&["--bogus"]).is_err());
        assert!(parse_strs(&["--idle-timeout", "x"]).is_err());
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
