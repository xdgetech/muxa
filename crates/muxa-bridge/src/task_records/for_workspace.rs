//! The agent's "For the workspace" section of `TASK.md` (ADR 0053 §3): read
//! at a Task's end, before its session closes and its worktree goes. muxa
//! never writes it. The text is never logged.
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use super::brief;
use crate::assistant::notes::MAX_NOTES;
use crate::records::Record;

/// Bytes of `TASK.md` read at most; a section past them is cut.
const MAX_READ: u64 = 1 << 20;

/// The section's text, as the item shows it, and its size as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// Trimmed. Control characters other than line breaks and tabs, and
    /// bytes that are not UTF-8, are U+FFFD. At most 16 KiB.
    pub text: String,
    /// Bytes of the trimmed section in the file.
    pub size: usize,
}

/// Blocking. The section of `<dir>/.muxa/TASK.md`; None without a file, a
/// marker or the heading, or when it is empty or "Nothing yet.".
pub fn read(dir: &Path, r: &Record) -> Result<Option<Section>, String> {
    let path = dir.join(brief::BRIEF);
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(&path);
    let file = match file {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    if !file.metadata().is_ok_and(|m| m.is_file()) {
        return Err(format!("{} is not a regular file", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(MAX_READ)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(section(r, &text).map(|raw| Section {
        size: raw.len(),
        text: shown(raw),
    }))
}

/// The trimmed text between the heading below the marker and the next `## `
/// heading.
pub(super) fn section<'a>(r: &Record, text: &'a str) -> Option<&'a str> {
    let below = &text[brief::marker_at(r, text)?..];
    let mut start = None;
    let mut offset = 0;
    for line in below.split_inclusive('\n') {
        let bare = line.trim_end();
        match start {
            Some(s) if bare.starts_with("## ") => return found(&below[s..offset]),
            None if bare == brief::FOR_WORKSPACE => start = Some(offset + line.len()),
            _ => {}
        }
        offset += line.len();
    }
    found(&below[start?..])
}

fn found(raw: &str) -> Option<&str> {
    let raw = raw.trim();
    (!raw.is_empty() && raw != brief::NOTHING_YET).then_some(raw)
}

/// Line breaks and tabs stay; any other control character becomes U+FFFD,
/// so it can never be approved. Cut to 16 KiB at a character boundary.
fn shown(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.replace("\r\n", "\n").chars() {
        if out.len() + c.len_utf8() > MAX_NOTES {
            break;
        }
        out.push(if c.is_control() && c != '\n' && c != '\t' {
            char::REPLACEMENT_CHARACTER
        } else {
            c
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::Status;

    fn record() -> Record {
        Record {
            version: 1,
            id: "t-7f3a2c".into(),
            rev: 1,
            workspace: "work".into(),
            title: "Fix login".into(),
            goal: "Make login work.\n## For the workspace\n\nnot the agent's".into(),
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

    fn with(section: &str) -> String {
        let fresh = brief::render(&record());
        let at = fresh.rfind("## For the workspace").unwrap();
        format!("{}{section}", &fresh[..at])
    }

    #[test]
    fn the_section_below_the_marker_is_read_up_to_the_next_heading() {
        let r = record();
        assert_eq!(section(&r, &brief::render(&r)), None, "Nothing yet.");
        let text = with("## For the workspace\n\n- Staging is eu-west.\n\n## Later\n\nmine\n");
        assert_eq!(section(&r, &text), Some("- Staging is eu-west."));
        let text = with("## For the workspace\n\n  \n");
        assert_eq!(section(&r, &text), None, "empty");
        let text = with("## For the workspace\n\n### Deploys\n\nFriday freeze.\n");
        assert_eq!(section(&r, &text), Some("### Deploys\n\nFriday freeze."));
        // No marker: the goal's look-alike heading is muxa's, not the agent's.
        assert_eq!(section(&r, "## For the workspace\n\nplanted\n"), None);
    }

    #[test]
    fn control_characters_become_replacement_characters_and_the_text_is_cut() {
        assert_eq!(shown("a\tb\r\nc\u{1b}[31m"), "a\tb\nc\u{fffd}[31m");
        let long = "é".repeat(MAX_NOTES);
        let cut = shown(&long);
        assert!(cut.len() <= MAX_NOTES && cut.len() > MAX_NOTES - 2);
    }

    #[test]
    fn a_missing_file_or_a_symlink_is_no_section_or_an_error() {
        let dir = std::env::temp_dir().join(format!(
            "muxa-for-workspace-{}-{:08x}",
            std::process::id(),
            getrandom::u32().unwrap()
        ));
        std::fs::create_dir_all(dir.join(".muxa")).unwrap();
        assert_eq!(read(&dir, &record()), Ok(None));
        let text = with("## For the workspace\n\nDecided: one queue.\n");
        std::fs::write(dir.join("elsewhere.md"), &text).unwrap();
        std::os::unix::fs::symlink(dir.join("elsewhere.md"), dir.join(brief::BRIEF)).unwrap();
        assert!(read(&dir, &record()).is_err());
        std::fs::remove_file(dir.join(brief::BRIEF)).unwrap();
        std::fs::write(dir.join(brief::BRIEF), &text).unwrap();
        let got = read(&dir, &record()).unwrap().unwrap();
        assert_eq!(got.text, "Decided: one queue.");
        assert_eq!(got.size, got.text.len());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
