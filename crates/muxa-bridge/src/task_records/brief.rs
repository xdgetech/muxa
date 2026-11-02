//! `.muxa/TASK.md`, the Task's working memory (ADR 0015, 0018). The bridge
//! owns the header above the notes marker; the agent owns the marker line and
//! everything below it, which a rewrite keeps byte for byte.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::records::Record;

pub(super) const DIR: &str = ".muxa";
const FILE: &str = "TASK.md";
/// `TASK.md` relative to the task's directory, as the Reader lists it.
pub const BRIEF: &str = ".muxa/TASK.md";
/// `HANDOFF.md` relative to the task's directory, as the Reader lists it (ADR 0018).
pub const HANDOFF: &str = ".muxa/HANDOFF.md";
/// The pattern added to git's exclude file.
const EXCLUDE: &str = ".muxa/";
/// A line that starts with this is the marker, whatever follows on it.
const MARKER_START: &str = "<!-- muxa:notes";
const MARKER: &str = "<!-- muxa:notes - muxa never changes anything below this line. -->";
const HOW_TO: &str = "## How to use this file\n\n\
     Keep the sections below the marker current as you work: what is done, what you \
     decided and why, and what comes next. A new agent session on this Task reads this \
     file first. muxa rewrites only the part above the marker.\n";
const NOTES: &str = "## Progress\n\nNothing yet.\n\n\
     ## Decisions\n\nNothing yet.\n\n\
     ## Next actions\n\nNothing yet.\n";
const CHECK_TEXT: &str =
    "muxa runs this after each of your turns. The Task is ready for review when it exits 0.";
const EARLIER: &str = "## Earlier notes\n\n";

/// The goal and where the work is: `TASK.md` as ADR 0015 first wrote it.
fn context(r: &Record) -> String {
    let mut out = format!(
        "# {}\n\n## Goal\n\n{}\n\n## Context\n\n",
        r.title,
        r.goal.trim_end()
    );
    out.push_str(&format!("- Task: {}\n- Workspace: {}\n", r.id, r.workspace));
    if let Some(project) = &r.project {
        out.push_str(&format!("- Project: {project}\n"));
    }
    if let Some(repo) = &r.repo {
        out.push_str(&format!("- Repo: {}\n", repo.display()));
    }
    if let Some(branch) = &r.branch {
        out.push_str(&format!("- Branch: {branch}\n"));
    }
    out
}

/// The part the bridge owns, up to the marker.
fn header(r: &Record) -> String {
    format!("{}\n{}{HOW_TO}\n", context(r), check(r))
}

/// The loop's check, so the agent can run it itself (ADR 0019). Empty
/// without a loop.
fn check(r: &Record) -> String {
    let Some(lp) = r.task_loop().ok().flatten() else {
        return String::new();
    };
    let longest = lp
        .check
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!(
        "## Check\n\n{fence}sh\n{}\n{fence}\n\n{CHECK_TEXT}\n\n",
        lp.check
    )
}

/// A new `TASK.md`.
pub fn render(r: &Record) -> String {
    format!("{}{MARKER}\n\n{NOTES}", header(r))
}

/// `TASK.md` rewritten from `existing`: a new header, and the agent's part
/// kept from the marker line on. Without a marker, nothing of the old text
/// is dropped: unless it is exactly what ADR 0015 wrote, it follows under
/// "Earlier notes".
pub fn merge(r: &Record, existing: Option<&str>) -> String {
    let Some(old) = existing.filter(|t| !t.trim().is_empty()) else {
        return render(r);
    };
    if let Some(at) = marker_at(old) {
        return format!("{}{}", header(r), &old[at..]);
    }
    if old == context(r) {
        return render(r);
    }
    let lead = if old.ends_with('\n') { "" } else { "\n" };
    format!("{}\n{EARLIER}{old}{lead}", render(r))
}

/// The byte offset of the marker line.
fn marker_at(text: &str) -> Option<usize> {
    if text.starts_with(MARKER_START) {
        return Some(0);
    }
    text.match_indices(&format!("\n{MARKER_START}"))
        .next()
        .map(|(i, _)| i + 1)
}

