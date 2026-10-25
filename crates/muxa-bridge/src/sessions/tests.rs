use std::path::PathBuf;
use std::process::Command;

use muxa_tmux::{PaneId, SessionId};

use super::*;
use crate::agents::{AgentAdapter, ChatSource, Error};

/// Sees its agent in the panes it is given, and nowhere else.
struct Stub(Vec<&'static str>);

impl AgentAdapter for Stub {
    fn id(&self) -> &'static str {
        "stub"
    }

    fn open(&self, _: &PaneInfo, _: &Processes) -> Result<Option<Box<dyn ChatSource>>, Error> {
        Ok(None)
    }

    fn claims(&self, pane: &PaneInfo, _: &Processes) -> bool {
        self.0.contains(&pane.pane.as_str())
    }
}

fn pane(n: u32, window: u32, role: Role, path: &str) -> Pane {
    Pane {
        id: PaneId::new(n),
        window: WindowId::new(window),
        index: n,
        role,
        cols: 80,
        rows: 24,
        command: "zsh".into(),
        path: path.into(),
        dead: false,
        pid: 0,
        shell_foreground: true,
        agent: None,
    }
}

fn session(panes: Vec<Pane>) -> Session {
    Session {
        id: SessionId::new(1),
        name: "s".into(),
        repo: None,
        repo_dir: None,
        branch: None,
        worktree: None,
        task: None,
        panes,
    }
}

fn roles(s: &Session) -> Vec<Role> {
    s.panes.iter().map(|p| p.role).collect()
}

#[test]
fn a_claimed_pane_is_the_agent_and_its_neighbours_are_shells() {
    let agents = Registry::new(vec![Box::new(Stub(vec!["%2"]))]);
    let mut s = session(vec![
        pane(1, 1, Role::Unknown, ""),
        pane(2, 1, Role::Unknown, ""),
        pane(3, 1, Role::Hidden, ""),
        // Another window without a claimed pane stays as it was.
        pane(4, 2, Role::Unknown, ""),
    ]);
    claim_agents(&mut s, &agents, &Processes::default());
    assert_eq!(
        roles(&s),
        [Role::Shell, Role::Agent, Role::Hidden, Role::Unknown]
    );
}

#[test]
fn tags_win_over_adapters() {
    let agents = Registry::new(vec![Box::new(Stub(vec!["%2"]))]);
    let mut s = session(vec![
        pane(1, 1, Role::Agent, ""),
        pane(2, 1, Role::Unknown, ""),
    ]);
    claim_agents(&mut s, &agents, &Processes::default());
    assert_eq!(roles(&s), [Role::Agent, Role::Unknown]);
}

#[test]
fn an_untagged_session_gets_its_repo_also_from_a_worktree() {
    let root = std::env::temp_dir().join(format!("muxa-sessions-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let app = root.join("app");
    std::fs::create_dir_all(&app).unwrap();
    let app = app.canonicalize().unwrap();
    let git = |dir: &PathBuf, args: &[&str]| {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&app, &["init", "-q", "-b", "main"]);
    git(
        &app,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "i",
        ],
    );
    let wt = app.parent().unwrap().join("app-x");
    git(
        &app,
        &["worktree", "add", "-q", "-b", "x", wt.to_str().unwrap()],
    );

    let mut tagged = session(vec![pane(1, 1, Role::Agent, wt.to_str().unwrap())]);
    tagged.repo = Some("tagged".into());
    tagged.repo_dir = Some("/tagged".into());
    let mut sessions = vec![
        session(vec![
            pane(2, 2, Role::Hidden, "/"),
            pane(3, 2, Role::Agent, wt.to_str().unwrap()),
        ]),
        tagged,
        session(vec![pane(4, 3, Role::Agent, root.to_str().unwrap())]),
    ];
    fill_repos(&mut sessions);
    assert_eq!(sessions[0].repo.as_deref(), Some("app"));
    assert_eq!(sessions[0].repo_dir.as_deref(), app.to_str());
    assert_eq!(sessions[1].repo.as_deref(), Some("tagged"));
    assert_eq!(sessions[1].repo_dir.as_deref(), Some("/tagged"));
    assert_eq!(sessions[2].repo, None, "not in a repo");
    let _ = std::fs::remove_dir_all(&root);
}
