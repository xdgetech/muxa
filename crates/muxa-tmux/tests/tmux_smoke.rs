//! End-to-end checks against a throwaway tmux server (tmux -L muxa-test-*).
//! Run with `make test-tmux`.
#![cfg(feature = "tmux-integration")]

use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use muxa_tmux::{Client, Error, Options, PaneId, Role, list_sessions, locate, send_text, snapshot};

static NEXT: AtomicU32 = AtomicU32::new(0);
/// Stops a server that kill-server cannot reach, or whose test run was killed.
const REAPER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scripts/lib/tmux-reaper.sh"
);

/// An isolated tmux server, killed on drop.
struct Server {
    socket: String,
    /// The server's pid and socket path, for `REAPER`.
    pid: String,
    path: String,
    watcher: Option<Child>,
}

impl Server {
    fn start() -> Self {
        let socket = format!(
            "muxa-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let mut server = Server {
            socket,
            pid: String::new(),
            path: String::new(),
            watcher: None,
        };
        server.tmux(&[
            "-f",
            "/dev/null",
            "new-session",
            "-d",
            "-s",
            "work",
            "-x",
            "80",
            "-y",
            "24",
            "sh",
        ]);
        server.tmux(&["set-option", "-g", "default-shell", "/bin/sh"]);
        let found = server.tmux(&["display-message", "-p", "#{pid} #{socket_path}"]);
        let (pid, path) = found.split_once(' ').expect("pid and socket path");
        (server.pid, server.path) = (pid.to_owned(), path.to_owned());
        // In its own process group, so it outlives a test run that is killed
        // with its group (Ctrl-C, a timeout) before Drop runs.
        let watcher = Command::new(REAPER)
            .args([&server.pid, &server.path, &std::process::id().to_string()])
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the reaper");
        server.watcher = Some(watcher);
        server
    }

    fn tmux(&self, args: &[&str]) -> String {
        let out = Command::new("tmux")
            .arg("-L")
            .arg(&self.socket)
            .args(args)
            .env_remove("TMUX")
            .output()
            .expect("run tmux");
        assert!(
            out.status.success(),
            "tmux {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim_end().to_owned()
    }

    fn options(&self) -> Options {
        Options {
            socket: Some(self.socket.clone()),
            ..Options::default()
        }
    }

    fn first_pane(&self) -> PaneId {
        self.tmux(&["display-message", "-p", "-t", "work", "#{pane_id}"])
            .parse()
            .expect("pane id")
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["-L", &self.socket, "kill-server"])
            .output();
        let _ = Command::new(REAPER).args([&self.pid, &self.path]).status();
        if let Some(mut watcher) = self.watcher.take() {
            let _ = watcher.kill();
            let _ = watcher.wait();
        }
    }
}

async fn wait_for(mut cond: impl FnMut() -> bool) {
    for _ in 0..50 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("condition not met in 5 s");
}

#[tokio::test]
async fn lists_sessions_with_roles() {
    let server = Server::start();
    let pane = server.first_pane();
    server.tmux(&[
        "set-option",
        "-p",
        "-t",
        &pane.to_string(),
        "@muxa_role",
        "agent",
    ]);
    let (client, _notes) = Client::attach(&server.options(), None).unwrap();

    let sessions = list_sessions(&client).await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].name, "work");
    assert_eq!(sessions[0].panes[0].id, pane);
    assert_eq!(sessions[0].panes[0].role, Role::Agent);
    assert_eq!(
        (sessions[0].panes[0].cols, sessions[0].panes[0].rows),
        (80, 24)
    );
    assert_eq!(locate(&client, pane).await.unwrap().0, sessions[0].id);
}

