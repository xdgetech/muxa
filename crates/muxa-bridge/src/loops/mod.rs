//! The Task Loop engine (ADR 0019 section 3): after each finished turn of a
//! Task's agent, run its check; on a fail, type a fixed prompt; on a pass,
//! hand the Task to the user. `step` decides, `check` runs the command, and
//! `typing` types only when no person does. The daemon calls [`Engine::tick`].

pub mod check;
pub mod step;
pub mod typing;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use muxa_tmux::{Client, PaneId, Role, Session, SessionId};
use tokio::task::JoinHandle;

use crate::agents::Registry;
use crate::records::{
    Loop, LoopSpec, LoopState, Reason, Record, Status, Store, StoreError, Workspaces, now,
};
use crate::task_records::{Config, link};
use check::{CheckError, CheckRun};
use step::{Action, Agent, Check, CheckEnd, Memory, Observation, effects, step};

const LIST_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_ATTEMPTS: usize = 3;

/// What the engine keeps between ticks, per Task with an active loop.
#[derive(Default)]
struct Live {
    memory: Memory,
    /// Dropping it kills the check's process group.
    check: Option<JoinHandle<Result<CheckEnd, CheckError>>>,
}

/// A loop that is no longer active leaves no check running.
impl Drop for Live {
    fn drop(&mut self) {
        if let Some(h) = self.check.take() {
            h.abort();
        }
    }
}

/// Runs every active loop. One per daemon; a single owner of the checks.
pub struct Engine {
    config: Arc<Config>,
    agents: Arc<Registry>,
    /// `PATH` for a check when tmux cannot be reached; `install` copies it.
    fallback_path: Option<String>,
    live: HashMap<String, Live>,
}

/// The agent pane of a Task, as one tick found it.
#[derive(Debug, Clone, Copy)]
struct Found {
    session: SessionId,
    pane: PaneId,
    agent: Agent,
}

impl Engine {
    pub fn new(config: Arc<Config>, agents: Arc<Registry>, fallback_path: Option<String>) -> Self {
        Self {
            config,
            agents,
            fallback_path,
            live: HashMap::new(),
        }
    }

    /// True while some loop is active, so the daemon keeps its tmux client.
    pub fn busy(&self) -> bool {
        !self.live.is_empty()
    }

    /// One look at every active loop. Without a tmux client, nothing moves:
    /// no agent state means no action (invariant 6).
    pub async fn tick(&mut self, client: Option<&Client>) {
        let records = self.active_records().await;
        self.live
            .retain(|id, _| records.iter().any(|r| r.0.id == *id));
        for (r, _) in &records {
            self.live.entry(r.id.clone()).or_default();
        }
        let Some(client) = client else { return };
        if records.is_empty() {
            return;
        }
        let Some(sessions) = self.sessions(client).await else {
            return;
        };
        let mut global = None;
        for (record, lp) in records {
            // A Task with no directory has nowhere to run its check.
            let found = record.dir.as_ref().and(find(&sessions, &record.id));
            self.one(client, &record, &lp, found, &mut global).await;
        }
    }

    /// Kills every running check and waits for it, for a clean exit. The
    /// loops stay in the records; a `checking` one is `armed` again on the
    /// next start (ADR 0019, "Daemon restart").
    pub async fn stop_checks(&mut self) {
        for (_, mut live) in self.live.drain() {
            if let Some(h) = live.check.take() {
                h.abort();
                let _ = h.await;
            }
        }
    }

