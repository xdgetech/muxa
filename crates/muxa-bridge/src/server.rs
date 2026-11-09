//! One client session: hello, then requests in order until the client
//! leaves, goes idle, or tmux exits.

use std::sync::Arc;
use std::time::Duration;

use muxa_protocol::{ErrorBody, ErrorCode, Message, Request};
use muxa_tmux::{Client, Options, PaneId};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tokio::sync::{mpsc, oneshot};

use crate::agents::Registry;
use crate::changes::{self, ChangeTask};
use crate::chat::{self, ChatTask};
use crate::convert::{self, error_body};
use crate::docs::{self, DocTask, ImageTask};
use crate::lines::{self, Line};
use crate::task_records;
use crate::tasks::{self, TaskError};
use crate::watch::{Command, Watcher};

/// The largest request line, e.g. a paste.
pub const MAX_REQUEST: usize = 4 << 20;
/// Messages the writer may queue. Output beyond this is dropped and the pane
/// resynced, so a slow client never stalls tmux.
const OUT_QUEUE: usize = 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const FLUSH_TIMEOUT: Duration = Duration::from_secs(2);
/// After the client leaves: a launch (60 s) and the wait for an idle agent (30 s).
const PENDING_LIMIT: Duration = Duration::from_secs(100);

pub struct Config {
    pub tmux: Options,
    pub idle: Duration,
    pub agents: Arc<Registry>,
    pub tasks: Arc<tasks::Config>,
    pub records: Arc<task_records::Config>,
    /// Where the host daemon's socket is; record requests go there when it runs.
    pub daemon_dir: Option<std::path::PathBuf>,
}

/// Why the session ended.
#[derive(Debug, PartialEq, Eq)]
pub enum End {
    ClientClosed,
    ClientBye,
    Idle,
    TmuxExited,
    WriterFailed,
    /// SIGTERM.
    Terminated,
}

pub async fn serve<R, W>(config: Config, input: R, output: W) -> End
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (out, out_rx) = mpsc::channel(OUT_QUEUE);
    let writer = tokio::spawn(write_loop(out_rx, output));
    let end = session(config, input, out).await;
    // All senders are gone now, so the writer drains and stops.
    let _ = tokio::time::timeout(FLUSH_TIMEOUT, writer).await;
    end
}

