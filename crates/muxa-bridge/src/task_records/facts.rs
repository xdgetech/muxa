//! `.muxa/HANDOFF.md` (ADR 0018): facts the bridge can check, written before
//! it restarts a Task's agent. Runs, git and changed files; never transcript
//! content, only where a transcript is.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::changes::git::{fork_point, git};
use crate::records::{Record, Run, RunReason, Seen, Status};
use crate::utc::stamp;

pub const FILE: &str = "HANDOFF.md";
const MAX_COMMITS: usize = 50;
const MAX_STATUS: usize = 200;
const MAX_FILES: usize = 100;
/// Entries a walk of a task directory visits at most.
const MAX_WALK: usize = 10_000;

/// What git says about a repo Task.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Git {
    pub branch: Option<String>,
    /// The base ref, when the branch has one.
    pub base: Option<String>,
    /// `<short hash> <subject>`, newest first.
    pub commits: Vec<String>,
    pub more_commits: bool,
    /// `git status --short` lines.
    pub status: Vec<String>,
    pub more_status: bool,
}

/// Files of a repo-less Task changed since a time, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changed {
    pub since: u64,
    /// Relative path and mtime.
    pub files: Vec<(String, u64)>,
    pub more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Work {
    Git(Git),
    Files(Changed),
    /// The Task has no directory the bridge can read.
    Unknown,
}

/// Collects the facts about the work in `dir`. Blocking.
pub fn work(record: &Record, dir: &Path, since: u64) -> Work {
    if record.repo.is_some() {
        return git_facts(dir).map_or(Work::Unknown, Work::Git);
    }
    if !dir.is_dir() {
        return Work::Unknown;
    }
    Work::Files(changed_files(dir, since))
}

fn git_facts(dir: &Path) -> Option<Git> {
    let status = lines(git(dir).args(["status", "--short"]))?;
    let branch = lines(git(dir).args(["symbolic-ref", "-q", "--short", "HEAD"]))
        .and_then(|l| l.into_iter().next());
    let mut facts = Git {
        branch,
        more_status: status.len() > MAX_STATUS,
        status: status.into_iter().take(MAX_STATUS).collect(),
        ..Git::default()
    };
    if let Some((base, fork)) = fork_point(dir) {
        let range = format!("{fork}..HEAD");
        let limit = format!("-n{}", MAX_COMMITS + 1);
        let commits = lines(git(dir).args(["log", "--format=%h %s", &limit, &range]))?;
        facts.more_commits = commits.len() > MAX_COMMITS;
        facts.commits = commits.into_iter().take(MAX_COMMITS).collect();
        facts.base = Some(base);
    }
    Some(facts)
}

/// A command's output lines, with control characters made visible.
fn lines(cmd: &mut std::process::Command) -> Option<Vec<String>> {
    let out = cmd.output().ok().filter(|o| o.status.success())?;
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(clean)
            .collect(),
    )
}

/// A name or subject on one line: a control character cannot start a fake
/// section in the file the agent reads.
fn clean(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// Files under `dir` changed at or after `since`. Hidden entries, `.muxa/`
/// among them, are skipped, and symlinks are not followed.
pub fn changed_files(dir: &Path, since: u64) -> Changed {
    let mut found = Vec::new();
    let mut stack = vec![PathBuf::new()];
    let mut visited = 0;
    let mut more = false;
    while let Some(rel) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(dir.join(&rel)) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            visited += 1;
            if visited > MAX_WALK {
                more = true;
                stack.clear();
                break;
            }
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let path = rel.join(&name);
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            if meta.is_file() && mtime >= since {
                found.push((clean(&path.to_string_lossy()), mtime));
            }
        }
    }
    found.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    more |= found.len() > MAX_FILES;
    found.truncate(MAX_FILES);
    Changed {
        since,
        files: found,
        more,
    }
}

