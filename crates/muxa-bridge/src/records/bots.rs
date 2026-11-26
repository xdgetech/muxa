//! `[[bot]]` in a workspace's `.muxa/routines.toml` (ADR 0082): a named owner
//! of Routines with a Soul and a Memory in `.muxa/bots/<name>/`. Parsing and
//! reading only; the runner gives them to each run.
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::Workspace;
use super::routines::{ReadFail, read_regular};
use super::workers::{self, Worker};

pub const BOTS_DIR: &str = "bots";
pub const SOUL_FILE: &str = "soul.md";
pub const MEMORY_FILE: &str = "memory.md";
pub const DEFAULT_DAILY_ITEMS: u32 = 3;
pub const MAX_DAILY_ITEMS: u32 = 50;
/// Bytes. The Soul and the Memory both go into the run's `.muxa/TASK.md`.
pub const MAX_SOUL: usize = 12 * 1024;
pub const MAX_MEMORY: usize = 16 * 1024;
/// Earlier memories kept for Restore, newest first: `history/memory-1.md` …
pub const KEEP_MEMORIES: usize = 5;
const HISTORY_DIR: &str = "history";
const MAX_NAME: usize = 64;
/// A lead's members (ADR 0085).
pub const MAX_MEMBERS: usize = 6;
pub const DEFAULT_HANDOFFS: u32 = 10;
pub const MAX_HANDOFFS: u32 = 50;
const MAX_DESCRIPTION: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bot {
    pub workspace: String,
    /// Its id within the workspace, and the name of its folder.
    pub name: String,
    pub description: Option<String>,
    /// None: the workspace default agent.
    pub worker: Option<String>,
    pub daily_items: u32,
    pub enabled: bool,
    /// The canonical `.muxa/` folder its files must stay in.
    pub dir: PathBuf,
    /// Its team when it leads one (ADR 0085).
    pub members: Vec<String>,
    pub handoffs_per_day: u32,
    /// Shell command patterns only this Bot may run (ADR 0088).
    pub commands: Vec<String>,
    /// Its logins in order, from its Workspace's list (ADR 0103). Empty:
    /// the Workspace's list.
    pub subscriptions: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawBot {
    name: String,
    description: Option<String>,
    worker: Option<String>,
    daily_items: Option<u32>,
    enabled: bool,
    #[serde(default)]
    members: Vec<String>,
    handoffs_per_day: Option<u32>,
    #[serde(default)]
    commands: Vec<String>,
    #[serde(default)]
    subscriptions: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BotFileError {
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("{path}: {reason}")]
    Invalid { path: PathBuf, reason: String },
}

impl RawBot {
    /// Errors name the Bot and the field, never a value.
    pub(super) fn check(
        self,
        workspace: &Workspace,
        dir: &Path,
        workers: &[Worker],
    ) -> Result<Bot, String> {
        name(&self.name)?;
        let at = |field: &str| format!("bot {:?}: {field}", self.name);
        if let Some(d) = &self.description {
            if d.chars().count() > MAX_DESCRIPTION || d.chars().any(char::is_control) {
                return Err(at(&format!(
                    "description must be one line of at most {MAX_DESCRIPTION} characters"
                )));
            }
        }
        let worker = match self.worker.as_deref() {
            None | Some(workers::DEFAULT_WORKER) => None,
            Some(w) => {
                let found = workers
                    .iter()
                    .find(|x| x.name == w)
                    .ok_or_else(|| at("worker is not configured"))?;
                if !found.allows(&workspace.name) {
                    return Err(at("worker is not allowed in its workspace"));
                }
                Some(w.to_owned())
            }
        };
        let daily_items = self.daily_items.unwrap_or(DEFAULT_DAILY_ITEMS);
        if !(1..=MAX_DAILY_ITEMS).contains(&daily_items) {
            return Err(at(&format!("daily_items must be 1 to {MAX_DAILY_ITEMS}")));
        }
        if self.members.len() > MAX_MEMBERS {
            return Err(at(&format!("members has at most {MAX_MEMBERS} Bots")));
        }
        let handoffs_per_day = self.handoffs_per_day.unwrap_or(DEFAULT_HANDOFFS);
        if !(1..=MAX_HANDOFFS).contains(&handoffs_per_day) {
            return Err(at(&format!("handoffs_per_day must be 1 to {MAX_HANDOFFS}")));
        }
        check_commands(&self.commands).map_err(|r| at(&r))?;
        // A login carries connectors and settings: a Bot stays inside its
        // Workspace's list (ADR 0087, 0103).
        let allowed: Vec<&str> = workspace.subscriptions.iter().map(String::as_str).collect();
        let owner = format!("bot {:?} of {:?}", self.name, workspace.name);
        let subscriptions = super::subscriptions::keep_known(self.subscriptions, &allowed, &owner);
        Ok(Bot {
            workspace: workspace.name.clone(),
            name: self.name,
            description: self.description,
            worker,
            daily_items,
            enabled: self.enabled,
            dir: dir.to_owned(),
            members: self.members,
            handoffs_per_day,
            commands: self.commands,
            subscriptions,
        })
    }
}

/// Teams are one level deep (ADR 0085): a member is another Bot of the file
/// that leads no team, named once.
pub(super) fn check_teams(bots: &[Bot]) -> Result<(), String> {
    for lead in bots.iter().filter(|b| !b.members.is_empty()) {
        let at = |m: &str| format!("bot {:?}: member {m:?}", lead.name);
        for (i, m) in lead.members.iter().enumerate() {
            let member = bots
                .iter()
                .find(|b| &b.name == m)
                .ok_or_else(|| format!("{} is not a [[bot]] of this file", at(m)))?;
            if member.name == lead.name {
                return Err(format!("bot {:?}: a Bot is not its own member", lead.name));
            }
            if !member.members.is_empty() {
                return Err(format!(
                    "{} leads a team itself; teams are one level deep",
                    at(m)
                ));
            }
            if lead.members[..i].contains(m) {
                return Err(format!("{} appears twice", at(m)));
            }
        }
    }
    Ok(())
}

/// A Bot's name is its folder's name: lowercase letters, digits, `-` and
/// `_`, starting with a letter or digit.
pub fn name(value: &str) -> Result<(), String> {
    let ok = !value.is_empty()
        && value.len() <= MAX_NAME
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        && value.as_bytes()[0].is_ascii_alphanumeric();
    if ok {
        Ok(())
    } else {
        Err(format!(
            "bot name must be 1 to {MAX_NAME} lowercase letters, digits, - or _, starting with a letter or digit"
        ))
    }
}

impl Bot {
    /// `.muxa/bots/<name>/`.
    pub fn folder(&self) -> PathBuf {
        self.dir.join(BOTS_DIR).join(&self.name)
    }

    pub fn files(&self) -> SoulFiles {
        SoulFiles {
            bound: self.dir.clone(),
            folder: self.folder(),
        }
    }

    /// The Soul as it is now; empty when the file is missing.
    pub fn read_soul(&self) -> Result<String, BotFileError> {
        self.files().read_soul()
    }

    /// The Memory as it is now; empty when the file is missing.
    pub fn read_memory(&self) -> Result<String, BotFileError> {
        self.files().read_memory()
    }

    /// Blocking. See [`SoulFiles::write_memory`].
    pub fn write_memory(&self, text: &str) -> Result<(), BotFileError> {
        self.files().write_memory(text)
    }

    /// The kept memory `n` (1 is the newest); empty when there is none.
    pub fn read_memory_version(&self, n: usize) -> Result<String, BotFileError> {
        self.files().read_memory_version(n)
    }
}

/// A folder with a `soul.md`, a `memory.md` and the kept memories: a Bot's,
/// or the Capture assistant's (ADR 0084). Nothing may lead out of `bound`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoulFiles {
    /// Canonical.
    pub bound: PathBuf,
    pub folder: PathBuf,
}

