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

/// A kind of agent, e.g. Claude Code. One instance serves every Task.
pub trait AgentAdapter: Send + Sync {
    /// Stable, shown to clients: `claude-code`.
    fn id(&self) -> &'static str;

    /// A source for this adapter's agent working in `dir`, a Task's working
    /// place. None when it sees none there.
    fn open_folder(
        &self,
        dir: &std::path::Path,
        procs: &Processes,
    ) -> Result<Option<Box<dyn ChatSource>>, Error>;
}

/// One agent conversation, polled for changes.
pub trait ChatSource: Send {
    /// What changed since the last call. The first call returns a `Reset`.
    fn poll(&mut self) -> Result<Vec<ChatEvent>, Error>;
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

    /// The first adapter that sees its agent working in `dir`.
    pub fn open_folder(&self, dir: &std::path::Path) -> Result<Option<Opened>, Error> {
        let procs = Processes::list()?;
        for adapter in &self.adapters {
            if let Some(source) = adapter.open_folder(dir, &procs)? {
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

    /// The processes named `name`.
    pub fn named(&self, name: &str) -> Vec<u32> {
        let mut pids: Vec<u32> = self
            .info
            .iter()
            .filter(|(_, (_, n))| n == name)
            .map(|(pid, _)| *pid)
            .collect();
        pids.sort_unstable();
        pids
    }

    /// `pid` was running when the list was made.
    pub fn alive(&self, pid: u32) -> bool {
        self.parent.contains_key(&pid)
    }

    /// When `pid` started, to the second.
    pub fn started(&self, pid: u32) -> Option<SystemTime> {
        self.at.checked_sub(self.info.get(&pid)?.0)
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

/// Each pid's working directory ([`process_cwds`]); tests stand in for the host.
pub type CwdsFn = dyn Fn(&[u32]) -> Result<HashMap<u32, PathBuf>, Error> + Send + Sync;

/// The programs of the coding agents muxa runs, as `ps` names them.
pub const AGENT_PROGRAMS: [&str; 2] = ["claude", codex::PROCESS];

/// A coding agent whose working directory is `dir`: its program and pid.
pub fn agent_in(
    dir: &std::path::Path,
    procs: &Processes,
    cwds: &CwdsFn,
) -> Result<Option<(String, u32)>, Error> {
    let canonical = |p: &std::path::Path| p.canonicalize().unwrap_or_else(|_| p.to_owned());
    let want = canonical(dir);
    let pids: Vec<(u32, &str)> = AGENT_PROGRAMS
        .iter()
        .flat_map(|name| procs.named(name).into_iter().map(move |pid| (pid, *name)))
        .collect();
    let all: Vec<u32> = pids.iter().map(|(pid, _)| *pid).collect();
    let cwd = cwds(&all)?;
    Ok(pids
        .into_iter()
        .find(|(pid, _)| cwd.get(pid).is_some_and(|c| canonical(c) == want))
        .map(|(pid, name)| (name.to_owned(), pid)))
}

/// The working directory of each of `pids` that still runs and that this
/// user may inspect. `ps` cannot print it: Linux has `/proc/<pid>/cwd`, macOS
/// has `lsof`, which ships with the system.
pub fn process_cwds(pids: &[u32]) -> Result<HashMap<u32, PathBuf>, Error> {
    if pids.is_empty() {
        return Ok(HashMap::new());
    }
    #[cfg(target_os = "linux")]
    {
        Ok(pids
            .iter()
            .filter_map(|pid| Some((*pid, std::fs::read_link(format!("/proc/{pid}/cwd")).ok()?)))
            .collect())
    }
    #[cfg(target_os = "macos")]
    {
        let list: Vec<String> = pids.iter().map(u32::to_string).collect();
        let out = Command::new("/usr/sbin/lsof")
            .args(["-w", "-a", "-d", "cwd", "-Fn", "-p", &list.join(",")])
            .output()
            .map_err(|e| Error::Processes(format!("lsof: {e}")))?;
        // lsof exits 1 when one of the pids is gone; its output still counts.
        Ok(parse_lsof_cwds(&String::from_utf8_lossy(&out.stdout)))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Ok(HashMap::new())
    }
}

/// `lsof -F n` output: a `p<pid>` line, then `n<path>` for its cwd.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn parse_lsof_cwds(text: &str) -> HashMap<u32, PathBuf> {
    let mut out = HashMap::new();
    let mut pid = None;
    for line in text.lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.parse::<u32>().ok();
        } else if let (Some(path), Some(p)) = (line.strip_prefix('n'), pid) {
            out.insert(p, PathBuf::from(path));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry asks the adapters in order, fresh for each folder.
    #[test]
    fn the_first_adapter_that_sees_an_agent_in_the_folder_wins() {
        struct Empty;
        impl ChatSource for Empty {
            fn poll(&mut self) -> Result<Vec<ChatEvent>, Error> {
                Ok(Vec::new())
            }
            fn entries(&self) -> Vec<ChatEntry> {
                Vec::new()
            }
            fn status(&self) -> Option<Agent> {
                None
            }
        }
        struct FixtureAdapter(&'static str, &'static str);
        impl AgentAdapter for FixtureAdapter {
            fn id(&self) -> &'static str {
                self.0
            }
            fn open_folder(
                &self,
                dir: &std::path::Path,
                _: &Processes,
            ) -> Result<Option<Box<dyn ChatSource>>, Error> {
                Ok((dir == std::path::Path::new(self.1)).then(|| Box::new(Empty) as _))
            }
        }
        let registry = Registry::new(vec![
            Box::new(FixtureAdapter("fixture-a", "/a")),
            Box::new(FixtureAdapter("fixture-b", "/b")),
            Box::new(FixtureAdapter("fixture-c", "/a")),
        ]);
        let opened = |dir: &str| {
            registry
                .open_folder(std::path::Path::new(dir))
                .unwrap()
                .map(|(id, _)| id)
        };
        assert_eq!(opened("/a"), Some("fixture-a"));
        assert_eq!(opened("/b"), Some("fixture-b"));
        assert_eq!(opened("/c"), None);
    }

    #[test]
    fn names_and_ages_come_from_ps() {
        let procs = Processes::parse(
            "  1     0 21-15:34:52 /sbin/launchd\n 100     1    01:02 -zsh\n 200   100 1:00:05 /opt/homebrew/bin/codex\n 300   200    00:03 /Applications/My App.app/x\n",
        );
        assert_eq!(procs.named("codex"), vec![200]);
        assert_eq!(procs.named("x"), vec![300], "a path with spaces");
        assert!(
            procs.named("zsh").is_empty(),
            "a login shell's name keeps its dash"
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
    fn cwds_come_from_lsof_fields() {
        let cwds = parse_lsof_cwds("p200\nfcwd\nn/work/my app\np300\nfcwd\nn/tmp\npjunk\nn/x\n");
        assert_eq!(cwds.len(), 2);
        assert_eq!(cwds[&200], PathBuf::from("/work/my app"));
        assert_eq!(cwds[&300], PathBuf::from("/tmp"));
        assert!(process_cwds(&[]).unwrap().is_empty());
    }

    /// This test process runs; its cwd is a fact the host reports.
    #[test]
    fn a_live_process_reports_its_cwd() {
        let me = std::process::id();
        let cwds = process_cwds(&[me]).unwrap();
        if cfg!(any(target_os = "linux", target_os = "macos")) {
            let want = std::env::current_dir().unwrap().canonicalize().unwrap();
            assert_eq!(cwds[&me].canonicalize().unwrap(), want);
        }
    }
}
