//! Routines (ADR 0026) over protocol v1: a bridge lists them, and refuses to
//! run one. Runs are the daemon's, on herdr-core (`src/v2/runner.rs`).
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{Bridge, Server};
use muxa_protocol::{ErrorCode, Message, work_level};
use serde_json::json;

struct Host {
    base: PathBuf,
}

impl Host {
    fn new(name: &str) -> Host {
        let base =
            std::env::temp_dir().join(format!("muxa-routines-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for d in ["root/.muxa", "state"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let base = base.canonicalize().unwrap();
        let muxa = base.join("root/.muxa");
        std::fs::write(muxa.join("brief.md"), "Read the sources.\n").unwrap();
        std::fs::write(
            base.join("workspaces.toml"),
            format!(
                "[[workspace]]\nname = \"w\"\nroot = \"{root}\"\n\
                 [workspace.routines]\nenabled = true\n\
                 [[worker]]\nname = \"Cx\"\nagent = \"codex\"\n",
                root = base.join("root").display(),
            ),
        )
        .unwrap();
        std::fs::write(
            muxa.join("routines.toml"),
            "[[routine]]\nname = \"Brief\"\ninstructions = \"brief.md\"\n\
             project = \"Ops\"\nenabled = true\n[routine.output]\nitems = 5\n\
             [[routine]]\nname = \"Off\"\ninstructions = \"brief.md\"\n\
             at = \"09:00\"\ntimezone = \"UTC\"\nenabled = false\n\
             [[routine]]\nname = \"Codex\"\ninstructions = \"brief.md\"\n\
             worker = \"Cx\"\nenabled = true\n",
        )
        .unwrap();
        Host { base }
    }

    fn flags(&self) -> Vec<String> {
        vec![
            "--state-dir".into(),
            self.base.join("state").display().to_string(),
            "--workspaces".into(),
            self.base.join("workspaces.toml").display().to_string(),
            "--task-launcher".into(),
            "builtin".into(),
            "--task-command".into(),
            "true".into(),
        ]
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_daemon_a_bridge_lists_routines_but_runs_none() {
    let server = Server::start();
    let h = Host::new("bridge");
    // A renamed repo still listed: the rest keeps working, and the
    // problems name it.
    let gone = h.base.join("renamed-repo");
    let file = h.base.join("workspaces.toml");
    let text = std::fs::read_to_string(&file).unwrap().replacen(
        "[workspace.routines]",
        &format!("repos = [\"{}\"]\n[workspace.routines]", gone.display()),
        1,
    );
    std::fs::write(&file, text).unwrap();
    let mut flags = h.flags();
    flags.extend([
        "--daemon-dir".into(),
        h.base.join("no-daemon").display().to_string(),
    ]);
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let (mut bridge, hello) = Bridge::start(&server, &flags).await;
    let Message::Hello { capabilities, .. } = &hello else {
        panic!("{hello:?}")
    };
    // The bridge serves ask sessions (ADR 0031) too, so it advertises the
    // highest level, 19 (ADR 0066). Its daemon answers the item requests a bridge forwards.
    assert_eq!(work_level(capabilities), muxa_protocol::WORK_NO_ASSISTANT);
    assert!(matches!(
        bridge.request(json!({"type": "list-routines"})).await,
        Message::Result { error: None, .. }
    ));
    let mut listed = Vec::new();
    let mut problems = Vec::new();
    while let Some(msg) = bridge.event(Duration::from_millis(300)).await {
        if let Message::RoutineList {
            entries,
            problems: p,
            ..
        } = msg
        {
            listed = entries;
            problems = p;
        }
    }
    let names: Vec<_> = listed.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Brief", "Off", "Codex"]);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].workspace, "w");
    assert!(
        problems[0].reason.contains(&gone.display().to_string()),
        "{problems:?}"
    );
    let off = listed.iter().find(|r| r.name == "Off").unwrap();
    assert_eq!((off.enabled, off.next_due), (false, None));
    let refused = bridge
        .request_within(
            json!({"type": "run-routine", "workspace": "w", "routine": "Brief"}),
            Duration::from_secs(10),
        )
        .await;
    match refused {
        Message::Result { error: Some(e), .. } => {
            assert_eq!(e.code, ErrorCode::Unsupported);
            assert!(e.message.contains("muxa daemon"), "{}", e.message);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        bridge.request(json!({"type": "list-inbox"})).await,
        Message::Result { error: None, .. }
    ));
    let unknown = bridge
        .request(json!({"type": "update-inbox", "entry": "e-00000000", "action": "dismiss"}))
        .await;
    assert!(
        matches!(unknown, Message::Result { error: Some(ref e), .. }
        if e.code == ErrorCode::BadRequest),
        "{unknown:?}"
    );
    assert!(!Path::new(&h.base.join("root/routines")).exists());
}
