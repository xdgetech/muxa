//! Follows the conversation of the agent in one pane, through whichever agent
//! adapter knows it, and streams `chat-entries`. Agent-neutral: it owns the
//! sequence numbers, the resend after an overflow, and the pane checks.

use std::sync::Arc;
use std::time::Duration;

use muxa_protocol::{Agent, ChatEntry, ChatPrompt, ErrorBody, ErrorCode, Message};
use muxa_tmux::{Client, PaneId};
use tokio::sync::{mpsc, oneshot};

use crate::agents::{ChatEvent, ChatSource, PaneInfo, Registry};
use crate::convert::{self, error_body};
use crate::task_records::observation::ChatBinding;

const TICK: Duration = Duration::from_millis(500);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

pub enum Command {
    Start {
        id: u64,
        pane: PaneId,
        ack: oneshot::Sender<()>,
    },
    Stop {
        id: u64,
        ack: oneshot::Sender<()>,
    },
}

pub struct ChatTask {
    client: Arc<Client>,
    out: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Command>,
    registry: Arc<Registry>,
    records: Arc<crate::task_records::Config>,
    active: Option<Active>,
}

struct Active {
    chat: u64,
    pane: PaneId,
    info: PaneInfo,
    binding: Option<ChatBinding>,
    agent: &'static str,
    /// Out on the blocking pool while a poll runs.
    source: Option<Box<dyn ChatSource>>,
    seq: u64,
    status: Option<Agent>,
    prompt: Option<ChatPrompt>,
    /// A message was dropped: send everything again as a reset.
    resync: bool,
}

impl Active {
    async fn refresh(
        &mut self,
        info: PaneInfo,
        binding: Option<ChatBinding>,
        registry: Arc<Registry>,
    ) -> Result<(), ErrorBody> {
        let replaced = self.info.pid != info.pid
            || self.info.session != info.session
            || self.binding != binding;
        self.info = info;
        self.binding = binding;
        if !replaced {
            return Ok(());
        }
        // A retained transcript is not evidence about the replacement process.
        self.source = None;
        self.status = None;
        self.prompt = None;
        let probe = self.info.clone();
        let rollout = self.binding.as_ref().and_then(|b| b.rollout.clone());
        let opened = tokio::task::spawn_blocking(move || open(&registry, &probe, rollout))
            .await
            .map_err(|_| error_body(ErrorCode::Internal, "cannot reopen the agent conversation"))?
            .map_err(|_| error_body(ErrorCode::Internal, "cannot reopen the agent conversation"))?;
        let Some((agent, source)) = opened else {
            return Err(error_body(
                ErrorCode::Unsupported,
                "the replacement agent is not yet known",
            ));
        };
        check_agent(self.binding.as_ref(), agent)?;
        self.agent = agent;
        self.source = Some(source);
        self.resync = true;
        Ok(())
    }
}

impl ChatTask {
    pub fn new(
        client: Arc<Client>,
        out: mpsc::Sender<Message>,
        rx: mpsc::Receiver<Command>,
        registry: Arc<Registry>,
        records: Arc<crate::task_records::Config>,
    ) -> Self {
        Self {
            client,
            out,
            rx,
            registry,
            records,
            active: None,
        }
    }

