//! Settings kept in memory (ADR 0074), for the fake server and tests. Ops
//! follow the daemon's rules for names and revisions, but check nothing of
//! the files: no paths, schedules or loading rules. A secret only turns a
//! connection's state to `set`; neither it nor an env value is kept.

use std::sync::Mutex;

use muxa_protocol::settings::{
    EnvKey, SecretState, Settings, SettingsAgent, SettingsBot, SettingsConnection, SettingsLoop,
    SettingsOp, SettingsRoutine, SettingsWorkspace,
};

use crate::TaskError;

pub struct MemorySettings {
    current: Mutex<Settings>,
}

impl MemorySettings {
    /// `start.revision` should be a number; each update adds 1.
    pub fn new(start: Settings) -> Self {
        MemorySettings {
            current: Mutex::new(start),
        }
    }

    pub fn view(&self) -> Settings {
        self.current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn update(&self, revision: &str, ops: Vec<SettingsOp>) -> Result<Settings, TaskError> {
        let mut current = self
            .current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if current.read_only {
            return Err(TaskError::ReadOnly(
                "the host locked its settings; change them on the host".into(),
            ));
        }
        if current.revision != revision {
            return Err(TaskError::Stale(
                "the settings changed since they were read; read them again".into(),
            ));
        }
        let mut next = current.clone();
        for (i, op) in ops.into_iter().enumerate() {
            let name = op.name();
            apply(&mut next, op)
                .map_err(|why| TaskError::Invalid(format!("ops[{i}] {name}: {why}")))?;
        }
        let n: u64 = next.revision.parse().unwrap_or(0);
        next.revision = (n + 1).to_string();
        *current = next.clone();
        Ok(next)
    }
}

fn workspace<'a>(s: &'a mut Settings, name: &str) -> Result<&'a mut SettingsWorkspace, String> {
    s.workspaces
        .iter_mut()
        .find(|w| w.name == name)
        .ok_or_else(|| format!("no Workspace named {name:?}"))
}

/// Adds `item` when no other has its name, or replaces the one that has.
fn put<T>(list: &mut Vec<T>, item: T, add: bool, name: impl Fn(&T) -> &str) -> Result<(), String> {
    let key = name(&item).to_owned();
    match (list.iter().position(|x| name(x) == key), add) {
        (None, true) => list.push(item),
        (Some(i), false) => list[i] = item,
        (Some(_), true) => return Err(format!("{key:?} exists already")),
        (None, false) => return Err(format!("there is no {key:?}")),
    }
    Ok(())
}

fn remove<T>(list: &mut Vec<T>, key: &str, name: impl Fn(&T) -> &str) -> Result<(), String> {
    let before = list.len();
    list.retain(|x| name(x) != key);
    if list.len() == before {
        return Err(format!("there is no {key:?}"));
    }
    Ok(())
}