/// Writes a new `<dir>/.muxa/TASK.md` for a new Task. In a repo, `.muxa/` is
/// excluded first, so it never shows as a change. Blocking.
pub fn create(dir: &Path, record: &Record, in_repo: bool) -> Result<PathBuf, String> {
    if in_repo {
        exclude(dir)?;
    }
    write_file(dir, FILE, &render(record))
}

/// Rewrites the header of `<dir>/.muxa/TASK.md` and keeps the agent's notes.
/// Blocking.
pub fn refresh(dir: &Path, record: &Record) -> Result<PathBuf, String> {
    let path = dir.join(DIR).join(FILE);
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    write_file(dir, FILE, &merge(record, existing.as_deref()))
}

/// Writes `<dir>/.muxa/<name>` through a temp file and a rename, so the
/// agent never reads half of it. Blocking.
pub(super) fn write_file(dir: &Path, name: &str, text: &str) -> Result<PathBuf, String> {
    let folder = dir.join(DIR);
    std::fs::create_dir_all(&folder)
        .map_err(|e| format!("cannot create {}: {e}", folder.display()))?;
    let path = folder.join(name);
    let tmp = folder.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, text).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("cannot write {}: {e}", path.display())
    })?;
    Ok(path)
}

/// Adds `.muxa/` to the repo's `info/exclude`, shared by all its worktrees.
fn exclude(dir: &Path) -> Result<(), String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(format!("{} is not in a git repository", dir.display()));
    }
    let common = String::from_utf8_lossy(&out.stdout);
    let info = Path::new(common.trim_end_matches('\n')).join("info");
    let path = info.join("exclude");
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let Some(addition) = exclude_addition(&existing) else {
        return Ok(());
    };
    std::fs::create_dir_all(&info).map_err(|e| format!("cannot create {}: {e}", info.display()))?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(addition.as_bytes()))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// What to append to an exclude file, or None when it already has the line.
pub fn exclude_addition(existing: &str) -> Option<String> {
    let present = existing
        .lines()
        .map(str::trim)
        .any(|l| l == EXCLUDE || l == "/.muxa/" || l == ".muxa");
    if present {
        return None;
    }
    let lead = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    Some(format!("{lead}{EXCLUDE}\n"))
}

#[cfg(test)]
mod check_tests {
    use super::*;
    use crate::records::{Loop, LoopSpec, Status};

    fn record() -> Record {
        Record {
            version: 1,
            id: "t-7f3a2c".into(),
            rev: 1,
            workspace: "work".into(),
            title: "Fix login".into(),
            goal: "Make login work.".into(),
            status: Status::Open,
            created_at: 1,
            updated_at: 1,
            project: None,
            repo: None,
            branch: None,
            dir: None,
            session: None,
            note: None,
            runs: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }

    fn with_check(check: &str) -> Record {
        let mut r = record();
        let spec = LoopSpec {
            check: check.into(),
            max_attempts: None,
            timeout: None,
        };
        r.set_loop(Some(&Loop::start(spec, None, false, 1)))
            .unwrap();
        r
    }

    #[test]
    fn a_loop_puts_its_check_in_the_header() {
        let text = render(&with_check("make check"));
        let check = text
            .find("## Check\n\n```sh\nmake check\n```\n\n")
            .expect("a Check section");
        assert!(text[check..].contains(CHECK_TEXT));
        assert!(check < text.find("## How to use this file").unwrap());
        assert!(check < text.find(MARKER).unwrap(), "muxa owns it");
        assert!(!render(&record()).contains("## Check"));
    }

    #[test]
    fn the_fence_outgrows_backticks_in_the_command() {
        let text = render(&with_check("echo ```x``` `y`"));
        assert!(text.contains("````sh\necho ```x``` `y`\n````\n"), "{text}");
    }

    #[test]
    fn starting_or_ending_a_loop_keeps_the_notes() {
        let old = format!("{}{MARKER}\n\nmine\n", header(&record()));
        let new = merge(&with_check("make check"), Some(&old));
        assert!(new.contains("## Check"));
        assert!(new.ends_with(&format!("{MARKER}\n\nmine\n")));
    }
}
