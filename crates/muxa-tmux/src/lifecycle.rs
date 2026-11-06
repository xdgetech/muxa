//! Creates and closes sessions for tasks. A new session is detached and has its
//! own window, so no existing window or pane changes size (ADR 0007).

use crate::client::Client;
use crate::device::{DEVICE_TAG, devices_of};
use crate::escape::quote;
use crate::{Error, PaneId, SessionId};

/// The names of every session.
pub async fn session_names(client: &Client) -> Result<Vec<String>, Error> {
    let reply = client
        .command(&format!("list-sessions -F {}", quote("#{session_name}")))
        .await?;
    Ok(lines(&reply.lines))
}

fn lines(raw: &[Vec<u8>]) -> Vec<String> {
    raw.iter()
        .map(|l| String::from_utf8_lossy(l).into_owned())
        .collect()
}

pub(crate) fn new_session_line(
    name: &str,
    dir: &str,
    env: &[(String, String)],
    command: &str,
) -> Result<String, Error> {
    let mut line = format!(
        "new-session -d -P -F {} -s {} -c {}",
        quote("#{session_id} #{pane_id}"),
        quote(name),
        quote(dir),
    );
    push_env(&mut line, env)?;
    line.push(' ');
    line.push_str(&quote(command));
    Ok(line)
}

fn new_session_command_line(
    name: &str,
    dir: &str,
    env: &[(String, String)],
    command: &[String],
) -> Result<String, Error> {
    let Some(program) = command.first() else {
        return Err(Error::Unexpected(
            "new-session command must not be empty".into(),
        ));
    };
    if program.trim().is_empty() || command.iter().any(|arg| arg.contains('\0')) {
        return Err(Error::Unexpected(
            "new-session command is empty or has a NUL byte".into(),
        ));
    }
    let mut line = format!(
        "new-session -d -P -F {} -s {} -c {}",
        quote("#{session_id} #{pane_id}"),
        quote(name),
        quote(dir),
    );
    push_env(&mut line, env)?;
    line.push_str(" --");
    for argument in command {
        line.push(' ');
        line.push_str(&quote(argument));
    }
    Ok(line)
}

/// Starts a detached session with an explicit shell command or direct argv.
/// One entry runs through the shell; multiple entries are executed directly.
/// See `scripts/repro/worker-respawn.sh` for the byte-preservation probe.
pub async fn new_session_command(
    client: &Client,
    name: &str,
    dir: &str,
    env: &[(String, String)],
    command: &[String],
) -> Result<(SessionId, PaneId), Error> {
    let reply = client
        .command(&new_session_command_line(name, dir, env, command)?)
        .await?;
    session_and_pane(&reply.lines)
}

/// Appends ` -e "KEY=VALUE"` for each pair.
fn push_env(line: &mut String, env: &[(String, String)]) -> Result<(), Error> {
    for (key, value) in env {
        if !env_key(key) {
            // The key only: a value may be a secret.
            return Err(Error::Unexpected(format!(
                "{key:?} is not an environment variable name"
            )));
        }
        line.push_str(" -e ");
        line.push_str(&quote(&format!("{key}={value}")));
    }
    Ok(())
}

pub(crate) fn respawn_pane_line(
    pane: PaneId,
    dir: &str,
    env: &[(String, String)],
) -> Result<String, Error> {
    let mut line = format!("respawn-pane -k -t {pane} -c {}", quote(dir));
    push_env(&mut line, env)?;
    Ok(line)
}

fn respawn_pane_command_line(
    pane: PaneId,
    dir: &str,
    env: &[(String, String)],
    command: &[String],
) -> Result<String, Error> {
    let Some(program) = command.first() else {
        return Err(Error::Unexpected(
            "respawn command must not be empty".into(),
        ));
    };
    if program.trim().is_empty() || command.iter().any(|arg| arg.contains('\0')) {
        return Err(Error::Unexpected(
            "respawn command is empty or has a NUL byte".into(),
        ));
    }
    let mut line = respawn_pane_line(pane, dir, env)?;
    line.push_str(" --");
    for argument in command {
        line.push(' ');
        line.push_str(&quote(argument));
    }
    Ok(line)
}

