//! Answers an `ask` in the pane, through the approval contract of ADR 0009:
//! only while the adapter reports the same menu the item asked about.
use std::sync::Arc;

use muxa_protocol::{ChatPrompt, ErrorBody, ErrorCode};
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
    let current = tokio::task::spawn_blocking(move || current(&agents, &info))
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

/// Blocking. The menu the pane's adapter reports now; None when there is none.
fn current(agents: &Registry, info: &PaneInfo) -> Result<Option<ChatPrompt>, ErrorBody> {
    let failed = |e: crate::agents::Error| {
        tracing::warn!(pane = %info.pane, error = %e, "cannot read the agent's conversation");
        error_body(ErrorCode::Internal, "cannot read the agent's conversation")
    };
    let Some((_, mut source)) = agents.open(info).map_err(failed)? else {
        return Ok(None);
    };
    source.poll(info).map_err(failed)?;
    Ok(source.prompt())
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