async fn session<R>(config: Config, input: R, out: mpsc::Sender<Message>) -> End
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let (client, notes) = match Client::attach(&config.tmux, None) {
        Ok(c) => c,
        Err(e) => return bye(&out, format!("cannot start tmux: {e}")).await,
    };
    let client = Arc::new(client);
    let tmux_version = match tokio::time::timeout(
        COMMAND_TIMEOUT,
        client.command("display-message -p \"#{version}\""),
    )
    .await
    {
        Ok(Ok(reply)) => reply
            .lines
            .first()
            .map(|l| String::from_utf8_lossy(l).into_owned())
            .unwrap_or_default(),
        Ok(Err(e)) => return bye(&out, e.to_string()).await,
        Err(_) => return bye(&out, "tmux did not answer".into()).await,
    };
    let mut capabilities: Vec<String> = [
        "chat",
        "chat-prompt",
        "docs",
        "tasks",
        "changes",
        "close-task",
        "images",
        "branch-changes",
        "viewport",
        "history",
    ]
    .map(String::from)
    .into();
    if config.records.enabled() {
        capabilities.push("task-records".into());
        capabilities.push("task-handoff".into());
        capabilities.push("task-restart-recovery".into());
        capabilities.push("tasks-changed".into());
        capabilities.push("workers".into());
        // Levels add up: 4 has the Inbox items of 3 (ADR 0033) and reading Tasks (ADR 0034),
        // 5 closes ask sessions (ADR 0039), 6 asks the workspace's agent from an item (ADR 0041),
        // 7 removes a Task's worktree at its end (ADR 0043), 8 keeps Captures and Work items (ADR 0046).
        capabilities.push(muxa_protocol::work_capability(
            muxa_protocol::WORK_ASSISTANT,
        ));
    }
    // Checked once per session (ADR 0019); each record request looks again.
    if let Some(dir) = &config.daemon_dir {
        let daemon = task_records::remote::probe(dir).await;
        tracing::info!(?daemon, "host daemon");
        // A daemon of this version runs the loop engine.
        if config.records.enabled() && daemon == task_records::remote::Probe::Ready {
            capabilities.push("task-loop".into());
        }
    }
    let hello = Message::Hello {
        protocol: muxa_protocol::VERSION,
        bridge: env!("CARGO_PKG_VERSION").into(),
        tmux: tmux_version,
        capabilities,
    };
    if out.send(hello).await.is_err() {
        return End::WriterFailed;
    }

    // A client with no target may land on a device session (ADR 0021).
    if let Err(e) = muxa_tmux::leave_device_session(&client).await {
        tracing::warn!(error = %e, "leaving a device session failed");
    }
    crate::sizer::remove_leftovers(&client).await;

    let (cmd_tx, cmd_rx) = mpsc::channel(8);
    let stop_watch = cmd_tx.clone();
    let mut watcher = tokio::spawn(
        Watcher::new(
            Arc::clone(&client),
            config.tmux.clone(),
            out.clone(),
            notes,
            cmd_rx,
        )
        .run(),
    );
    // Not in the select below: a chat ending is not tmux exiting.
    let (chat_tx, chat_rx) = mpsc::channel(8);
    let chats = tokio::spawn(
        ChatTask::new(
            Arc::clone(&client),
            out.clone(),
            chat_rx,
            Arc::clone(&config.agents),
            Arc::clone(&config.records),
        )
        .run(),
    );
    let (doc_tx, doc_rx) = mpsc::channel(8);
    let docs = tokio::spawn(
        DocTask::new(
            Arc::clone(&client),
            out.clone(),
            doc_rx,
            Arc::clone(&config.agents),
            Arc::clone(&config.records),
        )
        .run(),
    );
    let (image_tx, image_rx) = mpsc::channel(8);
    let images = tokio::spawn(
        ImageTask::new(
            Arc::clone(&client),
            out.clone(),
            image_rx,
            Arc::clone(&config.records),
        )
        .run(),
    );
    let (change_tx, change_rx) = mpsc::channel(8);
    let changes = tokio::spawn(ChangeTask::new(Arc::clone(&client), out.clone(), change_rx).run());
    let task_watch = config
        .records
        .state_dir
        .clone()
        .map(|dir| tokio::spawn(crate::task_watch::run(dir, out.clone())));
    let states = tokio::spawn(crate::states::run(
        Arc::clone(&client),
        out.clone(),
        Arc::clone(&config.agents),
        Arc::clone(&config.records),
    ));
    let (req_tx, req_rx) = mpsc::channel(64);
    let reader = tokio::spawn(read_requests(input, req_tx, out.clone()));
    let pending = task_records::Pending::default();
    let worker = Worker {
        client,
        out: out.clone(),
        watch: cmd_tx,
        chat: chat_tx,
        docs: doc_tx,
        images: image_tx,
        changes: change_tx,
        tasks: Arc::clone(&config.tasks),
        agents: Arc::clone(&config.agents),
        records: Arc::clone(&config.records),
        daemon_dir: config.daemon_dir.clone(),
        pending: pending.clone(),
    };

    let end = tokio::select! {
        end = worker.run(req_rx, config.idle) => end,
        _ = &mut watcher => End::TmuxExited,
        () = terminated() => End::Terminated,
    };
    reader.abort();
    // The watcher gives the window back and ends its device session.
    let (ack_tx, ack_rx) = oneshot::channel();
    if stop_watch.send(Command::Stop(ack_tx)).await.is_ok() {
        let _ = tokio::time::timeout(COMMAND_TIMEOUT, ack_rx).await;
    }
    watcher.abort();
    chats.abort();
    docs.abort();
    images.abort();
    changes.abort();
    states.abort();
    if let Some(w) = task_watch {
        w.abort();
    }
    if end == End::Idle {
        let _ = out
            .send(Message::Bye {
                reason: "idle timeout".into(),
            })
            .await;
    }
    pending.finish(PENDING_LIMIT).await;
    end
}

/// Resolves on SIGTERM, e.g. at logout or when sshd stops the session.
async fn terminated() {
    use tokio::signal::unix::{SignalKind, signal};
    match signal(SignalKind::terminate()) {
        Ok(mut term) => {
            term.recv().await;
        }
        Err(e) => {
            tracing::warn!(error = %e, "cannot watch for SIGTERM");
            std::future::pending::<()>().await;
        }
    }
}

async fn bye(out: &mpsc::Sender<Message>, reason: String) -> End {
    tracing::error!(%reason, "bridge exiting");
    let _ = out.send(Message::Bye { reason }).await;
    End::TmuxExited
}

