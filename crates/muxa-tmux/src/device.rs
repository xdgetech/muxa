//! Device sessions (ADR 0021): a session of its own per watched window, which
//! holds only that window (`link-window`), where a second control client sizes
//! it for the device. No user session's current window changes.
//! Facts: `scripts/repro/device-session*.sh`.

use crate::client::{Client, Flags, Options};
use crate::escape::quote;
use crate::sizing::{self, Grid};
use crate::{Error, SessionId, WindowId};

/// The session option that marks a device session. Session lists skip it.
pub const DEVICE_TAG: &str = "@muxa_device";
/// The pid of the bridge that owns a device session.
pub const OWNER_TAG: &str = "@muxa_device_pid";
const SEP: char = '\u{1f}';
/// The command of the session's first window, which is killed at once.
const PLACEHOLDER: &str = "cat";

/// One device session and the control client that sizes its window. End it
/// with [`Device::close`]. Dropped without it, the session stays until a
/// bridge removes it ([`unattached_devices`]). tmux's `destroy-unattached`
/// is not used: it crashed tmux 3.7c (`docs/findings/tmux-control-mode.md`).
#[derive(Debug)]
pub struct Device {
    session: SessionId,
    target: SessionId,
    window: WindowId,
    client: Client,
}

impl Device {
    /// Makes a device session that holds `window` of `target`, and attaches
    /// a client to it that sizes nothing yet. `owner` is this process's pid;
    /// `n` makes the name unique within it.
    pub async fn open(
        main: &Client,
        opts: &Options,
        target: SessionId,
        window: WindowId,
        owner: u32,
        n: u32,
    ) -> Result<Device, Error> {
        let reply = main.command(&create_line(owner, n)).await?;
        let line = reply
            .lines
            .first()
            .map(|l| String::from_utf8_lossy(l).trim().to_owned())
            .unwrap_or_default();
        let (session, first) = line
            .split_once(' ')
            .ok_or_else(|| Error::Unexpected(format!("new-session printed {line:?}")))?;
        let (session, first): (SessionId, WindowId) = (session.parse()?, first.parse()?);
        match attach(main, opts, session, window, first, owner).await {
            Ok(client) => Ok(Device {
                session,
                target,
                window,
                client,
            }),
            Err(e) => {
                if let Err(kill) = main.command(&format!("kill-session -t {session}")).await {
                    tracing::warn!(%session, error = %kill, "removing a device session failed");
                }
                Err(e)
            }
        }
    }

    pub fn session(&self) -> SessionId {
        self.session
    }

    /// The user's session that holds the window.
    pub fn target(&self) -> SessionId {
        self.target
    }

    pub fn window(&self) -> WindowId {
        self.window
    }

    /// The device client has ended, e.g. its session was killed.
    pub fn exited(&self) -> bool {
        self.client.exited()
    }

    /// Makes the device client the window's latest client at `size`. Then
    /// `main` switches to the target again, so a command with no target
    /// (`tmux attach` on the laptop) still picks the target, not this session.
    pub async fn take(&self, main: &Client, size: Grid) -> Result<(), Error> {
        sizing::take_size(&self.client, self.session, size).await?;
        main.command(&format!("switch-client -E -t {}", self.target))
            .await
            .map(drop)
    }

    /// A new size for a device client that already holds one.
    pub async fn resize(&self, size: Grid) -> Result<(), Error> {
        sizing::resize_client(&self.client, size).await
    }

    /// Gives the window back to the other clients (`held`: the size the
    /// client holds), then kills the session with the client still attached,
    /// which detaches it. A session that a person joined (`choose-tree`)
    /// stays; a bridge removes it once it is unattached.
    pub async fn close(self, main: &Client, held: Option<Grid>) -> Result<(), Error> {
        let released = match held.filter(|_| !self.exited()) {
            Some(size) => sizing::release_size(&self.client, size).await,
            None => Ok(()),
        };
        let clients = main
            .command(&format!(
                "list-clients -t {} -F {}",
                self.session,
                quote("#{client_name}")
            ))
            .await;
        let killed = match clients {
            Ok(reply) if reply.lines.len() <= 1 => main
                .command(&format!("kill-session -t {}", self.session))
                .await
                .map(drop),
            Ok(_) => Ok(()),
            // Already gone, e.g. closing the task killed it.
            Err(Error::Tmux { .. }) => Ok(()),
            Err(e) => Err(e),
        };
        released.and(killed)
    }
}

pub(crate) fn create_line(owner: u32, n: u32) -> String {
    format!(
        "new-session -d -P -F {} -s {} {PLACEHOLDER}",
        quote("#{session_id} #{window_id}"),
        quote(&format!("_muxa-{owner}-{n}"))
    )
}

/// Tags the session first, so a crash leaves a session a bridge can find.
/// `detach-on-destroy on`: when the session ends, its sized client detaches
/// instead of moving to a user session and sizing that session's window.
pub(crate) fn link_line(
    session: SessionId,
    window: WindowId,
    first: WindowId,
    owner: u32,
) -> String {
    format!(
        "set-option -t {session} {DEVICE_TAG} 1 ; set-option -t {session} {OWNER_TAG} {owner} ; set-option -t {session} detach-on-destroy on ; link-window -d -s {window} -t {session}: ; kill-window -t {first}"
    )
}

