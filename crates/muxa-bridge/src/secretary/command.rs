//! The agent command that continues the secretary's Claude Code conversation:
//! the workspace's command plus `--resume <id>` (docs/findings/claude-code-resume.md).

/// The program whose conversations muxa resumes.
const CLAUDE: &str = "claude";
/// Flags that already pick a conversation: muxa adds none then.
const PICKS_CONVERSATION: [&str; 6] = [
    "--resume",
    "-r",
    "--continue",
    "-c",
    "--session-id",
    "--fork-session",
];
/// Bytes after which the shell would run another command or redirect.
const SHELL_SYNTAX: &[char] = &[';', '&', '|', '<', '>', '`', '$', '(', ')', '\n'];

/// `command --resume <id>` when `command` starts Claude Code and `id` is a
/// Claude Code session id. None when it does not, so the agent starts fresh.
pub fn resume_command(command: &str, id: &str) -> Option<String> {
    if !valid_id(id) || command.contains(SHELL_SYNTAX) {
        return None;
    }
    let mut words = command.split_whitespace().skip_while(|w| assignment(w));
    let first = words.next()?;
    let program = if first == "env" {
        words.find(|w| !assignment(w))?
    } else {
        first
    };
    if program.rsplit('/').next() != Some(CLAUDE) {
        return None;
    }
    if command
        .split_whitespace()
        .any(|w| PICKS_CONVERSATION.contains(&w.split('=').next().unwrap_or(w)))
    {
        return None;
    }
    Some(format!("{} --resume {id}", command.trim_end()))
}

/// Claude Code's own rule for a session file's id (`agents/claude.rs`): it
/// lands on a shell command line, so nothing else passes.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// `KEY=value` before the program.
fn assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(key, _)| {
        !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "11111111-2222-4333-8444-555555555555";

    #[test]
    fn claude_gets_the_resume_flag_after_its_own_arguments() {
        assert_eq!(
            resume_command("claude", ID).as_deref(),
            Some("claude --resume 11111111-2222-4333-8444-555555555555")
        );
        assert_eq!(
            resume_command("/opt/bin/claude --model opus ", ID).as_deref(),
            Some("/opt/bin/claude --model opus --resume 11111111-2222-4333-8444-555555555555")
        );
        assert_eq!(
            resume_command("env FOO=1 claude", ID).as_deref(),
            Some("env FOO=1 claude --resume 11111111-2222-4333-8444-555555555555")
        );
        assert_eq!(
            resume_command("FOO=1 claude", ID).as_deref(),
            Some("FOO=1 claude --resume 11111111-2222-4333-8444-555555555555")
        );
    }

    #[test]
    fn other_agents_and_commands_start_fresh() {
        for command in [
            "codex",
            "cat",
            "claude-wrapper",
            "env FOO=1 codex",
            "",
            "claude --continue",
            "claude --resume=abc",
            "claude -r abc",
            "claude --session-id abc",
            "claude; rm -rf x",
            "claude && echo",
            "claude $(echo)",
        ] {
            assert_eq!(resume_command(command, ID), None, "{command:?}");
        }
    }

    #[test]
    fn an_id_that_is_not_claude_codes_is_never_used() {
        for id in ["", "abc; rm", "../x", "a b", &"a".repeat(65)] {
            assert_eq!(resume_command("claude", id), None, "{id:?}");
        }
    }
}