impl SoulFiles {
    pub fn read_soul(&self) -> Result<String, BotFileError> {
        self.read(SOUL_FILE, MAX_SOUL)
    }

    pub fn read_memory(&self) -> Result<String, BotFileError> {
        self.read(MEMORY_FILE, MAX_MEMORY)
    }

    /// Blocking. Replaces the memory with `text` atomically and keeps the
    /// one before as `history/memory-1.md`, the older ones shifted down.
    pub fn write_memory(&self, text: &str) -> Result<(), BotFileError> {
        let folder = &self.folder;
        let io_err = |path: &Path| {
            let path = path.to_owned();
            move |source| BotFileError::Read { path, source }
        };
        let history = folder.join(HISTORY_DIR);
        std::fs::create_dir_all(&history).map_err(io_err(&history))?;
        let real = folder.canonicalize().map_err(io_err(folder))?;
        if !real.starts_with(&self.bound) {
            return Err(BotFileError::Invalid {
                path: folder.clone(),
                reason: "leads outside its folder".into(),
            });
        }
        if text.len() > MAX_MEMORY {
            return Err(BotFileError::Invalid {
                path: real.join(MEMORY_FILE),
                reason: format!("is larger than {} KiB", MAX_MEMORY / 1024),
            });
        }
        let current = real.join(MEMORY_FILE);
        if current.exists() {
            let history = real.join(HISTORY_DIR);
            for n in (1..KEEP_MEMORIES).rev() {
                let from = history.join(version_file(n));
                if from.exists() {
                    std::fs::rename(&from, history.join(version_file(n + 1)))
                        .map_err(io_err(&from))?;
                }
            }
            std::fs::rename(&current, history.join(version_file(1))).map_err(io_err(&current))?;
        }
        let tmp = real.join(format!(".{MEMORY_FILE}.tmp"));
        std::fs::write(&tmp, text).map_err(io_err(&tmp))?;
        std::fs::rename(&tmp, &current).map_err(io_err(&current))
    }

