//! One client connection: the handshake, then control requests on channel 0
//! and terminal frames on the channels the client attached.

use std::collections::HashMap;
use std::io::Read;

use muxa_backend::{BackendError, SessionBackend, Size, TerminalId, TerminalInfo};
use muxa_protocol::v2::{self, ClientMsg, ErrorCode, Frame, Kind, Reply, ServerMsg};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;

use crate::attachment::{self, Cmd};
use crate::wire::{self, Incoming};

/// The most scrollback lines one attach may ask for.
const MAX_HISTORY: u32 = 10_000;

pub struct Config {
    /// `muxa 0.1.0`, sent in `welcome`.
    pub server: String,
    /// `herdr-core 0.1.21`, sent in `welcome`.
    pub backend: String,
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

struct Session<'a, B: SessionBackend> {
    backend: &'a B,
    out: mpsc::UnboundedSender<Frame>,
    ended: mpsc::UnboundedSender<u32>,
    attachments: HashMap<u32, mpsc::UnboundedSender<Cmd>>,
    next_channel: u32,
}

/// Serves one client until it leaves or the connection breaks. Detaches every
/// terminal it attached; the programs keep running.
pub async fn serve<B, R, W>(
    backend: &B,
    reader: R,
    writer: W,
    config: &Config,
) -> Result<(), ServeError>
where
    B: SessionBackend,
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let mut incoming = wire::spawn_reader(reader);
    let out = wire::spawn_writer(writer);

    handshake(&mut incoming, &out, config).await?;

    let (ended, mut ended_rx) = mpsc::unbounded_channel();
    let mut session = Session {
        backend,
        out,
        ended,
        attachments: HashMap::new(),
        next_channel: 1,
    };
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
        }
    };
    for cmds in session.attachments.values() {
        let _ = cmds.send(Cmd::Detach);
    }
    result
}

async fn handshake(
    incoming: &mut mpsc::Receiver<Incoming>,
    out: &mpsc::UnboundedSender<Frame>,
    config: &Config,
) -> Result<(), ServeError> {
    let frame = match incoming.recv().await {
        Some(Incoming::Frame(f)) if f.channel == 0 && f.kind == Kind::Data => f,
        Some(Incoming::Broken(e)) => return Err(ServeError::Broken(e)),
        _ => return Err(ServeError::NoHello),
    };
    let version = match serde_json::from_slice::<ClientMsg>(&frame.payload) {
        Ok(ClientMsg::Hello { version, .. }) => version,
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
    send(
        out,
        &ServerMsg::Welcome {
            version: v2::VERSION,
            server: config.server.clone(),
            backend: config.backend.clone(),
            token: token().map_err(ServeError::Token)?,
            resumed: Vec::new(),
        },
    );
    Ok(())
}

/// 128 random bits as hex, for resume (slice B3).
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

impl<B: SessionBackend> Session<'_, B> {
    async fn frame(&mut self, frame: Frame) {
        if frame.channel == 0 {
            if frame.kind == Kind::Data {
                self.control(&frame.payload).await;
            }
            return;
        }
        let Some(cmds) = self.attachments.get(&frame.channel) else {
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
                Some(cmds) => {
                    let _ = cmds.send(Cmd::Detach);
                    self.reply(id, Reply::Empty {});
                }
                None => self.error(
                    Some(id),
                    ErrorCode::NotFound,
                    format!("no channel {channel}"),
                ),
            },
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
        let (cmds, cmd_rx) = mpsc::unbounded_channel();
        self.attachments.insert(channel, cmds);
        // Reply and paint are queued before any output can be.
        self.reply(id, Reply::Attached { channel });
        send(&self.out, &ServerMsg::Paint { channel });
        tracing::info!(channel, terminal = %tid, "attached");
        tokio::spawn(attachment::run(
            channel,
            stream,
            cmd_rx,
            self.out.clone(),
            self.ended.clone(),
        ));
    }

    fn reply(&self, id: u64, body: Reply) {
        send(&self.out, &ServerMsg::Reply { id, body });
    }

    fn error(&self, id: Option<u64>, code: ErrorCode, message: String) {
        send(&self.out, &ServerMsg::Error { id, code, message });
    }
}
