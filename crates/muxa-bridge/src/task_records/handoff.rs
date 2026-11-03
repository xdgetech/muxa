//! How work reaches the agent (ADR 0015, 0018): one fixed prompt typed once
//! the agent is idle, and the run it belongs to filled in from what the
//! adapter reports. No user text goes on a command line or into a log.

use std::sync::Arc;
use std::time::Duration;

use muxa_tmux::{AgentState, AgentStatus, Client, PaneId, Role, SessionId};

use super::{Config, blocking, link};
use crate::agents::{Processes, Registry};
use crate::loops::typing;
use crate::records::store::ClaimedTask;
use crate::records::{PendingPrompt, PromptKind, Record, Seen, StoreError};

/// How long a new agent gets to report `idle`, in seconds.
pub const PROMPT_WAIT: u64 = 30;
const PROMPT_POLL: Duration = Duration::from_secs(1);
const LIST_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_ATTEMPTS: usize = 3;

/// A fixed prompt, and the note that replaces it when the agent never became idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prompt {
    pub kind: PromptKind,
    pub text: &'static str,
    pub not_sent: &'static str,
    /// Only an `idle` or `done` since this time counts, in epoch seconds. A
    /// state tag survives `respawn-pane`, and an old one would type into a
    /// starting agent.
    pub idle_since: Option<u64>,
}

pub const START: Prompt = Prompt {
    kind: PromptKind::Start,
    text: "Read .muxa/TASK.md and start.",
    not_sent: "The agent did not become idle within 30 seconds, so the start prompt \
               was not sent. In its pane, type: Read .muxa/TASK.md and start.",
    idle_since: None,
};

pub fn restart(at: u64) -> Prompt {
    Prompt {
        kind: PromptKind::Restart,
        text: "Read .muxa/TASK.md and .muxa/HANDOFF.md, then continue.",
        not_sent: "The restarted agent did not become idle within 30 seconds, so the \
                   prompt was not sent. In its pane, type: Read .muxa/TASK.md and \
                   .muxa/HANDOFF.md, then continue.",
        idle_since: Some(at),
    }
}

impl Prompt {
    /// The prompt a recorded wait is for. A restart's wait began at the restart.
    pub fn of(wait: PendingPrompt) -> Self {
        match wait.kind {
            PromptKind::Start => START,
            PromptKind::Restart => restart(wait.since),
        }
    }

    /// A finished turn (ADR 0019): `idle`, or a `done` no older than the
    /// wait that `began` (epoch seconds). tmux-agents keeps a finished turn
    /// `done` while no person watches the window.
    fn ready(&self, status: Option<AgentStatus>, began: u64) -> bool {
        let Some(s) = status else { return false };
        match (s.state, self.idle_since) {
            (AgentState::Idle, None) => true,
            (AgentState::Idle, Some(at)) => s.since.is_some_and(|since| since >= at),
            (AgentState::Done, at) => s.since.is_some_and(|since| since >= at.unwrap_or(began)),
            _ => false,
        }
    }
}

/// The agent pane of a Task's session, as tags and adapters describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPane {
    pub session: SessionId,
    pub pane: PaneId,
    pub status: Option<AgentStatus>,
    pub pid: u32,
    pub shell_foreground: bool,
    /// Kept separate from tags, which survive process replacement.
    pub adapter_status: Option<AgentStatus>,
    pub adapter_process_bound: bool,
    /// What an adapter reports about the agent there.
    pub seen: Option<Seen>,
}

/// Which session to look in.
#[derive(Debug, Clone)]
pub enum Linked {
    Session(SessionId),
    /// The session that runs this record, by tag or by `dir` (ADR 0015).
    Task(String),
}

/// A prompt to type, as the record's `pendingPrompt` holds it.
#[derive(Debug, Clone)]
pub struct Wait {
    pub task: String,
    pub which: Linked,
    pub pending: PendingPrompt,
}

