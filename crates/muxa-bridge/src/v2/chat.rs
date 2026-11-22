//! Chat on protocol v2 (plan F4): the conversation of the agent working in a
//! Task's folder, from the agent's own records through the adapters of
//! `agents/` (`docs/agents.md`). The server polls each open chat; the
//! approval card's keys become terminal input here, and only keys an adapter
//! checked against the agent's menu are offered (ADR 0009). Conversation text
//! never reaches the log.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use muxa_protocol::v2::{ChatOption, ChatPrompt, ChatUpdate};
use muxa_server::TaskError;

use crate::agents::{ChatEvent, ChatSource, Error, PaneInfo, Registry};

/// One open chat. The source is out of the map while a poll runs.
struct Slot {
    source: Option<Box<dyn ChatSource>>,
    dir: PathBuf,
    /// The prompt last sent, so an unchanged one is not sent again.
    prompt: Option<ChatPrompt>,
}

pub struct Chats {
    registry: Arc<Registry>,
    open: Mutex<HashMap<u64, Slot>>,
    next: AtomicU64,
}

impl Chats {
    pub fn new(registry: Arc<Registry>) -> Self {
        Chats {
            registry,
            open: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
        }
    }

    pub async fn open(&self, dir: &Path) -> Result<(u64, ChatUpdate), TaskError> {
        let (registry, dir) = (Arc::clone(&self.registry), dir.to_path_buf());
        let (dir, source, update) = tokio::task::spawn_blocking(move || {
            let (agent, mut source) =
                registry.open_folder(&dir).map_err(failed)?.ok_or_else(|| {
                    TaskError::Invalid(
                        "no Claude Code runs in this Task's folder, so it has no Chat".into(),
                    )
                })?;
            let events = source.poll(&pane(&dir)).map_err(failed)?;
            let mut update = update_of(events, source.as_ref());
            if !update.reset {
                update = ChatUpdate {
                    reset: true,
                    entries: source.entries(),
                    ..update
                };
            }
            tracing::info!(dir = %dir.display(), agent, "v2 chat opened");
            Ok::<_, TaskError>((dir, source, update))
        })
        .await
        .map_err(|e| TaskError::Failed(format!("the chat stopped: {e}")))??;
        let handle = self.next.fetch_add(1, Ordering::Relaxed);
        let slot = Slot {
            source: Some(source),
            dir,
            prompt: update.prompt.clone(),
        };
        self.lock().insert(handle, slot);
        Ok((handle, update))
    }

    /// What changed; None when nothing did. An error ends the chat.
    pub async fn poll(&self, handle: u64) -> Result<Option<ChatUpdate>, TaskError> {
        let (mut source, dir) = {
            let mut open = self.lock();
            let slot = open
                .get_mut(&handle)
                .ok_or_else(|| TaskError::Invalid(format!("no chat {handle}")))?;
            match slot.source.take() {
                Some(s) => (s, slot.dir.clone()),
                // A poll still runs.
                None => return Ok(None),
            }
        };
        let (source, polled) = tokio::task::spawn_blocking(move || {
            let polled = source.poll(&pane(&dir));
            let update = polled.map(|events| update_of(events, source.as_ref()));
            (source, update)
        })
        .await
        .map_err(|e| TaskError::Failed(format!("the chat stopped: {e}")))?;
        let mut open = self.lock();
        let Some(slot) = open.get_mut(&handle) else {
            return Ok(None);
        };
        slot.source = Some(source);
        match polled {
            Ok(update) => {
                let prompt_changed = update.prompt != slot.prompt;
                slot.prompt = update.prompt.clone();
                let empty = !update.reset && update.entries.is_empty();
                Ok((!empty || prompt_changed).then_some(update))
            }
            Err(Error::Gone(why)) => Err(TaskError::Failed(why)),
            Err(e) => {
                tracing::warn!(error = %e, "a v2 chat cannot be read now");
                Ok(None)
            }
        }
    }

    pub fn close(&self, handle: u64) {
        self.lock().remove(&handle);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Slot>> {
        self.open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The adapters read only the folder; protocol v2 has no pane.
fn pane(dir: &Path) -> PaneInfo {
    PaneInfo {
        pane: String::new(),
        session: String::new(),
        pid: 0,
        path: dir.display().to_string(),
        shell_foreground: false,
    }
}

fn failed(e: Error) -> TaskError {
    match e {
        Error::Gone(why) => TaskError::Invalid(why),
        e => TaskError::Failed(e.to_string()),
    }
}

/// The events as one update: a reset replaces what came before it.
fn update_of(events: Vec<ChatEvent>, source: &dyn ChatSource) -> ChatUpdate {
    let mut update = ChatUpdate {
        prompt: source.prompt().and_then(prompt),
        ..ChatUpdate::default()
    };
    for event in events {
        match event {
            ChatEvent::Reset { entries, truncated } => {
                update.reset = true;
                update.truncated = truncated;
                update.entries = entries;
            }
            ChatEvent::Upsert(entry) => {
                update.entries.retain(|e| e.id != entry.id);
                update.entries.push(entry);
            }
        }
    }
    update
}

/// The prompt with each option's keys as terminal input. A key this does
/// not know drops the whole prompt: muxa never guesses a menu's keys.
fn prompt(p: muxa_protocol::ChatPrompt) -> Option<ChatPrompt> {
    let options = p
        .options
        .into_iter()
        .map(|o| {
            let input: Option<String> = o.keys.iter().map(|k| key_input(k)).collect();
            Some(ChatOption {
                label: o.label,
                input: input?,
                cancel: o.cancel,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(ChatPrompt {
        entry: p.entry,
        kind: p.kind,
        question: p.question,
        options,
    })
}

/// The bytes of a tmux key name the adapters use.
fn key_input(key: &str) -> Option<&'static str> {
    const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
    match key {
        "Escape" => Some("\x1b"),
        "Enter" => Some("\r"),
        k => DIGITS.iter().find(|d| **d == k).copied(),
    }
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;
