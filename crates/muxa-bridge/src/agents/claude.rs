//! Claude Code. Each running instance writes `<dir>/sessions/<pid>.json` (its
//! session id, cwd, status and tmux pane) and appends its conversation to
//! `<dir>/projects/<escaped cwd>/<session id>.jsonl`. Both formats are private to
//! Claude Code; anything unexpected is skipped, never guessed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use muxa_protocol::{Agent, AgentState, ChatEntry, ChatKind, ChatTool};
use serde_json::Value;

use super::tail::{INITIAL_TAIL, JsonlTail, Poll};
use super::{AgentAdapter, ChatEvent, ChatSource, Error, PaneInfo, Processes};

pub const ID: &str = "claude-code";
/// Entries sent on a reset.
const INITIAL_ENTRIES: usize = 200;
/// Entries kept for resends; older ones are dropped.
const MAX_ENTRIES: usize = 500;
const MAX_TEXT: usize = 64 << 10;
const MAX_RESULT: usize = 2 << 10;
const MAX_SUMMARY: usize = 200;

pub struct ClaudeAdapter {
    dir: PathBuf,
}

impl ClaudeAdapter {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl AgentAdapter for ClaudeAdapter {
    fn id(&self) -> &'static str {
        ID
    }

    fn open(
        &self,
        pane: &PaneInfo,
        procs: &Processes,
    ) -> Result<Option<Box<dyn ChatSource>>, Error> {
        Ok(find_session(&self.dir, pane, procs)?
            .map(|s| Box::new(ClaudeSource::new(self.dir.clone(), s)) as Box<dyn ChatSource>))
    }
}

/// One `sessions/<pid>.json` file.
#[derive(Debug, Clone)]
struct SessionFile {
    path: PathBuf,
    pid: u32,
    session_id: String,
    cwd: String,
    status: Option<AgentState>,
    mtime: SystemTime,
}

impl SessionFile {
    fn read(path: &Path) -> Option<Self> {
        let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
        let v: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        let session_id = v.get("sessionId")?.as_str()?;
        if !valid_session_id(session_id) {
            return None;
        }
        Some(Self {
            path: path.to_owned(),
            pid: u32::try_from(v.get("pid")?.as_u64()?).ok()?,
            session_id: session_id.to_owned(),
            cwd: v
                .get("cwd")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            status: v.get("status").and_then(Value::as_str).and_then(state),
            mtime,
        })
    }

    /// Only interactive instances own a pane; `tmux` is "<session>:@<window>.%<pane>".
    fn belongs_to(v: &Value, pane: &PaneInfo) -> bool {
        if v.get("kind").and_then(Value::as_str) != Some("interactive") {
            return false;
        }
        let Some(tmux) = v.get("tmux").and_then(Value::as_str) else {
            return false;
        };
        // A session name can contain ':' and '.', so split from the right.
        let Some((session, place)) = tmux.rsplit_once(':') else {
            return false;
        };
        place.rsplit_once('.').is_some_and(|(_, p)| p == pane.pane) && session == pane.session
    }

    fn agent(&self) -> Option<Agent> {
        Some(Agent {
            state: self.status?,
            since: self
                .mtime
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs()),
        })
    }
}

fn state(s: &str) -> Option<AgentState> {
    match s {
        "busy" => Some(AgentState::Running),
        "waiting" => Some(AgentState::Waiting),
        "idle" => Some(AgentState::Idle),
        _ => None,
    }
}

/// The session id becomes part of a path; allow nothing but a UUID's characters.
fn valid_session_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// The live, interactive Claude Code instance in `pane`. Pane ids restart with
/// the tmux server, so the session name must match too, and the process must
/// still run inside the pane: files of exited instances can remain.
fn find_session(
    dir: &Path,
    pane: &PaneInfo,
    procs: &Processes,
) -> Result<Option<SessionFile>, Error> {
    let sessions = dir.join("sessions");
    let entries = match std::fs::read_dir(&sessions) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(Error::Io {
                path: sessions,
                source,
            });
        }
    };
    let mut found: Vec<SessionFile> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter(|p| {
            std::fs::read(p)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .is_some_and(|v| SessionFile::belongs_to(&v, pane))
        })
        .filter_map(|p| SessionFile::read(&p))
        .filter(|s| procs.is_within(s.pid, pane.pid))
        .collect();
    found.sort_by_key(|s| (s.cwd != pane.path, std::cmp::Reverse(s.mtime)));
    Ok(found.into_iter().next())
}

