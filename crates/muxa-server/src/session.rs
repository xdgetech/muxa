//! One client connection: the handshake, then control requests on channel 0
//! and terminal frames on the channels the client attached.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::time::Duration;

use muxa_backend::{
    BackendError, Change, ChangeStream, Labels, SessionBackend, Size, TerminalId, TerminalInfo,
};
use muxa_protocol::v2::{
    self, ClientMsg, ErrorCode, Frame, Kind, Reply, Resume, ResumeChannel, Resumed, ServerMsg,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};

use crate::attachment::{Attachment, Checked, Cmd, Link};
use crate::sessions::{Handle, Sessions};
use crate::tasks::{self, TaskError, TaskSpec, Tasks};
use crate::wire::{self, Incoming};

/// The most scrollback lines one attach may ask for.
const MAX_HISTORY: u32 = 10_000;

/// How long a client's attachments wait for its resume.
pub const PARK: Duration = Duration::from_secs(10 * 60);

pub struct Config {
    /// `muxa 0.1.0`, sent in `welcome`.
    pub server: String,
    /// `herdr-core 0.1.21`, sent in `welcome`.
    pub backend: String,
    /// [`PARK`], shorter in tests.
    pub park: Duration,
}

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("the client sent no hello")]
    NoHello,
    #[error("the client speaks protocol {0}, not 2")]
    Version(u32),
    #[error("broken connection: {0}")]
    Broken(String),
    #[error("could not make a session token: {0}")]
    Token(std::io::Error),
}

struct Session<'a, B: SessionBackend, T: Tasks> {
    backend: &'a B,
    tasks: &'a T,
    /// The backend's changes, after `watch`.
    changes: Option<B::Changes>,
    out: mpsc::UnboundedSender<Frame>,
    ended: mpsc::UnboundedSender<u32>,
    attachments: HashMap<u32, Handle>,
    next_channel: u32,
    park_for: Duration,
}

/// What to start on a resumed channel once `welcome` is out.
enum Start<S> {
    /// A parked attachment whose buffer covers the client's offset.
    Parked(u32),
    /// A fresh attach, with a paint.
    Fresh {
        channel: u32,
        terminal: TerminalId,
        stream: S,
        last_seq: u64,
    },
}

/// Serves one client until it leaves, the connection breaks or a resume
/// with its token takes over. Its attachments are then parked for
/// `config.park`; the programs keep running.
pub async fn serve<B, T, R, W>(
    backend: &B,
    tasks: &T,
    sessions: &Sessions,
    reader: R,
    writer: W,
    config: &Config,
) -> Result<(), ServeError>
where
    B: SessionBackend,
    T: Tasks,
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let mut incoming = wire::spawn_reader(reader);
    let out = wire::spawn_writer(writer);

    let resume = handshake(&mut incoming, &out).await?;

    let (ended, mut ended_rx) = mpsc::unbounded_channel();
    let mut session = Session {
        backend,
        tasks,
        changes: None,
        out,
        ended,
        attachments: HashMap::new(),
        next_channel: 1,
        park_for: config.park,
    };
    let (token, resumed, starts) = match resume {
        Some(r) => session.resume(sessions, r).await?,
        None => (token().map_err(ServeError::Token)?, Vec::new(), Vec::new()),
    };
    let mut kick = sessions.begin(&token);
    send(
        &session.out,
        &ServerMsg::Welcome {
            version: v2::VERSION,
            server: config.server.clone(),
            backend: config.backend.clone(),
            token: token.clone(),
            resumed,
        },
    );
    session.start(starts);

    let result = loop {
        tokio::select! {
            msg = incoming.recv() => match msg {
                Some(Incoming::Frame(frame)) => session.frame(frame).await,
                Some(Incoming::Broken(e)) => break Err(ServeError::Broken(e)),
                None => break Ok(()),
            },
            Some(channel) = ended_rx.recv() => {
                session.attachments.remove(&channel);
            }
            change = next_change(&mut session.changes) => session.change(change).await,
            _ = &mut kick => {
                tracing::info!("a resume took over this session");
                break Ok(());
            }
        }
    };
    for handle in session.attachments.values() {
        let _ = handle.cmds.send(Cmd::Park);
    }
    sessions.park(&token, session.attachments, config.park);
    result
}