/// Replaces a pane with an explicit command without selecting or resizing it.
/// One string is a shell command; two or more strings are argv executed directly
/// by tmux. Empty/NUL commands fail before dispatch. All values travel quoted on
/// the control client's stdin, including environment values. A later no-command
/// [`respawn_pane`] repeats this selected command (`scripts/repro/worker-respawn.sh`).
/// Native argv behavior is locally verified on tmux 3.7c; supported CI versions
/// must pass the same probe before worker execution is enabled.
pub async fn respawn_pane_command(
    client: &Client,
    pane: PaneId,
    dir: &str,
    env: &[(String, String)],
    command: &[String],
) -> Result<(), Error> {
    client
        .command(&respawn_pane_command_line(pane, dir, env, command)?)
        .await
        .map(drop)
}

/// Replaces the process of `pane` in place (`respawn-pane -k`): the pane
/// keeps its id, its size and its place in the layout, and nothing is
/// selected (`scripts/repro/respawn-pane.sh`). With no command given, tmux
/// reruns the pane's start command, or starts a shell when it had none.
/// `env` travels on the control client's stdin, as for [`new_session`].
pub async fn respawn_pane(
    client: &Client,
    pane: PaneId,
    dir: &str,
    env: &[(String, String)],
) -> Result<(), Error> {
    client
        .command(&respawn_pane_line(pane, dir, env)?)
        .await
        .map(drop)
}

/// The command `pane` was started with; empty when it started the default shell.
pub async fn pane_start_command(client: &Client, pane: PaneId) -> Result<String, Error> {
    let reply = client
        .command(&format!(
            "display-message -p -t {pane} {}",
            quote("#{pane_start_command}")
        ))
        .await?;
    Ok(lines(&reply.lines).concat())
}

