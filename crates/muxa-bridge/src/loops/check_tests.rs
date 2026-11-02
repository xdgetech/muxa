use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("muxa-check-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn spec(dir: &Path, command: &str, attempt: u32, timeout: u64) -> CheckRun {
    CheckRun {
        dir: dir.to_owned(),
        command: command.into(),
        attempt,
        timeout: Duration::from_secs(timeout),
        env: vec![
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("MARK".into(), "from-env".into()),
        ],
    }
}

fn read(path: PathBuf) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[tokio::test]
async fn runs_in_the_task_dir_with_only_the_given_environment() {
    let dir = temp("env");
    let end = run(spec(
        &dir,
        "pwd; echo \"$MARK\"; echo \"[$HOME]\"; echo err >&2; exit 3",
        1,
        10,
    ))
    .await
    .unwrap();
    assert!(matches!(end, CheckEnd::Exited { code: 3, .. }));
    let log = read(log_path(&dir, 1));
    assert!(log.contains(&dir.display().to_string()), "{log}");
    assert!(log.contains("from-env"));
    assert!(log.contains("[]"), "HOME is not inherited: {log}");
    assert!(log.contains("err"));
    let report = read(dir.join(REPORT));
    assert!(report.contains("- Attempt: 1"));
    assert!(report.contains("failed (exit 3)"));
    assert!(report.contains("from-env"));
}

#[tokio::test]
async fn a_pass_is_exit_0() {
    let dir = temp("pass");
    let end = run(spec(&dir, "true", 2, 10)).await.unwrap();
    assert!(matches!(end, CheckEnd::Exited { code: 0, .. }));
    assert!(read(dir.join(REPORT)).contains("passed (exit 0)"));
}

#[tokio::test]
async fn a_timeout_kills_the_whole_group() {
    let dir = temp("timeout");
    // A background child that would outlive its shell, and records its pid.
    let command = "sleep 300 & echo $! > child.pid; sleep 300";
    let mut s = spec(&dir, command, 1, 10);
    s.timeout = Duration::from_millis(500);
    let end = run(s).await.unwrap();
    assert!(matches!(end, CheckEnd::TimedOut { .. }));
    assert!(read(dir.join(REPORT)).contains("timed out"));
    let pid = read(dir.join("child.pid")).trim().to_owned();
    // `kill -0` fails once the process is gone (a zombie is reaped by init quickly).
    let mut gone = false;
    for _ in 0..50 {
        let alive = std::process::Command::new("kill")
            .args(["-0", &pid])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success();
        if !alive {
            gone = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(gone, "the background child {pid} still runs");
}

#[tokio::test]
async fn a_left_over_child_does_not_hold_the_check() {
    let dir = temp("leftover");
    let started = Instant::now();
    let end = run(spec(&dir, "sleep 300 & exit 0", 1, 30)).await.unwrap();
    assert!(matches!(end, CheckEnd::Exited { code: 0, .. }));
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn the_log_is_capped_and_says_so() {
    let dir = temp("cap");
    // 9 MiB of output.
    let command = "head -c 9437184 /dev/zero | tr '\\0' 'a'; echo; echo after";
    run(spec(&dir, command, 1, 60)).await.unwrap();
    let len = std::fs::metadata(log_path(&dir, 1)).unwrap().len();
    assert!(len <= MAX_LOG + TRUNCATED.len() as u64, "{len}");
    assert!(len >= MAX_LOG);
    let log = read(log_path(&dir, 1));
    assert!(log.ends_with(TRUNCATED));
    assert!(!log.contains("after"));
}

#[tokio::test]
async fn only_the_newest_logs_are_kept() {
    let dir = temp("prune");
    for attempt in 1..=(KEEP_LOGS as u32 + 3) {
        run(spec(&dir, "true", attempt, 10)).await.unwrap();
        // mtime resolution on some file systems is coarse.
        std::thread::sleep(Duration::from_millis(15));
    }
    let left = std::fs::read_dir(dir.join(".muxa/checks")).unwrap().count();
    assert_eq!(left, KEEP_LOGS);
    assert!(!log_path(&dir, 1).exists());
    assert!(log_path(&dir, KEEP_LOGS as u32 + 3).exists());
}

#[test]
fn the_report_holds_the_last_200_lines_without_escapes() {
    let dir = temp("tail");
    let folder = dir.join(".muxa/checks");
    std::fs::create_dir_all(&folder).unwrap();
    let mut text = String::new();
    for i in 0..300 {
        text.push_str(&format!("\u{1b}[31mline {i}\u{1b}[0m\r\n"));
    }
    text.push_str("```");
    std::fs::write(folder.join("4.log"), text).unwrap();
    report(
        &dir,
        4,
        CheckEnd::Exited {
            code: 1,
            seconds: 2,
        },
    )
    .unwrap();
    let r = read(dir.join(REPORT));
    assert!(!r.contains('\u{1b}'));
    assert!(!r.contains('\r'));
    assert!(!r.contains("line 100\n"));
    assert!(r.contains("line 101\n"));
    assert!(r.contains("line 299\n"));
    assert!(
        r.contains("````text\n"),
        "the fence outgrows the output's backticks"
    );
    assert!(r.contains("- Duration: 2 s"));
}

#[test]
fn escapes_and_controls_are_removed() {
    assert_eq!(strip_escapes("a\u{1b}[1;31mb\u{1b}[0m"), "ab");
    assert_eq!(strip_escapes("\u{1b}]0;title\u{7}x"), "x");
    assert_eq!(strip_escapes("\u{1b}]8;;url\u{1b}\\y"), "y");
    assert_eq!(strip_escapes("a\u{8}b\tc\r\nd\re"), "ab\tc\nd\ne");
    assert_eq!(strip_escapes("\u{1b}(Bz"), "z");
}

#[test]
fn global_env_drops_removed_variables() {
    let lines = [
        b"PATH=/bin".to_vec(),
        b"-SSH_AUTH_SOCK".to_vec(),
        b"A=b=c".to_vec(),
    ];
    assert_eq!(
        parse_global_env(&lines),
        vec![("PATH".into(), "/bin".into()), ("A".into(), "b=c".into())]
    );
}

#[test]
fn the_workspace_env_overlays_the_tmux_env() {
    let agent = AgentSettings {
        command: None,
        env: vec![
            ("GH_TOKEN".into(), "w".into()),
            ("PATH".into(), "/ws".into()),
        ],
    };
    let global = vec![("PATH".into(), "/bin".into()), ("HOME".into(), "/h".into())];
    let env = environment(Some(global), Some(&agent), Vec::new(), None);
    assert_eq!(
        env,
        vec![
            ("HOME".into(), "/h".into()),
            ("GH_TOKEN".into(), "w".into()),
            ("PATH".into(), "/ws".into()),
        ]
    );
    let own = vec![("PATH".into(), "/own".into()), ("HOME".into(), "/h".into())];
    let env = environment(None, None, own, Some("/installed"));
    assert_eq!(
        env,
        vec![
            ("HOME".into(), "/h".into()),
            ("PATH".into(), "/installed".into())
        ]
    );
}

#[test]
fn debug_hides_the_command_and_env() {
    let s = spec(Path::new("/x"), "echo secret", 1, 10);
    let d = format!("{s:?}");
    assert!(!d.contains("secret") && !d.contains("from-env"));
}