    pub fn read_memory_version(&self, n: usize) -> Result<String, BotFileError> {
        if !(1..=KEEP_MEMORIES).contains(&n) {
            return Err(BotFileError::Invalid {
                path: self.folder.clone(),
                reason: format!("a kept memory is 1 to {KEEP_MEMORIES}"),
            });
        }
        self.read(&format!("{HISTORY_DIR}/{}", version_file(n)), MAX_MEMORY)
    }

    /// Reads `file` in the folder; empty when it is missing. A symlink may
    /// not lead out of `bound`.
    pub fn read(&self, file: &str, max: usize) -> Result<String, BotFileError> {
        let path = self.folder.join(file);
        let invalid = |reason: String| BotFileError::Invalid {
            path: path.clone(),
            reason,
        };
        let real = match path.canonicalize() {
            Ok(real) => real,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(String::new()),
            Err(source) => {
                return Err(BotFileError::Read {
                    path: path.clone(),
                    source,
                });
            }
        };
        if !real.starts_with(&self.bound) {
            return Err(invalid("leads outside its folder".into()));
        }
        let bytes = match read_regular(&real, max) {
            Ok(bytes) => bytes,
            Err(ReadFail::Io(source)) => return Err(BotFileError::Read { path, source }),
            Err(ReadFail::Invalid("is too large")) => {
                return Err(invalid(format!("is larger than {} KiB", max / 1024)));
            }
            Err(ReadFail::Invalid(reason)) => return Err(invalid(reason.into())),
        };
        if bytes.contains(&0) {
            return Err(invalid("has a NUL byte".into()));
        }
        String::from_utf8(bytes).map_err(|_| invalid("is not UTF-8".into()))
    }
}