/// The text of `HANDOFF.md`. `runs` are the runs up to the one that ends
/// now. `replaced` is what the adapter saw in the pane, shown on its own when
/// no open run recorded that agent.
pub fn render(r: &Record, runs: &[Run], replaced: Option<&Seen>, work: &Work, now: u64) -> String {
    let mut out = format!(
        "# Handoff: {}\n\nmuxa wrote this file at {} before it restarted the agent of task {}. \
         It holds facts muxa checked, not a summary. Your working notes are in .muxa/TASK.md.\n\n\
         ## Task\n\n- Status: {}\n",
        r.title,
        stamp(now),
        r.id,
        status(r.status)
    );
    if let Some(note) = &r.note {
        out.push_str(&format!("- Note: {}\n", clean(note)));
    }
    out.push_str(
        "\n## Agent runs\n\nOldest first. A transcript is the agent's own record of its \
         session: read it when you need more detail.\n\n",
    );
    if runs.is_empty() && replaced.is_none() {
        out.push_str("No runs were recorded.\n");
    }
    for run in runs {
        out.push_str(&run_line(run));
    }
    if let Some(seen) = replaced {
        out.push_str(&format!("- The agent replaced now: {}\n", seen_text(seen)));
    }
    out.push('\n');
    out.push_str(&work_text(work));
    out
}

fn status(s: Status) -> &'static str {
    match s {
        Status::Open => "open",
        Status::Review => "review",
        Status::Done => "done",
        Status::Dropped => "dropped",
    }
}

fn run_line(run: &Run) -> String {
    let reason = match run.reason {
        RunReason::Start => "start",
        RunReason::Restart => "restart",
    };
    let end = run
        .ended_at
        .map_or_else(|| "end not recorded".to_owned(), stamp);
    let seen = Seen {
        agent: run
            .agent
            .clone()
            .unwrap_or_else(|| "agent not reported".into()),
        session: run.agent_session.clone(),
        transcript: run.transcript.clone(),
    };
    format!(
        "- Run {} ({reason}), {} to {end}: {}\n",
        run.id,
        stamp(run.started_at),
        seen_text(&seen)
    )
}

fn seen_text(seen: &Seen) -> String {
    let mut out = clean(&seen.agent);
    match &seen.session {
        Some(id) => out.push_str(&format!(". Session {}", clean(id))),
        None => out.push_str(". Session not reported"),
    }
    if let Some(path) = &seen.transcript {
        out.push_str(&format!(". Transcript: {}", clean(&path.to_string_lossy())));
    }
    out.push('.');
    out
}

fn work_text(work: &Work) -> String {
    match work {
        Work::Git(g) => git_text(g),
        Work::Files(c) => files_text(c),
        Work::Unknown => "## Work\n\nmuxa could not read the task's directory.\n".into(),
    }
}

fn git_text(g: &Git) -> String {
    let mut out = String::from("## Git\n\n");
    match &g.branch {
        Some(b) => out.push_str(&format!("- Branch: {b}\n")),
        None => out.push_str("- Branch: none (detached HEAD)\n"),
    }
    match &g.base {
        Some(base) => {
            out.push_str(&format!(
                "- Commits since it left {base}: {}{}\n",
                g.commits.len(),
                if g.more_commits { " or more" } else { "" }
            ));
            for c in &g.commits {
                out.push_str(&format!("  - {c}\n"));
            }
        }
        None => out.push_str("- No base branch found.\n"),
    }
    if g.status.is_empty() {
        out.push_str("\nNo uncommitted changes.\n");
        return out;
    }
    out.push_str("\n`git status --short`:\n\n```\n");
    for line in &g.status {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("```\n");
    if g.more_status {
        out.push_str(&format!("\nOnly the first {MAX_STATUS} lines are shown.\n"));
    }
    out
}

fn files_text(c: &Changed) -> String {
    let mut out = format!("## Files changed since {}\n\n", stamp(c.since));
    if c.files.is_empty() {
        out.push_str("None.\n");
    }
    for (path, mtime) in &c.files {
        out.push_str(&format!("- {path} ({})\n", stamp(*mtime)));
    }
    if c.more {
        out.push_str(&format!(
            "\nThere are more; only the newest {MAX_FILES} are shown.\n"
        ));
    }
    out
}

#[cfg(test)]
mod tests;
