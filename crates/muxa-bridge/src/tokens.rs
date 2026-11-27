//! Tokens for one day from the agents' own transcripts (ADR 0103 §6): Claude
//! Code's `message.usage` and Codex's `token_count` totals, by working folder.
//! Tokens only, never a share of a plan limit. No text of a line is kept.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use jiff::civil::Date;
use jiff::tz::TimeZone;
use muxa_protocol::v2::{TaskTokens, TokenUsage, Tokens, WorkspaceTokens};
use serde_json::Value;

/// A line longer than this is no usage line; it is skipped unread.
const MAX_LINE: usize = 1 << 20;
/// Codex files its rollouts by the day they started: a session that runs
/// into `day` started at most this many days before.
const CODEX_DAYS_BEFORE: i64 = 2;

/// Tokens as both agents can tell them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Count {
    /// Input not read from the cache: new input and cache writes.
    pub input: u64,
    /// Input read from the cache.
    pub cached: u64,
    /// Output, reasoning included.
    pub output: u64,
}

impl Count {
    pub fn add(self, o: Count) -> Count {
        Count {
            input: self.input + o.input,
            cached: self.cached + o.cached,
            output: self.output + o.output,
        }
    }

    fn after(self, before: Count) -> Count {
        Count {
            input: self.input.saturating_sub(before.input),
            cached: self.cached.saturating_sub(before.cached),
            output: self.output.saturating_sub(before.output),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Count::default()
    }
}

/// One local day, as Unix seconds `[start, end)`.
#[derive(Debug, Clone, Copy)]
pub struct Day {
    pub date: Date,
    start: i64,
    end: i64,
}

impl Day {
    pub fn new(date: Date, zone: &TimeZone) -> Option<Day> {
        let start = date.to_zoned(zone.clone()).ok()?.timestamp().as_second();
        let next = date.tomorrow().ok()?;
        let end = next.to_zoned(zone.clone()).ok()?.timestamp().as_second();
        Some(Day { date, start, end })
    }

