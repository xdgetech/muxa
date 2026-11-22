//! Codex CLI. Each conversation is appended to
//! `<dir>/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl`, whose first line
//! (`session_meta`) names its cwd. Codex records no process id, so a folder's
//! agent is a `codex` process whose cwd is the folder, and its conversation is
//! the newest rollout for that folder written since the process started.
//! The format is private to Codex; anything unexpected is skipped, never guessed.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use muxa_protocol::{Agent, AgentState, ChatEntry, ChatKind, ChatTool};
use serde_json::Value;

use super::log::{EntryLog, INITIAL_ENTRIES, MAX_RESULT, MAX_SUMMARY, MAX_TEXT, cap};
use super::tail::{INITIAL_TAIL, JsonlTail, MAX_LINE, Poll};
use super::{AgentAdapter, ChatEvent, ChatSource, CwdsFn, Error, Processes};

pub const ID: &str = "codex";
/// Moves Codex's state, and with it the rollouts (ADR 0016).
pub const DIR_ENV: &str = "CODEX_HOME";
/// The process name, the same for the Homebrew and the npm install.
pub const PROCESS: &str = "codex";
/// How often, in polls, the source checks the process and looks for a newer rollout.
const CHECK_EVERY: u32 = 8;
const DAY: Duration = Duration::from_secs(86_400);
/// The newest day directories searched. A Codex that runs longer than this
/// on one conversation has no Chat.
const MAX_DAYS: usize = 7;
/// The newest rollouts whose first line is read, across every Codex home.
const MAX_FILES: usize = 64;

type ListProcesses = Box<dyn Fn() -> Result<Processes, Error> + Send>;
type Cwds = Arc<CwdsFn>;

pub struct CodexAdapter {
    /// Codex homes, searched together: one per account (ADR 0016).
    dirs: Vec<PathBuf>,
    /// What a source lists to check that Codex still runs.
    list: fn() -> Result<Processes, Error>,
    cwds: Cwds,
}

impl CodexAdapter {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self {
            dirs,
            list: Processes::list,
            cwds: Arc::new(super::process_cwds),
        }
    }
}

impl AgentAdapter for CodexAdapter {
    fn id(&self) -> &'static str {
        ID
    }

    fn open_folder(
        &self,
        dir: &Path,
        procs: &Processes,
    ) -> Result<Option<Box<dyn ChatSource>>, Error> {
        let dir = canonical(dir);
        let Some(started) = started_in(&dir, procs, self.cwds.as_ref())? else {
            return Ok(None);
        };
        // None until Codex writes the rollout with the first prompt.
        let rollout = find_rollout(&self.dirs, &dir, started)?;
        Ok(Some(Box::new(CodexSource::new(
            self.dirs.clone(),
            dir,
            started,
            rollout,
            Box::new(self.list),
            Arc::clone(&self.cwds),
        ))))
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_owned())
}

/// When the first `codex` process whose cwd is `dir` started, or None when
/// none runs there.
fn started_in(dir: &Path, procs: &Processes, cwds: &CwdsFn) -> Result<Option<SystemTime>, Error> {
    let pids = procs.named(PROCESS);
    let cwd = cwds(&pids)?;
    Ok(pids
        .into_iter()
        .filter(|pid| cwd.get(pid).is_some_and(|c| canonical(c) == dir))
        .filter_map(|pid| procs.started(pid))
        .min())
}

/// The newest rollout for `cwd` written since `started`, in any of `dirs`.
/// Day directories are searched back to the day before `started`, which
/// covers time zones, and at most the newest `MAX_DAYS` of them.
fn find_rollout(
    dirs: &[PathBuf],
    cwd: &Path,
    started: SystemTime,
) -> Result<Option<PathBuf>, Error> {
    // `ps` ages are whole seconds.
    let since = started
        .checked_sub(Duration::from_secs(1))
        .unwrap_or(UNIX_EPOCH);
    let first_day = day_key(since.checked_sub(DAY).unwrap_or(UNIX_EPOCH));
    let mut found: Vec<(SystemTime, PathBuf)> = Vec::new();
    for dir in dirs {
        let mut days = day_dirs(&dir.join("sessions"), &first_day)?;
        days.sort_unstable_by(|a, b| b.cmp(a));
        for day in days.into_iter().take(MAX_DAYS) {
            found.extend(rollouts_since(&day, since));
        }
    }
    found.sort_unstable_by_key(|(mtime, _)| std::cmp::Reverse(*mtime));
    Ok(found
        .into_iter()
        .take(MAX_FILES)
        .map(|(_, path)| path)
        .find(|path| meta_cwd(path).is_some_and(|c| canonical(Path::new(&c)) == cwd)))
}

/// The `.jsonl` files in `day` changed since `since`, with their mtime. A day
/// that cannot be read is skipped: Codex may be writing it.
fn rollouts_since(day: &Path, since: SystemTime) -> Vec<(SystemTime, PathBuf)> {
    let Ok(files) = std::fs::read_dir(day) else {
        return Vec::new();
    };
    files
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|path| path.extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|path| {
            let mtime = std::fs::metadata(&path).ok()?.modified().ok()?;
            (mtime >= since).then_some((mtime, path))
        })
        .collect()
}

