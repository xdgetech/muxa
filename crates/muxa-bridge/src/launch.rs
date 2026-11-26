//! What an agent's launch command is: which program it starts, and Claude
//! Code's plan mode (ADR 0060). Pure.

use std::path::Path;

use crate::records::workers::LaunchCommand;

/// The Claude Code program, as a worker names it and `--task-command` defaults to.
const CLAUDE: &str = "claude";
const CODEX: &str = "codex";
pub const PLAN_ARGS: [&str; 2] = ["--permission-mode", "plan"];

/// Whether `command` starts Claude Code: its program, after leading `VAR=value`
/// words and `env` or `exec`, is `claude`. A wrapper script is not recognised.
pub fn is_claude_code(command: &LaunchCommand) -> bool {
    program(command) == Some(CLAUDE)
}

/// The file name of the program `command` starts, read as [`is_claude_code`] does.
pub fn program(command: &LaunchCommand) -> Option<&str> {
    match command {
        LaunchCommand::Native { program, .. } => Some(program),
        LaunchCommand::Shell(line) => {
            shell_program(line).and_then(|p| Path::new(p).file_name().and_then(|n| n.to_str()))
        }
    }
}

fn shell_program(line: &str) -> Option<&str> {
    line.split_whitespace()
        .find(|w| !w.contains('=') && *w != "env" && *w != "exec")
}

/// `line` with the plan flag when its program is Claude Code; any other
/// line as it is.
pub fn plan_line(line: &str) -> String {
    if is_claude_code(&LaunchCommand::Shell(line.to_owned())) {
        format!("{line} {}", PLAN_ARGS.join(" "))
    } else {
        line.to_owned()
    }
}

/// `line` with `prompt` as the agent's first message, for Claude Code and
/// Codex (ADR 0098). Any other program may not take a prompt argument, so its
/// line stays as it is.
pub fn prompt_line(line: &str, prompt: &str) -> String {
    let quoted = format!("'{}'", prompt.replace('\'', "'\\''"));
    match program(&LaunchCommand::Shell(line.to_owned())) {
        // `--` ends the options: a list flag such as `--mcp-config` would
        // take the prompt as one more item (docs/findings/claude-code-input.md).
        Some(CLAUDE) => format!("{line} -- {quoted}"),
        Some(CODEX) => format!("{line} {quoted}"),
        _ => line.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_claude_code_gets_the_plan_flag() {
        assert_eq!(plan_line("claude"), "claude --permission-mode plan");
        assert_eq!(
            plan_line("FOO=1 env /usr/bin/claude"),
            "FOO=1 env /usr/bin/claude --permission-mode plan"
        );
        assert_eq!(plan_line("codex"), "codex");
        assert!(is_claude_code(&LaunchCommand::Native {
            program: "claude",
            args: vec![]
        }));
    }

    #[test]
    fn only_claude_code_and_codex_get_the_prompt() {
        assert_eq!(
            prompt_line("claude --mcp-config '/a b'", "Read it's brief."),
            "claude --mcp-config '/a b' -- 'Read it'\\''s brief.'"
        );
        assert_eq!(prompt_line("codex -c x=1", "Go."), "codex -c x=1 'Go.'");
        assert_eq!(prompt_line("my-agent", "Go."), "my-agent");
    }
}
