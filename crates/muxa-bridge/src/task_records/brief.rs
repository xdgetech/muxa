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
#[cfg(test)]
fn header(r: &Record) -> String {
    header_with_section(r, &worker_section(r, None))
}

/// A reading Task's rules, or where a branched one wrote before (ADR 0034).
/// Empty for any other Task.
fn reading(r: &Record) -> String {
    if let Some(folder) = r.earlier_folder() {
        return format!(
            "## Task folder\n\n\
             Before it had a branch, this Task worked in {}. Read what it wrote there.\n\n",
            folder.display()
        );
    }
    let Some(repo) = r.repo.as_ref().filter(|_| r.is_read()) else {
        return String::new();
    };
    let repo = repo.display();
    format!(
        "## Reading the repo\n\n\
         This Task reads the repo at {repo}. It does not change it.\n\n\
         - Read {repo}/AGENTS.md or {repo}/CLAUDE.md first.\n\
         - Run git as `git -C {repo} ...`.\n\
         - Do not change files in {repo}. Write what this Task produces, such as a design, \
         a report or a plan, in this folder.\n\
         - When the work needs a change to the code, stop and ask the user for a branch. \
         muxa then makes a worktree and starts a new agent there with this file.\n\n"
    )
}

fn header_with_section(r: &Record, worker: &str) -> String {
    format!(
        "{}\n{}{}{worker}{HOW_TO}\n",
        context(r),
        reading(r),
        check(r)
    )
}

fn worker_section(r: &Record, instructions: Option<&str>) -> String {
    let Some(name) = r.worker() else {
        return String::new();
    };
    let mut body = format!("## Worker\n\n{name}\n\n");
    if let Some(text) = instructions {
        body.push_str(text);
        if !text.ends_with('\n') {
            body.push('\n');
        }
        body.push('\n');
    }
    // Length framing keeps instruction text containing Markdown/HTML markers intact.
    format!(
        "<!-- muxa:worker {} -->\n{body}<!-- muxa:worker-end -->\n\n",
        body.len()
    )
}

fn worker_frame<'a>(r: &Record, existing: &'a str) -> Option<(usize, usize, &'a str)> {
    let prefix = "<!-- muxa:worker ";
    let offset = managed_offset(r, existing);
    let start = existing[offset..].find(&format!("\n{prefix}"))? + offset + 1;
    if first_notes(existing, offset).is_some_and(|notes| start >= notes) {
        return None;
    }
    let body_start = existing[start..].find(" -->\n")? + start + " -->\n".len();
    let length = existing[start + prefix.len()..body_start - " -->\n".len()]
        .parse::<usize>()
        .ok()?;
    let body_end = body_start.checked_add(length)?;
    let body = existing.get(body_start..body_end)?;
    let suffix = "<!-- muxa:worker-end -->\n\n";
    existing
        .get(body_end..)?
        .starts_with(suffix)
        .then_some((start, body_end + suffix.len(), body))
}

fn retained_worker<'a>(r: &Record, existing: &'a str) -> Option<&'a str> {
    let name = r.worker()?;
    let (start, end, body) = worker_frame(r, existing)?;
    if !body.starts_with(&format!("## Worker\n\n{name}\n\n"))
        || end > marker_at(r, existing).unwrap_or(existing.len())
    {
        return None;
    }
    existing.get(start..end)
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
    render_with_worker(r, None)
}

/// Writes the validated instruction snapshot for this new run.
pub fn render_with_worker(r: &Record, instructions: Option<&str>) -> String {
    format!(
        "{}{MARKER}\n\n{NOTES}",
        header_with_section(r, &worker_section(r, instructions))
    )
}

/// `TASK.md` rewritten from `existing`: a new header, and the agent's part
/// kept from the marker line on. Without a marker, nothing of the old text
/// is dropped: unless it is exactly what ADR 0015 wrote, it follows under
/// "Earlier notes".
pub fn merge(r: &Record, existing: Option<&str>) -> String {
    let worker = existing
        .and_then(|old| retained_worker(r, old))
        .map(str::to_owned)
        .unwrap_or_else(|| worker_section(r, None));
    merge_header(r, existing, &header_with_section(r, &worker))
}

