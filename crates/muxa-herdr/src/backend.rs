//! herdr-core as a muxa session backend (ADR 0067).

use muxa_backend::{
    BackendError, Change, ChangeStream, Exit, Labels, Launch, ProgramStatus, Result,
    SessionBackend, Size, StreamEvent as BackendEvent, TerminalId, TerminalInfo, TerminalStream,
};
use serde_json::Value;

use crate::{
    Attach, AttachSpec, Client, Error, Event, EventFilter, Events, Name, RunSpec, Status,
    StreamEvent, Terminal,
};

/// A [`SessionBackend`] over one herdr-core connection.
#[derive(Clone)]
pub struct HerdrBackend {
    client: Client,
}

impl HerdrBackend {
    pub fn new(client: Client) -> Self {
        HerdrBackend { client }
    }

    pub fn client(&self) -> &Client {
        &self.client
    }
}

fn err(op: &'static str) -> impl Fn(Error) -> BackendError {
    move |e| match e {
        Error::Closed => BackendError::Closed,
        Error::Rpc {
            kind: Some(k),
            message,
            ..
        } if k == "not_found" => BackendError::NotFound(message),
        Error::InvalidName(_) => BackendError::Invalid(e.to_string()),
        other => BackendError::failed(op, other),
    }
}

fn status(s: Option<Status>) -> Option<ProgramStatus> {
    s.map(|s| ProgramStatus {
        state: s.state,
        app: s.app,
    })
}

fn info(t: Terminal) -> TerminalInfo {
    let live = t.is_live();
    let exit = (!live).then(|| Exit {
        reason: t.reason.clone().unwrap_or_default(),
        code: t.exit_code,
        signal: None,
    });
    TerminalInfo {
        id: TerminalId::new(t.address),
        name: t.name,
        live,
        status: status(t.status),
        labels: t.labels,
        size: Size {
            cols: t.cols,
            rows: t.rows,
        },
        exit,
        pid: t.pid.filter(|_| live),
        status_since_ms: t.status_ms,
    }
}

fn exit_from(v: &Value) -> Exit {
    Exit {
        reason: v["reason"].as_str().unwrap_or_default().to_owned(),
        code: v["exit_code"].as_i64(),
        signal: v["signal"].as_i64(),
    }
}

impl SessionBackend for HerdrBackend {
    type Stream = HerdrStream;
    type Changes = HerdrChanges;

    async fn list(&self) -> Result<Vec<TerminalInfo>> {
        let all = self.client.list(false).await.map_err(err("list"))?;
        Ok(all.into_iter().map(info).collect())
    }

    async fn get(&self, id: &TerminalId) -> Result<TerminalInfo> {
        self.client
            .get(id.as_str())
            .await
            .map(info)
            .map_err(err("get"))
    }

    async fn run(&self, launch: &Launch) -> Result<TerminalInfo> {
        // A Task's title is free text; herdr-core takes only a short slug,
        // unique among live terminals.
        let name = match launch.name.as_deref().and_then(Name::from_title) {
            Some(name) => {
                let live = self.client.list(false).await.map_err(err("run"))?;
                Some(name.free(|n| live.iter().any(|t| t.name.as_deref() == Some(n))))
            }
            None => None,
        };
        let spec = RunSpec {
            argv: launch.argv.clone(),
            cwd: launch
                .cwd
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            name,
            labels: launch.labels.clone(),
            env: launch.env.clone(),
            keep: false,
        };
        self.client.run(&spec).await.map(info).map_err(err("run"))
    }

    async fn attach(&self, id: &TerminalId, size: Size, history: u32) -> Result<HerdrStream> {
        let spec = AttachSpec {
            terminal: id.as_str().to_owned(),
            cols: size.cols,
            rows: size.rows,
            history,
            observe: false,
        };
        let attach = self.client.attach(&spec).await.map_err(err("attach"))?;
        Ok(HerdrStream { attach })
    }

    async fn read(&self, id: &TerminalId, lines: u32) -> Result<String> {
        self.client
            .read(id.as_str(), lines)
            .await
            .map_err(err("read"))
    }

    async fn kill(&self, id: &TerminalId) -> Result<()> {
        self.client
            .kill(id.as_str())
            .await
            .map(drop)
            .map_err(err("kill"))
    }

    async fn remove(&self, id: &TerminalId) -> Result<()> {
        self.client.remove(id.as_str()).await.map_err(err("remove"))
    }

