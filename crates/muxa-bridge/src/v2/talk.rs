//! Talking to a Bot (ADR 0083): a folder Task with `muxa.bot`. Each time its
//! agent reports `done`, the host reads `.muxa/bot.json` when it changed,
//! writes the Bot's memory, and turns a checked proposal into one Decide
//! item. The user applies it with `inbox-apply`; nothing changes before.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use muxa_backend::{Change, ChangeStream, Labels, SessionBackend, TerminalId, TerminalInfo};
use muxa_inbox::{Extra, Inbox, NewItem};
use muxa_protocol::v2::{ItemKind, SettingsProposal};
use muxa_server::tasks::{BOT, DIR, KIND, ROLE, WORKSPACE};
use sha2::{Digest, Sha256};

use super::talk_check::{self, OUTPUT};
use crate::records::bots::SoulFiles;
use crate::settings::SettingsStore;

/// The hash of the last `bot.json` handled, beside it.
const SEEN: &str = "bot.seen";
const CHECK: &str = "CHECK.md";
const MAX_TITLE: usize = 300;
/// Bytes of `bot.json`: a memory and up to 20 ops with their texts.
const MAX_OUTPUT: u64 = 512 << 10;

pub struct Talks {
    records: Arc<crate::config::Config>,
    settings: Arc<SettingsStore>,
    inbox: Arc<Inbox>,
}

impl Talks {
    pub fn new(
        records: Arc<crate::config::Config>,
        settings: Arc<SettingsStore>,
        inbox: Arc<Inbox>,
    ) -> Self {
        Talks {
            records,
            settings,
            inbox,
        }
    }

