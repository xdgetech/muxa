//! Routines (ADR 0026): a configured agent run on a trigger that publishes
//! briefings and proposals to the Inbox. Config is `records::routines`; this
//! module keeps the schedule and each Routine's state, and checks and
//! publishes a run's output. `v2::runner` runs them on herdr-core.

pub mod brief;
pub mod check;
pub mod identity;
pub mod output;
pub mod record;
pub mod runs;
pub mod schedule;
pub mod store;

use std::path::Path;

use muxa_protocol::{RoutineInfo, RoutineProblem, RoutineTrigger};

use crate::config::Config;
use crate::records::routines::Routine;
use store::{RoutineState, RoutineStore};

/// Every configured Routine with its state, in file order; the workspaces
/// whose routines file is broken; and those that turn their Routines off.
pub struct Listed {
    pub routines: Vec<RoutineInfo>,
    pub problems: Vec<RoutineProblem>,
    /// Workspace name and how many Routines its file has.
    pub off: Vec<(String, u32)>,
}

/// Blocking. See [`Listed`].
pub fn list(config: &Config) -> Result<Listed, String> {
    let workspaces = runs::load(config).map_err(|e| e.to_string())?;
    let store = match config.state_dir.as_ref().filter(|d| d.is_dir()) {
        Some(dir) => Some(RoutineStore::open(dir).map_err(|e| {
            tracing::warn!(error = %e, "cannot open routine state");
            e.to_string()
        })?),
        None => None,
    };
    let at = crate::records::now();
    let entries = workspaces
        .routines()
        .iter()
        .map(|r| {
            let state = store
                .as_ref()
                .and_then(|s| s.get(&r.workspace, &r.name).ok())
                .unwrap_or_else(|| RoutineState::new(&r.workspace, &r.name));
            info(r, &state, at)
        })
        .collect();
    let mut problems: Vec<RoutineProblem> = workspaces
        .routine_problems()
        .iter()
        .map(|p| {
            tracing::warn!(workspace = %p.workspace, reason = %p.reason, "routines file unusable");
            RoutineProblem {
                workspace: p.workspace.clone(),
                reason: p.reason.clone(),
            }
        })
        .collect();
    problems.extend(missing_repos(&workspaces));
    Ok(Listed {
        routines: entries,
        problems,
        off: turned_off(&workspaces),
    })
}

/// Workspaces without `[workspace.routines] enabled = true` whose file has
/// Routines: the loader skips them, so the apps say so.
fn turned_off(workspaces: &crate::records::Workspaces) -> Vec<(String, u32)> {
    workspaces
        .all()
        .iter()
        .filter(|w| w.routines.is_none())
        .filter_map(|w| Some((w.name.clone(), crate::records::routines::count(w)?)))
        .collect()
}

/// Listed repos that do not exist. The load skipped and logged them; the
/// user fixes `workspaces.toml`.
fn missing_repos(workspaces: &crate::records::Workspaces) -> Vec<RoutineProblem> {
    workspaces
        .all()
        .iter()
        .flat_map(|w| {
            w.missing_repos.iter().map(|path| RoutineProblem {
                workspace: w.name.clone(),
                reason: format!(
                    "repo {} in workspaces.toml does not exist; muxa skips it until you \
                     fix or remove that entry",
                    path.display()
                ),
            })
        })
        .collect()
}

/// The wire form. `nextDue` only for an enabled Routine with a trigger.
pub fn info(r: &Routine, state: &RoutineState, at: u64) -> RoutineInfo {
    let next_due = match (&r.trigger, r.enabled) {
        (Some(t), true) => jiff::Timestamp::from_second(at as i64)
            .ok()
            .and_then(|now| schedule::next_due(t, now, runs::last_slot(state)).ok())
            .and_then(|ts| u64::try_from(ts.as_second()).ok()),
        _ => None,
    };
    RoutineInfo {
        workspace: r.workspace.clone(),
        name: r.name.clone(),
        trigger: r.trigger.as_ref().map(|t| RoutineTrigger {
            at: t.at(),
            timezone: t.timezone.clone(),
            schedule: Some(t.schedule.clone()),
        }),
        enabled: r.enabled,
        worker: r.worker.clone(),
        project: r.project.clone(),
        next_due,
        last_success: state.last_success.clone(),
        last_failure: state.last_failure.clone(),
        running: state.running.clone(),
        bot: r.bot.clone(),
        on: r.on.map(|e| e.as_str().to_owned()),
    }
}

/// Blocking. The run folder's private output stage, and the state the last
/// run committed.
pub fn prepare_at(config: &Config, workspace: &str, name: &str, dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::DirBuilderExt;
    let stage = dir.join(output::STAGE_DIR);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&stage)
        .map_err(|e| format!("cannot create {}: {e}", stage.display()))?;
    let state_dir = runs::dir(config).map_err(|e| e.to_string())?;
    let state = RoutineStore::open(&state_dir)
        .and_then(|s| s.get(workspace, name))
        .map_err(|e| e.to_string())?;
    if let Some(value) = state.state {
        let path = dir.join(output::STATE);
        let bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
        std::fs::write(&path, bytes)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    Ok(())
}
