//! The v2 server on the fake backend at a Unix socket, for client tests in
//! other languages (`make test-apple-v2`). One terminal, `demo`, shows
//! `hello ` and echoes input as `echo:<bytes>`.
//!
//! Usage: `fake_server SOCKET`. It runs until killed.

use std::sync::Arc;

use muxa_backend::fake::FakeBackend;
use muxa_backend::{Launch, SessionBackend};
use muxa_server::{
    Config, Host, Inbox, PARK, Prepared, RoutineList, Services, Sessions, TaskError, TaskSpec,
    serve,
};
use tokio::net::UnixListener;

struct FakeHost;

impl Host for FakeHost {
    async fn prepare(&self, spec: &TaskSpec) -> Result<Prepared, TaskError> {
        Ok(Prepared {
            dir: format!("/work/{}", spec.branch).into(),
            repo: spec.repo.clone().into(),
            branch: spec.branch.clone(),
            argv: vec!["agent".into()],
            env: Default::default(),
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

    async fn run_routine(&self, _workspace: &str, name: &str) -> Result<(), TaskError> {
        match name {
            "digest" => Err(TaskError::Busy("digest already runs".into())),
            _ => Ok(()),
        }
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
