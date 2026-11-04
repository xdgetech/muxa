//! Answers an `ask` in the pane, through the approval contract of ADR 0009:
//! only while the adapter reports the same menu the item asked about.
use std::sync::Arc;
use std::time::Duration;

use muxa_protocol::{AgentState, ChatPrompt, ErrorBody, ErrorCode};
use muxa_tmux::PaneId;

use crate::agents::{PaneInfo, Registry};
use crate::convert::{self, error_body};
use crate::task_records::Context;

/// Sends the keys of option `option` of the menu the agent shows now.
pub(super) async fn pick(
    cx: &Context,
    pane: PaneId,
    asked: &ChatPrompt,
    option: usize,
) -> Result<(), ErrorBody> {
    let env = cx.env().await?;
    let info = crate::chat::pane_info(&env.client, pane).await?;
    let agents = Arc::clone(&cx.agents);
    let (current, _) = tokio::task::spawn_blocking(move || current(&agents, &info))
        .await
        .map_err(|_| error_body(ErrorCode::Internal, "cannot read the agent's menu"))??;
    let keys = keys(asked, current.as_ref(), option)?;
    muxa_tmux::send_keys(&env.client, pane, &keys)
        .await
        .map_err(|e| {
            tracing::warn!(%pane, error = %e, "cannot answer the agent's menu");
            convert::error(&e)
        })?;
    tracing::info!(%pane, option, "menu answered");
    Ok(())
}

/// How long a denied menu has to go away, and how often to look.
const TAKE_WAIT: Duration = Duration::from_secs(10);
const TAKE_POLL: Duration = Duration::from_millis(250);

/// Waits until the pane's adapter no longer reports the menu `asked` and
/// reports the agent idle: the agent took the answer and its turn ended.
pub(super) async fn wait_taken(
    cx: &Context,
    pane: PaneId,
    asked: &ChatPrompt,
) -> Result<(), ErrorBody> {
    let env = cx.env().await?;
    let deadline = tokio::time::Instant::now() + TAKE_WAIT;
    loop {
        let info = crate::chat::pane_info(&env.client, pane).await?;
        let agents = Arc::clone(&cx.agents);
        let (menu, state) = tokio::task::spawn_blocking(move || current(&agents, &info))
            .await
            .map_err(|_| error_body(ErrorCode::Internal, "cannot read the agent's menu"))??;
        if taken(asked, menu.as_ref(), state) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!(%pane, "the agent did not take a denied menu in time");
            return Err(error_body(
                ErrorCode::Conflict,
                "the menu was denied, but the agent did not become idle; send the text in the terminal",
            ));
        }
        tokio::time::sleep(TAKE_POLL).await;
    }
}

/// Pure. The menu is gone and the adapter reports a turn that ended.
pub(super) fn taken(
    asked: &ChatPrompt,
    menu: Option<&ChatPrompt>,
    state: Option<AgentState>,
) -> bool {
    menu.is_none_or(|m| m.entry != asked.entry)
        && matches!(state, Some(AgentState::Idle | AgentState::Done))
}

/// Blocking. The menu the pane's adapter reports now and the agent's state;
/// None for what it does not report.
fn current(
    agents: &Registry,
    info: &PaneInfo,
) -> Result<(Option<ChatPrompt>, Option<AgentState>), ErrorBody> {
    let failed = |e: crate::agents::Error| {
        tracing::warn!(pane = %info.pane, error = %e, "cannot read the agent's conversation");
        error_body(ErrorCode::Internal, "cannot read the agent's conversation")
    };
    let Some((_, mut source)) = agents.open(info).map_err(failed)? else {
        return Ok((None, None));
    };
    source.poll(info).map_err(failed)?;
    Ok((source.prompt(), source.status().map(|s| s.state)))
}

/// Pure. The option's keys, when `current` is the menu the item asked about.
pub(super) fn keys(
    asked: &ChatPrompt,
    current: Option<&ChatPrompt>,
    option: usize,
) -> Result<Vec<String>, ErrorBody> {
    current
        .filter(|c| c.entry == asked.entry)
        .and_then(|c| c.options.get(option))
        .map(|o| o.keys.clone())
        .ok_or_else(|| {
            error_body(
                ErrorCode::Conflict,
                "the agent's menu changed since the item was made; list the Inbox again",
            )
        })
}