    pub async fn run(mut self) {
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                cmd = self.rx.recv() => match cmd {
                    Some(cmd) => self.command(cmd).await,
                    None => return,
                },
                _ = tick.tick() => {
                    if self.active.is_some() {
                        self.tick().await;
                    }
                }
            }
        }
    }

    async fn command(&mut self, cmd: Command) {
        match cmd {
            Command::Start { id, pane, ack } => {
                self.active = None;
                let error = self.start(id, pane).await.err();
                let _ = self
                    .out
                    .send(Message::Result {
                        id: Some(id),
                        error,
                        sessions: None,
                    })
                    .await;
                let _ = ack.send(());
                // The first poll sends the reset right after the result.
                if self.active.is_some() {
                    self.poll().await;
                }
            }
            Command::Stop { id, ack } => {
                self.active = None;
                let _ = self
                    .out
                    .send(Message::Result {
                        id: Some(id),
                        error: None,
                        sessions: None,
                    })
                    .await;
                let _ = ack.send(());
            }
        }
    }

    async fn start(&mut self, chat: u64, pane: PaneId) -> Result<(), ErrorBody> {
        let (info, binding) = checked_pane_info(
            &self.client,
            pane,
            Arc::clone(&self.records),
            Arc::clone(&self.registry),
        )
        .await?;
        let registry = Arc::clone(&self.registry);
        let probe = info.clone();
        let rollout = binding.as_ref().and_then(|b| b.rollout.clone());
        let opened = tokio::task::spawn_blocking(move || open(&registry, &probe, rollout))
            .await
            .map_err(|e| error_body(ErrorCode::Internal, e.to_string()))?;
        match opened {
            Ok(Some((agent, source))) => {
                check_agent(binding.as_ref(), agent)?;
                self.active = Some(Active {
                    chat,
                    pane,
                    info,
                    binding,
                    agent,
                    source: Some(source),
                    seq: 0,
                    status: None,
                    prompt: None,
                    resync: false,
                });
                Ok(())
            }
            Ok(None) => Err(error_body(
                ErrorCode::Unsupported,
                format!("no agent adapter knows the program in pane {pane}"),
            )),
            Err(e) => {
                tracing::warn!(%pane, error = %e, "cannot open the agent's conversation");
                Err(error_body(ErrorCode::Internal, e.to_string()))
            }
        }
    }

    async fn tick(&mut self) {
        self.poll().await;
    }

    /// Proves the pane's identity again and follows a replacement. False
    /// when the chat ended.
    async fn confirm(&mut self, pane: PaneId) -> bool {
        let (info, binding) = match checked_pane_info(
            &self.client,
            pane,
            Arc::clone(&self.records),
            Arc::clone(&self.registry),
        )
        .await
        {
            Ok(info) => info,
            Err(error) => {
                self.end(error.message).await;
                return false;
            }
        };
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        if let Err(error) = active
            .refresh(info, binding, Arc::clone(&self.registry))
            .await
        {
            self.end(error.message).await;
            return false;
        }
        true
    }

    /// Polls the source on the blocking pool and sends what changed.
    async fn poll(&mut self) {
        let Some(pane) = self.active.as_ref().map(|active| active.pane) else {
            return;
        };
        if !self.confirm(pane).await {
            return;
        }
        let Some(active) = self.active.as_mut() else {
            return;
        };
        let Some(mut source) = active.source.take() else {
            return;
        };
        let info = active.info.clone();
        let resync = std::mem::take(&mut active.resync);
        let joined = tokio::task::spawn_blocking(move || {
            let events = source.poll(&info);
            let all = resync.then(|| source.entries());
            let status = source.status();
            let prompt = source.prompt();
            (source, events, all, status, prompt)
        })
        .await;
        let (source, events, all, status, prompt) = match joined {
            Ok(parts) => parts,
            Err(e) => return self.end(format!("the agent reader failed: {e}")).await,
        };
        let events = match events {
            Ok(events) => events,
            Err(e) => {
                tracing::warn!(pane = %info_pane(&self.active), error = %e, "chat ended");
                return self.end(e.to_string()).await;
            }
        };
        // Entries written after the identity changed, during the read, must
        // not reach the client: prove it again before sending.
        if !self.confirm(pane).await {
            return;
        }
        let Some(active) = self.active.as_mut() else {
            return;
        };
        if active.source.is_some() {
            // A replacement opened meanwhile; its first poll resends everything.
            return;
        }
        active.source = Some(source);
        let changed = status != active.status || prompt != active.prompt;
        active.status = status;
        active.prompt = prompt;
        let messages = batch(active, all, events, changed);
        for msg in messages {
            match self.out.try_send(msg) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    // The client is slow; the next tick sends everything again.
                    if let Some(a) = self.active.as_mut() {
                        a.resync = true;
                    }
                    return;
                }
                Err(mpsc::error::TrySendError::Closed(_)) => return,
            }
        }
    }

    async fn end(&mut self, reason: String) {
        if let Some(active) = self.active.take() {
            let _ = self
                .out
                .send(Message::ChatEnded {
                    chat: active.chat,
                    reason,
                })
                .await;
        }
    }
}

/// The pane's conversation: an owned Codex thread's rollout when the
/// binding names one (ADR 0028), else the first adapter that sees its agent.
fn open(
    registry: &Registry,
    pane: &PaneInfo,
    rollout: Option<std::path::PathBuf>,
) -> Result<Option<crate::agents::Opened>, crate::agents::Error> {
    match rollout {
        Some(path) => Ok(Some((
            crate::agents::codex::ID,
            Box::new(crate::agents::codex::CodexSource::owned(path)),
        ))),
        None => registry.open(pane),
    }
}

