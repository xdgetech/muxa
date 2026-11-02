//! Agent adapters: read what a coding agent records about its conversation and
//! turn it into the neutral `ChatEntry` list of the protocol. See `docs/agents.md`.
//!
//! Every function here may block (files, `ps`); callers run them on the blocking pool.

pub mod claude;
pub mod codex;
pub mod log;
pub mod tail;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime};

use muxa_protocol::{Agent, ChatEntry, ChatPrompt};

/// What an adapter knows about the pane it is asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneInfo {
    /// `%n`
    pub pane: String,
    pub session: String,
    /// The pane's first process.
    pub pid: u32,
    pub path: String,
    /// The pane is back at a shell: the agent most likely exited.
    pub shell_foreground: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot list processes: {0}")]
    Processes(String),
    #[error("{0}")]
    Gone(String),
}

/// A kind of agent, e.g. Claude Code. One instance serves every pane.
pub trait AgentAdapter: Send + Sync {
    /// Stable, shown to clients: `claude-code`.
    fn id(&self) -> &'static str;
    /// A source for the agent running in `pane`, or None when this adapter
    /// does not see its agent there.
    fn open(
        &self,
        pane: &PaneInfo,
        procs: &Processes,
    ) -> Result<Option<Box<dyn ChatSource>>, Error>;

    /// This adapter's agent, when it runs in `pane`, and its state. Cheaper than
    /// `open`; it decides a pane's role and state when no tag does.
    fn find(&self, _pane: &PaneInfo, _procs: &Processes) -> Option<Found> {
        None
    }

    /// Documents the agent in `pane` is writing, such as its current plan.
    fn documents(&self, _pane: &PaneInfo, _procs: &Processes) -> Result<Vec<AgentDoc>, Error> {
        Ok(Vec::new())
    }
}

/// An agent an adapter sees in a pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// As the agent records it; None when it records none, or the pane is back at a shell.
    pub status: Option<Agent>,
    /// The agent's own session, when the adapter knows it (ADR 0018).
    pub session: Option<AgentSession>,
}

/// An agent's own session: its id, and where its transcript is. A path only;
/// the transcript stays the agent's (invariant 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSession {
    pub id: String,
    pub transcript: Option<PathBuf>,
}

/// A document an agent keeps outside the repo, e.g. a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDoc {
    /// Shown to the user, e.g. the file name.
    pub name: String,
    pub path: PathBuf,
    /// Files may only be served from inside this directory.
    pub root: PathBuf,
}