    async fn subscribe(&self, labels: &Labels) -> Result<(Vec<TerminalInfo>, HerdrChanges)> {
        let filter = EventFilter {
            labels: labels.clone(),
            terminal: None,
        };
        let (_, terminals, events) = self
            .client
            .subscribe(&filter)
            .await
            .map_err(err("subscribe"))?;
        Ok((
            terminals.into_iter().map(info).collect(),
            HerdrChanges { events },
        ))
    }
}

pub struct HerdrStream {
    attach: Attach,
}

impl TerminalStream for HerdrStream {
    async fn next(&mut self) -> Option<BackendEvent> {
        Some(match self.attach.next().await? {
            StreamEvent::Output(bytes) => BackendEvent::Output(bytes),
            StreamEvent::Resize { cols, rows } => BackendEvent::Resized(Size { cols, rows }),
            StreamEvent::Closed { reason } => BackendEvent::Closed(exit_from(&reason)),
        })
    }

    fn input(&self, bytes: &[u8]) -> Result<()> {
        self.attach.input(bytes).map_err(err("input"))
    }

    fn resize(&self, size: Size) -> Result<()> {
        self.attach
            .resize(size.cols, size.rows)
            .map_err(err("resize"))
    }

    fn detach(&self) -> Result<()> {
        self.attach.detach().map_err(err("detach"))
    }
}

pub struct HerdrChanges {
    events: Events,
}

impl ChangeStream for HerdrChanges {
    async fn next(&mut self) -> Option<Result<Change>> {
        loop {
            let event = match self.events.next().await? {
                Ok(event) => event,
                Err(e) => return Some(Err(err("events")(e))),
            };
            if let Some(change) = change(event) {
                return Some(change);
            }
        }
    }
}

/// Maps one herdr-core event; `None` for kinds muxa does not use (turn,
/// title, labels, foreground, heartbeat, ...).
fn change(event: Event) -> Option<Result<Change>> {
    match event {
        Event::Missed { terminals, .. } => Some(Ok(Change::Resync(
            terminals.into_iter().map(info).collect(),
        ))),
        Event::Heartbeat { .. } => None,
        Event::Change {
            kind,
            terminal,
            data,
            ..
        } => {
            let id = TerminalId::new(terminal?);
            let change = match kind.as_str() {
                "created" | "restarted" => match serde_json::from_value::<Terminal>(data) {
                    Ok(t) => Change::Created(info(t)),
                    Err(e) => return Some(Err(BackendError::failed("events", e))),
                },
                "status" => Change::Status {
                    status: status(
                        serde_json::from_value(data["status"].clone())
                            .ok()
                            .flatten(),
                    ),
                    id,
                },
                "size" => Change::Resized {
                    size: Size {
                        cols: u16::try_from(data["cols"].as_u64()?).ok()?,
                        rows: u16::try_from(data["rows"].as_u64()?).ok()?,
                    },
                    id,
                },
                "lost" | "finished" => Change::Ended {
                    exit: exit_from(&data),
                    id,
                },
                "removed" => Change::Removed { id },
                _ => return None,
            };
            Some(Ok(change))
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn ev(kind: &str, data: Value) -> Event {
        Event::Change {
            seq: 1,
            kind: kind.into(),
            terminal: Some("t21".into()),
            data,
        }
    }

    #[test]
    fn events_map_to_backend_changes() {
        let id = TerminalId::new("t21");
        let status = ev(
            "status",
            json!({"records": {}, "reported": true,
            "status": {"app": "probe", "state": "working"}}),
        );
        assert_eq!(
            change(status).unwrap().unwrap(),
            Change::Status {
                id: id.clone(),
                status: Some(ProgramStatus {
                    state: "working".into(),
                    app: Some("probe".into())
                })
            }
        );
        let size = ev("size", json!({"cols": 120, "rows": 40}));
        assert_eq!(
            change(size).unwrap().unwrap(),
            Change::Resized {
                id: id.clone(),
                size: Size {
                    cols: 120,
                    rows: 40
                }
            }
        );
        let ended = ev(
            "finished",
            json!({"reason": "killed", "exit_code": null, "signal": 1}),
        );
        assert_eq!(
            change(ended).unwrap().unwrap(),
            Change::Ended {
                id,
                exit: Exit {
                    reason: "killed".into(),
                    code: None,
                    signal: Some(1)
                }
            }
        );
    }

    #[test]
    fn unused_kinds_and_heartbeats_are_skipped() {
        assert!(change(ev("turn", json!({"state": "done", "turn": 1}))).is_none());
        assert!(change(Event::Heartbeat { seq: 3 }).is_none());
    }
}
