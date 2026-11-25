//! An in-memory backend for the server's tests. A terminal's "program"
//! echoes input as `echo:<bytes>`; tests drive output and status directly.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::mpsc;

use crate::{
    BackendError, Change, ChangeStream, Exit, Labels, Launch, ProgramStatus, Result,
    SessionBackend, Size, StreamEvent, TerminalId, TerminalInfo, TerminalStream,
};

struct Term {
    info: TerminalInfo,
    screen: Vec<u8>,
    streams: Vec<mpsc::UnboundedSender<StreamEvent>>,
}

#[derive(Default)]
struct State {
    next: u64,
    terms: BTreeMap<TerminalId, Term>,
    subscribers: Vec<(Labels, mpsc::UnboundedSender<Change>)>,
    /// Every input applied, per terminal, in order.
    inputs: BTreeMap<TerminalId, Vec<Vec<u8>>>,
    launches: Vec<Launch>,
}

/// Cloning shares the terminals.
#[derive(Clone, Default)]
pub struct FakeBackend {
    state: Arc<Mutex<State>>,
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|p| p.into_inner())
}

fn not_found(_op: &'static str, id: &TerminalId) -> BackendError {
    BackendError::NotFound(format!("no terminal {id}"))
}

impl State {
    /// `name`, else `name-2`, `-3`…: as herdr-core, a live terminal's name is taken.
    fn free_name(&self, name: &str) -> String {
        let taken = |n: &str| {
            self.terms
                .values()
                .any(|t| t.info.live && t.info.name.as_deref() == Some(n))
        };
        if !taken(name) {
            return name.to_owned();
        }
        (2..)
            .map(|n| format!("{name}-{n}"))
            .find(|n| !taken(n))
            .unwrap_or_else(|| name.to_owned())
    }

    fn notify(&mut self, labels: &Labels, change: &Change) {
        self.subscribers.retain(|(want, tx)| {
            if !want.iter().all(|(k, v)| labels.get(k) == Some(v)) {
                return true;
            }
            tx.send(change.clone()).is_ok()
        });
    }

    fn output(&mut self, id: &TerminalId, bytes: &[u8]) {
        if let Some(t) = self.terms.get_mut(id) {
            t.screen.extend_from_slice(bytes);
            t.streams
                .retain(|tx| tx.send(StreamEvent::Output(bytes.to_vec())).is_ok());
        }
    }
}

impl FakeBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// The program writes `bytes`.
    pub fn emit(&self, id: &TerminalId, bytes: &[u8]) {
        lock(&self.state).output(id, bytes);
    }

    /// The program reports an OSC 7501 state.
    pub fn set_status(&self, id: &TerminalId, state: &str) {
        let mut s = lock(&self.state);
        let status = Some(ProgramStatus {
            state: state.to_owned(),
            app: Some("fake".to_owned()),
        });
        let Some(t) = s.terms.get_mut(id) else { return };
        t.info.status = status.clone();
        let labels = t.info.labels.clone();
        s.notify(
            &labels,
            &Change::Status {
                id: id.clone(),
                status,
            },
        );
    }

    /// Every input the terminal received, in order.
    pub fn inputs(&self, id: &TerminalId) -> Vec<Vec<u8>> {
        lock(&self.state)
            .inputs
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    /// The program ends with `exit`, as if it exited or crashed by itself.
    pub fn end(&self, id: &TerminalId, exit: Exit) -> Result<()> {
        let mut s = lock(&self.state);
        let t = s.terms.get_mut(id).ok_or_else(|| not_found("end", id))?;
        t.info.live = false;
        t.info.exit = Some(exit.clone());
        for tx in t.streams.drain(..) {
            let _ = tx.send(StreamEvent::Closed(exit.clone()));
        }
        let labels = t.info.labels.clone();
        s.notify(
            &labels,
            &Change::Ended {
                id: id.clone(),
                exit,
            },
        );
        Ok(())
    }

    /// Every `run`, in order.
    pub fn launches(&self) -> Vec<Launch> {
        lock(&self.state).launches.clone()
    }

    pub fn size(&self, id: &TerminalId) -> Option<Size> {
        lock(&self.state).terms.get(id).map(|t| t.info.size)
    }

    /// Streams currently attached to the terminal.
    pub fn viewers(&self, id: &TerminalId) -> usize {
        let mut s = lock(&self.state);
        s.terms.get_mut(id).map_or(0, |t| {
            t.streams.retain(|tx| !tx.is_closed());
            t.streams.len()
        })
    }
}

impl SessionBackend for FakeBackend {
    type Stream = FakeStream;
    type Changes = FakeChanges;