/// Replaces the worker snapshot only at a validated start/restart boundary.
pub fn merge_with_worker(r: &Record, existing: Option<&str>, instructions: Option<&str>) -> String {
    merge_header(
        r,
        existing,
        &header_with_section(r, &worker_section(r, instructions)),
    )
}

fn merge_header(r: &Record, existing: Option<&str>, header: &str) -> String {
    let fresh = || format!("{header}{MARKER}\n\n{NOTES}");
    let Some(old) = existing.filter(|t| !t.trim().is_empty()) else {
        return fresh();
    };
    if let Some(at) = marker_at(r, old) {
        return format!("{header}{}", &old[at..]);
    }
    if old == context(r) {
        return fresh();
    }
    let lead = if old.ends_with('\n') { "" } else { "\n" };
    format!("{}\n{EARLIER}{old}{lead}", fresh())
}

/// The byte offset of the marker line.
fn managed_offset(r: &Record, text: &str) -> usize {
    let context = context(r);
    if text.starts_with(&context) {
        context.len()
    } else {
        0
    }
}

fn first_notes(text: &str, offset: usize) -> Option<usize> {
    text.match_indices(MARKER_START)
        .map(|(at, _)| at)
        .find(|at| *at >= offset && (*at == 0 || text.as_bytes()[*at - 1] == b'\n'))
}

fn marker_at(r: &Record, text: &str) -> Option<usize> {
    let offset = managed_offset(r, text);
    let worker = worker_frame(r, text);
    let candidate = text[offset..]
        .find("\n<!-- muxa:worker ")
        .map(|at| at + offset + 1);
    if candidate.is_some_and(|at| at < first_notes(text, offset).unwrap_or(text.len()))
        && worker.is_none()
    {
        // A damaged managed section is preserved as earlier notes, never discarded.
        return None;
    }
    text.match_indices(MARKER_START)
        .map(|(at, _)| at)
        .find(|at| {
            *at >= offset
                && (*at == 0 || text.as_bytes()[*at - 1] == b'\n')
                && !worker.is_some_and(|(start, end, _)| *at >= start && *at < end)
        })
}

/// Writes a new `<dir>/.muxa/TASK.md` for a new Task. In a repo, `.muxa/` is
/// excluded first, so it never shows as a change. Blocking.
pub fn create(dir: &Path, record: &Record, in_repo: bool) -> Result<PathBuf, String> {
    if in_repo {
        exclude(dir)?;
    }
    write_file(dir, FILE, &render(record))
}

pub fn create_with_worker(
    dir: &Path,
    record: &Record,
    in_repo: bool,
    instructions: Option<&str>,
) -> Result<PathBuf, String> {
    if in_repo {
        exclude(dir)?;
    }
    write_file(dir, FILE, &render_with_worker(record, instructions))
}

