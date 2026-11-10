use std::path::PathBuf;

use super::*;

const SENTINEL: &str = "notes-mark-41c9";

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp() -> Dir {
    let dir = std::env::temp_dir().join(format!(
        "muxa-notes-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    Dir(dir.canonicalize().unwrap())
}

fn live(root: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(root.join(".muxa")).unwrap();
    std::fs::write(root.join(NOTES), bytes).unwrap();
}

#[test]
fn a_missing_file_is_empty_notes_with_the_base_of_no_bytes() {
    let d = temp();
    let snap = read(&d.0).unwrap();
    assert_eq!(snap.text, "");
    assert_eq!(
        snap.base,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    live(&d.0, b"# Notes\n");
    let snap = read(&d.0).unwrap();
    assert_eq!(snap.text, "# Notes\n");
    assert_eq!(snap.base, base_of(b"# Notes\n"));
}

#[test]
fn an_invalid_file_is_refused_with_its_path_and_size_and_no_text() {
    let d = temp();
    let cases: [(&[u8], &str); 3] = [
        (&[b'a'; MAX_NOTES + 1], "16385 bytes"),
        (b"notes-mark-41c9\0", "NUL"),
        (b"notes-mark-41c9\xff", "UTF-8"),
    ];
    for (bytes, needle) in cases {
        live(&d.0, bytes);
        let e = read(&d.0).unwrap_err().to_string();
        assert!(e.contains(needle), "{e}");
        assert!(e.contains(&d.0.join(NOTES).display().to_string()), "{e}");
        assert!(!e.contains(SENTINEL), "{e}");
    }
    live(&d.0, &[b'a'; MAX_NOTES]);
    assert_eq!(read(&d.0).unwrap().text.len(), MAX_NOTES);
}

#[test]
fn a_symlink_is_refused_for_the_file_and_for_its_folders() {
    let d = temp();
    let elsewhere = d.0.join("elsewhere.md");
    std::fs::write(&elsewhere, SENTINEL).unwrap();
    std::fs::create_dir_all(d.0.join(".muxa")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, d.0.join(NOTES)).unwrap();
    let e = read(&d.0).unwrap_err();
    assert!(matches!(e, NotesError::NotRegular { .. }), "{e}");

    let other = temp();
    std::fs::create_dir_all(other.0.join("real")).unwrap();
    std::fs::write(other.0.join("real/WORKSPACE.md"), SENTINEL).unwrap();
    std::os::unix::fs::symlink(other.0.join("real"), other.0.join(".muxa")).unwrap();
    let e = read(&other.0).unwrap_err();
    assert!(matches!(e, NotesError::NotRegular { .. }), "{e}");
    let e = apply(&other.0, "new", &base_of(b"")).unwrap_err();
    assert!(matches!(e, NotesError::NotRegular { .. }), "{e}");
    assert_eq!(
        std::fs::read_to_string(other.0.join("real/WORKSPACE.md")).unwrap(),
        SENTINEL
    );
}

#[test]
fn apply_writes_while_the_base_holds_and_creates_the_folder() {
    let d = temp();
    apply(&d.0, "# First\n", &base_of(b"")).unwrap();
    assert_eq!(
        std::fs::read_to_string(d.0.join(NOTES)).unwrap(),
        "# First\n"
    );
    // The same proposal again: the live file is no longer its base.
    let e = apply(&d.0, "# First\n", &base_of(b"")).unwrap_err();
    assert!(matches!(e, NotesError::Changed { .. }), "{e}");
    apply(&d.0, "# Second\n", &base_of(b"# First\n")).unwrap();
    assert_eq!(read(&d.0).unwrap().text, "# Second\n");
    // An edit by the user since the run read the file wins.
    live(&d.0, b"# Edited by hand\n");
    let e = apply(&d.0, "# Third\n", &base_of(b"# Second\n")).unwrap_err();
    assert!(matches!(e, NotesError::Changed { .. }), "{e}");
    assert_eq!(read(&d.0).unwrap().text, "# Edited by hand\n");
    let leftovers: Vec<_> = std::fs::read_dir(d.0.join(".muxa"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers, ["WORKSPACE.md"]);
}

#[test]
fn the_copy_is_the_snapshot_the_base_names() {
    let (root, run) = (temp(), temp());
    live(&root.0, b"# Keep\n");
    let base = read(&root.0).unwrap().base;
    copy_into(&root.0, &run.0, &base).unwrap();
    assert_eq!(read(&run.0).unwrap(), read(&root.0).unwrap());
    live(&root.0, b"# Changed\n");
    let e = copy_into(&root.0, &run.0, &base).unwrap_err();
    assert!(matches!(e, NotesError::Changed { .. }), "{e}");
}

#[test]
fn modified_is_the_files_time_and_never_follows_a_symlink() {
    let d = temp();
    assert_eq!(modified(&d.0).unwrap(), None);
    live(&d.0, b"# Notes\n");
    let at = modified(&d.0).unwrap().unwrap();
    let now = crate::records::now();
    assert!(at <= now && now - at < 60, "{at} {now}");

    let other = temp();
    std::fs::write(other.0.join("elsewhere.md"), SENTINEL).unwrap();
    std::fs::create_dir_all(other.0.join(".muxa")).unwrap();
    std::os::unix::fs::symlink(other.0.join("elsewhere.md"), other.0.join(NOTES)).unwrap();
    let e = modified(&other.0).unwrap_err();
    assert!(matches!(e, NotesError::NotRegular { .. }), "{e}");
}

#[test]
fn the_workspace_file_is_workspace_md_and_the_old_notes_are_only_seen() {
    let d = temp();
    assert_eq!(NOTES, ".muxa/WORKSPACE.md");
    std::fs::create_dir_all(d.0.join(".muxa/assistant")).unwrap();
    std::fs::write(d.0.join(OLD_NOTES), SENTINEL).unwrap();
    assert!(old_notes_present(&d.0));
    assert_eq!(read(&d.0).unwrap().text, "", "the old file is not read");
    assert_eq!(
        Place::Member("w").file(),
        ".muxa/assistant/staff/w/NOTES.md"
    );
    assert!(!old_notes_present(&temp().0));
}

#[test]
fn give_copies_the_live_file_and_removes_a_copy_without_one() {
    let (root, run) = (temp(), temp());
    assert_eq!(give(&root.0, &run.0).unwrap(), None);
    assert!(!run.0.join(NOTES).exists(), "no live file leaves no copy");
    live(&root.0, b"# Learned\n");
    let base = give(&root.0, &run.0).unwrap();
    assert_eq!(base, Some(base_of(b"# Learned\n")));
    assert_eq!(std::fs::read(run.0.join(NOTES)).unwrap(), b"# Learned\n");
    std::fs::remove_file(root.0.join(NOTES)).unwrap();
    assert_eq!(give(&root.0, &run.0).unwrap(), None);
    assert!(!run.0.join(NOTES).exists(), "the stale copy is gone");
    live(&root.0, &[b'a'; MAX_NOTES + 1]);
    let e = give(&root.0, &run.0).unwrap_err().to_string();
    assert!(e.contains("16385 bytes"), "{e}");
    assert!(e.contains(&root.0.join(NOTES).display().to_string()), "{e}");
    assert!(!run.0.join(NOTES).exists());
}

#[test]
fn append_adds_once_after_a_blank_line_and_refuses_past_the_cap() {
    let d = temp();
    let block = "## A (t-1, 2026-11-11)\n\nOne.\n";
    let mark = |t: &str| t.contains("(t-1, ");
    let got = append(&d.0, block, mark).unwrap();
    assert_eq!(
        got,
        Appended::Added {
            size: block.len() as u64
        }
    );
    assert_eq!(read(&d.0).unwrap().text, block);
    assert_eq!(append(&d.0, block, mark).unwrap(), Appended::Present);
    live(&d.0, b"# Mine");
    append(&d.0, "## B (t-2, 2026-11-11)\n\nTwo.\n", |_| false).unwrap();
    assert_eq!(
        read(&d.0).unwrap().text,
        "# Mine\n\n## B (t-2, 2026-11-11)\n\nTwo.\n"
    );
    live(&d.0, &[b'a'; MAX_NOTES - 4]);
    let e = append(&d.0, "12345", |_| false).unwrap_err();
    assert!(
        matches!(e, NotesError::WouldExceed { size: 16_387, .. }),
        "{e}"
    );
    assert!(
        e.to_string()
            .contains("would be 16,387 bytes; the cap is 16 KiB"),
        "{e}"
    );
    assert_eq!(read(&d.0).unwrap().text.len(), MAX_NOTES - 4);
}
