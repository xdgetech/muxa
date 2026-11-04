//! A reading Task's folder (ADR 0034 §1): Claude Code's local settings that
//! let the agent read the main checkout and refuse its file edits there.
//! Other agents get only `TASK.md`; nothing here enforces read-only for them.
use std::path::{Path, PathBuf};

use crate::records::workers::LaunchCommand;

/// The Claude Code program, as a worker names it and `--task-command` defaults to.
const CLAUDE: &str = "claude";
const DIR: &str = ".claude";
const FILE: &str = "settings.local.json";

/// Whether `command` starts Claude Code: its program, after leading `VAR=value`
/// words and `env` or `exec`, is `claude`. A wrapper script is not recognised.
pub fn is_claude_code(command: &LaunchCommand) -> bool {
    match command {
        LaunchCommand::Native { program, .. } => *program == CLAUDE,
        LaunchCommand::Shell(line) => shell_program(line)
            .is_some_and(|p| Path::new(p).file_name().and_then(|n| n.to_str()) == Some(CLAUDE)),
    }
}

fn shell_program(line: &str) -> Option<&str> {
    line.split_whitespace()
        .find(|w| !w.contains('=') && *w != "env" && *w != "exec")
}

/// The settings text. Paths in permission rules start with `//` to be
/// absolute; one `/` would anchor at the session's directory. Claude Code
/// matches `Edit` rules for every built-in tool that edits a file, `Write`
/// among them; a `Write(path)` rule never matches (code.claude.com permissions).
pub fn settings(main: &Path) -> String {
    let main = main.to_string_lossy();
    let rule = format!("Edit(/{}/**)", main.trim_end_matches('/'));
    let value = serde_json::json!({
        "permissions": {
            "additionalDirectories": [main],
            "deny": [rule],
        }
    });
    let mut text = serde_json::to_string_pretty(&value).unwrap_or_default();
    text.push('\n');
    text
}

/// Writes `<folder>/.claude/settings.local.json` unless one is there: the
/// user may have edited it. Blocking.
pub fn write_settings(folder: &Path, main: &Path) -> Result<Option<PathBuf>, String> {
    let dir = folder.join(DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let path = dir.join(FILE);
    if path.symlink_metadata().is_ok() {
        return Ok(None);
    }
    std::fs::write(&path, settings(main))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_is_its_program_after_env_words() {
        let shell = |s: &str| is_claude_code(&LaunchCommand::Shell(s.into()));
        assert!(shell("claude"));
        assert!(shell("claude --model opus"));
        assert!(shell("CLAUDE_CONFIG_DIR=/h/.c claude"));
        assert!(shell("env A=1 exec /usr/local/bin/claude"));
        assert!(!shell("codex"));
        assert!(!shell("my-claude-wrapper"));
        assert!(!shell("sh -c claude"));
        assert!(is_claude_code(&LaunchCommand::Native {
            program: "claude",
            args: Vec::new()
        }));
        assert!(!is_claude_code(&LaunchCommand::Native {
            program: "codex",
            args: Vec::new()
        }));
    }

    #[test]
    fn settings_add_the_checkout_and_deny_edits_by_absolute_path() {
        let v: serde_json::Value =
            serde_json::from_str(&settings(Path::new("/w/my repo"))).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"permissions": {
                "additionalDirectories": ["/w/my repo"],
                "deny": ["Edit(//w/my repo/**)"],
            }})
        );
    }

    #[test]
    fn an_existing_settings_file_is_kept() {
        let dir = std::env::temp_dir().join(format!("muxa-reading-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = write_settings(&dir, Path::new("/w/a")).unwrap();
        assert!(first.is_some());
        let path = dir.join(DIR).join(FILE);
        std::fs::write(&path, "{}").unwrap();
        assert_eq!(write_settings(&dir, Path::new("/w/b")).unwrap(), None);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
