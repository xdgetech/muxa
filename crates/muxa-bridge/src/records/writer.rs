//! `writer` and `writes` in `[workspace.assistant]` (ADR 0052 §3): the one
//! host worker that performs a workspace's approved writes, and whether the
//! workspace takes write proposals at all. Parsing and the checks against
//! `[[worker]]` only; `assistant/` makes the items and the writer's Tasks.
use muxa_protocol::Writes;

use super::staff::Staff;
use super::workers::{Worker, WorkerAgent};

/// A workspace's writer, as configured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Writer {
    /// None: the workspace takes no writes.
    pub worker: Option<String>,
    /// `review` only with a worker.
    pub writes: Writes,
}

impl Default for Writer {
    fn default() -> Self {
        Self {
            worker: None,
            writes: Writes::None,
        }
    }
}

impl Writer {
    /// The worker that performs approved writes, when the workspace takes them.
    pub fn active(&self) -> Option<&str> {
        self.worker
            .as_deref()
            .filter(|_| self.writes == Writes::Review)
    }
}

/// The shape of both fields. `staff` and `worker` are the workspace's
/// readers: a member that reads for the assistant never writes.
pub(super) fn parse(
    writer: Option<String>,
    writes: Option<&str>,
    overall: bool,
    staff: &Staff,
    worker: Option<&str>,
) -> Result<Writer, String> {
    if overall {
        if writer.is_some() || writes.is_some() {
            return Err("writer and writes do not apply to the overall workspace".into());
        }
        return Ok(Writer::default());
    }
    let writes = match (writes, &writer) {
        (None, Some(_)) | (Some("review"), Some(_)) => Writes::Review,
        (None, None) | (Some("none"), _) => Writes::None,
        (Some("review"), None) => {
            return Err(
                "writes = \"review\" needs a writer: name the worker that performs approved \
                 writes in writer, or set writes = \"none\""
                    .into(),
            );
        }
        (Some(_), _) => return Err("writes must be \"review\" or \"none\"".into()),
    };
    if let Some(name) = &writer {
        if worker == Some(name.as_str()) {
            return Err(format!(
                "writer {name:?} is also the workspace's worker: a worker that reads for the \
                 assistant never writes; name another worker"
            ));
        }
        if staff.members.contains(name) {
            return Err(format!(
                "writer {name:?} is on the staff: a worker that reads for the assistant never \
                 writes; name another worker"
            ));
        }
    }
    Ok(Writer {
        worker: writer,
        writes,
    })
}

/// The writer must be a Claude Code `[[worker]]` with a description that
/// this workspace may use (ADR 0048 §7, ADR 0052 §3).
pub(super) fn check(writer: &Writer, workspace: &str, workers: &[Worker]) -> Result<(), String> {
    let Some(name) = &writer.worker else {
        return Ok(());
    };
    let w = workers
        .iter()
        .find(|w| w.name == *name)
        .ok_or_else(|| format!("writer {name:?} is not configured"))?;
    if !w.allows(workspace) {
        return Err(format!(
            "writer {name:?} is not allowed in this workspace: add it to the worker's workspaces"
        ));
    }
    if w.agent != WorkerAgent::ClaudeCode {
        return Err(format!(
            "writer {name:?} runs {}, which has no automatic prompt delivery yet (ADR 0028); \
             the writer must be a Claude Code worker",
            w.agent.id()
        ));
    }
    if w.description.is_none() {
        return Err(format!(
            "writer {name:?} has no description: write its job description in the worker's \
             description"
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "writer_tests.rs"]
mod tests;