    async fn active_records(&self) -> Vec<(Record, Loop)> {
        let Some(dir) = self.config.state_dir.clone() else {
            return Vec::new();
        };
        let listed = tokio::task::spawn_blocking(move || {
            if !dir.is_dir() {
                return Ok(Vec::new());
            }
            Store::open(&dir)?.list()
        })
        .await;
        let records = match listed {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "loops cannot read task records");
                return Vec::new();
            }
            Err(e) => {
                tracing::warn!(error = %e, "loops: the record listing stopped");
                return Vec::new();
            }
        };
        records
            .into_iter()
            .filter_map(|r| match r.task_loop() {
                Ok(Some(lp)) if lp.active() => Some((r, lp)),
                Ok(_) => None,
                Err(e) => {
                    tracing::warn!(task = %r.id, error = %e, "unreadable loop; skipping it");
                    None
                }
            })
            .collect()
    }

    /// Every session, linked to records and with adapter states: one
    /// process listing for all loops.
    async fn sessions(&self, client: &Client) -> Option<Vec<Session>> {
        let listed = tokio::time::timeout(LIST_TIMEOUT, muxa_tmux::list_sessions(client)).await;
        let mut sessions = match listed {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                tracing::debug!(error = %e, "loops cannot list sessions");
                return None;
            }
            Err(_) => return None,
        };
        let (config, agents) = (Arc::clone(&self.config), Arc::clone(&self.agents));
        tokio::task::spawn_blocking(move || {
            link(&mut sessions, &config);
            crate::sessions::fill_agents(&mut sessions, &agents);
            crate::task_records::observation::refresh(&mut sessions, &config, &agents);
            sessions
        })
        .await
        .ok()
    }

    async fn one(
        &mut self,
        client: &Client,
        record: &Record,
        lp: &Loop,
        found: Option<Found>,
        global: &mut Option<Option<Vec<(String, String)>>>,
    ) {
        if record.extra.contains_key(crate::records::lifecycle::FIELD)
            || (record.worker().is_some() && !record.worker_run_bound())
            || record.pending_prompt().is_some()
        {
            if let Some(check) = self.live.get_mut(&record.id).and_then(|l| l.check.take()) {
                check.abort();
            }
            return;
        }
        let task = record.id.clone();
        let check = self.check_state(&task).await;
        let memory = self
            .live
            .get(&task)
            .map(|l| l.memory.clone())
            .unwrap_or_default();
        let t = now();
        let wants_quiet = memory.owed.is_some() || matches!(check, Check::Ended(_));
        let quiet = match (wants_quiet, found) {
            (true, Some(f)) => typing::quiet(client, f.session, t)
                .await
                .unwrap_or_else(|e| {
                    tracing::debug!(task, error = %e, "cannot list clients; not typing");
                    false
                }),
            _ => false,
        };
        let obs = Observation {
            now: t,
            status: record.status,
            agent: found.map_or(Agent::Missing, |f| f.agent),
            run_started_at: record.runs.last().map(|r| r.started_at),
            check,
            quiet,
        };
        let s = step(lp, &memory, &obs);
        let mut memory = s.memory;
        match s.action {
            Action::None => {
                if s.state != *lp {
                    self.save(record, lp, &s.state).await;
                }
            }
            Action::RunCheck { attempt } => {
                let env = self.env(client, record, global).await;
                let Some(dir) = self.config.state_dir.clone() else {
                    return;
                };
                let (id, rev, state) = (record.id.clone(), record.rev, s.state.clone());
                let claimed = tokio::task::spawn_blocking(move || {
                    let store = Store::open(dir)?;
                    let mut claim = store.claim(&id, rev)?;
                    if claim
                        .record()
                        .extra
                        .contains_key(crate::records::lifecycle::FIELD)
                    {
                        return Err(StoreError::Busy(id));
                    }
                    claim.update(|r| {
                        if let Err(e) = r.set_loop(Some(&state)) {
                            tracing::warn!(error = %e, "cannot encode loop");
                        }
                    })?;
                    Ok(claim)
                })
                .await;
                if let Ok(Ok(claim)) = claimed {
                    self.start_check(record, &s.state, attempt, env, claim);
                    tracing::info!(task, attempt, "check started");
                }
            }
            Action::Type { attempt, then } => {
                if let Some(f) = found {
                    let Some(dir) = self.config.state_dir.clone() else {
                        return;
                    };
                    let (id, rev) = (record.id.clone(), record.rev);
                    let claim =
                        tokio::task::spawn_blocking(move || Store::open(dir)?.claim(&id, rev))
                            .await;
                    let Ok(Ok(mut claim)) = claim else {
                        return;
                    };
                    if claim
                        .record()
                        .extra
                        .contains_key(crate::records::lifecycle::FIELD)
                    {
                        return;
                    }
                    let process =
                        match crate::task_records::restart::identity(client, f.session, f.pane)
                            .await
                        {
                            Ok(process) => process,
                            Err(_) => return,
                        };
                    if let Some(run) = claim.record().open_run() {
                        match run.restart_observation() {
                            Ok(None) => {}
                            Ok(Some(baseline)) => {
                                if baseline.replacement != process {
                                    return;
                                }
                                let Some(current) = self.sessions(client).await else {
                                    return;
                                };
                                let Some(observed) = find(&current, &record.id) else {
                                    return;
                                };
                                if observed.session != f.session
                                    || observed.pane != f.pane
                                    || observed.agent != f.agent
                                {
                                    return;
                                }
                            }
                            Err(_) => return,
                        }
                    }
                    let run = claim.record().open_run().map(|r| r.id);
                    let operation = match getrandom::u64() {
                        Ok(n) => format!("delivery-{n:016x}"),
                        Err(_) => return,
                    };
                    let at = now();
                    let op = crate::records::lifecycle::Operation {
                        version: 1,
                        id: operation,
                        phase: crate::records::lifecycle::Phase::Delivering,
                        source_run: run,
                        reserved_run: run.unwrap_or(0),
                        pane: f.pane.number(),
                        source_pid: process.pid,
                        worker: claim.record().worker().map(str::to_owned),
                        session: Some(f.session.number()),
                        source_started: Some(process.started),
                        replacement: None,
                        files: None,
                        since: at,
                        seen: None,
                    };
                    let claimed = tokio::task::spawn_blocking(move || {
                        claim.write_operation(Some(op))?;
                        Ok::<_, crate::records::lifecycle::Error>(claim)
                    })
                    .await;
                    let Ok(Ok(mut claim)) = claimed else {
                        return;
                    };
                    let prompt = if record.is_routine_run() {
                        crate::routines::record::fail_prompt(attempt)
                    } else {
                        typing::fail_prompt(attempt)
                    };
                    match typing::type_prompt(client, f.pane, &prompt).await {
                        Ok(()) => {
                            tracing::info!(task, pane = %f.pane, attempt, "fail prompt sent");
                            let then = Loop {
                                armed_at: at,
                                since: at,
                                ..*then
                            };
                            let written = tokio::task::spawn_blocking(move || {
                                claim
                                    .update(|r| {
                                        r.extra.remove(crate::records::lifecycle::FIELD);
                                        if let Err(e) = r.set_loop(Some(&then)) {
                                            tracing::warn!(error = %e, "cannot encode loop");
                                        }
                                    })
                                    .map(|_| ())
                            })
                            .await;
                            if matches!(written, Ok(Ok(()))) {
                                memory = step::typed(at);
                            }
                        }
                        Err(e) => {
                            tracing::warn!(task, pane = %f.pane, error = %e, "cannot type the fail prompt")
                        }
                    }
                }
            }
        }
        let live = self.live.entry(task).or_default();
        live.memory = memory;
        if s.state.state != LoopState::Checking {
            // A stopped loop leaves no check running.
            if let Some(h) = live.check.take() {
                h.abort();
            }
        }
    }

    async fn check_state(&mut self, task: &str) -> Check {
        let Some(live) = self.live.get_mut(task) else {
            return Check::Idle;
        };
        let Some(handle) = live.check.as_mut() else {
            return Check::Idle;
        };
        if !handle.is_finished() {
            return Check::Running;
        }
        let ended = handle.await;
        live.check = None;
        match ended {
            Ok(Ok(end)) => {
                let (exit, seconds) = match end {
                    CheckEnd::Exited { code, seconds } => (Some(code), seconds),
                    CheckEnd::TimedOut { seconds } => (None, seconds),
                };
                tracing::info!(task, exit, seconds, "check ended");
                Check::Ended(end)
            }
            // The check could not run: nothing counts, and the next turn tries again.
            Ok(Err(e)) => {
                tracing::warn!(task, error = %e, "cannot run the check");
                Check::Idle
            }
            Err(e) => {
                tracing::warn!(task, error = %e, "the check stopped");
                Check::Idle
            }
        }
    }

    /// The check's environment, read from tmux once per tick.
    async fn env(
        &self,
        client: &Client,
        record: &Record,
        global: &mut Option<Option<Vec<(String, String)>>>,
    ) -> Vec<(String, String)> {
        if global.is_none() {
            let read = check::global_env(client).await;
            if let Err(e) = &read {
                tracing::warn!(error = %e, "cannot read the tmux environment; checks use the daemon's");
            }
            *global = Some(read.ok());
        }
        let agent = self.workspace_agent(record).await;
        check::environment(
            global.clone().flatten(),
            agent.as_ref(),
            std::env::vars(),
            self.fallback_path.as_deref(),
        )
    }

    async fn workspace_agent(&self, record: &Record) -> Option<crate::records::AgentSettings> {
        let config = Arc::clone(&self.config);
        let name = record.workspace.clone();
        tokio::task::spawn_blocking(move || {
            let path = config.workspaces.as_ref()?;
            match Workspaces::load(path, config.home.as_deref()) {
                Ok(w) => w.get(&name)?.agent.clone(),
                Err(e) => {
                    tracing::warn!(error = %e, "cannot load workspaces for a check");
                    None
                }
            }
        })
        .await
        .ok()
        .flatten()
    }

    fn start_check(
        &mut self,
        record: &Record,
        lp: &Loop,
        attempt: u32,
        env: Vec<(String, String)>,
        claim: crate::records::store::ClaimedTask,
    ) {
        if record.is_routine_run() {
            // muxa's own output check, in process (ADR 0026 section 3).
            let (config, record) = (Arc::clone(&self.config), record.clone());
            let live = self.live.entry(record.id.clone()).or_default();
            live.check = Some(tokio::spawn(async move {
                drop(claim);
                let dir = record.dir.clone().unwrap_or_default();
                tokio::task::spawn_blocking(move || {
                    crate::routines::check::run(&config, &record, attempt)
                })
                .await
                .map_err(|e| CheckError::Io {
                    action: "check the routine output of",
                    path: dir,
                    source: std::io::Error::other(e),
                })
            }));
            return;
        }
        let Some(dir) = record.dir.clone() else {
            return;
        };
        let spec = CheckRun {
            dir,
            command: lp.check.clone(),
            attempt,
            timeout: Duration::from_secs(lp.timeout),
            env,
        };
        let live = self.live.entry(record.id.clone()).or_default();
        live.check = Some(tokio::spawn(check::run_claimed(spec, claim)));
    }

    /// Writes `new` over `old`, with what the change means for the status
    /// and the note. Skipped when the loop changed meanwhile, e.g. a
    /// `stop-loop`: the next tick steps from the record as it is.
    async fn save(&self, record: &Record, old: &Loop, new: &Loop) -> bool {
        let Some(dir) = self.config.state_dir.clone() else {
            return false;
        };
        let (task, old, new) = (record.id.clone(), old.clone(), new.clone());
        let written = tokio::task::spawn_blocking(move || write(&dir, &task, &old, &new)).await;
        match written {
            Ok(Ok(saved)) => saved,
            Ok(Err(e)) => {
                tracing::warn!(task = %record.id, error = %e, "cannot save a loop");
                false
            }
            Err(e) => {
                tracing::warn!(task = %record.id, error = %e, "saving a loop stopped");
                false
            }
        }
    }
}