/// Parses request lines. Unreadable ones are answered here, so the worker
/// only sees valid requests.
async fn read_requests<R: AsyncRead + Unpin>(
    input: R,
    tx: mpsc::Sender<Request>,
    out: mpsc::Sender<Message>,
) {
    let mut input = BufReader::new(input);
    loop {
        let line = match lines::next(&mut input, MAX_REQUEST).await {
            Ok(Some(Line::Text(line))) => line,
            Ok(Some(Line::TooLong)) => {
                let error = error_body(
                    ErrorCode::TooLarge,
                    format!("request longer than {MAX_REQUEST} bytes"),
                );
                let _ = out
                    .send(Message::Result {
                        id: None,
                        error: Some(error),
                        sessions: None,
                    })
                    .await;
                continue;
            }
            Ok(None) | Err(_) => return,
        };
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        match serde_json::from_slice::<Request>(&line) {
            Ok(req) => {
                if tx.send(req).await.is_err() {
                    return;
                }
            }
            Err(e) => {
                // Echo the id when the line is JSON with one, even if the rest is wrong.
                let id = serde_json::from_slice::<serde_json::Value>(&line)
                    .ok()
                    .and_then(|v| v.get("id")?.as_u64());
                let error = error_body(ErrorCode::BadRequest, e.to_string());
                let _ = out
                    .send(Message::Result {
                        id,
                        error: Some(error),
                        sessions: None,
                    })
                    .await;
            }
        }
    }
}

struct Worker {
    client: Arc<Client>,
    out: mpsc::Sender<Message>,
    watch: mpsc::Sender<Command>,
    chat: mpsc::Sender<chat::Command>,
    docs: mpsc::Sender<docs::Command>,
    images: mpsc::Sender<docs::Command>,
    changes: mpsc::Sender<changes::Command>,
    tasks: Arc<tasks::Config>,
    agents: Arc<Registry>,
    records: Arc<task_records::Config>,
    daemon_dir: Option<std::path::PathBuf>,
    pending: task_records::Pending,
}

impl Worker {
    /// Runs requests one at a time, so keystrokes reach tmux in order.
    async fn run(self, mut rx: mpsc::Receiver<Request>, idle: Duration) -> End {
        loop {
            let req = match tokio::time::timeout(idle, rx.recv()).await {
                Err(_) => return End::Idle,
                Ok(None) => return End::ClientClosed,
                Ok(Some(req)) => req,
            };
            let id = req.id();
            let bye = matches!(req, Request::Bye { .. });
            // Watch results are sent by the watcher, ahead of the first screen.
            if let Some(result) = self.handle(req).await {
                let error = result.err();
                if self
                    .out
                    .send(Message::Result {
                        id: Some(id),
                        error,
                        sessions: None,
                    })
                    .await
                    .is_err()
                {
                    return End::WriterFailed;
                }
            }
            if bye {
                return End::ClientBye;
            }
        }
    }

