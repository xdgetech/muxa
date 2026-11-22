//! One client connection: the handshake, then control requests on channel 0
//! and terminal frames on the channels the client attached.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::time::Duration;

use muxa_backend::{
    BackendError, Change, ChangeStream, Labels, SessionBackend, Size, TerminalId, TerminalInfo,
};
use muxa_inbox::Inbox;
use muxa_protocol::v2::{
    self, ClientMsg, DiffScope, ErrorCode, Frame, Kind, Reply, Resume, ResumeChannel, Resumed,
    ServerMsg,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot, watch};

use crate::attachment::{Attachment, Checked, Cmd, Link};
use crate::reader::{self, ReaderOpen, Readers};
use crate::sessions::{Handle, Sessions};
use crate::tasks::{self, Host, TaskError, TaskSpec};
use crate::wire::{self, Incoming};
use crate::{inbox, task_actions, task_items};

/// The most scrollback lines one attach may ask for.
const MAX_HISTORY: u32 = 10_000;

/// How often followed conversations are read again.
const CHAT_TICK: Duration = Duration::from_millis(700);

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

/// What every connection shares: one per server.
pub struct Services<'a, B, H> {
    pub backend: &'a B,
    pub host: &'a H,
    pub sessions: &'a Sessions,
    pub inbox: &'a Inbox,
}

struct Session<'a, B: SessionBackend, H: Host> {
    backend: &'a B,
    host: &'a H,
    inbox: &'a Inbox,
    /// The Inbox's revisions, after `watch`.
    inbox_watch: Option<watch::Receiver<u64>>,
    /// The host's Task Loop changes, after `watch`.
    loop_watch: Option<watch::Receiver<u64>>,
    /// The backend's changes, after `watch`.
    changes: Option<B::Changes>,
    out: mpsc::UnboundedSender<Frame>,
    ended: mpsc::UnboundedSender<u32>,
    attachments: HashMap<u32, Handle>,
    next_channel: u32,
    park_for: Duration,
    readers: Readers,
    /// Followed conversations: chat number to the host's handle.
    chats: HashMap<u32, u64>,
    next_chat: u32,
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
pub async fn serve<B, H, R, W>(
    services: &Services<'_, B, H>,
    reader: R,
    writer: W,
    config: &Config,
) -> Result<(), ServeError>
where
    B: SessionBackend,
    H: Host,
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let mut incoming = wire::spawn_reader(reader);
    let out = wire::spawn_writer(writer);

    let resume = handshake(&mut incoming, &out).await?;

    let (ended, mut ended_rx) = mpsc::unbounded_channel();
    let Services {
        backend,
        host,
        sessions,
        inbox,
    } = *services;
    let mut session = Session {
        backend,
        host,
        inbox,
        inbox_watch: None,
        loop_watch: None,
        changes: None,
        out,
        ended,
        attachments: HashMap::new(),
        next_channel: 1,
        park_for: config.park,
        readers: Readers::default(),
        chats: HashMap::new(),
        next_chat: 1,
    };
    let mut chat_tick = tokio::time::interval(CHAT_TICK);
    chat_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut reader_tick = tokio::time::interval(reader::TICK);
    reader_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
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
            _ = chat_tick.tick(), if !session.chats.is_empty() => {
                session.poll_chats().await;
            }
            _ = reader_tick.tick(), if !session.readers.open.is_empty() => {
                session.refresh_readers().await;
            }
            () = next_revision(&mut session.loop_watch) => {
                let loops = session.host.loops().await;
                send(&session.out, &ServerMsg::Loops { loops });
            }
            () = next_revision(&mut session.inbox_watch) => {
                let inbox = session.inbox.view();
                send(&session.out, &ServerMsg::Inbox { inbox });
            }
            _ = &mut kick => {
                tracing::info!("a resume took over this session");
                break Ok(());
            }
        }
    };
    // A resume opens its chats again.
    for handle in session.chats.values() {
        session.host.chat_close(*handle).await;
    }
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

pub(crate) fn code_of(e: &BackendError) -> ErrorCode {
    match e {
        BackendError::NotFound(_) => ErrorCode::NotFound,
        BackendError::Closed => ErrorCode::Closed,
        BackendError::Invalid(_) => ErrorCode::BadRequest,
        BackendError::Failed { .. } => ErrorCode::Backend,
    }
}

