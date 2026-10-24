//! Command-line flags, and finding tmux when SSH gives a minimal PATH.

use std::path::{Path, PathBuf};
use std::time::Duration;

pub const USAGE: &str =
    "usage: muxa-bridge [--tmux PATH] [--socket NAME] [--idle-timeout SECONDS] [--version]

Serves the muxa protocol on stdin/stdout. Run it as an SSH exec command.
  --tmux PATH             tmux binary (default: Homebrew, /usr/local/bin, then PATH)
  --socket NAME           tmux -L socket name (default: the user's default server)
  --idle-timeout SECONDS  exit after this long without a request (default: 60)
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
}

pub fn parse(args: impl IntoIterator<Item = std::ffi::OsString>) -> Result<Action, lexopt::Error> {
    use lexopt::prelude::*;
    let mut parser = lexopt::Parser::from_args(args);
    let mut out = Args {
        tmux: None,
        socket: None,
        idle: DEFAULT_IDLE,
    };
    while let Some(arg) = parser.next()? {
        match arg {
            Long("tmux") => out.tmux = Some(parser.value()?.into()),
            Long("socket") => out.socket = Some(parser.value()?.string()?),
            Long("idle-timeout") => out.idle = Duration::from_secs(parser.value()?.parse()?),
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
            parse_strs(&["--tmux", "/x/tmux", "--socket", "s", "--idle-timeout", "5"]).unwrap(),
            Action::Serve(Args {
                tmux: Some("/x/tmux".into()),
                socket: Some("s".into()),
                idle: Duration::from_secs(5)
            })
        );
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
