//! `ask-agent` (ADR 0041): one prompt with the item and the user's text, typed
//! into the workspace's Ask session at its root. The item stays open. The
//! prompt holds the item's summary, so it never reaches a log (invariant 5).
use std::sync::Arc;
use std::time::Duration;

use muxa_protocol::{ErrorBody, ErrorCode, InboxItem, ItemKind, ItemState, Message};
use muxa_tmux::{AgentState, SessionId};

use super::answer::MAX_TEXT;
use crate::convert::{self, error_body};
use crate::loops::typing;
use crate::task_records::handoff::{Linked, PROMPT_WAIT, agent_pane};
use crate::task_records::{Context, Env};

const POLL: Duration = Duration::from_secs(1);

/// `update-inbox` with `ask-agent`: `session-started`, then `inbox-saved`.
pub async fn ask(
    cx: &Context,
    request: u64,
    id: &str,
    text: Option<&str>,
) -> Result<Vec<Message>, ErrorBody> {
    let text = text
        .filter(|t| !t.trim().is_empty() && t.len() <= MAX_TEXT)
        .ok_or_else(|| bad(format!("ask-agent needs a text of 1 to {MAX_TEXT} bytes")))?;
    let item = open_item(cx, id).await?;
    let env = cx.env().await?;
    let (session, name, fresh) = root_session(&env, &item.workspace).await?;
    deliver(&env, session, &prompt(&item, text), fresh).await?;
    tracing::info!(item = id, session = %session, fresh, "asked the workspace's agent");
    Ok(vec![
        Message::SessionStarted {
            start: request,
            session: session.to_string(),
            name,
        },
        Message::InboxSaved {
            inbox: request,
            entry: None,
            item: Some(Box::new(item)),
        },
    ])
}

/// The item as listed, if it is open: a Task's item or a Routine's entry.
async fn open_item(cx: &Context, id: &str) -> Result<InboxItem, ErrorBody> {
    let (config, id) = (Arc::clone(&cx.records), id.to_owned());
    tokio::task::spawn_blocking(move || {
        let entries = crate::routines::inbox::list(&config)?;
        let item = super::list(&config, &entries)?
            .into_iter()
            .find(|i| i.id == id)
            .ok_or_else(|| bad(format!("no Inbox item {id}; list the Inbox again")))?;
        if item.state != ItemState::Open {
            return Err(bad(format!("Inbox item {id} is already closed")));
        }
        Ok(item)
    })
    .await
    .map_err(|e| error_body(ErrorCode::Internal, format!("inbox lookup failed: {e}")))?
}

/// The workspace's Ask session: the open one, or a new one (`true`).
async fn root_session(env: &Env, workspace: &str) -> Result<(SessionId, String, bool), ErrorBody> {
    let sessions = muxa_tmux::list_sessions(&env.client)
        .await
        .map_err(|e| convert::error(&e))?;
    if let Some(s) = crate::tasks::ask::root_session(&sessions, workspace) {
        return Ok((s.id, s.name.clone(), false));
    }
    let config = Arc::clone(&env.records);
    let workspaces = tokio::task::spawn_blocking(move || config.load_workspaces())
        .await
        .map_err(|e| error_body(ErrorCode::Internal, format!("workspace lookup failed: {e}")))??;
    let launched =
        crate::tasks::ask::start_root(&env.client, &env.tasks.command, &workspaces, workspace)
            .await
            .map_err(|e| {
                tracing::warn!(workspace, error = %e, "workspace ask session did not start");
                convert::task_error(&e)
            })?;
    Ok((launched.id, launched.name, true))
}

/// Pastes `prompt` into the session's agent pane. A new agent gets up to
/// [`PROMPT_WAIT`] seconds to report `idle`; a working one gets the text as
/// typed input, which Claude Code takes after its turn (ADR 0041 §4).
async fn deliver(
    env: &Env,
    session: SessionId,
    prompt: &str,
    fresh: bool,
) -> Result<(), ErrorBody> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(PROMPT_WAIT);
    let found = loop {
        let found = agent_pane(
            &env.client,
            &env.agents,
            &env.records,
            Linked::Session(session),
        )
        .await;
        let state = found.as_ref().and_then(|f| f.status).map(|s| s.state);
        match (fresh, state) {
            (true, Some(AgentState::Idle)) => break found,
            (true, _) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(POLL).await;
            }
            (true, _) => {
                return Err(conflict(
                    "the Ask session's agent did not become idle within 30 seconds; \
                     type the request in its session",
                ));
            }
            (false, state) => {
                ready(state)?;
                break found;
            }
        }
    };
    let found = found
        .ok_or_else(|| conflict("the Ask session has no agent pane; close it and ask again"))?;
    let quiet = typing::quiet(&env.client, found.session, crate::records::now())
        .await
        .map_err(|e| convert::error(&e))?;
    if !quiet {
        return Err(conflict(
            "someone is typing in the Ask session; try again in a few seconds",
        ));
    }
    typing::paste_prompt(&env.client, found.pane, prompt)
        .await
        .map_err(|e| {
            tracing::warn!(session = %session, pane = %found.pane, error = %e, "cannot send the request");
            convert::error(&e)
        })
}

