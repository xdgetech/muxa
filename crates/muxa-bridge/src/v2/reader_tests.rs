use std::path::PathBuf;
use std::process::Command;

use muxa_protocol::ChangeStatus;

use super::*;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let path =
            std::env::temp_dir().join(format!("muxa-v2-reader-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Dir(path.canonicalize().unwrap())
    }

    fn write(&self, rel: &str, text: &[u8]) {
        let file = self.0.join(rel);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    fn git(&self, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn open(kind: ReaderKind, path: &str) -> ReaderOpen {
    ReaderOpen {
        kind,
        path: path.into(),
        html: false,
        scope: DiffScope::Uncommitted,
    }
}

fn paths(list: &ReaderList) -> Vec<&str> {
    list.entries.iter().map(|e| e.path.as_str()).collect()
}

#[tokio::test]
async fn a_task_folder_serves_its_documents_and_images_only() {
    let d = Dir::new("folder");
    d.write("notes/plan.md", b"# Plan\n");
    d.write("report.html", b"<title>Report</title>");
    d.write("shot.png", b"\x89PNG");
    d.write(".env", b"SECRET=1");
    d.write(".muxa/TASK.md", b"# Task\n");
    let docs = list(&d.0, ReaderKind::Docs, false).await.unwrap();
    assert_eq!(paths(&docs), ["notes/plan.md", ".muxa/TASK.md"]);
    let html = list(&d.0, ReaderKind::Docs, true).await.unwrap();
    let report = html
        .entries
        .iter()
        .find(|e| e.path == "report.html")
        .unwrap();
    assert_eq!(report.title.as_deref(), Some("Report"));
    let read = read(&d.0, &open(ReaderKind::Docs, "notes/plan.md"))
        .await
        .unwrap();
    assert_eq!(read.text.as_deref(), Some("# Plan\n"));
    // Only listed files: HTML needs `html`, and a dot file never is.
    for path in ["report.html", ".env", "../x.md"] {
        assert!(matches!(
            super::read(&d.0, &open(ReaderKind::Docs, path)).await,
            Err(TaskError::Invalid(_))
        ));
    }
    let images = list(&d.0, ReaderKind::Images, false).await.unwrap();
    assert_eq!(paths(&images), ["shot.png"]);
    let image = super::read(&d.0, &open(ReaderKind::Images, "shot.png"))
        .await
        .unwrap();
    assert_eq!(image.data.as_deref(), Some("iVBORw=="));
    assert!(matches!(
        list(&d.0, ReaderKind::Changes, false).await,
        Err(TaskError::Invalid(_))
    ));
}

#[tokio::test]
async fn a_worktree_serves_its_uncommitted_diffs() {
    let d = Dir::new("repo");
    d.git(&["init", "-q", "-b", "main"]);
    d.write("a.txt", b"one\n");
    d.write("README.md", b"# Repo\n");
    d.git(&["add", "."]);
    d.git(&["commit", "-q", "-m", "first"]);
    d.write("a.txt", b"two\n");
    let changes = list(&d.0, ReaderKind::Changes, false).await.unwrap();
    assert_eq!(paths(&changes), ["a.txt"]);
    assert_eq!(changes.entries[0].status, Some(ChangeStatus::Modified));
    let diff = super::read(&d.0, &open(ReaderKind::Changes, "a.txt"))
        .await
        .unwrap();
    let text = diff.text.unwrap();
    assert!(text.contains("-one\n+two"), "{text}");
    assert!(matches!(
        super::read(&d.0, &open(ReaderKind::Changes, "README.md")).await,
        Err(TaskError::Invalid(_))
    ));
    let docs = list(&d.0, ReaderKind::Docs, false).await.unwrap();
    assert_eq!(paths(&docs), ["README.md"]);
}

/// The folder's zip, as the host serves it (ADR 0101).
async fn zip_folder(dir: &std::path::Path, folder: &str) -> Result<Vec<u8>, TaskError> {
    crate::v2::reader_folder::zip(dir, folder).await
}

fn unzip(bytes: Vec<u8>) -> Vec<(String, Vec<u8>)> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out: Vec<(String, Vec<u8>)> = (0..zip.len())
        .map(|i| {
            let mut f = zip.by_index(i).unwrap();
            let mut data = Vec::new();
            f.read_to_end(&mut data).unwrap();
            (f.name().to_owned(), data)
        })
        .collect();
    out.sort();
    out
}

#[tokio::test]
async fn a_folder_zips_only_its_listed_documents_and_images() {
    let d = Dir::new("zip");
    d.git(&["init", "-q", "-b", "main"]);
    d.write(".gitignore", b"output/\n");
    let run = "output/qa/T-1/run1";
    d.write(&format!("{run}/report.md"), b"# Report\n");
    d.write(&format!("{run}/page.html"), b"<title>Page</title>");
    d.write(&format!("{run}/screens/a.png"), b"\x89PNG-a");
    // Never listed, so never zipped (ADR 0099).
    d.write(&format!("{run}/secret-notes.md"), b"key");
    d.write(&format!("{run}/.env"), b"SECRET=1");
    d.write(&format!("{run}/data.json"), b"{}");
    d.write(&format!("{run}/node_modules/x.md"), b"dep");
    d.write("output/qa/T-1/run10/other.md", b"# Other\n");
    let zipped = zip_folder(&d.0, &format!("{run}/")).await.unwrap();
    assert_eq!(
        unzip(zipped),
        [
            ("run1/page.html".to_owned(), b"<title>Page</title>".to_vec()),
            ("run1/report.md".to_owned(), b"# Report\n".to_vec()),
            ("run1/screens/a.png".to_owned(), b"\x89PNG-a".to_vec()),
        ]
    );
    let all = unzip(zip_folder(&d.0, "").await.unwrap());
    assert_eq!(all.len(), 4);
    assert!(
        all.iter()
            .all(|(name, _)| name.starts_with("muxa-v2-reader-"))
    );
}

#[tokio::test]
async fn a_folder_names_only_listed_paths() {
    let d = Dir::new("zip-names");
    d.write("output/qa/report.md", b"# Report\n");
    for folder in [
        "output/q",
        "../output",
        "/output",
        "output/qa/report.md",
        "nope",
    ] {
        assert!(
            matches!(zip_folder(&d.0, folder).await, Err(TaskError::Invalid(_))),
            "{folder}"
        );
    }
}

#[tokio::test]
async fn a_folder_over_the_limit_is_refused_whole() {
    let d = Dir::new("zip-large");
    let half = vec![0u8; 6 << 20];
    d.write("shots/a.png", &half);
    d.write("shots/b.png", &half);
    let Err(TaskError::Invalid(message)) = zip_folder(&d.0, "shots").await else {
        panic!("a 12 MiB folder was zipped");
    };
    assert_eq!(
        message,
        "shots has 12 MB of files. Save Folder sends at most 11 MB. Save a folder inside it."
    );
}
