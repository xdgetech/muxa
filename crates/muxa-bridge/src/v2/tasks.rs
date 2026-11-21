//! The host's side of the v2 server: a Task's branch worktree, as the
//! built-in launcher makes it, the Workspace's agent command and environment
//! (ADR 0016), and the Workspace names for Capture.

use std::sync::Arc;

use muxa_server::{Host, Prepared, RoutineList, TaskError, TaskSpec};

use crate::routines::runs::{self, StartError, When};
use crate::task_records::{self, Context};
use crate::tasks::{self, builtin};

pub struct BridgeHost {
    /// `--task-command`: the agent when the Workspace names none.
    command: String,
    records: Arc<task_records::Config>,
    /// The daemon's Routine engine, for Run now.
    engine: Option<Context>,
}

impl BridgeHost {
    pub fn new(command: String, records: Arc<task_records::Config>) -> Self {
        BridgeHost {
            command,
            records,
            engine: None,
        }
    }

    /// Runs Routines through the daemon's engine.
    pub fn with_engine(self, cx: Context) -> Self {
        BridgeHost {
            engine: Some(cx),
            ..self
        }
    }
}

impl Host for BridgeHost {
    async fn prepare(&self, spec: &TaskSpec) -> Result<Prepared, TaskError> {
        let req = tasks::validate(&spec.repo, &spec.branch, spec.base.as_deref())
            .await
            .map_err(convert)?;
        let agent = task_records::agent_for_repo(&self.records, &req.repo).await;
        let worktree = builtin::worktree(&req).await.map_err(convert)?;
        let command = agent
            .as_ref()
            .and_then(|a| a.command.clone())
            .unwrap_or_else(|| self.command.clone());
        let line = if spec.plan {
            builtin::plan_line(&command)
        } else {
            command
        };
        if let Some(a) = &agent {
            // Keys only: values may be tokens.
            tracing::info!(keys = ?a.keys(), "agent environment");
        }
        Ok(Prepared {
            dir: worktree.path,
            repo: worktree.main,
            branch: req.branch,
            argv: shell_line(line),
            env: agent
                .map(|a| a.env.into_iter().collect())
                .unwrap_or_default(),
        })
    }

    async fn routines(&self) -> Result<RoutineList, TaskError> {
        let records = Arc::clone(&self.records);
        let listed = tokio::task::spawn_blocking(move || crate::routines::list(&records))
            .await
            .map_err(|e| TaskError::Failed(e.to_string()))?;
        let (routines, problems) = listed.map_err(|e| TaskError::Failed(e.message))?;
        Ok(RoutineList {
            routines: routines.into_iter().map(super::routines::to_v2).collect(),
            problems: problems.into_iter().map(super::routines::problem).collect(),
        })
    }

    async fn run_routine(&self, workspace: &str, name: &str) -> Result<(), TaskError> {
        let cx = self
            .engine
            .as_ref()
            .ok_or_else(|| TaskError::Invalid("this host runs no Routines".into()))?;
        runs::start(cx, workspace, name, When::Now(crate::records::now()))
            .await
            .map(drop)
            .map_err(|e| match e {
                StartError::Bad(m) | StartError::Unsupported(m) => TaskError::Invalid(m),
                StartError::Busy(m) => TaskError::Busy(m),
                StartError::Handled => TaskError::Busy(e.to_string()),
                StartError::Failed(m) => TaskError::Failed(m),
            })
    }

    async fn workspaces(&self) -> Vec<String> {
        let records = Arc::clone(&self.records);
        let loaded = tokio::task::spawn_blocking(move || records.load_workspaces()).await;
        match loaded {
            Ok(Ok(w)) => w.all().iter().map(|w| w.name.clone()).collect(),
            Ok(Err(_)) | Err(_) => Vec::new(),
        }
    }
}

/// The agent command is a shell line, as tmux ran it.
fn shell_line(line: String) -> Vec<String> {
    vec!["/bin/sh".into(), "-c".into(), line]
}

fn convert(e: tasks::TaskError) -> TaskError {
    match e {
        tasks::TaskError::Invalid(m) => TaskError::Invalid(m),
        other => TaskError::Failed(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_repo_that_is_not_there_is_invalid() {
        let host = BridgeHost::new(
            "claude".into(),
            Arc::new(task_records::Config {
                state_dir: None,
                workspaces: None,
                home: None,
            }),
        );
        let spec = TaskSpec {
            repo: "/nowhere/at/all".into(),
            branch: "fix".into(),
            base: None,
            plan: false,
        };
        assert!(matches!(
            host.prepare(&spec).await,
            Err(TaskError::Invalid(_))
        ));
    }

    #[test]
    fn the_command_runs_through_the_shell() {
        assert_eq!(
            shell_line("claude --x".into()),
            vec!["/bin/sh", "-c", "claude --x"]
        );
    }
}