/// A managed restart must prove identity again before opening or polling a source.
async fn checked_pane_info(
    client: &Client,
    pane: PaneId,
    records: Arc<crate::task_records::Config>,
    registry: Arc<Registry>,
) -> Result<(PaneInfo, Option<ChatBinding>), ErrorBody> {
    let mut sessions = tokio::time::timeout(COMMAND_TIMEOUT, muxa_tmux::list_sessions(client))
        .await
        .map_err(|_| error_body(ErrorCode::Tmux, "tmux did not answer in time"))?
        .map_err(|error| convert::error(&error))?;
    tokio::task::spawn_blocking(move || {
        let binding = crate::task_records::observation::chat_binding(
            &mut sessions,
            pane,
            &records,
            &registry,
        )
        .map_err(|_| {
            error_body(
                ErrorCode::Unsupported,
                "the restarted agent conversation is not yet confirmed; continue in Terminal",
            )
        })?;
        sessions
            .iter()
            .find_map(|session| {
                session
                    .panes
                    .iter()
                    .find(|candidate| candidate.id == pane)
                    .map(|candidate| crate::sessions::info(&session.name, candidate))
            })
            .ok_or_else(|| convert::error(&muxa_tmux::Error::NoPane(pane)))
            .map(|info| (info, binding))
    })
    .await
    .map_err(|_| error_body(ErrorCode::Internal, "cannot check the agent conversation"))?
}

pub(crate) async fn pane_info(client: &Client, pane: PaneId) -> Result<PaneInfo, ErrorBody> {
    let found = tokio::time::timeout(COMMAND_TIMEOUT, muxa_tmux::pane(client, pane))
        .await
        .map_err(|_| error_body(ErrorCode::Tmux, "tmux did not answer in time"))?;
    let (session, p) = found.map_err(|e| convert::error(&e))?;
    Ok(PaneInfo {
        pane: p.id.to_string(),
        session,
        pid: p.pid,
        path: p.path,
        shell_foreground: p.shell_foreground,
    })
}

fn check_agent(binding: Option<&ChatBinding>, agent: &str) -> Result<(), ErrorBody> {
    if binding.is_some_and(|binding| binding.seen.agent != agent) {
        return Err(error_body(
            ErrorCode::Unsupported,
            "the restarted agent conversation changed; continue in Terminal",
        ));
    }
    Ok(())
}

fn info_pane(active: &Option<Active>) -> String {
    active
        .as_ref()
        .map(|a| a.info.pane.clone())
        .unwrap_or_default()
}