/// The host daemon's client (ADR 0019): commands work, pane bytes never
/// come, and the end of its session ends it.
#[tokio::test]
async fn a_no_output_client_gets_no_pane_bytes() {
    let server = Server::start();
    let pane = server.first_pane();
    let flags = muxa_tmux::Flags { no_output: true };
    let (client, mut notes) = Client::attach_with(&server.options(), None, flags).unwrap();
    send_text(&client, pane, "echo no-output-marker\r")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let ok = client.command("display-message -p ok").await.unwrap();
    assert_eq!(ok.lines, vec![b"ok".to_vec()]);
    while let Ok(n) = notes.try_recv() {
        assert_ne!(n.name, "output", "a no-output client got %output");
    }
    assert!(!client.exited());

    server.tmux(&["kill-session", "-t", "work"]);
    for _ in 0..50 {
        if client.exited() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the client did not end with its session");
}

#[tokio::test]
async fn replies_stay_aligned_after_a_failed_command() {
    let server = Server::start();
    let (client, _notes) = Client::attach(&server.options(), None).unwrap();

    let results = client
        .run(
            "display-message -p one ; capture-pane -p -t %999 ; display-message -p three",
            3,
        )
        .await
        .unwrap();
    assert_eq!(results[0].as_ref().unwrap().lines, vec![b"one".to_vec()]);
    assert!(matches!(results[1], Err(Error::Tmux { .. })));
    assert!(matches!(results[2], Err(Error::Skipped { .. })));

    let after = client.command("display-message -p after").await.unwrap();
    assert_eq!(after.lines, vec![b"after".to_vec()]);
}

#[tokio::test]
async fn snapshot_and_input_round_trip() {
    let server = Server::start();
    let pane = server.first_pane();
    let (client, _notes) = Client::attach(&server.options(), None).unwrap();

    send_text(&client, pane, "echo 'a\"b$c é'").await.unwrap();
    muxa_tmux::send_keys(&client, pane, &["Enter".into()])
        .await
        .unwrap();
    wait_for(|| {
        server
            .tmux(&["capture-pane", "-p", "-t", &pane.to_string()])
            .contains("a\"b$c é\n")
    })
    .await;

    let screen = snapshot(&client, pane).await.unwrap();
    assert_eq!((screen.cols, screen.rows), (80, 24));
    assert!(String::from_utf8_lossy(&screen.data).contains("a\"b$c é"));
    assert!(matches!(
        snapshot(&client, PaneId::new(9999)).await,
        Err(Error::NoPane(_))
    ));
}

/// A colored text, and one the terminal wrapped, read as plain text.
#[tokio::test]
async fn screen_shows_finds_colored_and_wrapped_text() {
    let server = Server::start();
    let pane = server.first_pane();
    let (client, _notes) = Client::attach(&server.options(), None).unwrap();
    let long = format!("{}wrapped-end", "x".repeat(75));
    send_text(
        &client,
        pane,
        &format!("clear; printf '\\033[1;32mGreen\\033[0m mark\\n{long}\\n'"),
    )
    .await
    .unwrap();
    muxa_tmux::send_keys(&client, pane, &["Enter".into()])
        .await
        .unwrap();
    wait_for(|| {
        server
            .tmux(&["capture-pane", "-p", "-t", &pane.to_string()])
            .contains("Green mark")
    })
    .await;

    let shows = |text: &'static str| muxa_tmux::screen_shows(&client, pane, text);
    assert!(shows("Green mark").await.unwrap());
    assert!(shows("xxxwrapped-end").await.unwrap(), "-J joins the wrap");
    assert!(!shows("not on the screen").await.unwrap());
    assert!(matches!(
        muxa_tmux::screen_shows(&client, PaneId::new(9999), "x").await,
        Err(Error::NoPane(_))
    ));
}

#[tokio::test]
async fn never_changes_the_window_size() {
    let server = Server::start();
    let size = || {
        server.tmux(&[
            "display-message",
            "-p",
            "-t",
            "work",
            "#{window_width}x#{window_height}",
        ])
    };
    let before = size();
    let (client, _notes) = Client::attach(&server.options(), None).unwrap();
    list_sessions(&client).await.unwrap();
    snapshot(&client, server.first_pane()).await.unwrap();
    assert_eq!(size(), before);
}

/// Open question from docs/findings: hooks fired by our own commands must not
/// produce reply blocks that land in our reply queue.
#[tokio::test]
async fn hook_output_does_not_misalign_replies() {
    let server = Server::start();
    server.tmux(&[
        "new-session",
        "-d",
        "-s",
        "other",
        "-x",
        "80",
        "-y",
        "24",
        "sh",
    ]);
    server.tmux(&[
        "set-hook",
        "-g",
        "client-session-changed[90]",
        "display-message -p hooked",
    ]);
    server.tmux(&[
        "set-hook",
        "-g",
        "client-session-changed[91]",
        "run-shell 'echo hooked-shell'",
    ]);
    server.tmux(&[
        "set-hook",
        "-g",
        "client-attached[90]",
        "display-message -p attached",
    ]);
    let (client, _notes) = Client::attach(&server.options(), None).unwrap();
    for (i, target) in ["other", "work", "other", "work"].iter().enumerate() {
        client
            .command(&format!("switch-client -t ={target}"))
            .await
            .unwrap();
        let reply = client
            .command(&format!("display-message -p check-{i}"))
            .await
            .unwrap();
        assert_eq!(
            reply.lines,
            vec![format!("check-{i}").into_bytes()],
            "after switching to {target}"
        );
    }
}

/// Open question from docs/findings: does a notification ever arrive inside
/// a reply block? A pane prints digits only (never '%') while we snapshot
/// another pane; any reply line starting with '%' would be a notification.
#[tokio::test]
async fn notifications_never_arrive_inside_reply_blocks() {
    let server = Server::start();
    let noisy = server.first_pane();
    let quiet: PaneId = server
        .tmux(&[
            "split-window",
            "-d",
            "-P",
            "-F",
            "#{pane_id}",
            "-t",
            "work",
            "sh",
        ])
        .parse()
        .unwrap();
    let (client, mut notes) = Client::attach(&server.options(), None).unwrap();
    send_text(
        &client,
        noisy,
        "i=0; while :; do i=$((i+1)); echo $i $i $i $i; done",
    )
    .await
    .unwrap();
    muxa_tmux::send_keys(&client, noisy, &["Enter".into()])
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut outputs_between = 0;
    for _ in 0..200 {
        let screen = snapshot(&client, quiet).await.unwrap();
        assert!(
            !screen.data.windows(8).any(|w| w == b"%output "),
            "a notification landed inside a reply"
        );
        while let Ok(n) = notes.try_recv() {
            outputs_between += usize::from(n.output().is_some());
        }
    }
    muxa_tmux::send_keys(&client, noisy, &["C-c".into()])
        .await
        .unwrap();
    assert!(
        outputs_between > 0,
        "the noisy pane produced no output; the test proved nothing"
    );
}

/// ADR 0021: a device session sizes a window that is not the session's
/// current one, is hidden from every list, and ends with its client. Closing
/// the user's session closes its device sessions first.
#[tokio::test]
async fn a_device_session_sizes_its_window_and_stays_hidden() {
    use muxa_tmux::{Device, Grid, kill_session, last_other_session};

    let server = Server::start();
    let first = server.first_pane();
    let window: muxa_tmux::WindowId = server
        .tmux(&[
            "new-window",
            "-d",
            "-P",
            "-F",
            "#{window_id}",
            "-t",
            "work",
            "sh",
        ])
        .parse()
        .expect("window id");
    let (client, _notes) = Client::attach(&server.options(), None).unwrap();
    let work = locate(&client, first).await.unwrap().0;
    let current = server.tmux(&["display-message", "-p", "-t", "work", "#{window_id}"]);

    let device = Device::open(
        &client,
        &server.options(),
        work,
        window,
        std::process::id(),
        1,
    )
    .await
    .unwrap();
    let size = Grid {
        cols: 100,
        rows: 30,
    };
    device.take(&client, size).await.unwrap();
    let target = window.to_string();
    wait_for(|| {
        server.tmux(&[
            "display-message",
            "-p",
            "-t",
            &target,
            "#{window_width}x#{window_height}",
        ]) == "100x30"
    })
    .await;
    assert_eq!(
        server.tmux(&["display-message", "-p", "-t", "work", "#{window_id}"]),
        current,
        "the user's session changed its current window"
    );
    assert_eq!(
        server.tmux(&["list-sessions", "-F", "x"]).lines().count(),
        2
    );

    let sessions = list_sessions(&client).await.unwrap();
    assert_eq!(sessions.len(), 1, "the device session is listed");
    assert_eq!(sessions[0].id, work);
    let pane: PaneId = server
        .tmux(&["display-message", "-p", "-t", &target, "#{pane_id}"])
        .parse()
        .unwrap();
    assert_eq!(locate(&client, pane).await.unwrap().0, work);
    assert_eq!(last_other_session(&client, work).await.unwrap(), None);

    // Closing it ends the session; the window stays in the user's.
    device.close(&client, Some(size)).await.unwrap();
    assert_eq!(
        server.tmux(&["list-sessions", "-F", "x"]).lines().count(),
        1
    );
    assert_eq!(locate(&client, pane).await.unwrap().0, work);

    // Closing the user's session closes its device sessions, so its windows end.
    let device = Device::open(
        &client,
        &server.options(),
        work,
        window,
        std::process::id(),
        2,
    )
    .await
    .unwrap();
    let other = server.tmux(&[
        "new-session",
        "-d",
        "-P",
        "-F",
        "#{session_id}",
        "-s",
        "other",
        "sh",
    ]);
    client
        .command(&format!("switch-client -t {other}"))
        .await
        .unwrap();
    kill_session(&client, work).await.unwrap();
    wait_for(|| device.exited()).await;
    assert!(matches!(locate(&client, pane).await, Err(Error::NoPane(_))));
    assert_eq!(
        server.tmux(&["list-sessions", "-F", "#{session_name}"]),
        "other"
    );
}

/// W0: explicit worker commands replace each other without changing the pane.
/// These are shell fixtures, not native agent sessions or role validation.
#[tokio::test]
async fn worker_commands_respawn_in_place_with_exact_arguments() {
    use muxa_tmux::{new_session, respawn_pane, respawn_pane_command};
    use std::path::PathBuf;

    struct ProbeDir(PathBuf);
    impl Drop for ProbeDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn shell_arg(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
    let server = Server::start();
    let dir = ProbeDir(std::env::temp_dir().join(format!(
        "muxa-worker-probe-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    )));
    std::fs::create_dir_all(&dir.0).unwrap();
    let script = dir.0.join("fake-worker.sh");
    std::fs::write(
        &script,
        r#"set -eu
out=$1
shift
printf '%s\000' "$@" > "$out.argv"
printf '%s' "${MUXA_WORKER_PROBE_ENV-unset}" > "$out.env"
printf '%s' "$$" > "$out.pid"
exec sleep 600
"#,
    )
    .unwrap();
    let args = [
        "a b",
        "a'b",
        "a\"b",
        "$HOME",
        "\\path",
        "~",
        "日本語",
        "--profile",
        "semi;colon",
        "#{literal}",
        "",
    ];
    let command = |out: &std::path::Path, role: &str| {
        std::iter::once("/bin/sh".to_owned())
            .chain([
                shell_arg(&script.to_string_lossy()),
                shell_arg(&out.to_string_lossy()),
                shell_arg(role),
            ])
            .chain(args.iter().map(|arg| shell_arg(arg)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let (client, _notes) = Client::attach(&server.options(), None).unwrap();
    let workspace_env = "synthetic workspace ' \" $ \\ 日本語";
    let first = command(&dir.0.join("0"), "default");
    let (session, pane) = new_session(
        &client,
        "worker-probe",
        &dir.0.to_string_lossy(),
        &[("MUXA_WORKER_PROBE_ENV".into(), workspace_env.into())],
        &first,
    )
    .await
    .unwrap();
    // A different pane is active: replacing the worker must not select it.
    server.tmux(&[
        "split-window",
        "-d",
        "-h",
        "-t",
        &pane.to_string(),
        "sleep 600",
    ]);
    let other = server.tmux(&["list-panes", "-t", &session.to_string(), "-F", "#{pane_id}"]);
    let other = other.lines().find(|id| *id != pane.to_string()).unwrap();
    server.tmux(&["select-pane", "-t", other]);
    server.tmux(&[
        "new-window",
        "-d",
        "-t",
        &format!("{session}:"),
        "sleep 600",
    ]);
    server.tmux(&["select-window", "-t", &format!("{session}:1")]);
    struct Desktop(Child);
    impl Drop for Desktop {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let _desktop = Desktop(
        Command::new("python3")
            .args([
                "-I",
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../scripts/lib/pty-client.py"
                ),
                &server.socket,
                &session.to_string(),
                "120",
                "30",
                "60",
            ])
            .env_remove("TMUX")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for(|| {
        server
            .tmux(&["list-clients", "-F", "#{client_control_mode}"])
            .lines()
            .any(|line| line == "0")
    })
    .await;
    let shape = || {
        server.tmux(&[
            "display-message", "-p", "-t", &pane.to_string(),
            "#{pane_id} #{pane_width}x#{pane_height} #{window_layout} #{pane_active} #{window_active}",
        ])
    };
    let selection = || {
        format!(
            "{}\n{}",
            server.tmux(&[
                "list-sessions",
                "-F",
                "#{session_id} #{window_id} #{pane_id}"
            ]),
            server.tmux(&[
                "list-clients",
                "-F",
                "#{client_control_mode} #{session_id} #{window_id} #{pane_id}"
            ])
        )
    };
    let before = shape();
    let selected = selection();
    let initial_pid = server.tmux(&[
        "display-message",
        "-p",
        "-t",
        &pane.to_string(),
        "#{pane_pid}",
    ]);
    for invalid in [
        vec![],
        vec!["worker".to_owned(), "synthetic\0value".to_owned()],
    ] {
        assert!(
            respawn_pane_command(&client, pane, &dir.0.to_string_lossy(), &[], &invalid)
                .await
                .is_err()
        );
        assert_eq!(
            server.tmux(&[
                "display-message",
                "-p",
                "-t",
                &pane.to_string(),
                "#{pane_pid}"
            ]),
            initial_pid
        );
        assert_eq!(shape(), before);
        assert_eq!(selection(), selected);
    }
    let mut previous_pid = String::new();
    for (i, role) in ["default", "A", "B", "A-edited", "default"]
        .iter()
        .enumerate()
    {
        let out = dir.0.join(i.to_string());
        let cmd = command(&out, role);
        for explicit in [true, false] {
            if !explicit {
                respawn_pane(&client, pane, &dir.0.to_string_lossy(), &[])
                    .await
                    .unwrap();
            } else if i > 0 {
                let selected = if matches!(i, 2 | 3) {
                    // Multiple tmux arguments exec directly; no shell re-quoting.
                    std::iter::once("/bin/sh".to_owned())
                        .chain([
                            script.to_string_lossy().into_owned(),
                            out.to_string_lossy().into_owned(),
                            (*role).to_owned(),
                        ])
                        .chain(args.iter().map(|arg| (*arg).to_owned()))
                        .collect::<Vec<_>>()
                } else {
                    vec![cmd.clone()]
                };
                respawn_pane_command(&client, pane, &dir.0.to_string_lossy(), &[], &selected)
                    .await
                    .unwrap();
            }
            let pid = server.tmux(&[
                "display-message",
                "-p",
                "-t",
                &pane.to_string(),
                "#{pane_pid}",
            ]);
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if std::fs::read_to_string(out.with_extension("pid"))
                        .ok()
                        .as_deref()
                        == Some(pid.as_str())
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("the fake worker started");
            assert_ne!(
                pid, previous_pid,
                "a new worker process replaced the previous one"
            );
            previous_pid = pid;
            let expected: Vec<u8> = std::iter::once(*role)
                .chain(args)
                .flat_map(|a| a.bytes().chain([0]))
                .collect();
            assert_eq!(std::fs::read(out.with_extension("argv")).unwrap(), expected);
            assert_eq!(
                std::fs::read(out.with_extension("env")).unwrap(),
                workspace_env.as_bytes()
            );

            assert_eq!(shape(), before);
            assert_eq!(selection(), selected);
        }
    }
}

/// W0 baseline: tmux does not turn an old hook tag into fresh process evidence.
#[tokio::test]
async fn worker_respawn_retains_old_hook_tags() {
    use muxa_tmux::{AgentState, respawn_pane_command, set_pane_option};
    let server = Server::start();
    let pane = server.first_pane();
    let (client, _notes) = Client::attach(&server.options(), None).unwrap();
    set_pane_option(&client, pane, "@muxa_role", "agent")
        .await
        .unwrap();
    set_pane_option(&client, pane, "@agent_state", "waiting")
        .await
        .unwrap();
    set_pane_option(&client, pane, "@agent_state_at", "1")
        .await
        .unwrap();
    set_pane_option(&client, pane, "@agent_session_id", "old-fixture-session")
        .await
        .unwrap();
    let old_pid = server.tmux(&[
        "display-message",
        "-p",
        "-t",
        &pane.to_string(),
        "#{pane_pid}",
    ]);
    let dir = server.tmux(&[
        "display-message",
        "-p",
        "-t",
        &pane.to_string(),
        "#{pane_current_path}",
    ]);
    respawn_pane_command(&client, pane, &dir, &[], &["exec sleep 600".into()])
        .await
        .unwrap();
    let sessions = list_sessions(&client).await.unwrap();
    let found = sessions
        .iter()
        .flat_map(|s| &s.panes)
        .find(|p| p.id == pane)
        .unwrap();
    assert_ne!(found.pid.to_string(), old_pid);
    assert_eq!(found.agent.unwrap().state, AgentState::Waiting);
    assert_eq!(found.agent.unwrap().since, Some(1));
    assert_eq!(
        server.tmux(&[
            "show-options",
            "-p",
            "-qv",
            "-t",
            &pane.to_string(),
            "@agent_session_id"
        ]),
        "old-fixture-session"
    );
}

/// Worker creation must preserve native arguments without shell interpretation.
#[tokio::test]
async fn worker_new_session_preserves_native_arguments_and_existing_layout() {
    use muxa_tmux::new_session_command;
    let server = Server::start();
    let original = server.first_pane();
    let shape =
        || {
            server.tmux(&[
        "display-message", "-p", "-t", &original.to_string(),
        "#{session_id} #{window_id} #{pane_id} #{pane_width}x#{pane_height} #{window_layout}",
    ])
        };
    let before = shape();
    let dir = std::env::temp_dir().join(format!(
        "muxa-worker-create-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let script = dir.join("worker.sh");
    let output = dir.join("argv");
    let environment = dir.join("env");
    std::fs::write(&script, "out=$1\nvars=$2\nshift 2\nprintf '%s\\000' \"$@\" > \"$out\"\nprintf '%s' \"$MUXA_CREATE_ENV\" > \"$vars\"\nexec sleep 600\n").unwrap();
    let args = [
        "~",
        "$HOME",
        "semi;colon",
        "a'b",
        "a\"b",
        "\\path",
        "日本語",
        "#{literal}",
        "",
    ];
    let command: Vec<String> = [
        "/bin/sh".into(),
        script.to_string_lossy().into_owned(),
        output.to_string_lossy().into_owned(),
        environment.to_string_lossy().into_owned(),
    ]
    .into_iter()
    .chain(args.iter().map(|arg| (*arg).to_owned()))
    .collect();
    let (client, _notes) = Client::attach(&server.options(), None).unwrap();
    for invalid in [
        vec![],
        vec![String::new()],
        vec!["/bin/sh".into(), "bad\0argument".into()],
    ] {
        assert!(
            new_session_command(&client, "invalid", &dir.to_string_lossy(), &[], &invalid)
                .await
                .is_err()
        );
        assert_eq!(
            server.tmux(&["list-sessions", "-F", "#{session_name}"]),
            "work"
        );
    }
    let value = "synthetic ' \" $ \\ 日本語";
    let (session, pane) = new_session_command(
        &client,
        "worker-native",
        &dir.to_string_lossy(),
        &[("MUXA_CREATE_ENV".into(), value.into())],
        &command,
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !environment.exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let expected: Vec<u8> = args
        .into_iter()
        .flat_map(|arg| arg.bytes().chain([0]))
        .collect();
    assert_eq!(std::fs::read(&output).unwrap(), expected);
    assert_eq!(std::fs::read(&environment).unwrap(), value.as_bytes());
    assert_eq!(shape(), before);
    let found = list_sessions(&client).await.unwrap();
    assert!(
        found
            .iter()
            .any(|s| s.id == session && s.panes.iter().any(|p| p.id == pane))
    );
    assert_eq!(
        server.tmux(&[
            "display-message",
            "-p",
            "-t",
            &pane.to_string(),
            "#{pane_current_path}"
        ]),
        dir.canonicalize().unwrap().to_string_lossy()
    );
}

/// The agent's exit leaves a shell in the same pane; the shell's exit closes it.
#[tokio::test]
async fn a_pane_gets_a_shell_once_after_its_process_exits() {
    use muxa_tmux::{new_session, shell_after_exit};
    let server = Server::start();
    let dir = std::env::temp_dir().join(format!(
        "muxa shell \"after\" $exit-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let (client, _notes) = Client::attach(&server.options(), None).unwrap();
    let path = dir.to_string_lossy().into_owned();
    let (session, pane) = new_session(&client, "agent", &path, &[], "sleep 1")
        .await
        .unwrap();
    shell_after_exit(&client, pane, &path).await.unwrap();
    let show = |format: &str| {
        server
            .tmux(&["display-message", "-p", "-t", &pane.to_string(), format])
            .trim()
            .to_owned()
    };
    wait_for(|| show("#{pane_current_command}") != "sleep").await;
    assert_eq!(show("#{pane_id} #{pane_dead}"), format!("{pane} 0"));
    let shell = server.tmux(&["show-options", "-gv", "default-shell"]);
    assert!(
        show("#{pane_start_command}").starts_with(&format!("{} ", shell.trim())),
        "{}",
        show("#{pane_start_command}")
    );
    let here = std::fs::canonicalize(&dir).unwrap();
    assert_eq!(
        std::fs::canonicalize(show("#{pane_current_path}")).unwrap(),
        here
    );

    server.tmux(&["send-keys", "-t", &pane.to_string(), "exit", "Enter"]);
    let sessions = || server.tmux(&["list-sessions", "-F", "#{session_id}"]);
    wait_for(|| !sessions().lines().any(|s| s == session.to_string())).await;
}