/// The session holds only `window` before a client attaches: a client is a
/// candidate for every window of its session (findings).
async fn attach(
    main: &Client,
    opts: &Options,
    session: SessionId,
    window: WindowId,
    first: WindowId,
    owner: u32,
) -> Result<Client, Error> {
    for r in main
        .run(&link_line(session, window, first, owner), 5)
        .await?
    {
        r?;
    }
    let (client, notes) = Client::attach_with(opts, Some(session), Flags { no_output: true })?;
    // Only replies matter on this client.
    drop(notes);
    client.command("display-message -p ok").await?;
    Ok(client)
}

/// Device sessions with no client: (session, owner pid). A bridge's own is
/// unattached only while it opens; callers check that the owner is gone.
pub async fn unattached_devices(client: &Client) -> Result<Vec<(SessionId, Option<u32>)>, Error> {
    let format = format!(
        "#{{session_id}}{SEP}#{{{DEVICE_TAG}}}{SEP}#{{session_attached}}{SEP}#{{{OWNER_TAG}}}"
    );
    let reply = client
        .command(&format!("list-sessions -F {}", quote(&format)))
        .await?;
    Ok(reply
        .lines
        .iter()
        .filter_map(|l| {
            let text = String::from_utf8_lossy(l);
            let f: Vec<&str> = text.split(SEP).collect();
            let [id, device, attached, owner] = f.as_slice() else {
                return None;
            };
            (!device.is_empty() && *attached == "0")
                .then(|| Some((id.parse().ok()?, owner.parse().ok())))?
        })
        .collect())
}

/// The device sessions that hold a window of `session`. They keep the window
/// open after `session` is killed, so they go first.
pub async fn devices_of(client: &Client, session: SessionId) -> Result<Vec<SessionId>, Error> {
    let format = format!("#{{session_id}}{SEP}#{{window_id}}{SEP}#{{{DEVICE_TAG}}}");
    let reply = client
        .command(&format!("list-windows -a -F {}", quote(&format)))
        .await?;
    Ok(holders(&reply.lines, session))
}

fn holders(lines: &[Vec<u8>], session: SessionId) -> Vec<SessionId> {
    let rows: Vec<(String, String, bool)> = lines
        .iter()
        .filter_map(|l| {
            let text = String::from_utf8_lossy(l);
            let mut f = text.split(SEP);
            let (s, w, device) = (f.next()?, f.next()?, f.next().unwrap_or(""));
            Some((s.to_owned(), w.to_owned(), !device.is_empty()))
        })
        .collect();
    let own = session.to_string();
    let windows: Vec<&str> = rows
        .iter()
        .filter(|(s, _, _)| *s == own)
        .map(|(_, w, _)| w.as_str())
        .collect();
    let mut found: Vec<SessionId> = rows
        .iter()
        .filter(|(s, w, device)| *device && *s != own && windows.contains(&w.as_str()))
        .filter_map(|(s, _, _)| s.parse().ok())
        .collect();
    found.dedup();
    found
}

/// A client that attached with no target may land on a device session (the
/// most recently used one). It moves to the most recent user session.
pub async fn leave_device_session(client: &Client) -> Result<(), Error> {
    let reply = client
        .command(&format!(
            "display-message -p {}",
            quote(&format!("#{{session_id}}{SEP}#{{{DEVICE_TAG}}}"))
        ))
        .await?;
    let line = reply
        .lines
        .first()
        .map(|l| String::from_utf8_lossy(l).into_owned())
        .unwrap_or_default();
    let Some((current, device)) = line.split_once(SEP) else {
        return Ok(());
    };
    if device.is_empty() {
        return Ok(());
    }
    let current: SessionId = current.parse()?;
    if let Some(other) = crate::lifecycle::last_other_session(client, current).await? {
        client
            .command(&format!("switch-client -E -t {other}"))
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_target_by_id_and_quote_the_name() {
        assert_eq!(
            create_line(4242, 1),
            r##"new-session -d -P -F "#{session_id} #{window_id}" -s "_muxa-4242-1" cat"##
        );
        assert_eq!(
            link_line(SessionId::new(9), WindowId::new(5), WindowId::new(8), 4242),
            "set-option -t $9 @muxa_device 1 ; set-option -t $9 @muxa_device_pid 4242 ; set-option -t $9 detach-on-destroy on ; link-window -d -s @5 -t $9: ; kill-window -t @8"
        );
    }

    fn row(text: &str) -> Vec<u8> {
        text.replace('|', "\u{1f}").into_bytes()
    }

    #[test]
    fn device_sessions_that_hold_a_window_of_the_session_are_found() {
        let lines = [
            row("$0|@0|"),
            row("$0|@1|"),
            row("$1|@1|1"),
            row("$2|@5|1"),
            row("$3|@1|"),
            row("$4|@0|1"),
        ];
        assert_eq!(
            holders(&lines, SessionId::new(0)),
            vec![SessionId::new(1), SessionId::new(4)]
        );
        assert!(holders(&lines, SessionId::new(7)).is_empty());
    }
}