/// One agent conversation, polled for changes.
pub trait ChatSource: Send {
    /// What changed since the last call. The first call returns a `Reset`.
    fn poll(&mut self, pane: &PaneInfo) -> Result<Vec<ChatEvent>, Error>;
    /// Everything currently known, for a client that missed updates.
    fn entries(&self) -> Vec<ChatEntry>;
    /// The agent's state now; None when unknown.
    fn status(&self) -> Option<Agent>;
    /// What the agent waits on now, when the adapter knows that menu.
    fn prompt(&self) -> Option<ChatPrompt> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatEvent {
    /// Replace everything. `truncated`: older history exists but is not included.
    Reset {
        entries: Vec<ChatEntry>,
        truncated: bool,
    },
    /// Add an entry, or replace the one with the same id.
    Upsert(ChatEntry),
}

/// The id of the adapter that opened a source, and the source.
pub type Opened = (&'static str, Box<dyn ChatSource>);

/// The adapters, asked in order; the first that opens a source wins.
pub struct Registry {
    adapters: Vec<Box<dyn AgentAdapter>>,
}

impl Registry {
    pub fn new(adapters: Vec<Box<dyn AgentAdapter>>) -> Self {
        Self { adapters }
    }

    /// The default set, each adapter with its data directories (`data_dirs`).
    pub fn standard(dirs: &HashMap<String, Vec<PathBuf>>) -> Self {
        let of = |id: &str| dirs.get(id).cloned().unwrap_or_default();
        let mut adapters: Vec<Box<dyn AgentAdapter>> = Vec::new();
        let claude = of(claude::ID);
        if !claude.is_empty() {
            adapters.push(Box::new(claude::ClaudeAdapter::new(claude)));
        }
        let codex = of(codex::ID);
        if !codex.is_empty() {
            adapters.push(Box::new(codex::CodexAdapter::new(codex)));
        }
        Self::new(adapters)
    }

    /// Every adapter's documents for `pane`.
    pub fn documents(&self, pane: &PaneInfo) -> Result<Vec<AgentDoc>, Error> {
        let procs = Processes::list()?;
        let mut docs = Vec::new();
        for adapter in &self.adapters {
            docs.extend(adapter.documents(pane, &procs)?);
        }
        Ok(docs)
    }

    /// The first adapter that sees its agent running in `pane`.
    pub fn find(&self, pane: &PaneInfo, procs: &Processes) -> Option<Found> {
        self.identify(pane, procs).map(|(_, found)| found)
    }

    /// Like `find`, with the id of the adapter that saw the agent.
    pub fn identify(&self, pane: &PaneInfo, procs: &Processes) -> Option<(&'static str, Found)> {
        self.adapters
            .iter()
            .find_map(|a| a.find(pane, procs).map(|found| (a.id(), found)))
    }

    pub fn open(&self, pane: &PaneInfo) -> Result<Option<Opened>, Error> {
        let procs = Processes::list()?;
        for adapter in &self.adapters {
            if let Some(source) = adapter.open(pane, &procs)? {
                return Ok(Some((adapter.id(), source)));
            }
        }
        Ok(None)
    }
}

/// Each adapter's data directories (ADR 0016): every `--agent-dir` given for
/// it, else its default under `home`; then each directory that `dir_env`
/// finds for the adapter's variable, such as a workspace's
/// `CLAUDE_CONFIG_DIR`. No duplicates.
pub fn data_dirs(
    flags: &[(String, PathBuf)],
    home: Option<&std::path::Path>,
    dir_env: impl Fn(&str) -> Vec<PathBuf>,
) -> HashMap<String, Vec<PathBuf>> {
    let adapters = [
        (claude::ID, ".claude", claude::DIR_ENV),
        (codex::ID, ".codex", codex::DIR_ENV),
    ];
    let mut out = HashMap::new();
    for (id, default, env) in adapters {
        let mut dirs: Vec<PathBuf> = flags
            .iter()
            .filter(|(a, _)| a == id)
            .map(|(_, d)| d.clone())
            .collect();
        if dirs.is_empty() {
            dirs.extend(home.map(|h| h.join(default)));
        }
        for dir in dir_env(env) {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
        out.insert(id.to_owned(), dirs);
    }
    out
}

/// A snapshot of the process tree: each pid's parent, and its age and name when known.
#[derive(Debug, Clone)]
pub struct Processes {
    parent: HashMap<u32, u32>,
    info: HashMap<u32, (Duration, String)>,
    at: SystemTime,
}

impl Default for Processes {
    fn default() -> Self {
        Self::parse("")
    }
}

impl Processes {
    /// `ps` works the same on macOS and Linux and needs no extra crate or `unsafe`.
    pub fn list() -> Result<Self, Error> {
        let out = Command::new("ps")
            .args(["-A", "-o", "pid=,ppid=,etime=,comm="])
            .output()
            .map_err(|e| Error::Processes(e.to_string()))?;
        if !out.status.success() {
            return Err(Error::Processes(format!("ps exited with {}", out.status)));
        }
        Ok(Self::parse(&String::from_utf8_lossy(&out.stdout)))
    }

    /// Lines of `pid ppid [etime command]`. `command` is a name on Linux and a path
    /// on macOS, and may contain spaces.
    pub fn parse(text: &str) -> Self {
        let mut parent = HashMap::new();
        let mut info = HashMap::new();
        for line in text.lines() {
            let mut f = line.split_whitespace();
            let (Some(Ok(pid)), Some(Ok(ppid))) =
                (f.next().map(str::parse), f.next().map(str::parse))
            else {
                continue;
            };
            parent.insert(pid, ppid);
            if let (Some(age), Some(command)) = (f.next().and_then(elapsed), f.next()) {
                let rest: Vec<&str> = f.collect();
                let command = if rest.is_empty() {
                    command.to_owned()
                } else {
                    format!("{command} {}", rest.join(" "))
                };
                let name = command.rsplit('/').next().unwrap_or("").to_owned();
                info.insert(pid, (age, name));
            }
        }
        Self {
            parent,
            info,
            at: SystemTime::now(),
        }
    }

    /// The processes named `name` at or below `root`.
    pub fn named_within(&self, name: &str, root: u32) -> Vec<u32> {
        let mut pids: Vec<u32> = self
            .info
            .iter()
            .filter(|(pid, (_, n))| n == name && self.is_within(**pid, root))
            .map(|(pid, _)| *pid)
            .collect();
        pids.sort_unstable();
        pids
    }

    /// When `pid` started, to the second.
    pub fn started(&self, pid: u32) -> Option<SystemTime> {
        self.at.checked_sub(self.info.get(&pid)?.0)
    }

    /// `pid` is alive and is `root` or runs below it.
    pub fn is_within(&self, pid: u32, root: u32) -> bool {
        if root == 0 || !self.parent.contains_key(&pid) {
            return false;
        }
        let mut p = pid;
        // The depth cap guards against a cycle from a racy listing.
        for _ in 0..64 {
            if p == root {
                return true;
            }
            match self.parent.get(&p) {
                Some(&up) if up != p && up != 0 => p = up,
                _ => return false,
            }
        }
        false
    }
}

/// `ps` elapsed time: `[[dd-]hh:]mm:ss`.
fn elapsed(s: &str) -> Option<Duration> {
    let (days, clock) = match s.split_once('-') {
        Some((d, rest)) => (d.parse::<u64>().ok()?, rest),
        None => (0, s),
    };
    let mut secs = 0u64;
    let parts: Vec<&str> = clock.split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    for part in parts {
        secs = secs * 60 + part.parse::<u64>().ok()?;
    }
    Some(Duration::from_secs(days * 86_400 + secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_ages_come_from_ps() {
        let procs = Processes::parse(
            "  1     0 21-15:34:52 /sbin/launchd\n 100     1    01:02 -zsh\n 200   100 1:00:05 /opt/homebrew/bin/codex\n 300   200    00:03 /Applications/My App.app/x\n",
        );
        assert_eq!(procs.named_within("codex", 100), vec![200]);
        assert!(procs.named_within("codex", 1).contains(&200));
        assert!(
            procs.named_within("x", 100).contains(&300),
            "a path with spaces"
        );
        let age = procs
            .at
            .duration_since(procs.started(200).unwrap())
            .unwrap();
        assert_eq!(age, Duration::from_secs(3605));
        assert_eq!(elapsed("2-00:00:01"), Some(Duration::from_secs(172_801)));
        assert_eq!(elapsed("junk"), None);
    }

    #[test]
    fn data_dirs_are_the_flags_or_default_then_the_workspaces() {
        let home = std::path::Path::new("/h");
        let env = |key: &str| match key {
            claude::DIR_ENV => vec![PathBuf::from("/h/.claude-co"), PathBuf::from("/h/.claude")],
            _ => Vec::new(),
        };
        let dirs = data_dirs(&[], Some(home), env);
        assert_eq!(
            dirs[claude::ID],
            [PathBuf::from("/h/.claude"), PathBuf::from("/h/.claude-co")],
            "the default first, no duplicates"
        );
        assert_eq!(dirs[codex::ID], [PathBuf::from("/h/.codex")]);

        let flags = [
            (claude::ID.to_owned(), PathBuf::from("/a")),
            (claude::ID.to_owned(), PathBuf::from("/b")),
        ];
        let dirs = data_dirs(&flags, Some(home), env);
        assert_eq!(
            dirs[claude::ID],
            ["/a", "/b", "/h/.claude-co", "/h/.claude"].map(PathBuf::from),
            "repeated flags replace the default"
        );
        assert!(data_dirs(&[], None, |_| Vec::new())[codex::ID].is_empty());
    }

    #[test]
    fn descendants_are_within_their_pane() {
        let procs = Processes::parse("  1     0\n 100     1\n 200   100\n 300   200\n 400     1\n");
        assert!(procs.is_within(300, 100));
        assert!(procs.is_within(100, 100));
        assert!(!procs.is_within(400, 100));
        assert!(
            !procs.is_within(999, 100),
            "a dead pid is not within anything"
        );
        assert!(!procs.is_within(300, 0));
    }
}