/// `projects/<cwd with every non-alphanumeric as '-'>/<id>.jsonl`, or a search of
/// `projects/*/` when the escaping differs.
fn transcript(dir: &Path, session: &SessionFile) -> Option<PathBuf> {
    let projects = dir.join("projects");
    let name = format!("{}.jsonl", session.session_id);
    let escaped: String = session
        .cwd
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let direct = projects.join(escaped).join(&name);
    if direct.is_file() {
        return Some(direct);
    }
    std::fs::read_dir(&projects)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path().join(&name))
        .find(|p| p.is_file())
}

pub struct ClaudeSource {
    dir: PathBuf,
    session: SessionFile,
    tail: Option<JsonlTail>,
    conv: Conversation,
    status: Option<Agent>,
    /// The session id a reset was sent for; a new id needs a new reset.
    reset_for: Option<String>,
}

impl ClaudeSource {
    fn new(dir: PathBuf, session: SessionFile) -> Self {
        Self {
            dir,
            session,
            tail: None,
            conv: Conversation::default(),
            status: None,
            reset_for: None,
        }
    }

    /// Re-reads the session file when it changed. `/clear` and `/resume` keep the
    /// process but switch the session id, and with it the transcript.
    fn refresh_session(&mut self) -> Result<(), Error> {
        let path = self.session.path.clone();
        let mtime = match std::fs::metadata(&path) {
            Ok(m) => m.modified().ok(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::Gone("Claude Code exited".into()));
            }
            Err(source) => return Err(Error::Io { path, source }),
        };
        if mtime == Some(self.session.mtime) {
            return Ok(());
        }
        if let Some(next) = SessionFile::read(&path) {
            if next.session_id != self.session.session_id {
                self.tail = None;
                self.reset_for = None;
            }
            self.session = next;
        }
        Ok(())
    }

    fn reset(&mut self) -> Result<Option<ChatEvent>, Error> {
        self.conv = Conversation::default();
        let Some(path) = transcript(&self.dir, &self.session) else {
            // A new session writes its transcript with the first prompt.
            return Ok(self.mark_reset(Vec::new(), false));
        };
        let (tail, lines, cut) = JsonlTail::open(&path, INITIAL_TAIL)?;
        self.tail = Some(tail);
        for line in &lines {
            self.conv.apply(line);
        }
        let all = self.conv.entries.len();
        let entries = self.conv.entries[all.saturating_sub(INITIAL_ENTRIES)..].to_vec();
        Ok(self.mark_reset(entries, cut || all > INITIAL_ENTRIES))
    }

    fn mark_reset(&mut self, entries: Vec<ChatEntry>, truncated: bool) -> Option<ChatEvent> {
        if self.reset_for.as_deref() == Some(self.session.session_id.as_str()) && entries.is_empty()
        {
            return None;
        }
        self.reset_for = Some(self.session.session_id.clone());
        Some(ChatEvent::Reset { entries, truncated })
    }
}

impl ChatSource for ClaudeSource {
    fn poll(&mut self, pane: &PaneInfo) -> Result<Vec<ChatEvent>, Error> {
        self.refresh_session()?;
        // Back at a shell, the reported state is stale.
        self.status = if pane.shell_foreground {
            None
        } else {
            self.session.agent()
        };
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
        Ok(events)
    }

    fn entries(&self) -> Vec<ChatEntry> {
        let all = self.conv.entries.len();
        self.conv.entries[all.saturating_sub(INITIAL_ENTRIES)..].to_vec()
    }

    fn status(&self) -> Option<Agent> {
        self.status.clone()
    }
}

/// The normalized conversation so far. One assistant turn spans several lines
/// that share `message.id`; a tool's result arrives later in a user line.
#[derive(Default)]
struct Conversation {
    entries: Vec<ChatEntry>,
    index: HashMap<String, usize>,
    /// tool_use id -> entry id
    tools: HashMap<String, String>,
}

