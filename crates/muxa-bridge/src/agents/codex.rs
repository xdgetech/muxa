//! Codex CLI. Each conversation is appended to
//! `<dir>/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl`, whose first line
//! (`session_meta`) names its cwd. Codex records no pane, so a pane's `codex`
//! process is matched to the newest rollout in its cwd written since it started.
//! The format is private to Codex; anything unexpected is skipped, never guessed.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use muxa_protocol::{Agent, AgentState, ChatEntry, ChatKind, ChatTool};
use serde_json::Value;

use super::log::{EntryLog, INITIAL_ENTRIES, MAX_RESULT, MAX_SUMMARY, MAX_TEXT, cap};
use super::tail::{INITIAL_TAIL, JsonlTail, MAX_LINE, Poll};
use super::{AgentAdapter, ChatEvent, ChatSource, Error, Found, PaneInfo, Processes};

pub const ID: &str = "codex";
/// The process name, the same for the Homebrew and the npm install.
const PROCESS: &str = "codex";
/// How often, in polls, the source checks the process and looks for a newer rollout.
const CHECK_EVERY: u32 = 8;
/// Enough of a rollout's end to find its last turn event.
const STATE_TAIL: u64 = 256 << 10;
const DAY: Duration = Duration::from_secs(86_400);

type ListProcesses = Box<dyn Fn() -> Result<Processes, Error> + Send>;

pub struct CodexAdapter {
    dir: PathBuf,
}

impl CodexAdapter {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl AgentAdapter for CodexAdapter {
    fn id(&self) -> &'static str {
        ID
    }

    fn open(
        &self,
        pane: &PaneInfo,
        procs: &Processes,
    ) -> Result<Option<Box<dyn ChatSource>>, Error> {
        let Some(started) = started(pane, procs) else {
            return Ok(None);
        };
        let rollout = find_rollout(&self.dir, &pane.path, started)?;
        Ok(Some(Box::new(CodexSource::new(
            self.dir.clone(),
            started,
            rollout,
            Box::new(Processes::list),
        ))))
    }

    fn find(&self, pane: &PaneInfo, procs: &Processes) -> Option<Found> {
        let started = started(pane, procs)?;
        let status = find_rollout(&self.dir, &pane.path, started)
            .ok()
            .flatten()
            .and_then(|path| last_turn(&path))
            .filter(|_| !pane.shell_foreground);
        Some(Found { status })
    }
}

/// When the `codex` process in `pane` started, or None when there is none.
fn started(pane: &PaneInfo, procs: &Processes) -> Option<SystemTime> {
    procs
        .named_within(PROCESS, pane.pid)
        .into_iter()
        .filter_map(|pid| procs.started(pid))
        .min()
}

/// The newest rollout for `cwd` written since `started`. Day directories are
/// searched back to the day before `started`, which covers time zones.
fn find_rollout(dir: &Path, cwd: &str, started: SystemTime) -> Result<Option<PathBuf>, Error> {
    // `ps` ages are whole seconds.
    let since = started
        .checked_sub(Duration::from_secs(1))
        .unwrap_or(UNIX_EPOCH);
    let first_day = day_key(since.checked_sub(DAY).unwrap_or(UNIX_EPOCH));
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for day in day_dirs(&dir.join("sessions"), &first_day)? {
        let Ok(files) = std::fs::read_dir(&day) else {
            continue;
        };
        for path in files.filter_map(Result::ok).map(|e| e.path()) {
            if path.extension().is_none_or(|x| x != "jsonl") {
                continue;
            }
            let Some(mtime) = std::fs::metadata(&path)
                .ok()
                .and_then(|m| m.modified().ok())
            else {
                continue;
            };
            if mtime < since || best.as_ref().is_some_and(|(t, _)| *t >= mtime) {
                continue;
            }
            if meta_cwd(&path).as_deref() == Some(cwd) {
                best = Some((mtime, path));
            }
        }
    }
    Ok(best.map(|(_, p)| p))
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
    let days = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}/{month:02}/{day:02}")
}

/// The cwd in a rollout's first line.
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

/// The state after the last turn event near the end of a rollout.
fn last_turn(path: &Path) -> Option<Agent> {
    let (_, lines, _) = JsonlTail::open(path, STATE_TAIL).ok()?;
    lines.iter().rev().find_map(|l| {
        let v: Value = serde_json::from_slice(l).ok()?;
        turn_event(&v)
    })
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
    dir: PathBuf,
    started: SystemTime,
    path: Option<PathBuf>,
    tail: Option<JsonlTail>,
    conv: Conversation,
    status: Option<Agent>,
    /// The rollout a reset was sent for; None before the first.
    reset_for: Option<Option<PathBuf>>,
    list: ListProcesses,
    polls: u32,
}

impl CodexSource {
    fn new(dir: PathBuf, started: SystemTime, path: Option<PathBuf>, list: ListProcesses) -> Self {
        Self {
            dir,
            started,
            path,
            tail: None,
            conv: Conversation::default(),
            status: None,
            reset_for: None,
            list,
            polls: 0,
        }
    }

    /// Ends the chat when Codex exited; follows a newer rollout (`/new`) or the
    /// first one, which Codex writes with the first prompt.
    fn check(&mut self, pane: &PaneInfo) -> Result<(), Error> {
        let procs = (self.list)()?;
        if procs.named_within(PROCESS, pane.pid).is_empty() {
            return Err(Error::Gone("Codex exited".into()));
        }
        let newest = find_rollout(&self.dir, &pane.path, self.started)?;
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
    fn poll(&mut self, pane: &PaneInfo) -> Result<Vec<ChatEvent>, Error> {
        if self.polls % CHECK_EVERY == 0 {
            self.check(pane)?;
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
        // Back at a shell, the recorded state is stale.
        self.status = self.conv.turn.clone().filter(|_| !pane.shell_foreground);
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
