//! Creates and closes sessions for tasks. A new session is detached and has its
//! own window, so no existing window or pane changes size (ADR 0007).

use crate::client::Client;
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
    line.push(' ');
    line.push_str(&quote(command));
    Ok(line)
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
    let line = reply
        .lines
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

/// The most recently attached session other than `except`.
pub async fn last_other_session(
    client: &Client,
    except: SessionId,
) -> Result<Option<SessionId>, Error> {
    let reply = client
        .command(&format!(
            "list-sessions -F {}",
            quote("#{session_last_attached} #{session_id}")
        ))
        .await?;
    Ok(most_recent(&lines(&reply.lines), except))
}

pub(crate) fn most_recent(rows: &[String], except: SessionId) -> Option<SessionId> {
    rows.iter()
        .filter_map(|row| {
            let (at, id) = row.split_once(' ')?;
            Some((
                at.parse::<u64>().unwrap_or(0),
                id.parse::<SessionId>().ok()?,
            ))
        })
        .filter(|(_, id)| *id != except)
        .max_by_key(|(at, _)| *at)
        .map(|(_, id)| id)
}

/// Closes a session and every pane in it.
pub async fn kill_session(client: &Client, session: SessionId) -> Result<(), Error> {
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
            "100 $1".to_owned(),
            "300 $2".into(),
            "200 $3".into(),
            " $4".into(),
        ];
        assert_eq!(
            most_recent(&rows, SessionId::new(2)),
            Some(SessionId::new(3))
        );
        assert_eq!(most_recent(&rows[1..2], SessionId::new(2)), None);
    }
}