/// Any state but a menu: there the text would pick an option.
fn ready(state: Option<AgentState>) -> Result<(), ErrorBody> {
    match state {
        Some(AgentState::Waiting) => Err(conflict(
            "the Ask session's agent waits on a menu; answer it in its session first",
        )),
        Some(_) => Ok(()),
        None => Err(conflict(
            "the Ask session's agent state is unknown; type the request in its session",
        )),
    }
}

/// Pure. The item as the agent reads it, then the user's text.
fn prompt(item: &InboxItem, text: &str) -> String {
    let kind = match item.kind {
        ItemKind::Ask => "question from an agent",
        ItemKind::Review => "review",
        ItemKind::Failure => "failure",
        ItemKind::Proposal => "proposal",
        ItemKind::Briefing => "briefing",
        ItemKind::Sort => "proposal to sort a capture",
        ItemKind::Delegate => "proposal to ask a delegate",
        ItemKind::Hypothesis => "hypothesis about the user",
        ItemKind::Unknown => "item",
    };
    let source = match &item.routine {
        Some(r) => format!("Routine {r}"),
        None => format!("Task {}", item.title),
    };
    let mut out = format!(
        "About this Inbox item ({kind}, from {source} in workspace {}):\n\n{}\n",
        item.workspace, item.question
    );
    if let Some(summary) = item.summary.as_deref().filter(|s| !s.is_empty()) {
        out.push_str(&format!("\n{summary}\n"));
    }
    if let Some(p) = &item.proposal {
        out.push_str(&format!(
            "\nProposed action: {}\nTarget: {}\n\n{}\n",
            p.action, p.target, p.content
        ));
    }
    if !item.attachments.links.is_empty() {
        out.push_str("\nLinks:\n");
        for link in &item.attachments.links {
            out.push_str(&format!("- {}\n", link.url));
        }
    }
    out.push_str(&format!("\nMy request: {text}"));
    out
}

fn bad(m: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::BadRequest, m.into())
}

fn conflict(m: &str) -> ErrorBody {
    error_body(ErrorCode::Conflict, m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muxa_protocol::SourceLink;

    fn briefing() -> InboxItem {
        let mut i = super::super::store::tests_item("e-00000001", ItemState::Open);
        i.kind = ItemKind::Briefing;
        i.workspace = "lisk".into();
        i.routine = Some("delivery-daily".into());
        i.question = "Lisk v0.7 is due tomorrow".into();
        i.summary = Some("**10/14 projects done.**".into());
        i.attachments.links = vec![SourceLink {
            url: "https://linear.app/x/ENA-257".into(),
            title: None,
        }];
        i
    }

    #[test]
    fn the_prompt_holds_the_item_then_the_request() {
        let p = prompt(&briefing(), "Put this in Notion.");
        assert!(p.starts_with(
            "About this Inbox item (briefing, from Routine delivery-daily in workspace lisk):\n\nLisk v0.7 is due tomorrow\n"
        ));
        assert!(p.contains("\n**10/14 projects done.**\n"));
        assert!(p.contains("\nLinks:\n- https://linear.app/x/ENA-257\n"));
        assert!(p.ends_with("\nMy request: Put this in Notion."));
    }

    #[test]
    fn a_working_agent_takes_the_text_but_a_menu_does_not() {
        for ok in [
            AgentState::Idle,
            AgentState::Running,
            AgentState::Done,
            AgentState::Error,
        ] {
            assert!(ready(Some(ok)).is_ok(), "{ok:?}");
        }
        assert_eq!(
            ready(Some(AgentState::Waiting)).unwrap_err().code,
            ErrorCode::Conflict
        );
        assert_eq!(ready(None).unwrap_err().code, ErrorCode::Conflict);
    }
}
