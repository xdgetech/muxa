//! The Task Loop on the session backend (plan F3, ADR 0019): after each
//! finished turn (`done`, OSC 7501) of a Task's agent, the check runs in its
//! working place; a pass makes a Review item, a fail types the fixed prompt
//! until `max_attempts`, then makes a Failed item. The loop lives in the
//! working place's `.muxa/loop.json`, so a restarted daemon goes on with it.
//! Neither the check nor its output reaches the log.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use muxa_backend::{Change, ChangeStream, Labels, SessionBackend, TerminalId, TerminalInfo};
use muxa_inbox::{Inbox, NewItem};
use muxa_protocol::v2::{self, ItemKind, LoopInfo};
use muxa_server::TaskError;
use muxa_server::tasks::{BRANCH, DIR, KIND, ROLE, WORKSPACE};
use tokio::sync::watch;

use crate::loops::CheckEnd;
use crate::loops::check::{self, CheckRun};
use crate::loops::fail_prompt;
use crate::records::loops::{Checks, LastCheck, Loop, LoopSpec, LoopState, Reason};
use crate::records::{Workspaces, now};

const FILE: &str = ".muxa/loop.json";

/// A live Task agent with a loop file.
#[derive(Debug, Clone)]
struct Agent {
    dir: PathBuf,
    workspace: String,
    /// What items call the Task: its branch, else its title.
    title: String,
}

pub struct TaskLoops {
    config: Arc<crate::config::Config>,
    inbox: Arc<Inbox>,
    changes: watch::Sender<u64>,
    agents: Mutex<HashMap<TerminalId, Agent>>,
}