    async fn handle(&self, req: Request) -> Option<Result<(), ErrorBody>> {
        let client = &*self.client;
        let result = match req {
            Request::Ping { .. } => timed(client.command("display-message -p ok"))
                .await
                .map(drop),
            Request::List { id } => {
                return match timed(muxa_tmux::list_sessions(client)).await {
                    Ok(mut sessions) => {
                        let agents = Arc::clone(&self.agents);
                        let records = Arc::clone(&self.records);
                        let sessions = tokio::task::spawn_blocking(move || {
                            crate::sessions::enrich(&mut sessions, &agents);
                            task_records::observation::refresh(&mut sessions, &records, &agents);
                            sessions
                        })
                        .await;
                        let Ok(sessions) = sessions else {
                            return Some(Err(error_body(
                                ErrorCode::Internal,
                                "listing sessions failed",
                            )));
                        };
                        let msg = Message::Result {
                            id: Some(id),
                            error: None,
                            sessions: Some(convert::sessions(sessions)),
                        };
                        let _ = self.out.send(msg).await;
                        None
                    }
                    Err(e) => Some(Err(convert::error(&e))),
                };
            }
            Request::Watch { id, pane } => {
                return match pane.parse::<PaneId>() {
                    Ok(pane) => {
                        self.to_watcher(|ack| Command::Watch { id, pane, ack })
                            .await
                    }
                    Err(e) => Some(Err(convert::error(&e))),
                };
            }
            Request::Unwatch { id } => {
                return self.to_watcher(|ack| Command::Unwatch { id, ack }).await;
            }
            Request::History { id, pane, lines } => {
                return Some(self.history(id, &pane, lines).await);
            }
            Request::Viewport { cols, rows, .. } => {
                let Some(grid) = crate::viewport::valid(cols, rows) else {
                    return Some(Err(error_body(
                        ErrorCode::BadRequest,
                        "viewport: cols must be 10 to 1000 and rows 5 to 1000",
                    )));
                };
                if self.watch.send(Command::Viewport(grid)).await.is_err() {
                    return Some(Err(error_body(ErrorCode::Internal, "pane watcher stopped")));
                }
                Ok(())
            }
            // Chat results come from the chat task, ahead of the first entries.
            Request::Chat { id, pane } => {
                return match pane.parse::<PaneId>() {
                    Ok(pane) => {
                        handoff(
                            &self.chat,
                            |ack| chat::Command::Start { id, pane, ack },
                            "chat",
                        )
                        .await
                    }
                    Err(e) => Some(Err(convert::error(&e))),
                };
            }
            // Document results come from the docs task, after the list or ahead of the content.
            Request::Docs { id, pane } => {
                return match pane.parse::<PaneId>() {
                    Ok(pane) => {
                        handoff(
                            &self.docs,
                            |ack| docs::Command::List { id, pane, ack },
                            "docs",
                        )
                        .await
                    }
                    Err(e) => Some(Err(convert::error(&e))),
                };
            }
            Request::Doc { id, pane, path } => {
                return match pane.parse::<PaneId>() {
                    Ok(pane) => {
                        handoff(
                            &self.docs,
                            |ack| docs::Command::Open {
                                id,
                                pane,
                                path,
                                ack,
                            },
                            "docs",
                        )
                        .await
                    }
                    Err(e) => Some(Err(convert::error(&e))),
                };
            }
            Request::Changes { id, pane } => {
                return match pane.parse::<PaneId>() {
                    Ok(pane) => {
                        handoff(
                            &self.changes,
                            |ack| changes::Command::List { id, pane, ack },
                            "changes",
                        )
                        .await
                    }
                    Err(e) => Some(Err(convert::error(&e))),
                };
            }
            Request::Diff {
                id,
                pane,
                path,
                scope,
            } => {
                return match pane.parse::<PaneId>() {
                    Ok(pane) => {
                        handoff(
                            &self.changes,
                            |ack| changes::Command::Open {
                                id,
                                pane,
                                path,
                                scope,
                                ack,
                            },
                            "changes",
                        )
                        .await
                    }
                    Err(e) => Some(Err(convert::error(&e))),
                };
            }
            Request::Undiff { id } => {
                return handoff(
                    &self.changes,
                    |ack| changes::Command::Close { id, ack },
                    "changes",
                )
                .await;
            }
            Request::Images { id, pane } => {
                return match pane.parse::<PaneId>() {
                    Ok(pane) => {
                        handoff(
                            &self.images,
                            |ack| docs::Command::List { id, pane, ack },
                            "images",
                        )
                        .await
                    }
                    Err(e) => Some(Err(convert::error(&e))),
                };
            }
            Request::Image { id, pane, path } => {
                return match pane.parse::<PaneId>() {
                    Ok(pane) => {
                        handoff(
                            &self.images,
                            |ack| docs::Command::Open {
                                id,
                                pane,
                                path,
                                ack,
                            },
                            "images",
                        )
                        .await
                    }
                    Err(e) => Some(Err(convert::error(&e))),
                };
            }
            Request::Unimage { id } => {
                return handoff(
                    &self.images,
                    |ack| docs::Command::Close { id, ack },
                    "images",
                )
                .await;
            }
            Request::Undoc { id } => {
                return handoff(&self.docs, |ack| docs::Command::Close { id, ack }, "docs").await;
            }
            // A launch takes seconds; keystrokes must not wait for it.
            Request::NewTask {
                id,
                repo,
                branch,
                base,
            } => {
                let client = Arc::clone(&self.client);
                let config = Arc::clone(&self.tasks);
                let records = Arc::clone(&self.records);
                let out = self.out.clone();
                tokio::spawn(async move {
                    let started =
                        start_task(&client, &config, &records, &repo, &branch, base.as_deref())
                            .await;
                    let error = match started {
                        Ok(launched) => {
                            let session = launched.name;
                            let _ = out.send(Message::TaskCreated { task: id, session }).await;
                            None
                        }
                        Err(e) => Some(convert::task_error(&e)),
                    };
                    let _ = out
                        .send(Message::Result {
                            id: Some(id),
                            error,
                            sessions: None,
                        })
                        .await;
                });
                return None;
            }
            Request::StartSession {
                id,
                workspace,
                repo,
            } => {
                let client = Arc::clone(&self.client);
                let config = Arc::clone(&self.tasks);
                let records = Arc::clone(&self.records);
                let out = self.out.clone();
                self.pending.spawn(async move {
                    let started =
                        task_records::start_session(&client, &config, &records, &workspace, &repo)
                            .await;
                    let error = match started {
                        Ok(launched) => {
                            let msg = Message::SessionStarted {
                                start: id,
                                session: launched.id.to_string(),
                                name: launched.name,
                            };
                            let _ = out.send(msg).await;
                            None
                        }
                        Err(e) => Some(e),
                    };
                    let _ = out
                        .send(Message::Result {
                            id: Some(id),
                            error,
                            sessions: None,
                        })
                        .await;
                });
                return None;
            }
            Request::CloseTask { id, session, mode } => {
                let client = Arc::clone(&self.client);
                let config = Arc::clone(&self.tasks);
                let records = Arc::clone(&self.records);
                let out = self.out.clone();
                tokio::spawn(async move {
                    let error =
                        task_records::close::close_task(&client, &config, &records, &session, mode)
                            .await
                            .err();
                    let _ = out
                        .send(Message::Result {
                            id: Some(id),
                            error,
                            sessions: None,
                        })
                        .await;
                });
                return None;
            }
            Request::Unchat { id } => {
                return handoff(&self.chat, |ack| chat::Command::Stop { id, ack }, "chat").await;
            }
            Request::Keys { pane, keys, .. } => match self.typed(&pane) {
                Ok(p) => timed(muxa_tmux::send_keys(client, p, &keys)).await,
                Err(e) => Err(e),
            },
            Request::Text { pane, text, .. } => match self.typed(&pane) {
                Ok(p) => timed(muxa_tmux::send_text(client, p, &text)).await,
                Err(e) => Err(e),
            },
            Request::Bytes { pane, data, .. } => match self.typed(&pane) {
                Ok(p) => timed(muxa_tmux::send_bytes(client, p, &data)).await,
                Err(e) => Err(e),
            },
            Request::Paste { pane, text, .. } => match self.typed(&pane) {
                Ok(p) => timed(muxa_tmux::paste(client, p, &text)).await,
                Err(e) => Err(e),
            },
            // Only the daemon runs loops (ADR 0019) and assistant requests
            // (ADR 0046 §8). A create takes seconds.
            req @ (Request::CreateTask {
                r#loop: Some(_), ..
            }
            | Request::StartLoop { .. }
            | Request::StopLoop { .. }
            | Request::RunRoutine { .. }
            | Request::RequestAssistant { .. }
            | Request::CancelRequest { .. }) => {
                let (dir, out) = (self.daemon_dir.clone(), self.out.clone());
                self.pending.spawn(async move {
                    for msg in task_records::remote::serve_loop(dir.as_deref(), req).await {
                        let _ = out.send(msg).await;
                    }
                });
                return None;
            }
            // A launch or a restart takes seconds; keystrokes must not wait for it.
            req @ (Request::CreateTask { .. }
            | Request::RestartAgent { .. }
            | Request::BranchTask { .. }
            | Request::ParkTask { .. }
            | Request::RecoverRestart { .. }
            | Request::UpdateInbox { .. }
            | Request::Capture { .. }
            | Request::SortCapture { .. }
            | Request::UpdateWork { .. }) => {
                let (cx, dir, out) = (
                    self.records_context(),
                    self.daemon_dir.clone(),
                    self.out.clone(),
                );
                self.pending.spawn(async move {
                    for msg in task_records::remote::serve(&cx, dir.as_deref(), req).await {
                        let _ = out.send(msg).await;
                    }
                });
                return None;
            }
            req @ (Request::ListTasks { .. }
            | Request::UpdateTask { .. }
            | Request::ListRoutines { .. }
            | Request::ListInbox { .. }) => {
                let answer = task_records::remote::serve(
                    &self.records_context(),
                    self.daemon_dir.as_deref(),
                    req,
                )
                .await;
                for msg in answer {
                    if self.out.send(msg).await.is_err() {
                        return Some(Err(error_body(ErrorCode::Internal, "the writer stopped")));
                    }
                }
                return None;
            }
            Request::Bye { .. } => Ok(()),
        };
        Some(result.map_err(|e| convert::error(&e)))
    }

