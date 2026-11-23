//! What a Bot's Routine run gets beyond a plain run (ADR 0082): the Bot's
//! Soul and Memory for its brief, and Claude Code settings that deny edits
//! to the Workspace's `.muxa/` and, for `read_only`, to its repos.
use std::path::Path;

use super::git;
use crate::records::Workspace;
use crate::records::bots::Bot;
use crate::records::routines::Routine;
use crate::records::workers::LaunchCommand;
use crate::routines::brief::BotBrief;
use crate::routines::identity::Reach;

const SETTINGS_FILE: &str = "claude-settings.json";
const SETTINGS_FLAG: &str = "--settings";

/// Blocking. The Bot's files as they are now, its team when it leads one,
/// and what it can use in its Workspace `ws`.
pub fn brief(bot: &Bot, team: &[&Bot], ws: &Workspace) -> Result<BotBrief, String> {
    Ok(BotBrief {
        reach: Reach {
            workspace: ws.name.clone(),
            daily_items: bot.daily_items,
            connections: ws
                .connections
                .iter()
                .map(|c| format!("{} (${})", c.name, c.env))
                .collect(),
            mcp: ws.mcp.iter().map(|m| m.name.clone()).collect(),
            claude_connectors: ws.claude_connectors,
            allow: ws.bot_rules.allow.clone(),
            deny: ws.bot_rules.deny.clone(),
        },
        name: bot.name.clone(),
        soul: bot.read_soul().map_err(|e| e.to_string())?,
        memory: bot.read_memory().map_err(|e| e.to_string())?,
        team: team
            .iter()
            .map(|m| (m.name.clone(), m.description.clone()))
            .collect(),
    })
}

/// Claude Code, as a Bot always runs (ADR 0087).
pub const CLAUDE: &str = "claude";

/// Pure. A Bot runs Claude Code: the Workspace's agent when it is Claude Code
/// (its flags and model stay), else plain `claude`.
pub fn as_bot(command: LaunchCommand) -> LaunchCommand {
    if crate::launch::is_claude_code(&command) {
        command
    } else {
        LaunchCommand::Shell(CLAUDE.to_owned())
    }
}

/// Pure. muxa's deny `rules` with the Workspace's Bot rules (ADR 0087): its
/// deny rules join muxa's, and its allow rules come back for the settings.
pub fn bot_rules(ws: &Workspace, mut rules: Vec<String>) -> (Vec<String>, Vec<String>) {
    rules.extend(ws.bot_rules.deny.iter().cloned());
    (rules, ws.bot_rules.allow.clone())
}

/// Pure. Paths start with `//` to be absolute; `Edit(…)` covers every
/// built-in tool that edits a file. A Bot never edits its own Soul or the
/// routines file; a read-only run edits no repo either, except one that
/// holds `run_dir`, where it must write its output.
pub fn deny_rules(ws: &Workspace, routine: &Routine, run_dir: &Path) -> Vec<String> {
    let rule = deny;
    let mut rules = Vec::new();
    if routine.bot.is_some() || routine.read_only {
        rules.push(rule(&routine.dir));
    }
    if routine.read_only {
        rules.extend(
            ws.repos
                .iter()
                .filter(|r| !run_dir.starts_with(r))
                .map(|r| rule(r)),
        );
    }
    rules
}