fn version_file(n: usize) -> String {
    format!("memory-{n}.md")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_folder_safe() {
        for good in ["triage", "pr-watch", "a1", "x_y"] {
            assert!(name(good).is_ok(), "{good}");
        }
        for bad in ["", "Triage", "-x", "a b", "a/b", "..", "ü", &"a".repeat(65)] {
            assert!(name(bad).is_err(), "{bad}");
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("muxa-bots-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    fn bot(dir: &Path) -> Bot {
        Bot {
            workspace: "w".into(),
            name: "triage".into(),
            description: None,
            worker: None,
            daily_items: 3,
            enabled: true,
            dir: dir.canonicalize().unwrap(),
            members: Vec::new(),
            handoffs_per_day: DEFAULT_HANDOFFS,
            commands: Vec::new(),
            subscriptions: Vec::new(),
        }
    }

    #[test]
    fn missing_files_read_as_empty_and_present_ones_as_written() {
        let tmp = temp("read");
        let b = bot(&tmp);
        assert_eq!(b.read_soul().unwrap(), "");
        assert_eq!(b.read_memory().unwrap(), "");
        std::fs::create_dir_all(b.folder()).unwrap();
        std::fs::write(b.folder().join(SOUL_FILE), "I watch PRs.").unwrap();
        assert_eq!(b.read_soul().unwrap(), "I watch PRs.");
        std::fs::write(b.folder().join(MEMORY_FILE), "x".repeat(MAX_MEMORY + 1)).unwrap();
        assert!(b.read_memory().is_err());
    }

    #[test]
    fn a_symlink_out_of_the_folder_is_refused() {
        let tmp = temp("symlink");
        let muxa = tmp.join(".muxa");
        std::fs::create_dir_all(&muxa).unwrap();
        let b = bot(&muxa);
        std::fs::create_dir_all(b.folder()).unwrap();
        let outside = tmp.join("secret.txt");
        std::fs::write(&outside, "secret").unwrap();
        std::os::unix::fs::symlink(&outside, b.folder().join(SOUL_FILE)).unwrap();
        let e = b.read_soul().unwrap_err().to_string();
        assert!(
            e.contains("leads outside") && !e.contains("secret\""),
            "{e}"
        );
    }

    #[test]
    fn a_new_memory_keeps_the_last_five() {
        let tmp = temp("memory");
        let b = bot(&tmp);
        for n in 1..=7 {
            b.write_memory(&format!("v{n}")).unwrap();
        }
        assert_eq!(b.read_memory().unwrap(), "v7");
        assert_eq!(b.read_memory_version(1).unwrap(), "v6");
        assert_eq!(b.read_memory_version(5).unwrap(), "v2");
        assert!(b.read_memory_version(6).is_err());
        assert!(b.write_memory(&"x".repeat(MAX_MEMORY + 1)).is_err());
        assert_eq!(b.read_memory().unwrap(), "v7");
    }
}

/// Commands per Bot, and bytes per command.
const MAX_COMMANDS: usize = 50;
const MAX_COMMAND: usize = 500;

/// A Bot's `commands` (ADR 0088): each is the inside of a Claude Code
/// `Bash(...)` rule, one line, with no parenthesis that could end the rule.
pub fn check_commands(commands: &[String]) -> Result<(), String> {
    if commands.len() > MAX_COMMANDS {
        return Err(format!("commands has at most {MAX_COMMANDS} entries"));
    }
    match commands.iter().find(|c| {
        c.trim().is_empty()
            || c.len() > MAX_COMMAND
            || c.chars()
                .any(|ch| ch.is_control() || ch == '(' || ch == ')')
    }) {
        Some(bad) => Err(format!("commands: {bad:?} is not a command")),
        None => Ok(()),
    }
}

/// `[workspace.bots]`: Claude Code permission rules every Bot of the
/// Workspace gets, and nothing else does (ADR 0087).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BotRules {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

/// Rules per list, and bytes per rule.
const MAX_RULES: usize = 200;
const MAX_RULE: usize = 500;

/// The allow rules of a Workspace whose `[workspace.bots]` sets no `allow`
/// (ADR 0090): tools that read and never write. The web, and the read tools
/// of the common claude.ai connectors, which load only when the Workspace
/// lets them in. Gmail stays out: mail is where untrusted text arrives.
pub const DEFAULT_ALLOW: &[&str] = &[
    "WebSearch",
    "WebFetch",
    "mcp__claude_ai_Google_Calendar__list_*",
    "mcp__claude_ai_Google_Calendar__get_event",
    "mcp__claude_ai_Google_Calendar__suggest_time",
    "mcp__claude_ai_Google_Drive__search_files",
    "mcp__claude_ai_Google_Drive__read_file_content",
    "mcp__claude_ai_Google_Drive__get_file_metadata",
    "mcp__claude_ai_Google_Drive__list_*",
    "mcp__claude_ai_Google_Docs__read_doc",
    "mcp__claude_ai_Google_Sheets__get_*",
    "mcp__claude_ai_Slack__slack_read_*",
    "mcp__claude_ai_Slack__slack_search_*",
    "mcp__claude_ai_Notion__notion-search",
    "mcp__claude_ai_Notion__notion-fetch",
    "mcp__claude_ai_Notion__notion-get-*",
    "mcp__claude_ai_Notion__notion-query-*",
    "mcp__claude_ai_Notion__notion-list-*",
    "mcp__claude_ai_Linear__list_*",
    "mcp__claude_ai_Linear__get_*",
    "mcp__claude_ai_Meetings__list_meetings",
    "mcp__claude_ai_Meetings__get_*",
];

/// Pure. [`DEFAULT_ALLOW`] as owned rules.
pub fn default_allow() -> Vec<String> {
    DEFAULT_ALLOW.iter().map(|r| (*r).to_owned()).collect()
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawBotRules {
    /// None: [`DEFAULT_ALLOW`]. An empty list allows nothing.
    allow: Option<Vec<String>>,
    #[serde(default)]
    deny: Vec<String>,
}

impl RawBotRules {
    pub(super) fn check(self) -> Result<BotRules, String> {
        let allow = self.allow.unwrap_or_else(default_allow);
        for (list, rules) in [("allow", &allow), ("deny", &self.deny)] {
            if rules.len() > MAX_RULES {
                return Err(format!("bots.{list} has at most {MAX_RULES} rules"));
            }
            if let Some(bad) = rules.iter().find(|r| {
                r.trim().is_empty() || r.len() > MAX_RULE || r.chars().any(char::is_control)
            }) {
                return Err(format!("bots.{list}: {bad:?} is not a permission rule"));
            }
        }
        Ok(BotRules {
            allow,
            deny: self.deny,
        })
    }
}

#[cfg(test)]
mod command_tests {
    use super::check_commands;

    #[test]
    fn a_command_is_one_line_with_no_parenthesis() {
        assert!(
            check_commands(&["/x/collect.sh delivery:*".into(), "gh pr diff:*".into()]).is_ok()
        );
        for bad in ["", "rm -rf /) Bash(*", "a\nb"] {
            assert!(check_commands(&[bad.to_owned()]).is_err(), "{bad:?}");
        }
    }
}