impl TaskLoops {
    pub fn new(config: Arc<crate::config::Config>, inbox: Arc<Inbox>) -> Self {
        TaskLoops {
            config,
            inbox,
            changes: watch::channel(0).0,
            agents: Mutex::new(HashMap::new()),
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    /// Checks `spec` against the Workspace's `[workspace.loop]` checks, and
    /// fills in the check when the Workspace allows only one.
    pub async fn resolve(
        &self,
        workspace: &str,
        spec: &v2::LoopSpec,
    ) -> Result<LoopSpec, TaskError> {
        let (config, workspace, spec) =
            (Arc::clone(&self.config), workspace.to_owned(), spec.clone());
        blocking(move || {
            let workspaces = load(&config)?;
            let ws = workspaces
                .get(&workspace)
                .ok_or_else(|| TaskError::Invalid(format!("no workspace {workspace:?}")))?;
            let check = match (spec.check, &ws.loop_checks) {
                (Some(c), _) => c,
                (None, Some(Checks::List(list))) if list.len() == 1 => list[0].clone(),
                (None, _) => {
                    return Err(TaskError::Invalid(
                        "name the check: the workspace allows more than one, or any".into(),
                    ));
                }
            };
            let spec = LoopSpec {
                check,
                max_attempts: spec.max_attempts,
                timeout: spec.timeout,
            };
            spec.check(ws.loop_checks.as_ref())
                .map_err(TaskError::Invalid)?;
            Ok(spec)
        })
        .await
    }

    pub async fn start(
        &self,
        t: &TerminalInfo,
        spec: &v2::LoopSpec,
        reset: bool,
    ) -> Result<LoopInfo, TaskError> {
        let agent = agent_of(t)
            .ok_or_else(|| TaskError::Invalid(format!("{} is not a Task's agent", t.id)))?;
        let spec = self.resolve(&agent.workspace, spec).await?;
        let dir = agent.dir.clone();
        let lp = blocking(move || {
            let previous = read(&dir);
            let lp = Loop::start(spec, previous.as_ref(), reset, now());
            write(&dir, &lp)?;
            Ok(lp)
        })
        .await?;
        self.lock().insert(t.id.clone(), agent);
        tracing::info!(terminal = %t.id, "task loop started");
        self.changed();
        Ok(info(&t.id, &lp))
    }

    pub async fn stop(&self, terminal: &str) -> Result<LoopInfo, TaskError> {
        let id = TerminalId::new(terminal.to_owned());
        let agent = self
            .agent(&id)
            .ok_or_else(|| TaskError::Invalid(format!("{terminal} has no Task Loop")))?;
        let lp = self
            .update(&id, &agent, |lp| Some(lp.stopped(Reason::User, now())))
            .await
            .ok_or_else(|| TaskError::Failed(format!("cannot stop the loop of {terminal}")))?;
        Ok(info(&id, &lp))
    }

    pub async fn list(&self) -> Vec<LoopInfo> {
        let agents: Vec<(TerminalId, Agent)> = self
            .lock()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        blocking(move || {
            let mut out: Vec<LoopInfo> = agents
                .iter()
                .filter_map(|(id, a)| read(&a.dir).map(|lp| info(id, &lp)))
                .collect();
            out.sort_by(|a, b| a.terminal.cmp(&b.terminal));
            Ok(out)
        })
        .await
        .unwrap_or_default()
    }

    /// Follows the Task agents until the subscription ends.
    pub async fn watch<B>(self: &Arc<Self>, backend: &B) -> Result<(), String>
    where
        B: SessionBackend + Clone + 'static,
    {
        let filter = Labels::from([
            (KIND.to_owned(), "task".to_owned()),
            (ROLE.to_owned(), "agent".to_owned()),
        ]);
        let (now_live, mut changes) = backend
            .subscribe(&filter)
            .await
            .map_err(|e| e.to_string())?;
        self.rebuild(&now_live).await;
        while let Some(change) = changes.next().await {
            match change.map_err(|e| e.to_string())? {
                Change::Created(t) => self.rebuild_one(&t).await,
                Change::Status { id, status } => {
                    let state = status.map(|s| s.state).unwrap_or_default();
                    self.turn(backend, &id, &state).await;
                }
                Change::Ended { id, .. } => {
                    if let Some(a) = self.agent(&id) {
                        self.update(&id, &a, |lp| {
                            lp.active().then(|| lp.stopped(Reason::NoAgent, now()))
                        })
                        .await;
                    }
                }
                Change::Removed { id } => {
                    if self.lock().remove(&id).is_some() {
                        self.changed();
                    }
                }
                Change::Resync(all) => self.rebuild(&all).await,
                _ => {}
            }
        }
        Ok(())
    }

    /// What the agent reported: a finished turn checks, a question pauses.
    async fn turn<B>(self: &Arc<Self>, backend: &B, id: &TerminalId, state: &str)
    where
        B: SessionBackend + Clone + 'static,
    {
        let Some(agent) = self.agent(id) else { return };
        let dir = agent.dir.clone();
        let Ok(Some(lp)) = blocking(move || Ok(read(&dir))).await else {
            return;
        };
        let waiting = lp.state == LoopState::Paused && lp.reason == Some(Reason::Waiting);
        match state {
            "done" | "idle" if lp.state == LoopState::Armed || waiting => {
                let (this, backend, id) = (Arc::clone(self), backend.clone(), id.clone());
                tokio::spawn(async move { this.check(&backend, &id, agent).await });
            }
            "blocked" | "waiting" if lp.state == LoopState::Armed => {
                self.update(id, &agent, |lp| {
                    Some(Loop {
                        state: LoopState::Paused,
                        reason: Some(Reason::Waiting),
                        since: now(),
                        ..lp.clone()
                    })
                })
                .await;
            }
            "working" if waiting => {
                self.update(id, &agent, |lp| Some(armed(lp, lp.armed_at)))
                    .await;
            }
            _ => {}
        }
    }

    /// Runs the check, then passes, types the fail prompt or stops.
    async fn check<B: SessionBackend>(&self, backend: &B, id: &TerminalId, agent: Agent) {
        let claimed = self
            .update(id, &agent, |lp| {
                (lp.state == LoopState::Armed || lp.state == LoopState::Paused).then(|| Loop {
                    state: LoopState::Checking,
                    reason: None,
                    since: now(),
                    ..lp.clone()
                })
            })
            .await;
        let Some(lp) = claimed.filter(|lp| lp.state == LoopState::Checking) else {
            return;
        };
        let attempt = lp.attempts + 1;
        let end = match self.environment(&agent.workspace).await {
            Ok(env) => check::run(CheckRun {
                dir: agent.dir.clone(),
                command: lp.check.clone(),
                attempt,
                timeout: Duration::from_secs(lp.timeout),
                env,
            })
            .await
            .map_err(|e| e.to_string()),
            Err(e) => Err(e),
        };
        let last = |exit: Option<i32>, seconds: u64| LastCheck {
            attempt,
            exit,
            seconds,
            at: now(),
        };
        let (next, item) = match end {
            Err(e) => {
                tracing::warn!(terminal = %id, error = %e, "a task loop check did not run");
                let item = failed(
                    id,
                    &agent,
                    "The check could not run. Its log is in .muxa/checks/.",
                );
                (Some(Reason::Error), Some(item))
            }
            Ok(CheckEnd::Exited { code: 0, seconds }) => {
                let pass = Loop {
                    state: LoopState::Passed,
                    reason: None,
                    since: now(),
                    last_check: Some(last(Some(0), seconds)),
                    ..lp.clone()
                };
                self.finish(id, &agent, pass, Some(review(id, &agent)))
                    .await;
                return;
            }
            Ok(end) => {
                let (exit, seconds) = match end {
                    CheckEnd::Exited { code, seconds } => (Some(code), seconds),
                    CheckEnd::TimedOut { seconds } => (None, seconds),
                };
                let fail = Loop {
                    attempts: attempt,
                    last_check: Some(last(exit, seconds)),
                    ..lp.clone()
                };
                if attempt >= lp.max_attempts {
                    let note = format!(
                        "The check failed {attempt} times. The output is in .muxa/CHECK.md."
                    );
                    let stop = Loop {
                        state: LoopState::Stopped,
                        reason: Some(Reason::Limit),
                        since: now(),
                        ..fail
                    };
                    self.finish(id, &agent, stop, Some(failed(id, &agent, &note)))
                        .await;
                    return;
                }
                match super::runner::type_line(backend, id, &fail_prompt(attempt)).await {
                    Ok(()) => {
                        self.finish(id, &agent, armed(&fail, now()), None).await;
                        return;
                    }
                    Err(e) => {
                        tracing::info!(terminal = %id, error = %e, "the fail prompt was not typed");
                        (Some(Reason::NotSent), None)
                    }
                }
            }
        };
        let stop = Loop {
            state: LoopState::Stopped,
            reason: next,
            since: now(),
            ..lp
        };
        self.finish(id, &agent, stop, item).await;
    }

    /// Saves where the check left the loop, unless the user stopped it
    /// meanwhile, and adds the item.
    async fn finish(&self, id: &TerminalId, agent: &Agent, next: Loop, item: Option<NewItem>) {
        let want = next.clone();
        let saved = self
            .update(id, agent, move |lp| {
                (lp.state == LoopState::Checking).then_some(want)
            })
            .await;
        if saved.is_some_and(|lp| lp == next)
            && let Some(item) = item
            && let Err(e) = self.inbox.change(|b, at| b.add(item, at).map(drop))
        {
            tracing::warn!(terminal = %id, error = %e, "no Task Loop item");
        }
    }

    /// The check's environment: the daemon's own, with the Workspace's
    /// `agent.env` on top, as the agent gets it.
    async fn environment(&self, workspace: &str) -> Result<Vec<(String, String)>, String> {
        let (config, workspace) = (Arc::clone(&self.config), workspace.to_owned());
        blocking(move || {
            let workspaces = load(&config)?;
            let agent = workspaces.get(&workspace).and_then(|w| w.agent.clone());
            let path = std::env::var("PATH").ok();
            Ok(check::environment(
                None,
                agent.as_ref(),
                std::env::vars(),
                path.as_deref(),
            ))
        })
        .await
        .map_err(|e| e.to_string())
    }

    /// Applies `f` to the stored loop and saves what it returns; the loop as
    /// it is afterwards.
    async fn update(
        &self,
        id: &TerminalId,
        agent: &Agent,
        f: impl FnOnce(&Loop) -> Option<Loop> + Send + 'static,
    ) -> Option<Loop> {
        let dir = agent.dir.clone();
        let saved = blocking(move || {
            let Some(lp) = read(&dir) else {
                return Ok(None);
            };
            match f(&lp) {
                Some(next) if next != lp => {
                    write(&dir, &next)?;
                    Ok(Some((next, true)))
                }
                _ => Ok(Some((lp, false))),
            }
        })
        .await;
        match saved {
            Ok(Some((lp, changed))) => {
                if changed {
                    self.changed();
                }
                Some(lp)
            }
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(terminal = %id, error = %e, "cannot save a task loop");
                None
            }
        }
    }

    /// The live agents with a loop file. A check that a restart cut off is
    /// armed again.
    async fn rebuild(&self, live: &[TerminalInfo]) {
        let agents: Vec<(TerminalId, Agent)> = live
            .iter()
            .filter(|t| t.live)
            .filter_map(|t| Some((t.id.clone(), agent_of(t)?)))
            .collect();
        let found = blocking(move || {
            let mut found = HashMap::new();
            for (id, a) in agents {
                let Some(lp) = read(&a.dir) else { continue };
                if lp.state == LoopState::Checking {
                    write(&a.dir, &armed(&lp, now()))?;
                }
                found.insert(id, a);
            }
            Ok(found)
        })
        .await
        .unwrap_or_default();
        *self.lock() = found;
        self.changed();
    }

    async fn rebuild_one(&self, t: &TerminalInfo) {
        let Some(a) = agent_of(t) else { return };
        let dir = a.dir.clone();
        if blocking(move || Ok(read(&dir).is_some()))
            .await
            .unwrap_or(false)
        {
            self.lock().insert(t.id.clone(), a);
            self.changed();
        }
    }

    fn agent(&self, id: &TerminalId) -> Option<Agent> {
        self.lock().get(id).cloned()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<TerminalId, Agent>> {
        self.agents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn changed(&self) {
        self.changes.send_modify(|n| *n += 1);
    }
}

fn armed(lp: &Loop, at: u64) -> Loop {
    Loop {
        state: LoopState::Armed,
        reason: None,
        armed_at: at,
        since: now(),
        ..lp.clone()
    }
}

fn agent_of(t: &TerminalInfo) -> Option<Agent> {
    Some(Agent {
        dir: PathBuf::from(t.labels.get(DIR)?),
        workspace: t.labels.get(WORKSPACE)?.clone(),
        title: t
            .labels
            .get(BRANCH)
            .or(t.name.as_ref())
            .cloned()
            .unwrap_or_else(|| "a Task".into()),
    })
}

fn review(id: &TerminalId, agent: &Agent) -> NewItem {
    NewItem {
        kind: ItemKind::Review,
        title: format!("Review {}", agent.title),
        note: Some("The Task Loop's check passed.".into()),
        workspace: Some(agent.workspace.clone()),
        due: None,
        source: "task".into(),
        key: Some(format!("review:{id}")),
        once: false,
        terminal: Some(id.as_str().to_owned()),
        routine: None,
        place: None,
    }
}

fn failed(id: &TerminalId, agent: &Agent, note: &str) -> NewItem {
    NewItem {
        kind: ItemKind::Failed,
        title: format!("The Task Loop of {} stopped", agent.title),
        note: Some(note.to_owned()),
        workspace: Some(agent.workspace.clone()),
        due: None,
        source: "task".into(),
        key: Some(format!("loop:{id}:{}", now())),
        once: true,
        terminal: Some(id.as_str().to_owned()),
        routine: None,
        place: None,
    }
}

pub fn info(id: &TerminalId, lp: &Loop) -> LoopInfo {
    LoopInfo {
        terminal: id.as_str().to_owned(),
        check: lp.check.clone(),
        max_attempts: lp.max_attempts,
        timeout: lp.timeout,
        state: String::from(lp.state.clone()),
        attempts: lp.attempts,
        armed_at: lp.armed_at,
        since: lp.since,
        reason: lp.reason.clone().map(String::from),
        last_check: lp.last_check.as_ref().map(|c| v2::LastCheck {
            attempt: c.attempt,
            exit: c.exit,
            seconds: c.seconds,
            at: c.at,
        }),
    }
}

/// Blocking. The loop in `dir`, or None when there is none or it is unreadable.
fn read(dir: &Path) -> Option<Loop> {
    let text = std::fs::read_to_string(dir.join(FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Blocking. Writes through a temporary file, so a reader never sees half.
fn write(dir: &Path, lp: &Loop) -> Result<(), TaskError> {
    let path = dir.join(FILE);
    let fail =
        |e: std::io::Error| TaskError::Failed(format!("cannot write {}: {e}", path.display()));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(fail)?;
    }
    let text = serde_json::to_string_pretty(lp).map_err(|e| TaskError::Failed(e.to_string()))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(fail)?;
    std::fs::rename(&tmp, &path).map_err(fail)
}

fn load(config: &crate::config::Config) -> Result<Workspaces, TaskError> {
    crate::routines::runs::load(config).map_err(|e| TaskError::Failed(e.to_string()))
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, TaskError> + Send + 'static,
) -> Result<T, TaskError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| TaskError::Failed(format!("the Task Loop stopped: {e}")))?
}

#[cfg(test)]
#[path = "task_loop_tests.rs"]
mod tests;
