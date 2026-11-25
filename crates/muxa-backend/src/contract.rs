//! The contract every backend adapter must pass (ADR 0067). It runs one
//! scenario on one labelled terminal and removes it again. Needs `sh` and
//! `stty` on the backend's host.

use std::path::Path;
use std::time::Duration;

use crate::{
    Change, ChangeStream, Labels, Launch, SessionBackend, Size, StreamEvent, TerminalId,
    TerminalStream,
};

const WAIT: Duration = Duration::from_secs(5);
const APP: &str = "muxa-contract";

/// Prints `ready`, then answers each line with its text and the tty size, and
/// reports `working`, then `done` after the line `done`, with OSC 7501.
const SCRIPT: &str = r#"printf '\033]7501;state=working:app=muxa-contract\033\\'
echo ready
while read l; do
  echo "got:$l size:$(stty size)"
  [ "$l" = done ] && printf '\033]7501;state=done:app=muxa-contract\033\\'
done"#;

/// Runs the whole contract. `tag` must be unique to this run; it labels the
/// terminal so the scenario sees only its own. Errors name the broken fact.
pub async fn check<B: SessionBackend>(
    backend: &B,
    workdir: &Path,
    tag: &str,
) -> Result<(), String> {
    let labels: Labels = [("muxa.contract".to_owned(), tag.to_owned())].into();
    let (before, mut changes) = backend
        .subscribe(&labels)
        .await
        .map_err(|e| format!("subscribe: {e}"))?;
    if !before.is_empty() {
        return Err("a fresh label already matches terminals".into());
    }
    let launch = Launch {
        argv: vec!["sh".into(), "-c".into(), SCRIPT.into()],
        cwd: Some(workdir.to_path_buf()),
        name: Some(tag.to_owned()),
        labels: labels.clone(),
        env: Default::default(),
    };
    let info = backend
        .run(&launch)
        .await
        .map_err(|e| format!("run: {e}"))?;
    if !info.live || info.labels != labels {
        return Err(format!(
            "run returned live={} labels={:?}",
            info.live, info.labels
        ));
    }
    let id = info.id.clone();
    // herdr-core refuses a second live terminal with a taken name.
    if let Ok(twin) = backend.run(&launch).await {
        let _ = backend.kill(&twin.id).await;
        let _ = backend.remove(&twin.id).await;
        let _ = backend.kill(&id).await;
        let _ = backend.remove(&id).await;
        return Err("run took a name a live terminal has".into());
    }
    let result = scenario(backend, &id, &mut changes).await;
    // Clean up whatever happened; a cleanup error matters only after success.
    let _ = backend.kill(&id).await;
    let removed = backend.remove(&id).await;
    result?;
    removed.map_err(|e| format!("remove after kill: {e}"))?;
    if backend.get(&id).await.is_ok() {
        return Err("a removed terminal can still be read".into());
    }
    Ok(())
}

async fn scenario<B: SessionBackend>(
    backend: &B,
    id: &TerminalId,
    changes: &mut B::Changes,
) -> Result<(), String> {
    let mut stream = backend
        .attach(
            id,
            Size {
                cols: 100,
                rows: 30,
            },
            50,
        )
        .await
        .map_err(|e| format!("attach: {e}"))?;
    let mut seen = Vec::new();
    wait_for(
        &mut stream,
        &mut seen,
        "ready",
        "the first paint holds the screen",
    )
    .await?;

    stream.input(b"one\r").map_err(|e| format!("input: {e}"))?;
    wait_for(
        &mut stream,
        &mut seen,
        "got:one size:30 100",
        "input reaches the program, and attach's size is its size",
    )
    .await?;

    stream
        .resize(Size {
            cols: 120,
            rows: 40,
        })
        .map_err(|e| format!("resize: {e}"))?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    stream.input(b"two\r").map_err(|e| format!("input: {e}"))?;
    wait_for(
        &mut stream,
        &mut seen,
        "got:two size:40 120",
        "a resize reaches the program",
    )
    .await?;

    stream.input(b"done\r").map_err(|e| format!("input: {e}"))?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let status = backend
        .get(id)
        .await
        .map_err(|e| format!("get: {e}"))?
        .status;
    match status {
        Some(s) if s.state == "done" && s.app.as_deref() == Some(APP) => {}
        other => return Err(format!("OSC 7501 state not reported: {other:?}")),
    }

    backend.kill(id).await.map_err(|e| format!("kill: {e}"))?;
    let exit = closed(&mut stream).await?;
    if exit.reason != "killed" {
        return Err(format!(
            "kill closed the stream with reason {:?}",
            exit.reason
        ));
    }
    let after = backend
        .get(id)
        .await
        .map_err(|e| format!("get after kill: {e}"))?;
    if after.live {
        return Err("a killed terminal is still live".into());
    }

    expect_changes(changes, id).await
}

async fn wait_for<S: TerminalStream>(
    stream: &mut S,
    seen: &mut Vec<u8>,
    needle: &str,
    fact: &str,
) -> Result<(), String> {
    let found = tokio::time::timeout(WAIT, async {
        while !String::from_utf8_lossy(seen).contains(needle) {
            match stream.next().await {
                Some(StreamEvent::Output(bytes)) => seen.extend_from_slice(&bytes),
                Some(StreamEvent::Resized(_)) => {}
                Some(StreamEvent::Closed(exit)) => return Err(format!("stream closed: {exit:?}")),
                None => return Err("stream ended".to_owned()),
            }
        }
        Ok(())
    })
    .await;
    match found {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(format!("{fact}: {e}")),
        Err(_) => Err(format!("{fact}: not seen within {WAIT:?}")),
    }
}

async fn closed<S: TerminalStream>(stream: &mut S) -> Result<crate::Exit, String> {
    let exit = tokio::time::timeout(WAIT, async {
        loop {
            match stream.next().await {
                Some(StreamEvent::Closed(exit)) => return Some(exit),
                Some(_) => {}
                None => return None,
            }
        }
    })
    .await;
    match exit {
        Ok(Some(exit)) => Ok(exit),
        Ok(None) => Err("the stream ended without a reason".into()),
        Err(_) => Err("the stream did not close after kill".into()),
    }
}

/// Created first, then a status and a size change, then the end, all for `id`.
async fn expect_changes<C: ChangeStream>(changes: &mut C, id: &TerminalId) -> Result<(), String> {
    let mut kinds = Vec::new();
    let done = tokio::time::timeout(WAIT, async {
        while let Some(change) = changes.next().await {
            let change = change.map_err(|e| format!("change stream: {e}"))?;
            let (kind, about) = match &change {
                Change::Created(info) => ("created", Some(&info.id)),
                Change::Status { id, .. } => ("status", Some(id)),
                Change::Resized { id, .. } => ("resized", Some(id)),
                Change::Ended { id, .. } => ("ended", Some(id)),
                Change::Removed { id } => ("removed", Some(id)),
                Change::Resync(_) => ("resync", None),
            };
            if about.is_some_and(|about| about != id) {
                return Err(format!("a change for another terminal: {change:?}"));
            }
            kinds.push(kind);
            if kind == "ended" {
                return Ok(());
            }
        }
        Err("the change stream ended".to_owned())
    })
    .await;
    match done {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err(format!("no end within {WAIT:?}; saw {kinds:?}")),
    }
    let first_ok = kinds.first() == Some(&"created");
    if !first_ok || !kinds.contains(&"status") || !kinds.contains(&"resized") {
        return Err(format!("changes out of order or missing: {kinds:?}"));
    }
    Ok(())
}