impl Conversation {
    /// Applies one transcript line and returns the entries it added or changed.
    fn apply(&mut self, line: &[u8]) -> Vec<ChatEntry> {
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            return Vec::new();
        };
        let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
        if flag("isSidechain") || flag("isMeta") {
            return Vec::new();
        }
        let uuid = v.get("uuid").and_then(Value::as_str).unwrap_or("");
        let content = &v["message"]["content"];
        match v.get("type").and_then(Value::as_str) {
            Some("user") => self.user(uuid, content),
            Some("assistant") => {
                let id = v["message"]["id"].as_str().unwrap_or(uuid);
                self.assistant(id, content)
            }
            _ => Vec::new(),
        }
    }

    fn user(&mut self, uuid: &str, content: &Value) -> Vec<ChatEntry> {
        if let Some(text) = content.as_str() {
            return self.user_text(uuid, text).into_iter().collect();
        }
        let mut changed = Vec::new();
        let mut texts = Vec::new();
        for block in content.as_array().into_iter().flatten() {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => texts.extend(block.get("text").and_then(Value::as_str)),
                Some("tool_result") => changed.extend(self.tool_result(block)),
                _ => {}
            }
        }
        if !texts.is_empty() {
            changed.extend(self.user_text(uuid, &texts.join("\n\n")));
        }
        changed
    }

    fn user_text(&mut self, uuid: &str, text: &str) -> Option<ChatEntry> {
        // Slash commands are recorded as tagged text; their output is noise here.
        if text.contains("<local-command-stdout>") || text.contains("<local-command-stderr>") {
            return None;
        }
        let (kind, text) = if let Some(name) = between(text, "<command-name>", "</command-name>") {
            let args = between(text, "<command-args>", "</command-args>").unwrap_or("");
            (
                ChatKind::Notice,
                format!("{name} {args}").trim_end().to_owned(),
            )
        } else if text.starts_with("[Request interrupted") {
            (ChatKind::Notice, "Interrupted".to_owned())
        } else {
            (ChatKind::User, text.to_owned())
        };
        Some(self.upsert(ChatEntry {
            id: format!("u:{uuid}"),
            kind,
            text: cap(&text, MAX_TEXT),
            tool: None,
        }))
    }

    fn tool_result(&mut self, block: &Value) -> Option<ChatEntry> {
        let use_id = block.get("tool_use_id")?.as_str()?;
        // A result whose call was before the initial cut has nothing to attach to.
        let entry_id = self.tools.get(use_id)?.clone();
        let mut entry = self.entries[*self.index.get(&entry_id)?].clone();
        let tool = entry.tool.as_mut()?;
        tool.result = Some(cap(&result_text(block.get("content")), MAX_RESULT));
        tool.error = block
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Some(self.upsert(entry))
    }

    fn assistant(&mut self, message_id: &str, content: &Value) -> Vec<ChatEntry> {
        let mut changed = Vec::new();
        for block in content.as_array().into_iter().flatten() {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    let Some(text) = block.get("text").and_then(Value::as_str) else {
                        continue;
                    };
                    let id = format!("a:{message_id}");
                    let merged = match self.index.get(&id) {
                        Some(&i) => format!("{}\n\n{text}", self.entries[i].text),
                        None => text.to_owned(),
                    };
                    changed.push(self.upsert(ChatEntry {
                        id,
                        kind: ChatKind::Assistant,
                        text: cap(&merged, MAX_TEXT),
                        tool: None,
                    }));
                }
                Some("tool_use") => {
                    let Some(use_id) = block.get("id").and_then(Value::as_str) else {
                        continue;
                    };
                    let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
                    let id = format!("t:{use_id}");
                    self.tools.insert(use_id.to_owned(), id.clone());
                    changed.push(self.upsert(ChatEntry {
                        id,
                        kind: ChatKind::Tool,
                        text: String::new(),
                        tool: Some(ChatTool {
                            name: name.to_owned(),
                            summary: summary(name, block.get("input").unwrap_or(&Value::Null)),
                            result: None,
                            error: false,
                        }),
                    }));
                }
                // Thinking and anything new stay out of the chat.
                _ => {}
            }
        }
        changed
    }

    fn upsert(&mut self, entry: ChatEntry) -> ChatEntry {
        match self.index.get(&entry.id) {
            Some(&i) => self.entries[i] = entry.clone(),
            None => {
                self.index.insert(entry.id.clone(), self.entries.len());
                self.entries.push(entry.clone());
                if self.entries.len() > MAX_ENTRIES + MAX_ENTRIES / 5 {
                    self.trim();
                }
            }
        }
        entry
    }

    fn trim(&mut self) {
        let drop = self.entries.len() - MAX_ENTRIES;
        self.entries.drain(..drop);
        self.index = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
        self.tools.retain(|_, id| self.index.contains_key(id));
    }
}

/// One line about a tool call: the command, file, pattern or URL it works on.
fn summary(name: &str, input: &Value) -> String {
    let key = match name {
        "Bash" => "command",
        "Read" | "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => "file_path",
        "Grep" | "Glob" => "pattern",
        "WebFetch" => "url",
        "WebSearch" => "query",
        "Task" | "Agent" => "description",
        _ => "",
    };
    let text = input
        .get(key)
        .and_then(Value::as_str)
        .or_else(|| input.as_object()?.values().find_map(Value::as_str))
        .unwrap_or("");
    cap(text.lines().next().unwrap_or(""), MAX_SUMMARY)
}

/// A tool result is a string or a list of text blocks; images are left out.
fn result_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)? + start;
    Some(text[start..end].trim())
}

/// At most `max` bytes, cut on a character boundary, with an ellipsis when cut.
fn cap(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

#[cfg(test)]
mod tests;
