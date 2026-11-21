//! The v2 server on the fake backend at a Unix socket, for client tests in
//! other languages (`make test-apple-v2`). One terminal, `demo`, shows
//! `hello ` and echoes input as `echo:<bytes>`. Its folder's Reader has one
//! document, one changed file and one image, all fixed.
//!
//! Usage: `fake_server SOCKET`. It runs until killed.

use std::path::Path;
use std::sync::Arc;

use muxa_backend::fake::FakeBackend;
use muxa_backend::{Launch, SessionBackend};
use muxa_protocol::ChangeStatus;
use muxa_protocol::v2::{ReaderContent, ReaderEntry, ReaderKind, ReaderList};
use muxa_server::{
    Config, Host, Inbox, PARK, Prepared, ReaderOpen, RoutineList, Services, Sessions, TaskError,
    TaskSpec, serve,
};
use tokio::net::UnixListener;

struct FakeHost;

impl Host for FakeHost {
    async fn prepare(&self, spec: &TaskSpec) -> Result<Prepared, TaskError> {
        let (repo, branch) = spec.branch().unwrap_or(("", &spec.title));
        Ok(Prepared {
            dir: format!("/work/{branch}").into(),
            workspace: spec.workspace.clone(),
            title: spec.title.clone(),
            repo: Some(repo.into()),
            branch: Some(branch.to_owned()),
            argv: vec!["agent".into()],
            env: Default::default(),
            places: Vec::new(),
        })
    }

    async fn workspaces(&self) -> Vec<String> {
        vec!["Personal".into()]
    }

    async fn routines(&self) -> Result<RoutineList, TaskError> {
        let routine = |name: &str, at: Option<&str>| muxa_protocol::v2::Routine {
            workspace: "Personal".into(),
            name: name.into(),
            enabled: true,
            at: at.map(str::to_owned),
            timezone: at.map(|_| "Europe/Berlin".to_owned()),
            schedule: None,
            next_due: None,
            running_since: None,
            last: None,
        };
        Ok(RoutineList {
            routines: vec![routine("digest", Some("09:00")), routine("audit", None)],
            problems: vec![],
        })
    }

    async fn run_routine<B: muxa_backend::SessionBackend>(
        &self,
        _backend: &B,
        _workspace: &str,
        name: &str,
    ) -> Result<(), TaskError> {
        match name {
            "digest" => Err(TaskError::Busy("digest already runs".into())),
            _ => Ok(()),
        }
    }

    async fn reader_list(
        &self,
        _dir: &Path,
        kind: ReaderKind,
        _html: bool,
    ) -> Result<ReaderList, TaskError> {
        let entry = match kind {
            ReaderKind::Docs => ReaderEntry {
                path: "PLAN.md".into(),
                ..ReaderEntry::default()
            },
            ReaderKind::Changes => ReaderEntry {
                path: "a.txt".into(),
                status: Some(ChangeStatus::Modified),
                added: Some(1),
                removed: Some(1),
                ..ReaderEntry::default()
            },
            ReaderKind::Images => ReaderEntry {
                path: "shot.png".into(),
                size: Some(4),
                ..ReaderEntry::default()
            },
        };
        Ok(ReaderList {
            root: Some("demo".into()),
            entries: vec![entry],
            ..ReaderList::default()
        })
    }

    async fn reader_read(
        &self,
        _dir: &Path,
        open: &ReaderOpen,
    ) -> Result<ReaderContent, TaskError> {
        let (text, data) = match (open.kind, open.path.as_str()) {
            (ReaderKind::Docs, "PLAN.md") => (Some("# Plan\n"), None),
            (ReaderKind::Changes, "a.txt") => (Some("@@ -1 +1 @@\n-one\n+two\n"), None),
            (ReaderKind::Images, "shot.png") => (None, Some("iVBORw==")),
            _ => {
                return Err(TaskError::Invalid(format!(
                    "{} is not a listed file",
                    open.path
                )));
            }
        };
        Ok(ReaderContent {
            path: open.path.clone(),
            text: text.map(str::to_owned),
            data: data.map(str::to_owned),
            ..ReaderContent::default()
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: fake_server SOCKET")?;
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    let backend = FakeBackend::new();
    let demo = Launch {
        name: Some("demo".into()),
        labels: [("muxa.dir".to_owned(), "/work/demo".to_owned())].into(),
        ..Launch::default()
    };
    let id = backend.run(&demo).await?.id;
    backend.emit(&id, b"hello ");
    let sessions = Arc::new(Sessions::new());
    let inbox = Arc::new(Inbox::memory());
    println!("ready");
    loop {
        let (stream, _) = listener.accept().await?;
        let (backend, sessions, inbox) =
            (backend.clone(), Arc::clone(&sessions), Arc::clone(&inbox));
        tokio::spawn(async move {
            let (r, w) = stream.into_split();
            let config = Config {
                server: "muxa fake".into(),
                backend: "fake".into(),
                park: PARK,
            };
            let services = Services {
                backend: &backend,
                host: &FakeHost,
                sessions: &sessions,
                inbox: &inbox,
            };
            let _ = serve(&services, r, w, &config).await;
        });
    }
}
