//! Types the user's reply to a finished turn into a Task's agent pane, as its
//! next prompt (ADR 0033 §1). Only on a turn that ended, never on a menu.
use std::sync::Arc;

use muxa_protocol::{ErrorBody, ErrorCode};
use muxa_tmux::{AgentState, AgentStatus};

use super::create::Env;
use super::handoff::{Linked, agent_pane, bound_run_observation};
use super::{blocking, store_error};
use crate::convert::{self, error_body};
use crate::loops::typing;

/// Types `text` now, or refuses. Nothing waits for the turn to end.
pub async fn send(env: &Env, task: &str, text: &str) -> Result<(), ErrorBody> {
    let (config, id) = (Arc::clone(&env.records), task.to_owned());
    let record = blocking(move || config.store()?.get(&id).map_err(|e| store_error(&e))).await?;
    if record.extra.contains_key(crate::records::lifecycle::FIELD)
        || record.pending_prompt().is_some()
    {
        return Err(conflict(
            "the Task's agent is starting or restarting; try again in a moment",
        ));
    }
    let found = agent_pane(
        &env.client,
        &env.agents,
        &env.records,
        Linked::Task(task.to_owned()),
    )
    .await
    .ok_or_else(|| {
        error_body(
            ErrorCode::TaskFailed,
            "no session runs this Task with an agent pane",
        )
    })?;
    let found = bound_run_observation(&record, found).ok_or_else(|| {
        conflict("the agent in the Task's pane is not confirmed yet; reply in the terminal")
    })?;
    ready(found.status)?;
    let quiet = typing::quiet(&env.client, found.session, crate::records::now())
        .await
        .map_err(|e| convert::error(&e))?;
    if !quiet {
        return Err(conflict(
            "someone is typing in the Task's session; try again in a few seconds",
        ));
    }
    typing::paste_prompt(&env.client, found.pane, text)
        .await
        .map_err(|e| {
            tracing::warn!(task, pane = %found.pane, error = %e, "cannot send a reply");
            convert::error(&e)
        })?;
    tracing::info!(task, pane = %found.pane, "reply sent");
    Ok(())
}

/// A turn that ended: `idle`, `done`, or `error` (a failure's Change course).
/// Never `waiting`: the text would answer the menu.
fn ready(status: Option<AgentStatus>) -> Result<(), ErrorBody> {
    match status.map(|s| s.state) {
        Some(AgentState::Idle | AgentState::Done | AgentState::Error) => Ok(()),
        Some(AgentState::Waiting) => Err(conflict(
            "the agent waits on a menu; answer it first, then reply",
        )),
        Some(AgentState::Running) => {
            Err(conflict("the agent is working; reply when its turn ends"))
        }
        None => Err(conflict(
            "the agent's state is unknown; reply in the terminal",
        )),
    }
}

fn conflict(message: &str) -> ErrorBody {
    error_body(ErrorCode::Conflict, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(state: AgentState) -> Option<AgentStatus> {
        Some(AgentStatus { state, since: None })
    }

    #[test]
    fn a_reply_waits_for_a_turn_that_ended() {
        for ok in [AgentState::Idle, AgentState::Done, AgentState::Error] {
            assert!(ready(status(ok)).is_ok(), "{ok:?}");
        }
        for busy in [AgentState::Waiting, AgentState::Running] {
            assert_eq!(ready(status(busy)).unwrap_err().code, ErrorCode::Conflict);
        }
        assert_eq!(ready(None).unwrap_err().code, ErrorCode::Conflict);
    }
}
