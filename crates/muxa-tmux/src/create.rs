//! Creates sessions for new tasks. A new session is detached and has its own
//! window, so no existing window or pane changes size (ADR 0007).

use crate::client::Client;
use crate::escape::quote;
use crate::{Error, PaneId};

/// The names of every session.
pub async fn session_names(client: &Client) -> Result<Vec<String>, Error> {
    let reply = client
        .command(&format!("list-sessions -F {}", quote("#{session_name}")))
        .await?;
    Ok(reply
        .lines
        .iter()
        .map(|l| String::from_utf8_lossy(l).into_owned())
        .collect())
}

pub(crate) fn new_session_line(name: &str, dir: &str, command: &str) -> String {
    format!(
        "new-session -d -P -F {} -s {} -c {} {}",
        quote("#{pane_id}"),
        quote(name),
        quote(dir),
        quote(command)
    )
}

/// Starts a detached session `name` in `dir` running `command` (through the
/// shell), and returns its pane. The attached clients stay where they are.
pub async fn new_session(
    client: &Client,
    name: &str,
    dir: &str,
    command: &str,
) -> Result<PaneId, Error> {
    let reply = client
        .command(&new_session_line(name, dir, command))
        .await?;
    let line = reply
        .lines
        .first()
        .ok_or_else(|| Error::Unexpected("new-session printed no pane".into()))?;
    String::from_utf8_lossy(line).trim().parse()
}

/// Sets a user option (`@name`) on one pane.
pub async fn set_pane_option(
    client: &Client,
    pane: PaneId,
    name: &str,
    value: &str,
) -> Result<(), Error> {
    if !name.starts_with('@') {
        return Err(Error::Unexpected(format!("{name:?} is not a user option")));
    }
    client
        .command(&format!(
            "set-option -p -t {pane} {} {}",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_value_is_quoted() {
        assert_eq!(
            new_session_line("feat-x", "/w/a \"b\"", "claude --model $M"),
            r##"new-session -d -P -F "#{pane_id}" -s "feat-x" -c "/w/a \"b\"" "claude --model \$M""##
        );
    }
}