/// Types the waited-for prompt into the agent pane once the agent is ready,
/// and fills in the run from what the adapter reports. Not on `waiting`:
/// that includes a permission menu, where the text would pick an option.
/// [`PROMPT_WAIT`] seconds after the wait began, the record's note says the
/// prompt was not sent. The record's `pendingPrompt` is cleared either way;
/// a wait whose `pendingPrompt` is gone was ended by another process.
pub async fn send_prompt(
    client: Arc<Client>,
    agents: Arc<Registry>,
    config: Arc<Config>,
    wait: Wait,
) {
    let Wait {
        task,
        which,
        pending,
    } = wait;
    let prompt = Prompt::of(pending);
    let record = {
        let (config, task) = (Arc::clone(&config), task.clone());
        blocking(move || {
            config
                .store()?
                .get(&task)
                .map_err(|e| super::store_error(&e))
        })
        .await
    };
    let Ok(record) = record else { return };
    let left = (pending.since + PROMPT_WAIT).saturating_sub(crate::records::now());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(left);
    let seen = loop {
        let mut seen = None;
        let found = agent_pane(&client, &agents, &config, which.clone()).await;
        if let Some(found) = found.and_then(|found| bound_observation(&record, pending, found)) {
            if record
                .open_run()
                .is_some_and(|run| !run.extra.contains_key(crate::records::observation::FIELD))
            {
                seen = found.seen.clone();
            }
            if prompt.ready(found.status, pending.since)
                && person_quiet(&client, found.session, &task).await
            {
                match claim_pending(&config, &task, pending).await {
                    Ok(None) => {
                        tracing::info!(task, "the prompt wait ended elsewhere");
                        return;
                    }
                    Err(e) => {
                        tracing::debug!(task, error = %e.message, "prompt claim unavailable");
                    }
                    Ok(Some(mut claim)) => 'delivery: {
                        seen = None;
                        // Read again under the lifecycle claim: the process may have
                        // changed while we waited for readiness or another writer.
                        let Some(current) = agent_pane(&client, &agents, &config, which.clone())
                            .await
                            .and_then(|found| bound_observation(claim.record(), pending, found))
                        else {
                            break 'delivery;
                        };
                        if current.session != found.session
                            || current.pane != found.pane
                            || current.pid != found.pid
                            || !prompt.ready(current.status, pending.since)
                        {
                            break 'delivery;
                        }
                        let process =
                            match super::restart::identity(&client, current.session, current.pane)
                                .await
                            {
                                Ok(process) => process,
                                Err(_) => break 'delivery,
                            };
                        if process.pid != current.pid
                            || !bound_process(claim.record(), pending, &process)
                        {
                            break 'delivery;
                        }
                        seen = current.seen;
                        let at = crate::records::now();
                        let operation = match getrandom::u64() {
                            Ok(n) => format!("delivery-{n:016x}"),
                            Err(_) => return,
                        };
                        let op = crate::records::lifecycle::Operation {
                            version: 1,
                            id: operation,
                            phase: crate::records::lifecycle::Phase::Delivering,
                            source_run: Some(pending.run),
                            reserved_run: pending.run,
                            pane: found.pane.number(),
                            source_pid: process.pid,
                            worker: claim.record().worker().map(str::to_owned),
                            session: Some(found.session.number()),
                            source_started: Some(process.started),
                            replacement: None,
                            files: None,
                            since: at,
                            seen: None,
                        };
                        let claimed = blocking(move || {
                            claim.write_operation(Some(op)).map_err(|_| {
                                super::error_body(
                                    muxa_protocol::ErrorCode::Conflict,
                                    "prompt delivery requires recovery",
                                )
                            })?;
                            Ok(claim)
                        })
                        .await;
                        let Ok(claim) = claimed else {
                            return;
                        };
                        let sent = match typing::type_prompt(&client, found.pane, prompt.text).await
                        {
                            Ok(()) => {
                                tracing::info!(task, pane = %found.pane, "prompt sent");
                                true
                            }
                            Err(e) => {
                                tracing::warn!(task, pane = %found.pane, error = %e, "cannot send the prompt");
                                false
                            }
                        };
                        // Keep restart and other senders excluded through bookkeeping.
                        if !sent {
                            // Even partial input is uncertain; no automatic second attempt.
                            return;
                        }
                        finish_claimed_wait(claim, pending, seen, at).await;
                        return;
                    }
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            break seen;
        }
        tokio::time::sleep(PROMPT_POLL).await;
    };
    tracing::info!(task, "agent not idle; prompt not sent");
    end_wait(&config, task, pending, seen, Some(prompt.not_sent)).await;
}

/// Only locally prepared named runs can use the internal sender while the wire gate is closed.
pub(super) fn supports_prompt(record: &Record, pending: PendingPrompt) -> bool {
    if record.extra.contains_key(crate::records::lifecycle::FIELD) {
        return false;
    }
    let Some(run) = record.open_run().filter(|run| run.id == pending.run) else {
        return false;
    };
    let matching_kind = matches!(
        (pending.kind, run.reason),
        (PromptKind::Start, crate::records::RunReason::Start)
            | (PromptKind::Restart, crate::records::RunReason::Restart)
    );
    matching_kind && (record.worker().is_none() || record.worker_run_bound())
}

async fn claim_pending(
    config: &Config,
    task: &str,
    pending: PendingPrompt,
) -> Result<Option<ClaimedTask>, muxa_protocol::ErrorBody> {
    let (config, task) = (config.clone(), task.to_owned());
    blocking(move || {
        let store = config.store()?;
        let rev = store.get(&task).map_err(|e| super::store_error(&e))?.rev;
        let claim = store
            .claim(&task, rev)
            .map_err(|e| super::store_error(&e))?;
        let r = claim.record();
        if r.extra.contains_key(crate::records::lifecycle::FIELD) {
            return Err(super::store_error(&StoreError::Busy(task)));
        }
        if r.pending_prompt() != Some(pending)
            || r.open_run().map(|run| run.id) != Some(pending.run)
        {
            return Ok(None);
        }
        if !supports_prompt(r, pending) {
            return Err(super::error_body(
                muxa_protocol::ErrorCode::Unsupported,
                "worker continuation requires a bound run and assignment",
            ));
        }
        Ok(Some(claim))
    })
    .await
}

async fn finish_claimed_wait(
    mut claim: ClaimedTask,
    pending: PendingPrompt,
    seen: Option<Seen>,
    at: u64,
) -> bool {
    let result = blocking(move || {
        let armed = claim
            .record()
            .task_loop()
            .ok()
            .flatten()
            .filter(crate::records::Loop::active)
            .map(|lp| crate::records::Loop {
                state: crate::records::LoopState::Armed,
                reason: None,
                armed_at: at,
                since: at,
                ..lp
            });
        let armed = armed.map(serde_json::to_value).transpose().map_err(|_| {
            super::error_body(
                muxa_protocol::ErrorCode::Internal,
                "cannot prepare loop bookkeeping",
            )
        })?;
        claim
            .update(move |r| {
                r.extra.remove(crate::records::lifecycle::FIELD);
                r.set_pending_prompt(None);
                if let Some(armed) = armed {
                    r.extra.insert("loop".into(), armed);
                }
                if let (Some(seen), Some(run)) = (&seen, r.run_mut(pending.run)) {
                    run.fill(seen);
                }
            })
            .map_err(|e| super::store_error(&e))?;
        Ok(())
    })
    .await;
    if let Err(e) = &result {
        tracing::warn!(error = %e.message, "cannot finish a claimed prompt wait");
    }
    result.is_ok()
}

/// Clears the wait, if it is still this one, with the run's facts and a note.
pub async fn end_wait(
    config: &Config,
    task: String,
    pending: PendingPrompt,
    seen: Option<Seen>,
    note: Option<&'static str>,
) {
    write(config, task, move |r| {
        if r.pending_prompt() != Some(pending) {
            return;
        }
        r.set_pending_prompt(None);
        if let Some(note) = note {
            r.note = Some(note.to_owned());
        }
        if let (Some(seen), Some(run)) = (&seen, r.run_mut(pending.run)) {
            run.fill(seen);
        }
    })
    .await;
}

/// No person typed on the session in the last seconds (ADR 0019). In doubt, wait.
async fn person_quiet(client: &Client, session: SessionId, task: &str) -> bool {
    match typing::quiet(client, session, crate::records::now()).await {
        Ok(quiet) => quiet,
        Err(e) => {
            tracing::debug!(task, error = %e, "cannot list clients; not typing yet");
            false
        }
    }
}

/// The first live agent pane of the linked session, with its state and what
/// its adapter reports.
pub async fn agent_pane(
    client: &Client,
    agents: &Arc<Registry>,
    config: &Arc<Config>,
    which: Linked,
) -> Option<AgentPane> {
    let listed = tokio::time::timeout(LIST_TIMEOUT, muxa_tmux::list_sessions(client))
        .await
        .ok()?
        .ok()?;
    let (agents, config) = (Arc::clone(agents), Arc::clone(config));
    tokio::task::spawn_blocking(move || {
        let mut sessions = listed;
        if matches!(which, Linked::Task(_)) {
            link(&mut sessions, &config);
        }
        let mut mine: Vec<_> = sessions
            .into_iter()
            .filter(|s| match &which {
                Linked::Session(id) => s.id == *id,
                Linked::Task(task) => s.task_id.as_deref() == Some(task.as_str()),
            })
            .take(1)
            .collect();
        crate::sessions::fill_agents(&mut mine, &agents);
        let session = mine.into_iter().next()?;
        let pane = session
            .panes
            .iter()
            .find(|p| p.role == Role::Agent && !p.dead)?;
        let procs = Processes::list().ok()?;
        let found = agents.identify(&crate::sessions::info(&session.name, pane), &procs);
        let adapter_process_bound = found.as_ref().is_some_and(|(_, found)| found.process_bound);
        let adapter_status = found
            .as_ref()
            .and_then(|(_, found)| found.status.as_ref())
            .map(crate::convert::agent_status);
        let seen = found.map(|(agent, found)| Seen {
            agent: agent.to_owned(),
            session: found.session.as_ref().map(|s| s.id.clone()),
            transcript: found.session.and_then(|s| s.transcript),
        });
        Some(AgentPane {
            session: session.id,
            pane: pane.id,
            status: pane.agent,
            pid: pane.pid,
            shell_foreground: pane.shell_foreground,
            adapter_status,
            adapter_process_bound,
            seen,
        })
    })
    .await
    .ok()
    .flatten()
}

fn bound_observation(
    record: &Record,
    pending: PendingPrompt,
    found: AgentPane,
) -> Option<AgentPane> {
    record.open_run().filter(|run| run.id == pending.run)?;
    bound_run_observation(record, found)
}

/// A follow-up restart and a sender must use the current run's evidence alike.
pub(super) fn bound_run_observation(record: &Record, mut found: AgentPane) -> Option<AgentPane> {
    let Some(run) = record.open_run() else {
        return record.worker().is_none().then_some(found);
    };
    if record.worker().is_some() && !record.worker_run_bound() {
        return None;
    }
    if run
        .extra
        .get("outcome")
        .is_some_and(|value| value == "unconfirmed")
    {
        return None;
    }
    if let Some(baseline) = run.restart_observation().ok()? {
        if !baseline.accepts(
            found.session.number(),
            found.pane.number(),
            found.pid,
            found.shell_foreground,
            found.adapter_process_bound,
            found.seen.as_ref(),
        ) {
            return None;
        }
        found.status = Some(found.adapter_status?);
    }
    Some(found)
}

fn bound_process(
    record: &Record,
    pending: PendingPrompt,
    process: &crate::records::lifecycle::ProcessIdentity,
) -> bool {
    let Some(run) = record.open_run().filter(|run| run.id == pending.run) else {
        return false;
    };
    match run.restart_observation() {
        Ok(None) => true,
        Ok(Some(baseline)) => baseline.replacement == *process,
        Err(_) => false,
    }
}

/// Applies `change` to the current record, retrying when another bridge
/// wrote in between. Returns the record written.
pub async fn write(
    config: &Config,
    task: String,
    change: impl Fn(&mut Record) + Send + 'static,
) -> Option<Record> {
    let config = config.clone();
    let done = blocking(move || {
        let store = config.store()?;
        for _ in 0..WRITE_ATTEMPTS {
            let rev = store.get(&task).map_err(|e| super::store_error(&e))?.rev;
            match store.update(&task, rev, &change) {
                Ok(r) => return Ok(r),
                Err(StoreError::Conflict { .. }) => continue,
                Err(e) => return Err(super::store_error(&e)),
            }
        }
        Err(super::store_error(&StoreError::Busy(task)))
    })
    .await;
    match done {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::warn!(error = %e.message, "cannot update a task record");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_prompt_resumption_requires_matching_assignment_and_valid_binding() {
        let mut record: Record = serde_json::from_value(serde_json::json!({
            "version":1,"id":"t-000001","rev":1,"workspace":"work","title":"fixture",
            "goal":"","status":"open","createdAt":100,"updatedAt":100,"worker":"Reviewer"
        }))
        .unwrap();
        let run = record.start_run(crate::records::RunReason::Start, 100);
        let pending = PendingPrompt {
            kind: PromptKind::Start,
            since: 100,
            run,
        };
        assert!(
            !supports_prompt(&record, pending),
            "legacy named runs stay gated"
        );
        let current = record.run_mut(run).unwrap();
        crate::records::observation::bind_start(
            current,
            "start-t-000001",
            1,
            2,
            crate::records::lifecycle::ProcessIdentity {
                pid: 200,
                started: "fixture-start".into(),
            },
        );
        for worker in [Some("Other"), None] {
            record.run_mut(run).unwrap().set_worker(worker);
            assert!(!supports_prompt(&record, pending));
        }
        record.run_mut(run).unwrap().set_worker(Some("Reviewer"));
        assert!(supports_prompt(&record, pending));
        assert!(!supports_prompt(
            &record,
            PendingPrompt {
                kind: PromptKind::Restart,
                ..pending
            }
        ));
        record
            .run_mut(run)
            .unwrap()
            .extra
            .insert("outcome".into(), serde_json::json!("unconfirmed"));
        assert!(!supports_prompt(&record, pending));
        record.run_mut(run).unwrap().extra.remove("outcome");
        assert!(!supports_prompt(
            &record,
            PendingPrompt {
                run: run + 1,
                ..pending
            }
        ));
        record.extra.insert(
            crate::records::lifecycle::FIELD.into(),
            serde_json::json!({"version":99}),
        );
        assert!(
            !supports_prompt(&record, pending),
            "claimed delivery never resumes automatically"
        );
        record.extra.remove(crate::records::lifecycle::FIELD);
        record
            .run_mut(run)
            .unwrap()
            .extra
            .get_mut(crate::records::observation::FIELD)
            .unwrap()["version"] = serde_json::json!(99);
        assert!(!supports_prompt(&record, pending));
        record
            .run_mut(run)
            .unwrap()
            .extra
            .remove(crate::records::observation::FIELD);
        assert!(!supports_prompt(&record, pending));
    }

    #[test]
    fn restart_readiness_requires_new_adapter_evidence_even_in_the_same_second() {
        use crate::records::lifecycle::ProcessIdentity;
        use crate::records::observation::{Baseline, FIELD};

        let mut record: Record = serde_json::from_value(serde_json::json!({
            "version":1,"id":"t-000001","rev":1,"workspace":"work","title":"fixture",
            "goal":"","status":"open","createdAt":100,"updatedAt":100
        }))
        .unwrap();
        let run = record.start_run(crate::records::RunReason::Restart, 100);
        let pending = PendingPrompt {
            kind: PromptKind::Restart,
            since: 100,
            run,
        };
        let old = Seen {
            agent: "fixture".into(),
            session: Some("old".into()),
            transcript: Some("/fixture/old.jsonl".into()),
        };
        let process = ProcessIdentity {
            pid: 200,
            started: "same-second-start".into(),
        };
        let baseline = Baseline {
            version: 1,
            operation: "restart-fixture".into(),
            session: 1,
            pane: 2,
            replacement: process.clone(),
            source: Some(old.clone()),
        };
        record
            .run_mut(run)
            .unwrap()
            .extra
            .insert(FIELD.into(), serde_json::to_value(baseline).unwrap());
        let mut found = AgentPane {
            session: SessionId::new(1),
            pane: PaneId::new(2),
            pid: 200,
            shell_foreground: false,
            status: idle(Some(100)),
            adapter_status: None,
            adapter_process_bound: true,
            seen: Some(old),
        };
        assert!(bound_observation(&record, pending, found.clone()).is_none());

        found.seen = Some(Seen {
            agent: "fixture".into(),
            session: Some("new".into()),
            transcript: Some("/fixture/new.jsonl".into()),
        });
        assert!(
            bound_observation(&record, pending, found.clone()).is_none(),
            "a retained idle tag cannot prove readiness"
        );
        found.adapter_status = status(AgentState::Waiting, Some(100));
        assert!(
            !Prompt::of(pending).ready(
                bound_observation(&record, pending, found.clone())
                    .unwrap()
                    .status,
                100
            )
        );
        found.adapter_status = idle(Some(100));
        assert!(
            Prompt::of(pending).ready(
                bound_observation(&record, pending, found.clone())
                    .unwrap()
                    .status,
                100
            )
        );
        found.adapter_status = status(AgentState::Running, Some(100));
        assert_eq!(
            bound_run_observation(&record, found.clone())
                .unwrap()
                .status,
            found.adapter_status,
            "follow-up restart must see Running despite a retained Idle tag"
        );
        found.adapter_process_bound = false;
        assert!(
            bound_run_observation(&record, found.clone()).is_none(),
            "a timestamp-only match cannot authorize restart or input"
        );
        found.adapter_process_bound = true;
        assert!(bound_process(&record, pending, &process));
        assert!(!bound_process(
            &record,
            pending,
            &ProcessIdentity {
                pid: 200,
                started: "different-start".into()
            }
        ));
        record
            .run_mut(run)
            .unwrap()
            .extra
            .insert(FIELD.into(), serde_json::json!({"version":99}));
        assert!(bound_observation(&record, pending, found.clone()).is_none());
        assert!(!bound_process(&record, pending, &process));
        record.run_mut(run).unwrap().extra.remove(FIELD);
        found.seen = None;
        found.adapter_status = None;
        assert_eq!(
            bound_observation(&record, pending, found.clone())
                .unwrap()
                .status,
            found.status,
            "legacy hook-only waits retain their behavior"
        );
        record
            .run_mut(run)
            .unwrap()
            .extra
            .insert("outcome".into(), serde_json::json!("unconfirmed"));
        assert!(
            bound_run_observation(&record, found).is_none(),
            "explicitly unconfirmed history cannot use the legacy fallback"
        );
    }

    struct Fixture(std::path::PathBuf);

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn an_ended_wait_cannot_fill_a_new_runs_history_or_timeout_note() {
        let dir = Fixture(
            std::env::temp_dir().join(format!("muxa-ended-wait-{:08x}", getrandom::u32().unwrap())),
        );
        let store = crate::records::Store::open(&dir.0).unwrap();
        let record = store
            .create(crate::records::NewRecord {
                workspace: "work".into(),
                title: "fixture".into(),
                ..Default::default()
            })
            .unwrap();
        let old = PendingPrompt {
            kind: PromptKind::Restart,
            since: 100,
            run: 1,
        };
        let current = PendingPrompt {
            kind: PromptKind::Restart,
            since: 101,
            run: 2,
        };
        store
            .update(&record.id, record.rev, |r| {
                r.start_run(crate::records::RunReason::Restart, 100);
                r.start_run(crate::records::RunReason::Restart, 101);
                r.set_pending_prompt(Some(current));
            })
            .unwrap();
        let config = Config {
            state_dir: Some(dir.0.clone()),
            workspaces: None,
            home: None,
        };
        end_wait(
            &config,
            record.id.clone(),
            old,
            Some(Seen {
                agent: "old".into(),
                session: Some("old-session".into()),
                transcript: None,
            }),
            Some("obsolete timeout"),
        )
        .await;
        let result = store.get(&record.id).unwrap();
        assert_eq!(result.pending_prompt(), Some(current));
        assert!(result.note.is_none());
        assert!(result.runs.iter().all(|run| run.agent.is_none()));
    }

    #[tokio::test]
    async fn prompt_claim_excludes_another_sender_and_restart_until_bookkeeping() {
        let dir = Fixture(std::env::temp_dir().join(format!(
            "muxa-prompt-claim-{:08x}",
            getrandom::u32().unwrap()
        )));
        let store = crate::records::Store::open(&dir.0).unwrap();
        let created = store
            .create(crate::records::NewRecord {
                workspace: "work".into(),
                title: "fixture".into(),
                ..Default::default()
            })
            .unwrap();
        let pending = PendingPrompt {
            kind: PromptKind::Start,
            since: 1,
            run: 1,
        };
        let record = store
            .update(&created.id, created.rev, |r| {
                r.start_run(crate::records::RunReason::Start, 1);
                r.set_pending_prompt(Some(pending));
                let lp = crate::records::Loop::start(
                    crate::records::LoopSpec {
                        check: "true".into(),
                        max_attempts: Some(2),
                        timeout: Some(10),
                    },
                    None,
                    false,
                    1,
                );
                r.set_loop(Some(&lp)).unwrap();
            })
            .unwrap();
        let config = Config {
            state_dir: Some(dir.0.clone()),
            workspaces: None,
            home: None,
        };
        let claim = claim_pending(&config, &record.id, pending)
            .await
            .unwrap()
            .unwrap();
        assert!(claim_pending(&config, &record.id, pending).await.is_err());
        assert!(matches!(
            store.claim(&record.id, record.rev),
            Err(StoreError::Busy(_))
        ));
        assert!(finish_claimed_wait(claim, pending, None, 7).await);
        let current = store.get(&record.id).unwrap();
        assert!(current.pending_prompt().is_none());
        assert_eq!(current.rev, record.rev + 1);
        assert_eq!(current.task_loop().unwrap().unwrap().armed_at, 7);
        assert!(
            claim_pending(&config, &record.id, pending)
                .await
                .unwrap()
                .is_none()
        );

        let blocked = store
            .update(&record.id, current.rev, |r| {
                r.set_pending_prompt(Some(pending));
                r.extra.insert(
                    crate::records::lifecycle::FIELD.into(),
                    serde_json::json!({"version":99}),
                );
            })
            .unwrap();
        assert!(claim_pending(&config, &record.id, pending).await.is_err());
        assert_eq!(store.get(&record.id).unwrap(), blocked);
        let mut repair = store.claim(&record.id, blocked.rev).unwrap();
        repair
            .update(|r| {
                r.extra.remove(crate::records::lifecycle::FIELD);
                r.extra
                    .insert("worker".into(), serde_json::json!("Reviewer"));
            })
            .unwrap();
        let named = repair.record().clone();
        drop(repair);
        let error = claim_pending(&config, &record.id, pending)
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, muxa_protocol::ErrorCode::Unsupported);
        assert_eq!(store.get(&record.id).unwrap(), named);
    }

    fn status(state: AgentState, since: Option<u64>) -> Option<AgentStatus> {
        Some(AgentStatus { state, since })
    }

    fn idle(since: Option<u64>) -> Option<AgentStatus> {
        status(AgentState::Idle, since)
    }

    fn done(since: Option<u64>) -> Option<AgentStatus> {
        status(AgentState::Done, since)
    }

    #[test]
    fn a_restart_prompt_waits_for_an_idle_newer_than_the_restart() {
        assert!(START.ready(idle(None), 50));
        assert!(!START.ready(None, 50));
        let p = restart(100);
        assert!(
            !p.ready(idle(Some(99)), 150),
            "an idle from before the restart"
        );
        assert!(!p.ready(idle(None), 150), "an idle of unknown age");
        assert!(p.ready(idle(Some(100)), 150));
        assert!(!p.ready(status(AgentState::Running, Some(200)), 150));
    }

    #[test]
    fn a_fresh_done_is_a_finished_turn() {
        assert!(START.ready(done(Some(50)), 50));
        assert!(
            !START.ready(done(Some(49)), 50),
            "a done from before the wait"
        );
        assert!(!START.ready(done(None), 50), "a done of unknown age");
        let p = restart(100);
        assert!(
            p.ready(done(Some(100)), 150),
            "since the restart, not the wait"
        );
        assert!(!p.ready(done(Some(99)), 150));
        for state in [AgentState::Waiting, AgentState::Error, AgentState::Running] {
            assert!(!START.ready(status(state, Some(60)), 50), "{state:?}");
        }
    }
}