/// `sessions/YYYY/MM/DD` directories whose date is `first` ("YYYY/MM/DD") or later.
fn day_dirs(sessions: &Path, first: &str) -> Result<Vec<PathBuf>, Error> {
    let mut days = Vec::new();
    for year in numbered(sessions)? {
        for month in numbered(&sessions.join(&year))? {
            for day in numbered(&sessions.join(&year).join(&month))? {
                if format!("{year}/{month}/{day}").as_str() >= first {
                    days.push(sessions.join(&year).join(&month).join(&day));
                }
            }
        }
    }
    Ok(days)
}

/// The all-digit names in `dir`; none when it does not exist.
fn numbered(dir: &Path) -> Result<Vec<String>, Error> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(Error::Io {
                path: dir.to_owned(),
                source,
            });
        }
    };
    Ok(entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        .collect())
}

/// "YYYY/MM/DD" of `t` in UTC, as Codex names its day directories.
fn day_key(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (year, month, day) = crate::utc::date(secs);
    format!("{year:04}/{month:02}/{day:02}")
}

/// The cwd in a rollout's first line (`session_meta`).
fn meta_cwd(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut line = Vec::new();
    BufReader::new(file.take(MAX_LINE as u64))
        .read_until(b'\n', &mut line)
        .ok()?;
    let v: Value = serde_json::from_slice(&line).ok()?;
    if v.get("type")?.as_str()? != "session_meta" {
        return None;
    }
    Some(v["payload"].get("cwd")?.as_str()?.to_owned())
}

/// Codex records when a turn starts and ends; it records no approval wait.
fn turn_event(v: &Value) -> Option<Agent> {
    if v.get("type")?.as_str()? != "event_msg" {
        return None;
    }
    let p = &v["payload"];
    let (state, at) = match p.get("type")?.as_str()? {
        "task_started" => (AgentState::Running, "started_at"),
        "task_complete" => (AgentState::Idle, "completed_at"),
        "turn_aborted" => (AgentState::Idle, ""),
        _ => return None,
    };
    Some(Agent {
        state,
        since: p.get(at).and_then(Value::as_u64),
    })
}

pub struct CodexSource {
    dirs: Vec<PathBuf>,
    /// The folder, canonical.
    dir: PathBuf,
    started: SystemTime,
    path: Option<PathBuf>,
    tail: Option<JsonlTail>,
    conv: Conversation,
    status: Option<Agent>,
    /// The rollout a reset was sent for; None before the first.
    reset_for: Option<Option<PathBuf>>,
    list: ListProcesses,
    cwds: Cwds,
    polls: u32,
}

impl CodexSource {
    fn new(
        dirs: Vec<PathBuf>,
        dir: PathBuf,
        started: SystemTime,
        path: Option<PathBuf>,
        list: ListProcesses,
        cwds: Cwds,
    ) -> Self {
        Self {
            dirs,
            dir,
            started,
            path,
            tail: None,
            conv: Conversation::default(),
            status: None,
            reset_for: None,
            list,
            cwds,
            polls: 0,
        }
    }

    /// Ends the chat when Codex left the folder; follows a newer rollout
    /// (`/new`) or the first one, which Codex writes with the first prompt.
    fn check(&mut self) -> Result<(), Error> {
        let procs = (self.list)()?;
        if started_in(&self.dir, &procs, self.cwds.as_ref())?.is_none() {
            return Err(Error::Gone("Codex exited".into()));
        }
        let newest = find_rollout(&self.dirs, &self.dir, self.started)?;
        if newest.is_some() && newest != self.path {
            self.path = newest;
            self.tail = None;
        }
        Ok(())
    }

    fn reset(&mut self) -> Result<Option<ChatEvent>, Error> {
        self.conv = Conversation::default();
        let Some(path) = self.path.clone() else {
            return Ok(self.mark_reset(Vec::new(), false));
        };
        let (tail, lines, cut) = JsonlTail::open(&path, INITIAL_TAIL)?;
        self.tail = Some(tail);
        for line in &lines {
            self.conv.apply(line);
        }
        let all = self.conv.log.len();
        Ok(self.mark_reset(self.conv.log.recent(), cut || all > INITIAL_ENTRIES))
    }

    fn mark_reset(&mut self, entries: Vec<ChatEntry>, truncated: bool) -> Option<ChatEvent> {
        if self.reset_for.as_ref() == Some(&self.path) && entries.is_empty() {
            return None;
        }
        self.reset_for = Some(self.path.clone());
        Some(ChatEvent::Reset { entries, truncated })
    }
}

