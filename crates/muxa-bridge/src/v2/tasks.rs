//! The host's side of a v2 Task start: the branch worktree, as the built-in
//! launcher makes it, and the Workspace's agent command and environment
//! (ADR 0016). The server runs the agent on herdr-core.

use std::sync::Arc;

use muxa_server::{Prepared, TaskError, TaskSpec, Tasks};

use crate::task_records;
use crate::tasks::{self, builtin};

pub struct HostTasks {
    /// `--task-command`: the agent when the Workspace names none.
    command: String,
    records: Arc<task_records::Config>,
}

impl HostTasks {
    pub fn new(command: String, records: Arc<task_records::Config>) -> Self {
        HostTasks { command, records }
    }
}

impl Tasks for HostTasks {
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
        let host = HostTasks::new(
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