/// Blocking. An agent's output file: a regular file, not a symlink that
/// could point the host at another file, of at most `max` bytes.
pub fn read_output(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("no {name}: {e}"))?;
    if !meta.file_type().is_file() {
        return Err(format!("{name} must be a regular file, not a symlink"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(max + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("cannot read {name}: {e}"))?;
    if bytes.len() as u64 > max {
        return Err(format!("{name} is larger than {} KiB", max / 1024));
    }
    Ok(bytes)
}

/// Pure. The rule that denies Claude Code's edit tools under `path`.
pub fn deny(path: &Path) -> String {
    format!("Edit(/{}/**)", path.to_string_lossy().trim_end_matches('/'))
}

/// Blocking. Writes the settings into the run's `.muxa/` and adds the flag
/// that loads them; the command as it was when there is nothing to set.
/// `dirs` are folders the agent may read besides its own.
pub fn with_settings(
    command: LaunchCommand,
    dir: &Path,
    rules: &[String],
    dirs: &[std::path::PathBuf],
) -> Result<LaunchCommand, String> {
    with_permissions(command, dir, rules, &[], dirs)
}

/// As [`with_settings`], with `allow` rules too: a Bot's Workspace rules.
pub fn with_permissions(
    command: LaunchCommand,
    dir: &Path,
    rules: &[String],
    allow: &[String],
    dirs: &[std::path::PathBuf],
) -> Result<LaunchCommand, String> {
    if rules.is_empty() && allow.is_empty() && dirs.is_empty() {
        return Ok(command);
    }
    let dirs: Vec<String> = dirs
        .iter()
        .map(|d| d.to_string_lossy().into_owned())
        .collect();
    let value = serde_json::json!({"permissions": {"allow": allow, "deny": rules, "additionalDirectories": dirs}});
    let mut text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    text.push('\n');
    let path = git::write_file(dir, SETTINGS_FILE, &text)?;
    let path = path.to_string_lossy().into_owned();
    Ok(match command {
        LaunchCommand::Shell(line) => LaunchCommand::Shell(format!(
            "{line} {SETTINGS_FLAG} '{}'",
            path.replace('\'', "'\\''")
        )),
        LaunchCommand::Native { program, mut args } => {
            args.extend([SETTINGS_FLAG.to_owned(), path]);
            LaunchCommand::Native { program, args }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn routine(bot: Option<&str>, read_only: bool) -> Routine {
        Routine {
            workspace: "w".into(),
            name: "R".into(),
            trigger: None,
            worker: None,
            instructions: "/ws/.muxa/r.md".into(),
            dir: "/ws/.muxa".into(),
            items: 3,
            direct_writes: false,
            project: None,
            enabled: true,
            bot: bot.map(str::to_owned),
            read_only,
            on: None,
        }
    }

    fn workspace() -> Workspace {
        Workspace {
            name: "w".into(),
            root: "/ws".into(),
            repos: vec![PathBuf::from("/code/app"), PathBuf::from("/code/lib/")],
            missing_repos: Vec::new(),
            hidden_repos: Vec::new(),
            agent: None,
            loop_checks: None,
            park_after: None,
            routines: None,
            connections: Vec::new(),
            mcp: Vec::new(),
            claude_connectors: false,
            bot_rules: Default::default(),
        }
    }

    #[test]
    fn a_plain_run_gets_no_rules() {
        assert!(
            deny_rules(
                &workspace(),
                &routine(None, false),
                Path::new("/ws/routines/r-1")
            )
            .is_empty()
        );
    }

    #[test]
    fn a_bot_run_may_not_edit_the_muxa_folder() {
        assert_eq!(
            deny_rules(
                &workspace(),
                &routine(Some("b"), false),
                Path::new("/ws/routines/r-1")
            ),
            ["Edit(//ws/.muxa/**)"]
        );
    }

    #[test]
    fn a_read_only_run_may_edit_no_repo() {
        assert_eq!(
            deny_rules(
                &workspace(),
                &routine(None, true),
                Path::new("/ws/routines/r-1")
            ),
            [
                "Edit(//ws/.muxa/**)",
                "Edit(//code/app/**)",
                "Edit(//code/lib/**)"
            ]
        );
    }

    #[test]
    fn a_repo_that_holds_the_run_stays_writable() {
        let mut ws = workspace();
        ws.repos.push(PathBuf::from("/ws"));
        let rules = deny_rules(&ws, &routine(None, true), Path::new("/ws/routines/r-1"));
        assert!(!rules.contains(&"Edit(//ws/**)".to_owned()), "{rules:?}");
        assert!(rules.contains(&"Edit(//code/app/**)".to_owned()));
    }

    #[test]
    fn the_settings_file_is_written_and_loaded_by_flag() {
        let dir = std::env::temp_dir().join(format!("muxa-bot-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let rules = vec!["Edit(//ws/.muxa/**)".to_owned()];
        let native = LaunchCommand::Native {
            program: "claude",
            args: vec![],
        };
        let LaunchCommand::Native { args, .. } = with_settings(native, &dir, &rules, &[]).unwrap()
        else {
            panic!("native stays native");
        };
        assert_eq!(args[0], SETTINGS_FLAG);
        let text = std::fs::read_to_string(&args[1]).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["permissions"]["deny"][0], "Edit(//ws/.muxa/**)");
        let shell =
            with_settings(LaunchCommand::Shell("claude".into()), &dir, &rules, &[]).unwrap();
        let LaunchCommand::Shell(line) = shell else {
            panic!("shell stays shell");
        };
        assert!(line.starts_with("claude --settings '"), "{line}");
        let same = with_settings(LaunchCommand::Shell("claude".into()), &dir, &[], &[]).unwrap();
        assert!(matches!(same, LaunchCommand::Shell(l) if l == "claude"));
    }
}
