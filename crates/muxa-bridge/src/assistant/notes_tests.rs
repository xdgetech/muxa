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
    std::fs::create_dir_all(root.join(".muxa/assistant")).unwrap();
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
    std::fs::create_dir_all(d.0.join(".muxa/assistant")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, d.0.join(NOTES)).unwrap();
    let e = read(&d.0).unwrap_err();
    assert!(matches!(e, NotesError::NotRegular { .. }), "{e}");

    let other = temp();
    std::fs::create_dir_all(other.0.join("real")).unwrap();
    std::fs::write(other.0.join("real/NOTES.md"), SENTINEL).unwrap();
    std::fs::create_dir_all(other.0.join(".muxa")).unwrap();
    std::os::unix::fs::symlink(other.0.join("real"), other.0.join(".muxa/assistant")).unwrap();
    let e = read(&other.0).unwrap_err();
    assert!(matches!(e, NotesError::NotRegular { .. }), "{e}");
    let e = apply(&other.0, "new", &base_of(b"")).unwrap_err();
    assert!(matches!(e, NotesError::NotRegular { .. }), "{e}");
    assert_eq!(
        std::fs::read_to_string(other.0.join("real/NOTES.md")).unwrap(),
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
    let leftovers: Vec<_> = std::fs::read_dir(d.0.join(".muxa/assistant"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers, ["NOTES.md"]);
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