/// Blocking. Ok(false) when the record's loop is no longer `old`.
fn write(dir: &Path, task: &str, old: &Loop, new: &Loop) -> Result<bool, StoreError> {
    let store = Store::open(dir)?;
    let fx = effects(old, new);
    for _ in 0..WRITE_ATTEMPTS {
        let current = store.get(task)?;
        if current.task_loop().ok().flatten().as_ref() != Some(old) {
            return Ok(false);
        }
        let result = store.update(task, current.rev, |r| {
            if let Err(e) = r.set_loop(Some(new)) {
                tracing::warn!(task, error = %e, "cannot encode a loop");
                return;
            }
            if fx.review && r.status == crate::records::Status::Open {
                // A published background run is finished; a Task waits for review.
                r.status = if r.is_background_run() {
                    crate::records::Status::Done
                } else {
                    crate::records::Status::Review
                };
            }
            if let Some(note) = &fx.note {
                r.note = Some(note.clone());
            }
        });
        match result {
            Ok(r) => {
                tracing::info!(task, rev = r.rev, state = %String::from(new.state.clone()), "loop saved");
                return Ok(true);
            }
            Err(StoreError::Conflict { .. }) => continue,
            Err(e) => return Err(e),
        }
    }
    Err(StoreError::Busy(task.to_owned()))
}

