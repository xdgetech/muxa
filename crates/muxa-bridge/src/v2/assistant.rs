//! The Capture assistant (ADR 0084): one short Claude Code run per captured
//! item, with no Workspace, no connections and no MCP servers. Its
//! suggestion goes on the item; only the user's accept applies it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use muxa_backend::{
    Change, ChangeStream, Labels, Launch, SessionBackend, TerminalId, TerminalInfo,
};
use muxa_inbox::Inbox;
use muxa_protocol::v2::{InboxItem, ItemSuggestion};

use super::Herdr;
use super::assistant_brief::{self, RawSuggestion};
use super::review_runs::take_new;
use super::{bot_run, mcp, runner};
use crate::records::workers::LaunchCommand;
use crate::records::{Workspaces, now};

pub const KIND: &str = "assistant";
const ITEM: &str = "muxa.item";
const STARTED: &str = "muxa.started";
const CURSOR: &str = "assistant";
/// Runs at a time; more wait for the next scan.
const MAX_RUNNING: usize = 3;
/// A run that has not suggested by then ends.
const RUN_LIMIT: u64 = 10 * 60;
const SCAN: Duration = Duration::from_secs(30);

pub struct Assistant {
    config: Arc<crate::config::Config>,
    inbox: Arc<Inbox>,
    /// `--task-command`, when `[assistant]` names no worker.
    fallback: String,
}

impl Assistant {
    pub fn new(config: Arc<crate::config::Config>, inbox: Arc<Inbox>, fallback: String) -> Self {
        Assistant {
            config,
            inbox,
            fallback,
        }
    }