/// `[A-Za-z_][A-Za-z0-9_]*`: no `=` that would move the split into the value.
fn env_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Starts a detached session `name` in `dir` running `command` (through the
/// shell), and returns it and its pane. The attached clients stay where they are.
/// `env` goes to the session's environment (`-e`, tmux 3.2), so the command and
/// later windows of the session see it. It travels on the control client's
/// stdin, never on a command line that `ps` shows.
pub async fn new_session(
    client: &Client,
    name: &str,
    dir: &str,
    env: &[(String, String)],
    command: &str,
) -> Result<(SessionId, PaneId), Error> {
    let reply = client
        .command(&new_session_line(name, dir, env, command)?)
        .await?;
    session_and_pane(&reply.lines)
}

fn session_and_pane(lines: &[Vec<u8>]) -> Result<(SessionId, PaneId), Error> {
    let line = lines
        .first()
        .map(|l| String::from_utf8_lossy(l).into_owned())
        .ok_or_else(|| Error::Unexpected("new-session printed no pane".into()))?;
    let (session, pane) = line
        .trim()
        .split_once(' ')
        .ok_or_else(|| Error::Unexpected(format!("new-session printed {line:?}")))?;
    Ok((session.parse()?, pane.parse()?))
}

fn user_option(name: &str) -> Result<(), Error> {
    if name.starts_with('@') {
        Ok(())
    } else {
        Err(Error::Unexpected(format!("{name:?} is not a user option")))
    }
}

/// Sets a user option (`@name`) on one pane.
pub async fn set_pane_option(
    client: &Client,
    pane: PaneId,
    name: &str,
    value: &str,
) -> Result<(), Error> {
    user_option(name)?;
    client
        .command(&format!(
            "set-option -p -t {pane} {} {}",
            quote(name),
            quote(value)
        ))
        .await
        .map(drop)
}

/// Sets a user option (`@name`) on one session.
pub async fn set_session_option(
    client: &Client,
    session: SessionId,
    name: &str,
    value: &str,
) -> Result<(), Error> {
    user_option(name)?;
    client
        .command(&format!(
            "set-option -t {session} {} {}",
            quote(name),
            quote(value)
        ))
        .await
        .map(drop)
}

/// When `pane`'s process exits, the pane gets a login shell in `dir` instead
/// of closing, so its session stays. It holds once: the shell's own exit closes
/// the pane. A `respawn-pane -k` is no exit (`scripts/repro/shell-after-exit.sh`).
pub async fn shell_after_exit(client: &Client, pane: PaneId, dir: &str) -> Result<(), Error> {
    let shell = global_option(client, "default-shell")
        .await?
        .unwrap_or_else(|| DEFAULT_SHELL.to_owned());
    for line in shell_after_exit_lines(pane, dir, &shell) {
        client.command(&line).await?;
    }
    Ok(())
}

const DEFAULT_SHELL: &str = "/bin/sh";

/// The hook first: an exit after `remain-on-exit` but before it would leave a dead pane.
fn shell_after_exit_lines(pane: PaneId, dir: &str, shell: &str) -> [String; 2] {
    let once = format!(
        "set-hook -pu -t {pane} pane-died ; set-option -pu -t {pane} remain-on-exit ; respawn-pane -k -t {pane} -c {} -- {} -l",
        quote(dir),
        quote(shell)
    );
    [
        format!("set-hook -p -t {pane} pane-died {}", quote(&once)),
        format!("set-option -p -t {pane} remain-on-exit on"),
    ]
}

/// A global option's value; None when it is unset or empty.
pub async fn global_option(client: &Client, name: &str) -> Result<Option<String>, Error> {
    let reply = client
        .command(&format!("show-options -gqv {}", quote(name)))
        .await?;
    Ok(reply
        .lines
        .first()
        .map(|l| String::from_utf8_lossy(l).into_owned())
        .filter(|v| !v.is_empty()))
}

/// The clients attached to `session`, control clients included.
pub async fn clients_of(client: &Client, session: SessionId) -> Result<Vec<String>, Error> {
    let reply = client
        .command(&format!(
            "list-clients -t {session} -F {}",
            quote("#{client_name}")
        ))
        .await?;
    Ok(lines(&reply.lines))
}

/// Moves one attached client to `session`.
pub async fn switch_client(client: &Client, name: &str, session: SessionId) -> Result<(), Error> {
    client
        .command(&format!("switch-client -c {} -t {session}", quote(name)))
        .await
        .map(drop)
}

/// The most recently attached session other than `except`. Device sessions
/// (ADR 0021) do not count.
pub async fn last_other_session(
    client: &Client,
    except: SessionId,
) -> Result<Option<SessionId>, Error> {
    let reply = client
        .command(&format!(
            "list-sessions -F {}",
            quote(&format!(
                "#{{session_last_attached}} #{{session_id}} #{{{DEVICE_TAG}}}"
            ))
        ))
        .await?;
    Ok(most_recent(&lines(&reply.lines), except))
}

pub(crate) fn most_recent(rows: &[String], except: SessionId) -> Option<SessionId> {
    rows.iter()
        .filter_map(|row| {
            let mut fields = row.split(' ');
            let (at, id) = (fields.next()?, fields.next()?);
            if fields.next().is_some_and(|device| !device.is_empty()) {
                return None;
            }
            Some((
                at.parse::<u64>().unwrap_or(0),
                id.parse::<SessionId>().ok()?,
            ))
        })
        .filter(|(_, id)| *id != except)
        .max_by_key(|(at, _)| *at)
        .map(|(_, id)| id)
}

/// Closes a session and every pane in it. Its device sessions go first: they
/// would keep its windows open (ADR 0021).
pub async fn kill_session(client: &Client, session: SessionId) -> Result<(), Error> {
    for device in devices_of(client, session).await? {
        client.command(&format!("kill-session -t {device}")).await?;
    }
    client
        .command(&format!("kill-session -t {session}"))
        .await
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_value_is_quoted() {
        assert_eq!(
            new_session_line("feat-x", "/w/a \"b\"", &[], "claude --model $M").unwrap(),
            r##"new-session -d -P -F "#{session_id} #{pane_id}" -s "feat-x" -c "/w/a \"b\"" "claude --model \$M""##
        );
        let env = [
            ("CLAUDE_CONFIG_DIR".to_owned(), "/h/.claude-a".to_owned()),
            ("TOKEN".to_owned(), "a\"b $c".to_owned()),
        ];
        assert_eq!(
            new_session_line("t", "/d", &env, "claude").unwrap(),
            r##"new-session -d -P -F "#{session_id} #{pane_id}" -s "t" -c "/d" -e "CLAUDE_CONFIG_DIR=/h/.claude-a" -e "TOKEN=a\"b \$c" "claude""##
        );
    }

    #[test]
    fn the_shell_after_exit_hook_quotes_the_directory_and_shell() {
        let [hook, option] = shell_after_exit_lines(PaneId::new(3), "/w/a \"b\" $x", "/bin/zsh");
        assert_eq!(
            hook,
            r##"set-hook -p -t %3 pane-died "set-hook -pu -t %3 pane-died ; set-option -pu -t %3 remain-on-exit ; respawn-pane -k -t %3 -c \"/w/a \\\"b\\\" \\\$x\" -- \"/bin/zsh\" -l""##
        );
        assert_eq!(option, "set-option -p -t %3 remain-on-exit on");
    }

    #[test]
    fn a_respawn_names_the_pane_by_id_and_quotes_every_value() {
        let env = [("TOKEN".to_owned(), "a\"b $c".to_owned())];
        assert_eq!(
            respawn_pane_line(PaneId::new(7), "/w/a b", &env).unwrap(),
            r##"respawn-pane -k -t %7 -c "/w/a b" -e "TOKEN=a\"b \$c""##
        );
        let bad = [("A=B".to_owned(), "secret".to_owned())];
        assert!(respawn_pane_line(PaneId::new(7), "/d", &bad).is_err());
    }

    #[test]
    fn an_explicit_respawn_quotes_shell_commands_and_native_argv() {
        let env = [("TOKEN".to_owned(), "a\"b $c".to_owned())];
        assert_eq!(
            respawn_pane_command_line(
                PaneId::new(7),
                "/w/a b",
                &env,
                &["exec worker --profile '$x'".into()]
            )
            .unwrap(),
            r##"respawn-pane -k -t %7 -c "/w/a b" -e "TOKEN=a\"b \$c" -- "exec worker --profile '\$x'""##
        );
        assert_eq!(
            respawn_pane_command_line(
                PaneId::new(7),
                "/w/a b",
                &[],
                &["/bin/worker".into(), "a\"b $c".into(), "日本語".into()]
            )
            .unwrap(),
            r##"respawn-pane -k -t %7 -c "/w/a b" -- "/bin/worker" "a\"b \$c" "日本語""##
        );
        for command in [
            vec![],
            vec![String::new()],
            vec!["   ".into()],
            vec!["worker".into(), "synthetic\0secret".into()],
        ] {
            let error = respawn_pane_command_line(PaneId::new(7), "/d", &[], &command)
                .unwrap_err()
                .to_string();
            assert!(!error.contains("synthetic"));
        }
        assert!(
            respawn_pane_command_line(
                PaneId::new(7),
                "/d",
                &[("A=B".into(), "secret".into())],
                &["worker".into()]
            )
            .is_err()
        );
    }

    #[test]
    fn an_env_key_with_equals_is_refused_without_its_value() {
        let env = [("A=B".to_owned(), "secret".to_owned())];
        let err = new_session_line("t", "/d", &env, "claude")
            .unwrap_err()
            .to_string();
        assert!(err.contains("A=B") && !err.contains("secret"), "{err}");
    }

    #[test]
    fn the_most_recent_other_session_is_picked() {
        let rows = [
            "100 $1 ".to_owned(),
            "300 $2 ".into(),
            "200 $3 ".into(),
            " $4 ".into(),
            "400 $5 1".into(),
        ];
        assert_eq!(
            most_recent(&rows, SessionId::new(2)),
            Some(SessionId::new(3))
        );
        assert_eq!(most_recent(&rows[1..2], SessionId::new(2)), None);
    }
}