    async fn list(&self) -> Result<Vec<TerminalInfo>> {
        let s = lock(&self.state);
        Ok(s.terms
            .values()
            .filter(|t| t.info.live)
            .map(|t| t.info.clone())
            .collect())
    }

    async fn get(&self, id: &TerminalId) -> Result<TerminalInfo> {
        lock(&self.state)
            .terms
            .get(id)
            .map(|t| t.info.clone())
            .ok_or_else(|| not_found("get", id))
    }

    async fn run(&self, launch: &Launch) -> Result<TerminalInfo> {
        let mut s = lock(&self.state);
        s.launches.push(launch.clone());
        s.next += 1;
        let id = TerminalId::new(format!("f{}", s.next));
        let info = TerminalInfo {
            id: id.clone(),
            name: launch.name.as_deref().map(|n| s.free_name(n)),
            live: true,
            status: None,
            labels: launch.labels.clone(),
            size: Size { cols: 80, rows: 24 },
            exit: None,
        };
        s.terms.insert(
            id,
            Term {
                info: info.clone(),
                screen: Vec::new(),
                streams: Vec::new(),
            },
        );
        s.notify(&launch.labels, &Change::Created(info.clone()));
        Ok(info)
    }

    async fn attach(&self, id: &TerminalId, size: Size, _history: u32) -> Result<FakeStream> {
        let mut s = lock(&self.state);
        let t = s.terms.get_mut(id).ok_or_else(|| not_found("attach", id))?;
        let (tx, rx) = mpsc::unbounded_channel();
        let _ = tx.send(StreamEvent::Output(t.screen.clone()));
        t.streams.push(tx);
        t.info.size = size;
        Ok(FakeStream {
            backend: self.clone(),
            id: id.clone(),
            rx,
            done: false,
        })
    }

    async fn read(&self, id: &TerminalId, _lines: u32) -> Result<String> {
        lock(&self.state)
            .terms
            .get(id)
            .map(|t| String::from_utf8_lossy(&t.screen).into_owned())
            .ok_or_else(|| not_found("read", id))
    }

    async fn kill(&self, id: &TerminalId) -> Result<()> {
        let exit = Exit {
            reason: "killed".to_owned(),
            code: None,
            signal: Some(1),
        };
        self.end(id, exit)
    }

    async fn remove(&self, id: &TerminalId) -> Result<()> {
        let mut s = lock(&self.state);
        let t = s.terms.remove(id).ok_or_else(|| not_found("remove", id))?;
        s.notify(&t.info.labels, &Change::Removed { id: id.clone() });
        Ok(())
    }

    async fn subscribe(&self, labels: &Labels) -> Result<(Vec<TerminalInfo>, FakeChanges)> {
        let mut s = lock(&self.state);
        let matching = s
            .terms
            .values()
            .filter(|t| labels.iter().all(|(k, v)| t.info.labels.get(k) == Some(v)))
            .map(|t| t.info.clone())
            .collect();
        let (tx, rx) = mpsc::unbounded_channel();
        s.subscribers.push((labels.clone(), tx));
        Ok((matching, FakeChanges { rx }))
    }
}

pub struct FakeStream {
    backend: FakeBackend,
    id: TerminalId,
    rx: mpsc::UnboundedReceiver<StreamEvent>,
    done: bool,
}

impl TerminalStream for FakeStream {
    async fn next(&mut self) -> Option<StreamEvent> {
        if self.done {
            return None;
        }
        let event = self.rx.recv().await?;
        if matches!(event, StreamEvent::Closed(_)) {
            self.done = true;
        }
        Some(event)
    }

    fn input(&self, bytes: &[u8]) -> Result<()> {
        let mut s = lock(&self.backend.state);
        if !s.terms.get(&self.id).is_some_and(|t| t.info.live) {
            return Err(BackendError::Closed);
        }
        s.inputs
            .entry(self.id.clone())
            .or_default()
            .push(bytes.to_vec());
        let mut echo = b"echo:".to_vec();
        echo.extend_from_slice(bytes);
        s.output(&self.id, &echo);
        Ok(())
    }

    fn resize(&self, size: Size) -> Result<()> {
        let mut s = lock(&self.backend.state);
        let Some(t) = s.terms.get_mut(&self.id) else {
            return Err(BackendError::Closed);
        };
        t.info.size = size;
        let labels = t.info.labels.clone();
        s.notify(
            &labels,
            &Change::Resized {
                id: self.id.clone(),
                size,
            },
        );
        Ok(())
    }

    fn detach(&self) -> Result<()> {
        Ok(())
    }
}

pub struct FakeChanges {
    rx: mpsc::UnboundedReceiver<Change>,
}

impl ChangeStream for FakeChanges {
    async fn next(&mut self) -> Option<Result<Change>> {
        self.rx.recv().await.map(Ok)
    }
}