/// Copies `<from>/.muxa/TASK.md` into the worktree `to`, with `.muxa/`
/// excluded first (ADR 0034 §3). The restart then rewrites its header.
/// Blocking.
pub fn copy_into(from: &Path, to: &Path) -> Result<PathBuf, String> {
    exclude(to)?;
    let source = from.join(DIR).join(FILE);
    let text = std::fs::read_to_string(&source)
        .map_err(|e| format!("cannot read {}: {e}", source.display()))?;
    write_file(to, FILE, &text)
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
    #[test]
    fn worker_snapshots_survive_loop_refresh_and_notes_are_byte_exact() {
        let mut r = record();
        r.extra
            .insert("worker".into(), serde_json::json!("Reviewer"));
        r.goal =
            "Keep these as data:\n<!-- muxa:worker invalid -->\n<!-- muxa:notes goal -->\n".into();
        let instructions = "日本語 instructions\n<!-- muxa:worker-end -->\n<!-- muxa:notes fixture -->\n## How to use this file\n";
        let text = render_with_worker(&r, Some(instructions));
        let at = marker_at(&r, &text).unwrap();
        let notes = "<!-- muxa:notes - owned by agent -->\n\n私の notes\n";
        let text = format!("{}{notes}", &text[..at]);
        let spec = LoopSpec {
            check: "make check".into(),
            max_attempts: None,
            timeout: None,
        };
        r.set_loop(Some(&Loop::start(spec, None, false, 1)))
            .unwrap();
        let refreshed = merge(&r, Some(&text));
        assert!(refreshed.contains(instructions));
        assert!(refreshed.contains("## Check"));
        assert!(refreshed.ends_with(notes));
        let edited = merge_with_worker(&r, Some(&refreshed), Some("new instructions"));
        assert!(edited.contains("new instructions"));
        assert!(!edited.contains(instructions));
        assert!(edited.ends_with(notes));
        r.extra.remove("worker");
        let cleared = merge_with_worker(&r, Some(&edited), None);
        assert!(!cleared.contains("## Worker"));
        assert!(!cleared.contains("new instructions"));
        assert!(cleared.ends_with(notes));
    }

    #[test]
    fn another_workers_snapshot_is_never_reused() {
        let mut r = record();
        r.extra.insert("worker".into(), serde_json::json!("First"));
        let old = render_with_worker(&r, Some("old private instructions"));
        r.extra.insert("worker".into(), serde_json::json!("Second"));
        let new = merge(&r, Some(&old));
        assert!(!new.contains("old private instructions"));
        assert!(new.contains("## Worker\n\nSecond"));
    }
    #[test]
    fn a_damaged_worker_frame_preserves_all_old_content() {
        let mut r = record();
        r.extra
            .insert("worker".into(), serde_json::json!("Reviewer"));
        let old = render_with_worker(
            &r,
            Some("private instructions\n<!-- muxa:notes embedded -->\n"),
        );
        let old = old.replacen("<!-- muxa:worker ", "<!-- muxa:worker invalid", 1);
        let refreshed = merge(&r, Some(&old));
        assert!(refreshed.ends_with(&old));
    }
    #[test]
    fn a_reading_task_names_its_repo_and_rules_then_its_folder_after_branch() {
        let mut r = record();
        r.repo = Some("/w/app".into());
        r.extra = crate::records::reading::new_fields();
        let text = render(&r);
        assert!(text.contains("- Repo: /w/app\n"));
        assert!(text.contains("Read /w/app/AGENTS.md or /w/app/CLAUDE.md first."));
        assert!(text.contains("`git -C /w/app ...`"));
        assert!(text.contains("ask the user for a branch"));
        assert!(text.find("## Reading the repo").unwrap() < text.find(MARKER).unwrap());
        let notes = format!("{MARKER}\n\nmine\n");
        let old = format!("{}{notes}", header(&r));
        r.dir = Some("/w/tasks/t-1".into());
        r.set_folder(std::path::Path::new("/w/tasks/t-1"));
        r.set_read(false);
        r.branch = Some("fix-login".into());
        r.dir = Some("/w/.worktrees/app-fix-login".into());
        let branched = merge(&r, Some(&old));
        assert!(!branched.contains("## Reading the repo"));
        assert!(branched.contains("this Task worked in /w/tasks/t-1"));
        assert!(branched.contains("- Branch: fix-login\n"));
        assert!(branched.ends_with(&notes), "the agent's notes stay");
    }

    #[test]
    fn worker_looking_text_in_agent_notes_is_untouched() {
        let r = record();
        let notes = format!("{MARKER}\n\n<!-- muxa:worker invalid -->\n## Worker\nnotes\n");
        let old = format!("{}{notes}", header(&r));
        assert_eq!(merge(&r, Some(&old)), old);
    }
}