    /// Parses an input request's pane, and tells the watcher the client typed,
    /// without waiting: keystrokes never wait for a resize.
    fn typed(&self, pane: &str) -> Result<PaneId, muxa_tmux::Error> {
        let pane = pane.parse()?;
        let _ = self.watch.try_send(Command::Typed(pane));
        Ok(pane)
    }

    fn records_context(&self) -> task_records::Context {
        task_records::Context {
            tmux: Arc::new(task_records::Attached(Arc::clone(&self.client))),
            tasks: Arc::clone(&self.tasks),
            records: Arc::clone(&self.records),
            agents: Arc::clone(&self.agents),
            pending: self.pending.clone(),
            daemon: false,
        }
    }

    /// Sends the pane's scrollback and screen, then the caller sends the `result`.
    async fn history(&self, id: u64, pane: &str, lines: Option<u32>) -> Result<(), ErrorBody> {
        let pane = pane.parse::<PaneId>().map_err(|e| convert::error(&e))?;
        let lines = lines.unwrap_or(muxa_protocol::DEFAULT_HISTORY_LINES);
        if !(1..=muxa_protocol::MAX_HISTORY_LINES).contains(&lines) {
            return Err(error_body(
                ErrorCode::BadRequest,
                format!(
                    "history: lines must be 1 to {}",
                    muxa_protocol::MAX_HISTORY_LINES
                ),
            ));
        }
        let history = timed(muxa_tmux::history(&self.client, pane, lines))
            .await
            .map_err(|e| convert::error(&e))?;
        let msg = Message::History {
            history: id,
            pane: pane.to_string(),
            cols: history.cols,
            data: history.data,
            truncated: history.truncated,
        };
        self.out
            .send(msg)
            .await
            .map_err(|_| error_body(ErrorCode::Internal, "the client left"))
    }

