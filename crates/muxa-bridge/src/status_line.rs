//! Claude Code's status line as muxa's usage source (ADR 0103 §6). muxa adds
//! `muxa-bridge statusline` to an agent's `--settings`; it records the plan
//! limits from its stdin, then runs and prints the login's own status line.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use serde_json::{Value, json};

use crate::records::workers::LaunchCommand;

/// The status line input: a session's model, folder and limits; small.
const MAX_INPUT: u64 = 256 * 1024;
const MAX_SETTINGS: u64 = 1024 * 1024;
/// The file every muxa writer of Claude Code settings uses, in `.muxa/`.
const SETTINGS_FILE: &str = "claude-settings.json";
const SETTINGS_FLAG: &str = "--settings";

/// What the hook needs to call back into this binary.
#[derive(Debug, Clone)]
pub struct Hook {
    pub exe: PathBuf,
    pub state_dir: PathBuf,
    pub subscription: String,
}

impl Hook {
    /// The daemon's own binary; None when it cannot be found.
    pub fn current(state_dir: Option<&Path>, subscription: &str) -> Option<Hook> {
        Some(Hook {
            exe: std::env::current_exe().ok()?,
            state_dir: state_dir?.to_owned(),
            subscription: subscription.to_owned(),
        })
    }

    fn command_line(&self) -> String {
        format!(
            "{} statusline --subscription {} --state-dir {}",
            quote(&self.exe.to_string_lossy()),
            quote(&self.subscription),
            quote(&self.state_dir.to_string_lossy())
        )
    }
}

/// One word for `/bin/sh`.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Blocking. `command` with muxa's status line in its Claude Code settings:
/// merged into the `.muxa/claude-settings.json` it already passes, else in a
/// new one. A command that passes settings of its own is left as it is,
/// since a second `--settings` could hide them.
pub fn add(command: LaunchCommand, dir: &Path, hook: &Hook) -> Result<LaunchCommand, String> {
    if !crate::launch::is_claude_code(&command) {
        return Ok(command);
    }
    let ours = dir.join(crate::v2::git::DIR).join(SETTINGS_FILE);
    let passed = match &command {
        LaunchCommand::Shell(line) => last_shell_value(line, SETTINGS_FLAG),
        LaunchCommand::Native { args, .. } => args
            .iter()
            .rposition(|a| a == SETTINGS_FLAG)
            .and_then(|i| args.get(i + 1).cloned()),
    };
    let mut settings = match &passed {
        Some(path) if Path::new(path) == ours => read_settings(&ours)?,
        Some(_) => return Ok(command),
        None => json!({}),
    };
    settings["statusLine"] = json!({"type": "command", "command": hook.command_line()});
    let mut text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    text.push('\n');
    let path = crate::v2::git::write_file(dir, SETTINGS_FILE, &text)?;
    if passed.is_some() {
        return Ok(command);
    }
    let path = path.to_string_lossy().into_owned();
    Ok(match command {
        LaunchCommand::Shell(line) => {
            LaunchCommand::Shell(format!("{line} {SETTINGS_FLAG} {}", quote(&path)))
        }
        LaunchCommand::Native { program, mut args } => {
            args.extend([SETTINGS_FLAG.to_owned(), path]);
            LaunchCommand::Native { program, args }
        }
    })
}

/// [`add`] for a shell line.
pub fn add_to_line(line: String, dir: &Path, hook: &Hook) -> Result<String, String> {
    match add(LaunchCommand::Shell(line), dir, hook)? {
        LaunchCommand::Shell(line) => Ok(line),
        LaunchCommand::Native { .. } => Err("a shell line became a program".into()),
    }
}

fn read_settings(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{} is not JSON: {e}", path.display()))
}

/// Pure. The word after the last `flag` in a shell line muxa wrote: one
/// single-quoted word (with `'\''` for a quote), or a bare one.
fn last_shell_value(line: &str, flag: &str) -> Option<String> {
    let at = line.rfind(&format!("{flag} "))? + flag.len() + 1;
    let rest = &line[at..];
    let Some(mut quoted) = rest.strip_prefix('\'') else {
        return rest.split_whitespace().next().map(str::to_owned);
    };
    let mut out = String::new();
    loop {
        let end = quoted.find('\'')?;
        out.push_str(&quoted[..end]);
        match quoted[end..].strip_prefix("'\\''") {
            Some(more) => {
                out.push('\'');
                quoted = more;
            }
            None => return Some(out),
        }
    }
}

/// `muxa-bridge statusline --subscription NAME --state-dir DIR`, as Claude
/// Code runs it. It never fails the status line: a bad input records nothing.
pub fn main(subscription: &str, state_dir: &Path) -> ExitCode {
    let mut input = Vec::new();
    let _ = std::io::stdin().take(MAX_INPUT).read_to_end(&mut input);
    if let Ok(value) = serde_json::from_slice::<Value>(&input) {
        let windows = crate::limits::from_claude(&value);
        let _ = crate::limits::record(state_dir, subscription, &windows, crate::records::now());
    }
    if let Some(own) = own_command() {
        print!("{}", run_own(&own, &input));
        let _ = std::io::stdout().flush();
    }
    ExitCode::SUCCESS
}

/// The login's own status line: `statusLine.command` in its `settings.json`,
/// the folder Claude Code reads (`CLAUDE_CONFIG_DIR`, else `~/.claude`).
fn own_command() -> Option<String> {
    let home = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude")))?;
    let file = std::fs::File::open(home.join("settings.json")).ok()?;
    let mut text = String::new();
    file.take(MAX_SETTINGS).read_to_string(&mut text).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let line = &value["statusLine"];
    (line["type"] == "command")
        .then(|| line["command"].as_str().map(str::to_owned))
        .flatten()
        .filter(|c| !c.trim().is_empty())
}

/// What the user's command prints with the same input; nothing on a failure.
fn run_own(command: &str, input: &[u8]) -> String {
    let child = Command::new("/bin/sh")
        .args(["-c", command])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return String::new();
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input);
    }
    child
        .wait_with_output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Pure. A subscription's login terminal (ADR 0103 §7): the agent in a
/// login `shell`, so its PATH is the user's, with the subscription's folder.
/// Claude Code asks to log in on a new folder, or takes `/login`; Codex has
/// `codex login`.
pub fn login_launch(
    sub: &crate::records::subscriptions::Subscription,
    shell: Vec<String>,
    home: Option<&Path>,
) -> muxa_server::LoginLaunch {
    let program = match sub.agent {
        crate::records::workers::WorkerAgent::ClaudeCode => "claude",
        crate::records::workers::WorkerAgent::Codex => "codex login",
    };
    let shell_path = shell.into_iter().next().unwrap_or_else(|| "/bin/sh".into());
    muxa_server::LoginLaunch {
        argv: vec![shell_path, "-l".into(), "-c".into(), program.into()],
        cwd: home.map(Path::to_path_buf),
        env: sub.env().into_iter().collect(),
    }
}

#[cfg(test)]
#[path = "status_line_tests.rs"]
mod tests;