impl ChatSource for CodexSource {
    fn poll(&mut self) -> Result<Vec<ChatEvent>, Error> {
        if self.polls % CHECK_EVERY == 0 {
            self.check()?;
        }
        self.polls = self.polls.wrapping_add(1);
        let mut events = Vec::new();
        let replaced = match self.tail.as_mut() {
            None => true,
            Some(tail) => match tail.poll()? {
                Poll::Replaced => true,
                Poll::Lines(lines) => {
                    for line in &lines {
                        events.extend(self.conv.apply(line).into_iter().map(ChatEvent::Upsert));
                    }
                    false
                }
            },
        };
        if replaced {
            self.tail = None;
            events.extend(self.reset()?);
        }
        self.status = self.conv.turn.clone();
        Ok(events)
    }

    fn entries(&self) -> Vec<ChatEntry> {
        self.conv.log.recent()
    }

    fn status(&self) -> Option<Agent> {
        self.status.clone()
    }
}

/// The normalized conversation so far. A call's output arrives in a later line
/// with the same `call_id`.
#[derive(Default)]
struct Conversation {
    log: EntryLog,
    /// call_id -> entry id
    calls: HashMap<String, String>,
    turn: Option<Agent>,
}

impl Conversation {
    /// Applies one rollout line and returns the entries it added or changed.
    fn apply(&mut self, line: &[u8]) -> Vec<ChatEntry> {
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            return Vec::new();
        };
        if let Some(turn) = turn_event(&v) {
            self.turn = Some(turn);
            return Vec::new();
        }
        if v.get("type").and_then(Value::as_str) != Some("response_item") {
            return Vec::new();
        }
        let p = &v["payload"];
        let changed = match p.get("type").and_then(Value::as_str) {
            Some("message") => self.message(p),
            Some("function_call" | "custom_tool_call") => self.call(p),
            Some("function_call_output" | "custom_tool_call_output") => self.output(p),
            // Reasoning and anything new stay out of the chat.
            _ => None,
        };
        changed.into_iter().collect()
    }

    fn message(&mut self, p: &Value) -> Option<ChatEntry> {
        let id = p.get("id")?.as_str()?;
        let kind = match p.get("role")?.as_str()? {
            "user" => ChatKind::User,
            "assistant" => ChatKind::Assistant,
            // Developer and system messages are Codex's own instructions.
            _ => return None,
        };
        let text = p
            .get("content")?
            .as_array()?
            .iter()
            .filter_map(|c| c.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n\n");
        if text.is_empty() || (kind == ChatKind::User && injected(&text)) {
            return None;
        }
        Some(self.upsert(ChatEntry {
            id: format!("m:{id}"),
            kind,
            text: cap(&text, MAX_TEXT),
            tool: None,
        }))
    }

    fn call(&mut self, p: &Value) -> Option<ChatEntry> {
        let call_id = p.get("call_id")?.as_str()?;
        let name = p.get("name").and_then(Value::as_str).unwrap_or("tool");
        let input = p
            .get("arguments")
            .or_else(|| p.get("input"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let id = format!("t:{call_id}");
        self.calls.insert(call_id.to_owned(), id.clone());
        Some(self.upsert(ChatEntry {
            id,
            kind: ChatKind::Tool,
            text: String::new(),
            tool: Some(ChatTool {
                name: name.to_owned(),
                summary: summary(input),
                result: None,
                error: false,
            }),
        }))
    }

    fn output(&mut self, p: &Value) -> Option<ChatEntry> {
        let call_id = p.get("call_id")?.as_str()?;
        // An output whose call was before the initial cut has nothing to attach to.
        let entry_id = self.calls.get(call_id)?.clone();
        let mut entry = self.log.get(&entry_id)?.clone();
        entry.tool.as_mut()?.result = Some(cap(&output_text(p.get("output")), MAX_RESULT));
        Some(self.upsert(entry))
    }

    fn upsert(&mut self, entry: ChatEntry) -> ChatEntry {
        let (entry, trimmed) = self.log.upsert(entry);
        if trimmed {
            let log = &self.log;
            self.calls.retain(|_, id| log.contains(id));
        }
        entry
    }
}

/// Context Codex adds as user messages: AGENTS.md, and whole tagged blocks such
/// as `<environment_context>…</environment_context>`.
fn injected(text: &str) -> bool {
    let t = text.trim();
    if t.starts_with("# AGENTS.md") {
        return true;
    }
    let Some(tag) = t
        .strip_prefix('<')
        .and_then(|r| r.split_once('>'))
        .map(|(tag, _)| tag)
    else {
        return false;
    };
    !tag.is_empty()
        && tag.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
        && t.ends_with(&format!("</{tag}>"))
}

/// One line about a call: its command, or the first line of its input.
fn summary(input: &str) -> String {
    let line = match serde_json::from_str::<Value>(input) {
        Ok(Value::Object(args)) => match args.get("command").or_else(|| args.get("cmd")) {
            // `["bash", "-lc", "<script>"]`: the script is what was run.
            Some(Value::Array(parts)) => parts
                .last()
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            Some(Value::String(s)) => s.clone(),
            _ => args
                .values()
                .find_map(Value::as_str)
                .unwrap_or("")
                .to_owned(),
        },
        _ => input.to_owned(),
    };
    let first = line
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    cap(first, MAX_SUMMARY)
}

/// An output is a string or a list of text items.
fn output_text(output: Option<&Value>) -> String {
    match output {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests;