    async fn to_watcher(
        &self,
        make: impl FnOnce(oneshot::Sender<()>) -> Command,
    ) -> Option<Result<(), ErrorBody>> {
        handoff(&self.watch, make, "pane watcher").await
    }
}

async fn start_task(
    client: &Client,
    config: &tasks::Config,
    records: &task_records::Config,
    repo: &str,
    branch: &str,
    base: Option<&str>,
) -> Result<tasks::Launched, TaskError> {
    let mut req = tasks::validate(repo, branch, base).await?;
    req.agent = task_records::agent_for_repo(records, &req.repo).await;
    let launcher = tasks::pick_for(config, client, &req).await;
    let result = launcher.launch(client, &req).await;
    match &result {
        Ok(_) => tracing::info!(launcher = launcher.id(), "task started"),
        Err(e) => tracing::warn!(launcher = launcher.id(), error = %e, "task failed"),
    }
    result
}

/// Hands a request to a task that sends its own result, and waits until it has,
/// so later requests stay in order.
async fn handoff<C>(
    to: &mpsc::Sender<C>,
    make: impl FnOnce(oneshot::Sender<()>) -> C,
    what: &str,
) -> Option<Result<(), ErrorBody>> {
    let (ack, done) = oneshot::channel();
    if to.send(make(ack)).await.is_err() || done.await.is_err() {
        return Some(Err(error_body(
            ErrorCode::Internal,
            format!("{what} stopped"),
        )));
    }
    None
}

async fn timed<T>(
    f: impl Future<Output = Result<T, muxa_tmux::Error>>,
) -> Result<T, muxa_tmux::Error> {
    tokio::time::timeout(COMMAND_TIMEOUT, f)
        .await
        .unwrap_or_else(|_| {
            Err(muxa_tmux::Error::Exited(
                "tmux did not answer in time".into(),
            ))
        })
}

/// Writes one JSON line per message, flushing whenever the queue is empty.
async fn write_loop<W: AsyncWrite + Unpin>(mut rx: mpsc::Receiver<Message>, output: W) {
    let mut w = BufWriter::new(output);
    while let Some(msg) = rx.recv().await {
        let mut batch = vec![msg];
        while let Ok(more) = rx.try_recv() {
            batch.push(more);
        }
        for msg in batch {
            let mut line = match serde_json::to_vec(&msg) {
                Ok(line) => line,
                Err(e) => {
                    tracing::error!(error = %e, "cannot encode a message");
                    continue;
                }
            };
            line.push(b'\n');
            if w.write_all(&line).await.is_err() {
                return; // the client is gone (broken pipe)
            }
        }
        if w.flush().await.is_err() {
            return;
        }
    }
    let _ = w.flush().await;
}