/// Turns one poll into messages, in order: a reset starts a new message, and
/// upserts after it share one. Every message carries the current status and
/// prompt; when only those changed, a message without entries carries them.
fn batch(
    active: &mut Active,
    all: Option<Vec<ChatEntry>>,
    events: Vec<ChatEvent>,
    changed: bool,
) -> Vec<Message> {
    let mut parts: Vec<(bool, Vec<ChatEntry>, bool)> = Vec::new();
    if let Some(all) = all {
        parts.push((true, all, false));
    }
    for event in events {
        match event {
            ChatEvent::Reset { entries, truncated } => parts.push((true, entries, truncated)),
            ChatEvent::Upsert(entry) => match parts.last_mut() {
                Some((_, entries, _)) => entries.push(entry),
                None => parts.push((false, vec![entry], false)),
            },
        }
    }
    if parts.is_empty() && changed {
        parts.push((false, Vec::new(), false));
    }
    parts
        .into_iter()
        .map(|(reset, entries, truncated)| {
            active.seq += 1;
            Message::ChatEntries {
                chat: active.chat,
                seq: active.seq,
                reset,
                entries,
                status: active.status.clone(),
                agent: reset.then(|| active.agent.to_owned()),
                truncated,
                prompt: active.prompt.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::{AgentAdapter, Error, Processes};
    use muxa_protocol::{AgentState, ChatKind};

    fn entry(id: &str) -> ChatEntry {
        ChatEntry {
            id: id.into(),
            kind: ChatKind::User,
            text: id.into(),
            tool: None,
        }
    }

    fn active() -> Active {
        Active {
            chat: 7,
            pane: "%1".parse().unwrap(),
            info: PaneInfo {
                pane: "%1".into(),
                session: "s".into(),
                pid: 1,
                path: "/".into(),
                shell_foreground: false,
            },
            binding: None,
            agent: "fake",
            source: None,
            seq: 0,
            status: Some(Agent {
                state: AgentState::Running,
                since: Some(5),
            }),
            prompt: None,
            resync: false,
        }
    }

    fn summary(messages: &[Message]) -> Vec<(u64, bool, Vec<String>, Option<String>)> {
        messages
            .iter()
            .map(|m| match m {
                Message::ChatEntries {
                    seq,
                    reset,
                    entries,
                    agent,
                    ..
                } => (
                    *seq,
                    *reset,
                    entries.iter().map(|e| e.id.clone()).collect(),
                    agent.clone(),
                ),
                other => panic!("unexpected {other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_reset_starts_a_message_and_upserts_share_one() {
        let mut a = active();
        let events = vec![
            ChatEvent::Upsert(entry("x")),
            ChatEvent::Reset {
                entries: vec![entry("a")],
                truncated: true,
            },
            ChatEvent::Upsert(entry("b")),
            ChatEvent::Upsert(entry("c")),
        ];
        let messages = batch(&mut a, None, events, false);
        assert_eq!(
            summary(&messages),
            [
                (1, false, vec!["x".into()], None),
                (
                    2,
                    true,
                    vec!["a".into(), "b".into(), "c".into()],
                    Some("fake".into())
                ),
            ]
        );
        let Message::ChatEntries {
            truncated,
            status,
            chat,
            ..
        } = &messages[1]
        else {
            panic!()
        };
        assert!(*truncated);
        assert_eq!(*chat, 7);
        assert_eq!(status.as_ref().unwrap().state, AgentState::Running);
    }

    #[test]
    fn a_resync_resends_everything_and_status_alone_is_sent() {
        let mut a = active();
        let messages = batch(&mut a, Some(vec![entry("a"), entry("b")]), vec![], false);
        assert_eq!(
            summary(&messages),
            [(1, true, vec!["a".into(), "b".into()], Some("fake".into()))]
        );
        assert!(batch(&mut a, None, vec![], false).is_empty());
        let messages = batch(&mut a, None, vec![], true);
        assert_eq!(summary(&messages), [(2, false, vec![], None)]);
    }

    struct Fake(Option<&'static str>);

    impl AgentAdapter for Fake {
        fn id(&self) -> &'static str {
            self.0.unwrap_or("never")
        }

        fn open(&self, _: &PaneInfo, _: &Processes) -> Result<Option<Box<dyn ChatSource>>, Error> {
            Ok(self.0.map(|_| Box::new(FakeSource) as Box<dyn ChatSource>))
        }
    }

    struct FakeSource;

    impl ChatSource for FakeSource {
        fn poll(&mut self, _: &PaneInfo) -> Result<Vec<ChatEvent>, Error> {
            Ok(vec![])
        }
        fn entries(&self) -> Vec<ChatEntry> {
            vec![]
        }
        fn status(&self) -> Option<Agent> {
            None
        }
    }

    #[test]
    fn the_first_adapter_that_sees_its_agent_wins() {
        let registry = Registry::new(vec![
            Box::new(Fake(None)),
            Box::new(Fake(Some("second"))),
            Box::new(Fake(Some("third"))),
        ]);
        let (id, _) = registry.open(&active().info).unwrap().unwrap();
        assert_eq!(id, "second");
        assert!(
            Registry::new(vec![Box::new(Fake(None))])
                .open(&active().info)
                .unwrap()
                .is_none()
        );
    }

    struct ProcessAdapter(&'static str, u32);

    impl AgentAdapter for ProcessAdapter {
        fn id(&self) -> &'static str {
            self.0
        }

        fn open(
            &self,
            pane: &PaneInfo,
            _: &Processes,
        ) -> Result<Option<Box<dyn ChatSource>>, Error> {
            Ok(
                (pane.pid == self.1)
                    .then(|| Box::new(ProcessSource(self.0)) as Box<dyn ChatSource>),
            )
        }
    }

    struct ProcessSource(&'static str);

    impl ChatSource for ProcessSource {
        fn poll(&mut self, _: &PaneInfo) -> Result<Vec<ChatEvent>, Error> {
            Ok(vec![])
        }
        fn entries(&self) -> Vec<ChatEntry> {
            vec![entry(self.0)]
        }
        fn status(&self) -> Option<Agent> {
            None
        }
    }

    #[tokio::test]
    async fn a_replacement_reopens_chat_without_reusing_old_entries_or_readiness() {
        let registry = Arc::new(Registry::new(vec![
            Box::new(ProcessAdapter("a", 1)),
            Box::new(ProcessAdapter("b", 2)),
            Box::new(ProcessAdapter("a", 3)),
        ]));
        let mut a = active();
        a.source = Some(Box::new(ProcessSource("old")));
        a.seq = 10;
        for (pid, agent) in [(2, "b"), (3, "a")] {
            let mut info = a.info.clone();
            info.pid = pid;
            a.refresh(info, None, Arc::clone(&registry)).await.unwrap();
            assert!(a.status.is_none());
            assert!(a.prompt.is_none());
            assert!(a.resync);
            let entries = a.source.as_ref().unwrap().entries();
            let messages = batch(&mut a, Some(entries), vec![], true);
            assert_eq!(
                summary(&messages),
                [(a.seq, true, vec![agent.into()], Some(agent.into()))]
            );
        }
        assert_eq!(a.seq, 12, "the existing subscription keeps its sequence");
        let mut info = a.info.clone();
        info.pid = 4;
        assert!(a.refresh(info, None, registry).await.is_err());
        assert!(
            a.source.is_none(),
            "unknown replacement cannot poll the old transcript"
        );
        assert!(a.status.is_none());
        assert!(a.prompt.is_none());
    }
}