async fn handshake(
    incoming: &mut mpsc::Receiver<Incoming>,
    out: &mpsc::UnboundedSender<Frame>,
) -> Result<Option<Resume>, ServeError> {
    let frame = match incoming.recv().await {
        Some(Incoming::Frame(f)) if f.channel == 0 && f.kind == Kind::Data => f,
        Some(Incoming::Broken(e)) => return Err(ServeError::Broken(e)),
        _ => return Err(ServeError::NoHello),
    };
    let (version, resume) = match serde_json::from_slice::<ClientMsg>(&frame.payload) {
        Ok(ClientMsg::Hello {
            version, resume, ..
        }) => (version, resume),
        _ => return Err(ServeError::NoHello),
    };
    if version != v2::VERSION {
        send(
            out,
            &ServerMsg::Error {
                id: None,
                code: ErrorCode::Version,
                message: format!("this server speaks protocol {}", v2::VERSION),
            },
        );
        return Err(ServeError::Version(version));
    }
    Ok(resume)
}

/// 128 random bits as hex: the session token a client resumes with.
fn token() -> std::io::Result<String> {
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn send(out: &mpsc::UnboundedSender<Frame>, msg: &ServerMsg) {
    match Frame::control(msg) {
        Ok(frame) => {
            let _ = out.send(frame);
        }
        Err(e) => tracing::warn!(error = %e, "control message not encoded"),
    }
}

fn to_wire(t: TerminalInfo) -> v2::Terminal {
    v2::Terminal {
        id: t.id.as_str().to_owned(),
        name: t.name,
        live: t.live,
        status: t.status.map(|s| v2::Status {
            state: s.state,
            app: s.app,
        }),
        labels: t.labels,
        cols: t.size.cols,
        rows: t.size.rows,
    }
}

fn code_of(e: &BackendError) -> ErrorCode {
    match e {
        BackendError::NotFound(_) => ErrorCode::NotFound,
        BackendError::Closed => ErrorCode::Closed,
        BackendError::Invalid(_) => ErrorCode::BadRequest,
        BackendError::Failed { .. } => ErrorCode::Backend,
    }
}

impl<B: SessionBackend, T: Tasks> Session<'_, B, T> {
    async fn frame(&mut self, frame: Frame) {
        if frame.channel == 0 {
            if frame.kind == Kind::Data {
                self.control(&frame.payload).await;
            }
            return;
        }
        let Some(Handle { cmds, .. }) = self.attachments.get(&frame.channel) else {
            // Late frames for a channel that just closed.
            return;
        };
        let cmd = match frame.kind {
            Kind::Data => frame.split_u64().map(|(seq, bytes)| Cmd::Input {
                seq,
                bytes: bytes.to_vec(),
            }),
            Kind::Credit => frame.as_u32().map(Cmd::Credit),
            Kind::Resize => frame
                .as_size()
                .filter(|(c, r)| *c > 0 && *r > 0)
                .map(|(cols, rows)| Cmd::Resize(Size { cols, rows })),
            Kind::Close => Some(Cmd::Detach),
            Kind::Ack => None,
        };
        if let Some(cmd) = cmd {
            let _ = cmds.send(cmd);
        }
    }

    async fn control(&mut self, payload: &[u8]) {
        let msg = match serde_json::from_slice::<ClientMsg>(payload) {
            Ok(msg) => msg,
            Err(e) => {
                self.error(None, ErrorCode::BadRequest, format!("bad message: {e}"));
                return;
            }
        };
        match msg {
            ClientMsg::Hello { .. } => {
                self.error(None, ErrorCode::BadRequest, "hello was already sent".into());
            }
            ClientMsg::Terminals { id } => match self.backend.list().await {
                Ok(all) => self.reply(
                    id,
                    Reply::Terminals {
                        terminals: all.into_iter().map(to_wire).collect(),
                    },
                ),
                Err(e) => self.error(Some(id), code_of(&e), e.to_string()),
            },
            ClientMsg::Attach {
                id,
                terminal,
                cols,
                rows,
                history,
            } => {
                self.attach(id, terminal, Size { cols, rows }, history)
                    .await;
            }
            ClientMsg::Detach { id, channel } => match self.attachments.get(&channel) {
                Some(handle) => {
                    let _ = handle.cmds.send(Cmd::Detach);
                    self.reply(id, Reply::Empty {});
                }
                None => self.error(
                    Some(id),
                    ErrorCode::NotFound,
                    format!("no channel {channel}"),
                ),
            },
            ClientMsg::StartTask {
                id,
                repo,
                branch,
                base,
                plan,
            } => {
                let spec = TaskSpec {
                    repo,
                    branch,
                    base,
                    plan,
                };
                self.start_task(id, &spec).await;
            }
            ClientMsg::Watch { id } => self.watch(id).await,
        }
    }

    async fn start_task(&mut self, id: u64, spec: &TaskSpec) {
        let prepared = match self.tasks.prepare(spec).await {
            Ok(p) => p,
            Err(e) => {
                let code = match e {
                    TaskError::Invalid(_) => ErrorCode::BadRequest,
                    TaskError::Failed(_) => ErrorCode::Backend,
                };
                self.error(Some(id), code, e.to_string());
                return;
            }
        };
        match tasks::start(self.backend, prepared).await {
            Ok((terminal, existing)) => {
                tracing::info!(terminal = %terminal.id, existing, "task agent");
                self.reply(
                    id,
                    Reply::Started {
                        terminal: to_wire(terminal),
                        existing,
                    },
                );
            }
            Err(e) => self.error(Some(id), code_of(&e), e.to_string()),
        }
    }

    /// Subscribes to every terminal's changes. A second `watch` starts over.
    async fn watch(&mut self, id: u64) {
        match self.backend.subscribe(&Labels::new()).await {
            Ok((_, changes)) => {
                self.changes = Some(changes);
                self.reply(id, Reply::Empty {});
            }
            Err(e) => self.error(Some(id), code_of(&e), e.to_string()),
        }
    }

    /// Sends a change as the terminal now is. `None` ends the watch.
    async fn change(&mut self, change: Option<Result<Change, BackendError>>) {
        let id = match change {
            Some(Ok(Change::Created(t))) => {
                send(
                    &self.out,
                    &ServerMsg::Changed {
                        terminal: to_wire(t),
                    },
                );
                return;
            }
            Some(Ok(
                Change::Status { id, .. } | Change::Resized { id, .. } | Change::Ended { id, .. },
            )) => id,
            Some(Ok(Change::Removed { id })) => {
                let terminal = id.as_str().to_owned();
                send(&self.out, &ServerMsg::Removed { terminal });
                return;
            }
            Some(Ok(Change::Resync(_))) => {
                send(&self.out, &ServerMsg::Resync {});
                return;
            }
            Some(Err(e)) => {
                tracing::info!(error = %e, "a change was lost");
                send(&self.out, &ServerMsg::Resync {});
                return;
            }
            None => {
                tracing::info!("the backend ended the watch");
                self.changes = None;
                send(&self.out, &ServerMsg::Resync {});
                return;
            }
        };
        match self.backend.get(&id).await {
            Ok(t) => send(
                &self.out,
                &ServerMsg::Changed {
                    terminal: to_wire(t),
                },
            ),
            Err(BackendError::NotFound(_)) => {
                let terminal = id.as_str().to_owned();
                send(&self.out, &ServerMsg::Removed { terminal });
            }
            Err(e) => {
                tracing::info!(error = %e, "a changed terminal could not be read");
                send(&self.out, &ServerMsg::Resync {});
            }
        }
    }

    async fn attach(&mut self, id: u64, terminal: String, size: Size, history: u32) {
        if size.cols == 0 || size.rows == 0 {
            self.error(
                Some(id),
                ErrorCode::BadRequest,
                "cols and rows must be above 0".into(),
            );
            return;
        }
        let tid = TerminalId::new(terminal);
        let stream = match self
            .backend
            .attach(&tid, size, history.min(MAX_HISTORY))
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                self.error(Some(id), code_of(&e), e.to_string());
                return;
            }
        };
        let channel = self.next_channel;
        self.next_channel += 1;
        // The reply goes before the paint and any output.
        self.reply(id, Reply::Attached { channel });
        self.spawn(channel, tid, stream, 0);
    }

    /// Paints `channel` and starts its attachment.
    fn spawn(&mut self, channel: u32, terminal: TerminalId, stream: B::Stream, last_seq: u64) {
        let (cmds, cmd_rx) = mpsc::unbounded_channel();
        send(&self.out, &ServerMsg::Paint { channel });
        tracing::info!(channel, terminal = %terminal, "attached");
        self.attachments.insert(channel, Handle { terminal, cmds });
        let attachment = Attachment::new(channel, stream, last_seq, self.link(), self.park_for);
        tokio::spawn(attachment.run(cmd_rx));
    }

    fn link(&self) -> Link {
        Link {
            out: self.out.clone(),
            ended: self.ended.clone(),
        }
    }

    /// Takes over the parked attachments of `resume.token` and decides, per
    /// channel, between its buffer and a fresh attach. Sends nothing.
    async fn resume(
        &mut self,
        sessions: &Sessions,
        resume: Resume,
    ) -> Result<(String, Vec<Resumed>, Vec<Start<B::Stream>>), ServeError> {
        let claimed = sessions.claim(&resume.token).await;
        let token = match claimed {
            Some(_) => resume.token.clone(),
            None => token().map_err(ServeError::Token)?,
        };
        let mut parked = claimed.unwrap_or_default();
        let mut seen = HashSet::new();
        let mut resumed = Vec::new();
        let mut starts = Vec::new();
        for ch in resume.channels {
            // Never reuse a channel the client named: it may still send on it.
            self.next_channel = self.next_channel.max(ch.channel.saturating_add(1));
            if ch.channel == 0 || ch.cols == 0 || ch.rows == 0 || !seen.insert(ch.channel) {
                continue;
            }
            let checked = match parked.remove(&ch.channel) {
                Some(h) if h.terminal.as_str() == ch.terminal => check(h, ch.offset).await,
                Some(h) => {
                    let _ = h.cmds.send(Cmd::Detach);
                    None
                }
                None => None,
            };
            let start = match checked {
                Some((Checked::Resume(seq), handle)) => {
                    self.attachments.insert(ch.channel, handle);
                    Some((Start::Parked(ch.channel), Some(seq)))
                }
                Some((Checked::Repaint(seq), _)) => {
                    self.reattach(&ch, seq).await.map(|s| (s, Some(seq)))
                }
                None => self.reattach(&ch, ch.input_seq).await.map(|s| (s, None)),
            };
            if let Some((start, input_seq)) = start {
                resumed.push(Resumed {
                    channel: ch.channel,
                    input_seq,
                });
                starts.push(start);
            }
        }
        for h in parked.into_values() {
            let _ = h.cmds.send(Cmd::Detach);
        }
        Ok((token, resumed, starts))
    }

    async fn reattach(&self, ch: &ResumeChannel, last_seq: u64) -> Option<Start<B::Stream>> {
        let terminal = TerminalId::new(ch.terminal.clone());
        let size = Size {
            cols: ch.cols,
            rows: ch.rows,
        };
        match self
            .backend
            .attach(&terminal, size, ch.history.min(MAX_HISTORY))
            .await
        {
            Ok(stream) => Some(Start::Fresh {
                channel: ch.channel,
                terminal,
                stream,
                last_seq,
            }),
            Err(e) => {
                tracing::info!(channel = ch.channel, error = %e, "not resumed");
                None
            }
        }
    }

    /// Runs what `resume` decided, after `welcome`.
    fn start(&mut self, starts: Vec<Start<B::Stream>>) {
        for start in starts {
            match start {
                Start::Parked(channel) => {
                    if let Some(h) = self.attachments.get(&channel) {
                        let _ = h.cmds.send(Cmd::Resume(self.link()));
                    }
                }
                Start::Fresh {
                    channel,
                    terminal,
                    stream,
                    last_seq,
                } => self.spawn(channel, terminal, stream, last_seq),
            }
        }
    }

    fn reply(&self, id: u64, body: Reply) {
        send(&self.out, &ServerMsg::Reply { id, body });
    }

    fn error(&self, id: Option<u64>, code: ErrorCode, message: String) {
        send(&self.out, &ServerMsg::Error { id, code, message });
    }
}

/// Asks a parked attachment whether its buffer covers `offset`. `None` when
/// it is gone (expired, or its program's end was already sent).
async fn check(handle: Handle, offset: u64) -> Option<(Checked, Handle)> {
    let (reply, answer) = oneshot::channel();
    handle.cmds.send(Cmd::Check { offset, reply }).ok()?;
    answer.await.ok().map(|c| (c, handle))
}

async fn next_change<C: ChangeStream>(
    changes: &mut Option<C>,
) -> Option<Result<Change, BackendError>> {
    match changes {
        Some(c) => c.next().await,
        None => std::future::pending().await,
    }
}