    /// Starts runs for new captured items, and ends runs past the limit.
    pub async fn scan<B: SessionBackend>(&self, backend: &B) {
        let live: Vec<TerminalInfo> = match backend.list().await {
            Ok(l) => l
                .into_iter()
                .filter(|t| t.live && t.labels.get("muxa.kind").map(String::as_str) == Some(KIND))
                .collect(),
            Err(_) => return,
        };
        let at = now();
        let mut running = live.len();
        for t in &live {
            let started: u64 = t
                .labels
                .get(STARTED)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            if at.saturating_sub(started) > RUN_LIMIT {
                runner::end_terminal(backend, &t.id).await;
                running -= 1;
            }
        }
        let config = Arc::clone(&self.config);
        let enabled = tokio::task::spawn_blocking(move || {
            config
                .load_workspaces()
                .is_ok_and(|all| all.assistant().enabled)
        })
        .await
        .unwrap_or(false);
        // Turned off, new items pass by; turned on, only what fits now.
        let room = if enabled {
            MAX_RUNNING.saturating_sub(running)
        } else {
            usize::MAX
        };
        if room == 0 {
            return;
        }
        let items = take_new(&self.inbox, CURSOR, room, |i| {
            i.source == "capture" && i.suggestion.is_none()
        });
        if !enabled || items.is_empty() {
            return;
        }
        let (config, fallback) = (Arc::clone(&self.config), self.fallback.clone());
        let launches = tokio::task::spawn_blocking(move || {
            let all = config.load_workspaces().ok()?;
            Some(
                items
                    .iter()
                    .filter_map(|item| match launch(&config, &all, &fallback, item) {
                        Ok(l) => Some(l),
                        Err(e) => {
                            tracing::info!(item = %item.id, error = %e, "no assistant run");
                            None
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
        for l in launches {
            if let Err(e) = backend.run(&l).await {
                tracing::info!(error = %e, "the assistant did not start");
            }
        }
    }

    /// Follows the assistant's terminals until the subscription ends.
    pub async fn watch<B: SessionBackend>(&self, backend: &B) -> Result<(), String> {
        let filter = Labels::from([("muxa.kind".to_owned(), KIND.to_owned())]);
        let (initial, mut changes) = backend
            .subscribe(&filter)
            .await
            .map_err(|e| e.to_string())?;
        let mut known: BTreeMap<TerminalId, TerminalInfo> =
            initial.into_iter().map(|t| (t.id.clone(), t)).collect();
        while let Some(change) = changes.next().await {
            match change.map_err(|e| e.to_string())? {
                Change::Created(t) => {
                    known.insert(t.id.clone(), t);
                }
                Change::Status { id, status } => {
                    let Some(t) = known.get_mut(&id) else {
                        continue;
                    };
                    let done = status.as_ref().is_some_and(|s| s.state == "done");
                    t.status = status;
                    if done {
                        let t = t.clone();
                        self.done(backend, &t).await;
                    }
                }
                Change::Removed { id } => {
                    known.remove(&id);
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// The run ended its turn: its suggestion goes on the item, its memory
    /// is kept, and its agent and folder go.
    async fn done<B: SessionBackend>(&self, backend: &B, t: &TerminalInfo) {
        let (Some(item), Some(dir)) = (t.labels.get(ITEM).cloned(), t.labels.get("muxa.dir"))
        else {
            return;
        };
        let dir = PathBuf::from(dir);
        let (config, inbox, folder) = (
            Arc::clone(&self.config),
            Arc::clone(&self.inbox),
            dir.clone(),
        );
        let kept = tokio::task::spawn_blocking(move || keep(&config, &inbox, &item, &folder)).await;
        if let Ok(Err(e)) = kept {
            tracing::info!(terminal = %t.id, error = %e, "no suggestion kept");
        }
        runner::end_terminal(backend, &t.id).await;
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Blocking. Checks the run's output against the Workspaces and Bots.
fn keep(
    config: &crate::config::Config,
    inbox: &Inbox,
    item: &str,
    dir: &Path,
) -> Result<(), String> {
    let bytes = bot_run::read_output(&dir.join(".muxa").join(assistant_brief::OUTPUT), 64 << 10)?;
    let raw: RawSuggestion = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let all = config.load_workspaces()?;
    let current = inbox.item(item).map_err(|e| e.to_string())?;
    let (suggestion, memory) = assistant_brief::checked(raw, &all, &current)?;
    if let Some(memory) = memory {
        let files = config
            .workspaces
            .as_deref()
            .and_then(crate::records::assistant::files)
            .ok_or("the assistant's folder cannot be resolved")?;
        files.write_memory(&memory).map_err(|e| e.to_string())?;
    }
    if suggestion != ItemSuggestion::default() {
        inbox
            .change(|b, _| b.suggest(item, suggestion))
            .map_err(|e| e.to_string())?;
        tracing::info!(item, "suggestion kept");
    }
    Ok(())
}

/// Blocking. The run's folder, brief and agent for `item`.
fn launch(
    config: &crate::config::Config,
    all: &Workspaces,
    fallback: &str,
    item: &InboxItem,
) -> Result<Launch, String> {
    let state = config.state_dir.as_ref().ok_or("no state dir")?;
    let workspaces_file = config.workspaces.as_deref().ok_or("no workspaces file")?;
    let files = crate::records::assistant::files(workspaces_file)
        .ok_or("the assistant's folder cannot be resolved")?;
    let dir = state.join("assistant").join("runs").join(&item.id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let command = match all.assistant().worker.as_deref() {
        None => LaunchCommand::Shell(fallback.to_owned()),
        Some(name) => {
            let w = all.worker(name).ok_or("the assistant's worker is gone")?;
            LaunchCommand::Native {
                program: w.program(),
                args: w.args.clone(),
            }
        }
    };
    if !crate::launch::is_claude_code(&command) {
        return Err("the assistant needs Claude Code".into());
    }
    let text = assistant_brief::brief(all, &files, item, &decisions(&files.folder));
    super::git::write_file(&dir, "TASK.md", &text)?;
    // No MCP servers, and nothing to edit beside its folder.
    let config_file = mcp::write(&dir, &[], &[])?;
    let command = mcp::with_flags(command, &config_file);
    let rules = [bot_run::deny(&files.bound)];
    let command = bot_run::with_settings(command, &dir, &rules, &[])?;
    Ok(Launch {
        argv: runner::argv(&command, runner::START_PROMPT),
        cwd: Some(dir.clone()),
        name: Some(format!("assistant {}", item.id)),
        labels: Labels::from([
            ("muxa.kind".to_owned(), KIND.to_owned()),
            ("muxa.role".to_owned(), "agent".to_owned()),
            ("muxa.dir".to_owned(), dir.to_string_lossy().into_owned()),
            (ITEM.to_owned(), item.id.clone()),
            (STARTED.to_owned(), now().to_string()),
        ]),
        env: BTreeMap::new(),
    })
}

/// `decisions.jsonl` beside the Soul: the user's last accepts and dismisses.
const DECISIONS: &str = "decisions.jsonl";
const KEEP_DECISIONS: usize = 30;

fn decisions(folder: &Path) -> Vec<String> {
    std::fs::read_to_string(folder.join(DECISIONS))
        .map(|t| t.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

/// Blocking. Keeps one decision, the last 30 only.
pub fn note_decision(
    config: &crate::config::Config,
    item: &InboxItem,
    suggestion: &ItemSuggestion,
    accepted: bool,
) -> Result<(), String> {
    let files = config
        .workspaces
        .as_deref()
        .and_then(crate::records::assistant::files)
        .ok_or("the assistant's folder cannot be resolved")?;
    std::fs::create_dir_all(&files.folder).map_err(|e| e.to_string())?;
    let line = serde_json::json!({
        "title": item.title,
        "suggestion": suggestion,
        "accepted": accepted,
    })
    .to_string();
    let mut lines = decisions(&files.folder);
    lines.push(line);
    let from = lines.len().saturating_sub(KEEP_DECISIONS);
    let text: String = lines[from..].iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(files.folder.join(DECISIONS), text).map_err(|e| e.to_string())
}

/// Scans every [`SCAN`] and on each Inbox change; watches the runs.
pub fn spawn(herdr: Arc<Herdr>, assistant: Arc<Assistant>) {
    let a = Arc::clone(&assistant);
    super::keep_watching(Arc::clone(&herdr), "capture assistant", move |backend| {
        let a = Arc::clone(&a);
        async move { a.watch(&backend).await }
    });
    tokio::spawn(async move {
        let mut changes = assistant.inbox.subscribe();
        loop {
            if let Ok(backend) = herdr.backend().await {
                assistant.scan(&backend).await;
            }
            let _ = tokio::time::timeout(SCAN, changes.changed()).await;
        }
    });
}