impl<B: SessionBackend, H: Host> Session<'_, B, H> {
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
                workspace,
                title,
                goal,
                places,
                connections,
                mcp,
                plan,
                task_loop,
            } => {
                let spec = TaskSpec {
                    workspace,
                    title,
                    goal,
                    places,
                    connections,
                    mcp,
                    plan,
                };
                self.start_task(id, &spec, task_loop).await;
            }
            ClientMsg::Loops { id } => {
                let loops = self.host.loops().await;
                self.reply(id, Reply::Loops { loops });
            }
            ClientMsg::LoopStart {
                id,
                terminal,
                spec,
                reset,
            } => {
                let started = match self.agent(&terminal).await {
                    Ok(t) => self
                        .host
                        .loop_start(&t, &spec, reset)
                        .await
                        .map_err(|e| (task_code(&e), e.to_string())),
                    Err(e) => Err(e),
                };
                match started {
                    Ok(info) => self.reply(id, Reply::Loop { info }),
                    Err((code, message)) => self.error(Some(id), code, message),
                }
            }
            ClientMsg::ChatOpen { id, terminal } => self.open_chat(id, &terminal).await,
            ClientMsg::ChatClose { id, chat } => match self.chats.remove(&chat) {
                Some(handle) => {
                    self.host.chat_close(handle).await;
                    self.reply(id, Reply::Empty {});
                }
                None => self.error(Some(id), ErrorCode::NotFound, format!("no chat {chat}")),
            },
            ClientMsg::LoopStop { id, terminal } => match self.host.loop_stop(&terminal).await {
                Ok(info) => self.reply(id, Reply::Loop { info }),
                Err(e) => self.error(Some(id), task_code(&e), e.to_string()),
            },
            ClientMsg::Watch { id } => self.watch(id).await,
            ClientMsg::Routines { id } => match self.host.routines().await {
                Ok(list) => self.reply(
                    id,
                    Reply::Routines {
                        routines: list.routines,
                        problems: list.problems,
                    },
                ),
                Err(e) => self.error(Some(id), task_code(&e), e.to_string()),
            },
            ClientMsg::RunRoutine {
                id,
                workspace,
                name,
            } => match self.host.run_routine(self.backend, &workspace, &name).await {
                Ok(()) => self.reply(id, Reply::Empty {}),
                Err(e) => self.error(Some(id), task_code(&e), e.to_string()),
            },
            ClientMsg::TaskReview { id, terminal, note } => {
                match task_items::review(self.backend, self.inbox, &terminal, note).await {
                    Ok(()) => self.reply(
                        id,
                        Reply::Inbox {
                            inbox: self.inbox.view(),
                        },
                    ),
                    Err((code, message)) => self.error(Some(id), code, message),
                }
            }
            ClientMsg::ReaderList {
                id,
                terminal,
                kind,
                html,
            } => {
                let listed = match reader::folder(self.backend, &terminal).await {
                    Ok(dir) => self
                        .host
                        .reader_list(&dir, kind, html)
                        .await
                        .map_err(|e| (task_code(&e), e.to_string())),
                    Err(e) => Err(e),
                };
                match listed {
                    Ok(list) => self.reply(id, Reply::Listed { list }),
                    Err((code, message)) => self.error(Some(id), code, message),
                }
            }
            ClientMsg::ReaderOpen {
                id,
                terminal,
                kind,
                path,
                html,
                scope,
            } => {
                let open = ReaderOpen {
                    kind,
                    path,
                    html,
                    scope: scope.unwrap_or(DiffScope::Uncommitted),
                };
                self.open_reader(id, &terminal, open).await;
            }
            ClientMsg::ReaderClose { id, reader } => match self.readers.open.remove(&reader) {
                Some(_) => self.reply(id, Reply::Empty {}),
                None => self.error(Some(id), ErrorCode::NotFound, format!("no reader {reader}")),
            },
            ClientMsg::Workspaces { id } => match self.host.workspace_list().await {
                Ok(workspaces) => self.reply(id, Reply::Workspaces { workspaces }),
                Err(e) => self.error(Some(id), task_code(&e), e.to_string()),
            },
            ClientMsg::InboxApprove { id, item, worktree } => {
                let mode = worktree.unwrap_or_default();
                let done =
                    task_actions::approve(self.backend, self.host, self.inbox, &item, mode).await;
                self.inbox_reply(id, done);
            }
            ClientMsg::InboxRequestChanges { id, item, comment } => {
                let done =
                    task_actions::request_changes(self.backend, self.inbox, &item, &comment).await;
                self.inbox_reply(id, done);
            }
            ClientMsg::InboxRetry { id, item } => {
                let done = task_actions::retry(self.backend, self.host, self.inbox, &item).await;
                self.inbox_reply(id, done);
            }
            other => match inbox::handle(other, self.inbox, self.host).await {
                Some((id, Ok(body))) => self.reply(id, body),
                Some((id, Err((code, message)))) => self.error(Some(id), code, message),
                None => self.error(None, ErrorCode::BadRequest, "unknown request".into()),
            },
        }
    }

    async fn open_reader(&mut self, id: u64, terminal: &str, open: ReaderOpen) {
        let dir = match reader::folder(self.backend, terminal).await {
            Ok(dir) => dir,
            Err((code, message)) => return self.error(Some(id), code, message),
        };
        match self.host.reader_read(&dir, &open).await {
            Ok(content) => {
                let reader = self.readers.add(dir, open, &content);
                self.reply(id, Reply::Opened { reader, content });
            }
            Err(e) => self.error(Some(id), task_code(&e), e.to_string()),
        }
    }

    /// Sends each open file that changed, and ends the ones that cannot be
    /// read any more.
    async fn refresh_readers(&mut self) {
        let numbers: Vec<u32> = self.readers.open.keys().copied().collect();
        for reader in numbers {
            let Some(o) = self.readers.open.get(&reader) else {
                continue;
            };
            match self.host.reader_read(&o.dir, &o.open).await {
                Ok(content) => {
                    let stamp = reader::stamp(&content);
                    if let Some(o) = self.readers.open.get_mut(&reader)
                        && o.sent != stamp
                    {
                        o.sent = stamp;
                        send(&self.out, &ServerMsg::Reader { reader, content });
                    }
                }
                Err(e) => {
                    self.readers.open.remove(&reader);
                    let reason = e.to_string();
                    send(&self.out, &ServerMsg::ReaderEnded { reader, reason });
                }
            }
        }
    }

    async fn open_chat(&mut self, id: u64, terminal: &str) {
        let dir = match self.agent(terminal).await {
            Ok(t) => t.labels.get(tasks::DIR).map(std::path::PathBuf::from),
            Err((code, message)) => return self.error(Some(id), code, message),
        };
        let Some(dir) = dir else {
            return self.error(
                Some(id),
                ErrorCode::BadRequest,
                format!("{terminal} has no folder"),
            );
        };
        match self.host.chat_open(&dir).await {
            Ok((handle, update)) => {
                let chat = self.next_chat;
                self.next_chat += 1;
                self.chats.insert(chat, handle);
                self.reply(id, Reply::Chat { chat, update });
            }
            Err(e) => self.error(Some(id), task_code(&e), e.to_string()),
        }
    }

    /// Sends what changed in each followed conversation, and ends the ones
    /// that are over.
    async fn poll_chats(&mut self) {
        let open: Vec<(u32, u64)> = self.chats.iter().map(|(c, h)| (*c, *h)).collect();
        for (chat, handle) in open {
            match self.host.chat_poll(handle).await {
                Ok(Some(update)) => send(&self.out, &ServerMsg::Chat { chat, update }),
                Ok(None) => {}
                Err(e) => {
                    self.chats.remove(&chat);
                    self.host.chat_close(handle).await;
                    let reason = e.to_string();
                    send(&self.out, &ServerMsg::ChatEnded { chat, reason });
                }
            }
        }
    }

    /// `terminal` when it is a Task's agent.
    async fn agent(&self, terminal: &str) -> Result<TerminalInfo, (ErrorCode, String)> {
        let t = self
            .backend
            .get(&TerminalId::new(terminal.to_owned()))
            .await
            .map_err(|e| (code_of(&e), e.to_string()))?;
        if task_items::is_agent(&t) {
            Ok(t)
        } else {
            Err((
                ErrorCode::BadRequest,
                format!("{terminal} is not a Task's agent; start Tasks with start-task"),
            ))
        }
    }

    async fn start_task(&mut self, id: u64, spec: &TaskSpec, task_loop: Option<v2::LoopSpec>) {
        if let Some(l) = &task_loop
            && let Err(e) = self.host.loop_check(&spec.workspace, l).await
        {
            return self.error(Some(id), task_code(&e), e.to_string());
        }
        let prepared = match self.host.prepare(spec).await {
            Ok(p) => p,
            Err(e) => {
                let code = task_code(&e);
                self.error(Some(id), code, e.to_string());
                return;
            }
        };
        match tasks::start(self.backend, prepared).await {
            Ok((terminal, existing)) => {
                tracing::info!(terminal = %terminal.id, existing, "task agent");
                if let Some(l) = &task_loop
                    && let Err(e) = self.host.loop_start(&terminal, l, false).await
                {
                    let message = format!("the Task started, but its loop did not: {e}");
                    return self.error(Some(id), task_code(&e), message);
                }
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
                let mut revisions = self.inbox.subscribe();
                revisions.mark_unchanged();
                self.inbox_watch = Some(revisions);
                self.loop_watch = self.host.loop_changes().map(|mut r| {
                    r.mark_unchanged();
                    r
                });
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

    /// The whole Inbox after a change, or why it did not happen.
    fn inbox_reply(&self, id: u64, done: Result<(), (ErrorCode, String)>) {
        match done {
            Ok(()) => self.reply(
                id,
                Reply::Inbox {
                    inbox: self.inbox.view(),
                },
            ),
            Err((code, message)) => self.error(Some(id), code, message),
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

/// Waits for a new Inbox revision; forever when not watching or closed.
async fn next_revision(revisions: &mut Option<watch::Receiver<u64>>) {
    let changed = match revisions.as_mut() {
        Some(r) => r.changed().await.is_ok(),
        None => false,
    };
    if !changed {
        *revisions = None;
        std::future::pending::<()>().await;
    }
}

pub(crate) fn task_code(e: &TaskError) -> ErrorCode {
    match e {
        TaskError::Invalid(_) => ErrorCode::BadRequest,
        TaskError::Busy(_) => ErrorCode::Conflict,
        TaskError::Failed(_) => ErrorCode::Backend,
        TaskError::WouldLoseWork(_) => ErrorCode::WouldLoseWork,
    }
}