/// The first live agent pane of the session that runs `task`.
fn find(sessions: &[Session], task: &str) -> Option<Found> {
    let s = sessions
        .iter()
        .find(|s| s.task_id.as_deref() == Some(task))?;
    let pane = s.panes.iter().find(|p| p.role == Role::Agent && !p.dead)?;
    Some(Found {
        session: s.id,
        pane: pane.id,
        agent: if pane.shell_foreground {
            Agent::Missing
        } else {
            Agent::Present(pane.agent)
        },
    })
}

#[derive(Debug, thiserror::Error)]
pub enum LoopError {
    /// The request cannot be met; the message says why, without the command.
    #[error("{0}")]
    Bad(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}. Fix the workspaces file on the host.")]
    Config(#[from] crate::records::workspaces::ConfigError),
}

/// The loop of a `create-task`, checked before the Task exists: the check
/// is allowed, and `main` (the repo's main checkout) is one of the
/// workspace's repos (ADR 0031). A Task without a repo gets a directory under the root. Blocking.
pub fn precheck(
    config: &Config,
    workspace: &str,
    main: Option<&Path>,
    spec: &LoopSpec,
) -> Result<(), LoopError> {
    let workspaces = load(config)?;
    let ws = known(&workspaces, workspace)?;
    spec.check(ws.loop_checks.as_ref())
        .map_err(LoopError::Bad)?;
    if let Some(main) = main {
        let main = main.canonicalize().unwrap_or_else(|_| main.to_owned());
        if !ws.has_repo(&main) {
            return Err(LoopError::Bad(format!(
                "a loop needs a repo from the repos of workspace {workspace:?}"
            )));
        }
    }
    Ok(())
}

fn load(config: &Config) -> Result<Workspaces, LoopError> {
    Ok(match &config.workspaces {
        Some(path) => Workspaces::load(path, config.home.as_deref())?,
        None => Workspaces::default(),
    })
}

fn known<'a>(
    workspaces: &'a Workspaces,
    name: &str,
) -> Result<&'a crate::records::Workspace, LoopError> {
    workspaces.get(name).ok_or_else(|| {
        LoopError::Bad(format!(
            "workspace {name:?} is not in workspaces.toml on the host"
        ))
    })
}