    /// Follows the Task agents until the subscription ends.
    pub async fn watch<B: SessionBackend>(&self, backend: &B) -> Result<(), String> {
        let filter = Labels::from([
            (KIND.to_owned(), "task".to_owned()),
            (ROLE.to_owned(), "agent".to_owned()),
        ]);
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
                    if done && t.labels.contains_key(BOT) {
                        self.turn_ended(t.clone()).await;
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

    /// Blocking work off the async threads.
    pub async fn turn_ended(&self, t: TerminalInfo) {
        let (records, settings, inbox) = (
            Arc::clone(&self.records),
            Arc::clone(&self.settings),
            Arc::clone(&self.inbox),
        );
        let done = tokio::task::spawn_blocking(move || {
            let Some(talk) = Talk::of(&t) else { return };
            talk.handle(&records, &settings, &inbox);
        })
        .await;
        if let Err(e) = done {
            tracing::warn!(error = %e, "a talk's output was not handled");
        }
    }
}

fn refused(why: &str) -> Vec<String> {
    vec![
        format!("refused: {why}"),
        format!("Fix .muxa/{OUTPUT} and end your turn again."),
    ]
}

/// A talk by its terminal's labels.
pub(crate) struct Talk {
    pub workspace: String,
    pub bot: String,
    pub terminal: String,
    pub dir: PathBuf,
}

impl Talk {
    pub(crate) fn of(t: &TerminalInfo) -> Option<Self> {
        Some(Talk {
            workspace: t.labels.get(WORKSPACE)?.clone(),
            bot: t.labels.get(BOT)?.clone(),
            terminal: t.id.as_str().to_owned(),
            dir: PathBuf::from(t.labels.get(DIR)?),
        })
    }

    /// Blocking. A `bot.json` already handled, or none, changes nothing.
    pub(crate) fn handle(
        &self,
        records: &crate::config::Config,
        settings: &SettingsStore,
        inbox: &Inbox,
    ) {
        let muxa = self.dir.join(".muxa");
        let path = muxa.join(OUTPUT);
        if std::fs::symlink_metadata(&path).is_err() {
            return;
        }
        let (lines, hash) = match super::bot_run::read_output(&path, MAX_OUTPUT) {
            Err(why) => (refused(&why), None),
            Ok(bytes) => {
                let hash: String = Sha256::digest(&bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                if std::fs::read_to_string(muxa.join(SEEN)).is_ok_and(|s| s.trim() == hash) {
                    return;
                }
                let lines = match self.accept(&bytes, &hash, records, settings, inbox) {
                    Ok(lines) => {
                        tracing::info!(bot = %self.bot, terminal = %self.terminal, "talk output handled");
                        lines
                    }
                    Err(why) => {
                        tracing::info!(bot = %self.bot, terminal = %self.terminal, "talk output refused");
                        refused(&why)
                    }
                };
                (lines, Some(hash))
            }
        };
        let report = format!("# Check\n\n{}\n", lines.join("\n"));
        let kept = std::fs::write(muxa.join(CHECK), report)
            .and_then(|()| hash.map_or(Ok(()), |h| std::fs::write(muxa.join(SEEN), h)));
        if let Err(e) = kept {
            tracing::warn!(terminal = %self.terminal, error = %e, "cannot keep a talk's check");
        }
    }

    fn accept(
        &self,
        bytes: &[u8],
        hash: &str,
        records: &crate::config::Config,
        settings: &SettingsStore,
        inbox: &Inbox,
    ) -> Result<Vec<String>, String> {
        let out = talk_check::parse(bytes)?;
        let mut lines = Vec::new();
        // The proposal is checked before the memory is written: a refused
        // output changes nothing.
        let proposal = match out.proposal {
            None => None,
            Some(p) => {
                let view = settings.view().map_err(|e| e.to_string())?;
                let ws = view
                    .workspaces
                    .iter()
                    .find(|w| w.name == self.workspace)
                    .ok_or("the talk's Workspace is gone")?;
                let ops = talk_check::check_ops(ws, &self.bot, p.ops)?;
                settings
                    .check_ops(ops.clone())
                    .map_err(|e| format!("the settings would not load: {e}"))?;
                Some((p.summary, ops, view.revision))
            }
        };
        if let Some(memory) = &out.memory {
            self.memory_files(records)?
                .write_memory(memory)
                .map_err(|e| format!("memory not written: {e}"))?;
            lines.push("passed: memory written.".to_owned());
        }
        if let Some((summary, ops, revision)) = proposal {
            let note: String = ops
                .iter()
                .map(|o| format!("- {}\n", talk_check::describe(o)))
                .collect();
            let title: String = format!("{} proposes: {}", self.bot, summary.trim())
                .chars()
                .take(MAX_TITLE)
                .collect();
            let new = NewItem {
                kind: ItemKind::Decide,
                title,
                note: Some(note),
                workspace: Some(self.workspace.clone()),
                due: None,
                source: "task".into(),
                key: Some(format!("talk:{}:{}", self.terminal, &hash[..16])),
                once: true,
                terminal: Some(self.terminal.clone()),
                routine: None,
                place: None,
                extra: Extra {
                    bot: Some(self.bot.clone()),
                    settings_proposal: Some(SettingsProposal { revision, ops }),
                    ..Extra::default()
                },
            };
            inbox
                .change(|b, now| b.add(new, now).map(drop))
                .map_err(|e| format!("the proposal did not reach the Inbox: {e}"))?;
            lines.push("passed: the proposal is in the user's Inbox. They apply it there.".into());
        }
        if lines.is_empty() {
            lines.push("passed: nothing to do.".into());
        }
        Ok(lines)
    }

    /// The Bot's folder, also for a Bot the talk is about to propose.
    fn memory_files(&self, records: &crate::config::Config) -> Result<SoulFiles, String> {
        let all = records.load_workspaces()?;
        if let Some(bot) = all.bot(&self.workspace, &self.bot) {
            return Ok(bot.files());
        }
        crate::records::bots::name(&self.bot)?;
        let ws = all
            .get(&self.workspace)
            .ok_or("the talk's Workspace is gone")?;
        let dir = crate::records::routines::routines_dir(ws)?
            .ok_or("the Workspace has no .muxa/ folder yet")?;
        Ok(SoulFiles {
            folder: dir.join(crate::records::bots::BOTS_DIR).join(&self.bot),
            bound: dir,
        })
    }
}