fn apply(s: &mut Settings, op: SettingsOp) -> Result<(), String> {
    match op {
        SettingsOp::WorkspaceAdd { name, root, repos } => {
            let fresh = SettingsWorkspace {
                name,
                root,
                repos,
                found_repos: Vec::new(),
                hidden_repos: Vec::new(),
                park_after: None,
                agent: SettingsAgent::default(),
                task_loop: None,
                routines_enabled: false,
                direct_writes: false,
                connections: Vec::new(),
                mcp: Vec::new(),
                routines: Vec::new(),
                bots: Vec::new(),
                routines_problem: None,
            };
            put(&mut s.workspaces, fresh, true, |w| &w.name)
        }
        SettingsOp::WorkspaceUpdate {
            workspace: name,
            root,
            repos,
            hidden_repos,
            agent_command,
            park_after,
        } => {
            let w = workspace(s, &name)?;
            if let Some(hidden) = hidden_repos {
                w.hidden_repos = hidden;
            }
            if let Some(root) = root {
                w.root = root;
            }
            if let Some(repos) = repos {
                w.repos = repos;
            }
            if let Some(command) = agent_command {
                w.agent.command = command;
            }
            if let Some(park) = park_after {
                w.park_after = park;
            }
            Ok(())
        }
        SettingsOp::WorkspaceRemove { workspace } => {
            remove(&mut s.workspaces, &workspace, |w| &w.name)
        }
        SettingsOp::EnvSet {
            workspace: w, key, ..
        } => {
            let env = &mut workspace(s, &w)?.agent.env;
            if !env.iter().any(|e| e.key == key) {
                env.push(EnvKey { key, set: true });
                env.sort_by(|a, b| a.key.cmp(&b.key));
            }
            Ok(())
        }
        SettingsOp::EnvUnset { workspace: w, key } => {
            remove(&mut workspace(s, &w)?.agent.env, &key, |e| &e.key)
        }
        SettingsOp::LoopChecks {
            workspace: w,
            checks,
        } => {
            workspace(s, &w)?.task_loop = checks.map(|checks| SettingsLoop { checks });
            Ok(())
        }
        SettingsOp::RoutinesGate {
            workspace: w,
            enabled,
            direct_writes,
        } => {
            let w = workspace(s, &w)?;
            w.routines_enabled = enabled;
            w.direct_writes = direct_writes;
            Ok(())
        }
        SettingsOp::ConnectionAdd {
            workspace: w,
            connection,
            secret,
        } => {
            let state = secret.map_or(SecretState::Missing, |_| SecretState::Set);
            let item = SettingsConnection {
                spec: connection,
                secret: state,
            };
            put(&mut workspace(s, &w)?.connections, item, true, |c| {
                &c.spec.name
            })
        }
        SettingsOp::ConnectionUpdate {
            workspace: w,
            connection,
            secret,
        } => {
            let list = &mut workspace(s, &w)?.connections;
            let old = list
                .iter()
                .find(|c| c.spec.name == connection.name)
                .map_or(SecretState::Missing, |c| c.secret);
            let state = secret.map_or(old, |_| SecretState::Set);
            let item = SettingsConnection {
                spec: connection,
                secret: state,
            };
            put(list, item, false, |c| &c.spec.name)
        }
        SettingsOp::ConnectionSecret {
            workspace: w, name, ..
        } => {
            let c = workspace(s, &w)?
                .connections
                .iter_mut()
                .find(|c| c.spec.name == name)
                .ok_or_else(|| format!("there is no {name:?}"))?;
            c.secret = SecretState::Set;
            Ok(())
        }
        SettingsOp::ConnectionRemove { workspace: w, name } => {
            remove(&mut workspace(s, &w)?.connections, &name, |c| &c.spec.name)
        }
        SettingsOp::McpAdd { workspace: w, mcp } => {
            put(&mut workspace(s, &w)?.mcp, mcp, true, |m| &m.name)
        }
        SettingsOp::McpUpdate { workspace: w, mcp } => {
            put(&mut workspace(s, &w)?.mcp, mcp, false, |m| &m.name)
        }
        SettingsOp::McpRemove { workspace: w, name } => {
            remove(&mut workspace(s, &w)?.mcp, &name, |m| &m.name)
        }
        SettingsOp::RoutineAdd {
            workspace: w,
            routine,
            instructions_text,
        } => {
            let item = SettingsRoutine {
                spec: routine,
                instructions_text,
            };
            put(&mut workspace(s, &w)?.routines, item, true, |r| {
                &r.spec.name
            })
        }
        SettingsOp::RoutineUpdate {
            workspace: w,
            routine,
            instructions_text,
        } => {
            let list = &mut workspace(s, &w)?.routines;
            let kept = list
                .iter()
                .find(|r| r.spec.name == routine.name)
                .and_then(|r| r.instructions_text.clone());
            let item = SettingsRoutine {
                spec: routine,
                instructions_text: instructions_text.or(kept),
            };
            put(list, item, false, |r| &r.spec.name)
        }
        SettingsOp::RoutineRemove { workspace: w, name } => {
            remove(&mut workspace(s, &w)?.routines, &name, |r| &r.spec.name)
        }
        SettingsOp::BotAdd {
            workspace: w,
            bot,
            soul_text,
        } => {
            let item = SettingsBot {
                spec: bot,
                soul_text: Some(soul_text.unwrap_or_default()),
                memory_text: Some(String::new()),
                memory_versions: 0,
            };
            put(&mut workspace(s, &w)?.bots, item, true, |b| &b.spec.name)
        }
        SettingsOp::BotUpdate {
            workspace: w,
            bot,
            soul_text,
        } => {
            let list = &mut workspace(s, &w)?.bots;
            let old = list
                .iter()
                .find(|b| b.spec.name == bot.name)
                .cloned()
                .ok_or_else(|| format!("there is no {:?}", bot.name))?;
            let item = SettingsBot {
                spec: bot,
                soul_text: soul_text.or(old.soul_text),
                ..old
            };
            put(list, item, false, |b| &b.spec.name)
        }
        SettingsOp::BotRemove { workspace: w, name } => {
            let ws = workspace(s, &w)?;
            if ws
                .routines
                .iter()
                .any(|r| r.spec.bot.as_deref() == Some(&name))
            {
                return Err(format!(
                    "bot {name:?} still owns Routines; remove them first"
                ));
            }
            remove(&mut ws.bots, &name, |b| &b.spec.name)
        }
        SettingsOp::BotMemoryRestore {
            workspace: w,
            name,
            version,
        } => {
            let bot = workspace(s, &w)?
                .bots
                .iter_mut()
                .find(|b| b.spec.name == name)
                .ok_or_else(|| format!("there is no {name:?}"))?;
            if version == 0 || version > bot.memory_versions {
                return Err(format!("bot {name:?} has no kept memory {version}"));
            }
            Ok(())
        }
        SettingsOp::WorkerAdd { worker } => put(&mut s.workers, worker, true, |w| &w.name),
        SettingsOp::WorkerUpdate { worker } => put(&mut s.workers, worker, false, |w| &w.name),
        SettingsOp::WorkerRemove { name } => remove(&mut s.workers, &name, |w| &w.name),
    }
}