/// `start-loop`, or the loop of a `create-task`: checks the workspace's
/// rules, arms the loop from now, and puts the check in `TASK.md`. Blocking.
pub fn start(
    config: &Config,
    task: &str,
    spec: LoopSpec,
    reset: bool,
) -> Result<Record, LoopError> {
    let store = open(config)?;
    let workspaces = load(config)?;
    for _ in 0..WRITE_ATTEMPTS {
        let record = store.get(task)?;
        if record.worker().is_some() && !record.worker_run_bound() {
            return Err(LoopError::Bad(
                "the worker's run is not confirmed; restart the agent first".into(),
            ));
        }
        let ws = known(&workspaces, &record.workspace)?;
        spec.check(ws.loop_checks.as_ref())
            .map_err(LoopError::Bad)?;
        ws.may_loop(&record).map_err(LoopError::Bad)?;
        if record.status != Status::Open {
            return Err(LoopError::Bad(format!(
                "task {task} is not open; set it to open first"
            )));
        }
        let previous = record.task_loop().ok().flatten();
        let lp = Loop::start(spec.clone(), previous.as_ref(), reset, now());
        let mut claim = match store.claim(task, record.rev) {
            Ok(claim) => claim,
            Err(StoreError::Conflict { .. }) => continue,
            Err(e) => return Err(e.into()),
        };
        if claim
            .record()
            .extra
            .contains_key(crate::records::lifecycle::FIELD)
        {
            return Err(StoreError::Busy(task.to_owned()).into());
        }
        match claim.update(|r| {
            if let Err(e) = r.set_loop(Some(&lp)) {
                tracing::warn!(task, error = %e, "cannot encode a loop");
            }
        }) {
            Ok(r) => {
                tracing::info!(task, rev = r.rev, "loop started");
                refresh_brief(r);
                return Ok(r.clone());
            }
            Err(StoreError::Conflict { .. }) => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(StoreError::Busy(task.to_owned()).into())
}

/// `stop-loop`: stops an active loop with reason `user`. Without an active
/// loop, the record is returned as it is. Blocking.
pub fn stop(config: &Config, task: &str) -> Result<Record, LoopError> {
    let store = open(config)?;
    for _ in 0..WRITE_ATTEMPTS {
        let record = store.get(task)?;
        let Some(lp) = record.task_loop().ok().flatten().filter(Loop::active) else {
            return Ok(record);
        };
        let stopped = lp.stopped(Reason::User, now());
        let note = effects(&lp, &stopped).note;
        match store.update(task, record.rev, |r| {
            if let Err(e) = r.set_loop(Some(&stopped)) {
                tracing::warn!(task, error = %e, "cannot encode a loop");
            }
            if note.is_some() {
                r.note = note.clone();
            }
        }) {
            Ok(r) => {
                tracing::info!(task, rev = r.rev, "loop stopped by the user");
                return Ok(r);
            }
            Err(StoreError::Conflict { .. }) => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(StoreError::Busy(task.to_owned()).into())
}

/// `daemon stop` and `uninstall`: every active loop stops with reason
/// `user`. Returns the ids it stopped. Blocking.
pub fn stop_all(config: &Config) -> Result<Vec<String>, LoopError> {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return Ok(Vec::new());
    };
    let active: Vec<String> = Store::open(dir)?
        .list()?
        .into_iter()
        .filter(|r| r.task_loop().ok().flatten().is_some_and(|l| l.active()))
        .map(|r| r.id)
        .collect();
    for id in &active {
        stop(config, id)?;
    }
    Ok(active)
}

fn open(config: &Config) -> Result<Store, LoopError> {
    let dir = config
        .state_dir
        .as_ref()
        .ok_or_else(|| LoopError::Bad("this host keeps no task records: HOME is not set".into()))?;
    Ok(Store::open(dir)?)
}

/// `TASK.md` gets the check; the agent's notes stay. A failure is logged:
/// the loop works without it.
fn refresh_brief(record: &Record) {
    let Some(dir) = &record.dir else { return };
    if let Err(e) = crate::task_records::brief::refresh(dir, record) {
        tracing::warn!(task = %record.id, error = %e, "cannot write TASK.md");
    }
}