    fn holds(&self, line: &Value) -> Option<Moment> {
        let at: jiff::Timestamp = line.get("timestamp")?.as_str()?.parse().ok()?;
        let s = at.as_second();
        Some(if s < self.start {
            Moment::Before
        } else if s < self.end {
            Moment::In
        } else {
            Moment::After
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Moment {
    Before,
    In,
    After,
}

/// Blocking. Every agent's tokens on `day`, by its working folder as the
/// transcript names it. A file that cannot be read counts nothing.
pub fn by_folder(
    claude_dirs: &[PathBuf],
    codex_dirs: &[PathBuf],
    day: &Day,
) -> HashMap<String, Count> {
    let mut out: HashMap<String, Count> = HashMap::new();
    let mut add = |cwd: String, c: Count| {
        let at = out.entry(cwd).or_default();
        *at = at.add(c);
    };
    for path in claude_dirs.iter().flat_map(|d| claude_files(d, day)) {
        for (cwd, c) in claude_file(&path, day) {
            add(cwd, c);
        }
    }
    for path in codex_dirs.iter().flat_map(|d| codex_files(d, day)) {
        if let Some((cwd, c)) = codex_file(&path, day) {
            add(cwd, c);
        }
    }
    out
}

/// `projects/*/*.jsonl` and the subagents' `projects/*/<session>/subagents/*.jsonl`
/// changed since the day began.
fn claude_files(dir: &Path, day: &Day) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for project in subdirs(&dir.join("projects")) {
        out.extend(jsonl_since(&project, day));
        for session in subdirs(&project) {
            out.extend(jsonl_since(&session.join("subagents"), day));
        }
    }
    out
}

/// `sessions/YYYY/MM/DD/*.jsonl` of `day` and the days before it, changed
/// since the day began.
fn codex_files(dir: &Path, day: &Day) -> Vec<PathBuf> {
    (0..=CODEX_DAYS_BEFORE)
        .filter_map(|n| day.date.checked_sub(jiff::Span::new().days(n)).ok())
        .flat_map(|d| {
            let folder = dir
                .join("sessions")
                .join(format!("{:04}", d.year()))
                .join(format!("{:02}", d.month()))
                .join(format!("{:02}", d.day()));
            jsonl_since(&folder, day)
        })
        .collect()
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default()
}

fn jsonl_since(dir: &Path, day: &Day) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter(|e| {
            e.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .is_some_and(|t| t.as_secs() as i64 >= day.start)
        })
        .map(|e| e.path())
        .collect()
}

/// The lines of `path` that hold `needle`, parsed. Bad lines are skipped.
fn lines_with(path: &Path, needle: &str) -> Vec<Value> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .split(b'\n')
        .map_while(Result::ok)
        .filter(|l| l.len() <= MAX_LINE)
        .filter_map(|l| String::from_utf8(l).ok())
        .filter(|l| l.contains(needle))
        .filter_map(|l| serde_json::from_str(&l).ok())
        .collect()
}

/// Claude Code writes one line per content block, each with the message's
/// whole `usage`: a message counts once, by its id.
pub fn claude_file(path: &Path, day: &Day) -> HashMap<String, Count> {
    let mut messages: HashMap<String, (String, Count)> = HashMap::new();
    for (n, line) in lines_with(path, "\"usage\"").iter().enumerate() {
        if line.get("type").and_then(Value::as_str) != Some("assistant")
            || day.holds(line) != Some(Moment::In)
        {
            continue;
        }
        let (Some(cwd), Some(usage)) = (
            line.get("cwd").and_then(Value::as_str),
            line.pointer("/message/usage"),
        ) else {
            continue;
        };
        let tokens = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
        let count = Count {
            input: tokens("input_tokens") + tokens("cache_creation_input_tokens"),
            cached: tokens("cache_read_input_tokens"),
            output: tokens("output_tokens"),
        };
        let id = line
            .pointer("/message/id")
            .and_then(Value::as_str)
            .map_or_else(|| format!("line {n}"), str::to_owned);
        messages.insert(id, (cwd.to_owned(), count));
    }
    let mut out: HashMap<String, Count> = HashMap::new();
    for (cwd, c) in messages.into_values() {
        let at = out.entry(cwd).or_default();
        *at = at.add(c);
    }
    out
}

/// Codex's `token_count` totals grow through a session: its tokens on `day`
/// are its last total of the day less its last one before.
pub fn codex_file(path: &Path, day: &Day) -> Option<(String, Count)> {
    let cwd = lines_with(path, "\"session_meta\"")
        .into_iter()
        .find(|l| l.get("type").and_then(Value::as_str) == Some("session_meta"))?
        .pointer("/payload/cwd")?
        .as_str()?
        .to_owned();
    let (mut before, mut last) = (Count::default(), None);
    for line in lines_with(path, "\"token_count\"") {
        let Some(total) = line.pointer("/payload/info/total_token_usage") else {
            continue;
        };
        let tokens = |k: &str| total.get(k).and_then(Value::as_u64).unwrap_or(0);
        let (input, cached) = (tokens("input_tokens"), tokens("cached_input_tokens"));
        let count = Count {
            input: input.saturating_sub(cached),
            cached,
            output: tokens("output_tokens"),
        };
        match day.holds(&line) {
            Some(Moment::Before) => before = count,
            Some(Moment::In) => last = Some(count),
            _ => {}
        }
    }
    let used = last?.after(before);
    (!used.is_empty()).then_some((cwd, used))
}

impl From<Count> for Tokens {
    fn from(c: Count) -> Self {
        Tokens {
            input: c.input,
            cached: c.cached,
            output: c.output,
        }
    }
}

/// How many folders `token-usage` names.
pub const MAX_TASKS: usize = 50;

/// Pure. The folders' tokens by Workspace, and by folder inside one other
/// than its root. A folder outside every Workspace is left out: its path
/// never leaves the host. `roots` are canonical; a nested root wins.
pub fn assign(by: HashMap<String, Count>, roots: &[(String, PathBuf)], day: &Day) -> TokenUsage {
    let mut workspaces: HashMap<&str, Count> = HashMap::new();
    let mut tasks = Vec::new();
    for (cwd, count) in by {
        let dir = Path::new(&cwd);
        let Some((name, root)) = roots
            .iter()
            .filter(|(_, root)| dir.starts_with(root))
            .max_by_key(|(_, root)| root.components().count())
        else {
            continue;
        };
        let at = workspaces.entry(name).or_default();
        *at = at.add(count);
        if dir != root {
            let title = dir
                .file_name()
                .map_or_else(|| cwd.clone(), |n| n.to_string_lossy().into_owned());
            tasks.push(TaskTokens {
                workspace: name.clone(),
                dir: cwd.clone(),
                title,
                tokens: count.into(),
            });
        }
    }
    let mut workspaces: Vec<WorkspaceTokens> = workspaces
        .into_iter()
        .map(|(w, c)| WorkspaceTokens {
            workspace: w.to_owned(),
            tokens: c.into(),
        })
        .collect();
    let work = |t: &Tokens| std::cmp::Reverse(t.input + t.output);
    workspaces.sort_by(|a, b| {
        work(&a.tokens)
            .cmp(&work(&b.tokens))
            .then(a.workspace.cmp(&b.workspace))
    });
    tasks.sort_by(|a, b| {
        work(&a.tokens)
            .cmp(&work(&b.tokens))
            .then(a.dir.cmp(&b.dir))
    });
    tasks.truncate(MAX_TASKS);
    TokenUsage {
        day: day.date.to_string(),
        workspaces,
        tasks,
    }
}

#[cfg(test)]
#[path = "tokens_tests.rs"]
mod tests;
