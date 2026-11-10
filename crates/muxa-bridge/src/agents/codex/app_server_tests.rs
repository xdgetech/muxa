//! Against `tests/data/codex-fake.py`, which speaks the App Server protocol
//! and never calls a model.
use super::*;

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/codex-fake.py");

struct Home(PathBuf);

impl Home {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "muxa-app-server-{}-{name}-{:08x}",
            std::process::id(),
            getrandom::u32().unwrap()
        ));
        std::fs::create_dir_all(dir.join("fake")).unwrap();
        Self(dir)
    }

    fn endpoint(&self) -> Endpoint {
        Endpoint::new(FAKE, Some(self.0.clone())).with_timeout(Duration::from_secs(5))
    }

    fn steer(&self, name: &str, text: &str) {
        std::fs::write(self.0.join("fake").join(name), text).unwrap();
    }

    fn log(&self, name: &str) -> Vec<String> {
        std::fs::read_to_string(self.0.join("fake").join(name))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_thread_starts_idle_with_its_rollout_path_and_reads_back_by_id() {
    let home = Home::new("start");
    let app = home.endpoint();
    app.start_daemon().unwrap();
    assert_eq!(home.log("calls.log"), ["daemon-start"]);
    let cwd = home.0.join("run");
    let thread = app.start_thread(&cwd).unwrap();
    assert_eq!(thread.status, Status::Idle);
    let path = thread.path.clone().unwrap();
    assert!(path.to_string_lossy().contains(thread.id.as_str()));
    assert!(!path.exists(), "no rollout before the first turn");
    let starts = home.log("starts.log");
    assert_eq!(starts.len(), 1);
    let start: Value = serde_json::from_str(&starts[0]).unwrap();
    assert_eq!(start["historyMode"], "legacy");
    assert_eq!(start["cwd"], cwd.to_string_lossy().as_ref());
    assert_eq!(app.read_thread(&thread.id).unwrap(), thread);
}

#[test]
fn statuses_map_and_anything_else_fails() {
    let home = Home::new("status");
    let app = home.endpoint();
    let thread = app.start_thread(&home.0).unwrap();
    let file = format!("status-{}", thread.id);
    for (word, want) in [
        ("active", Status::Active { waiting: false }),
        ("approval", Status::Active { waiting: true }),
        ("notLoaded", Status::NotLoaded),
        ("systemError", Status::SystemError),
        ("idle", Status::Idle),
    ] {
        home.steer(&file, word);
        assert_eq!(app.read_thread(&thread.id).unwrap().status, want, "{word}");
    }
    for word in ["missing", "bogus"] {
        home.steer(&file, word);
        assert!(
            matches!(
                app.read_thread(&thread.id),
                Err(Error::Shape {
                    what: "thread status",
                    ..
                })
            ),
            "{word}"
        );
    }
    let unknown = ThreadId::parse("00000000-0000-0000-0000-000000000000").unwrap();
    assert!(matches!(
        app.read_thread(&unknown),
        Err(Error::Rpc {
            method: "thread/read",
            ..
        })
    ));
    assert!(matches!(ThreadId::parse("not a uuid"), Err(Error::BadId)));
}

#[test]
fn a_turn_is_sent_once_and_a_refusal_is_not_uncertain() {
    let home = Home::new("turn");
    let app = home.endpoint();
    let thread = app.start_thread(&home.0).unwrap();
    app.start_turn(&thread.id, "fixed prompt").unwrap();
    assert_eq!(
        home.log("turns.log"),
        [format!("{}\tfixed prompt", thread.id)]
    );
    home.steer("refuse-turn", "");
    let refused = app.start_turn(&thread.id, "fixed prompt").unwrap_err();
    assert!(!refused.uncertain, "an error answer is definite");
    assert_eq!(home.log("turns.log").len(), 1);
}

#[test]
fn a_dropped_or_silent_turn_is_uncertain() {
    let home = Home::new("drop");
    let app = home.endpoint();
    let thread = app.start_thread(&home.0).unwrap();
    home.steer("drop-turn", "");
    let dropped = app.start_turn(&thread.id, "fixed prompt").unwrap_err();
    assert!(dropped.uncertain);
    assert!(matches!(dropped.error, Error::Lost));
    std::fs::remove_file(home.0.join("fake/drop-turn")).unwrap();

    let short = Endpoint::new(FAKE, Some(home.0.clone())).with_timeout(Duration::from_secs(1));
    home.steer("silent-turn", "");
    let unanswered = short.start_turn(&thread.id, "fixed prompt").unwrap_err();
    assert!(unanswered.uncertain);
    assert!(matches!(unanswered.error, Error::Timeout(_)));
    std::fs::remove_file(home.0.join("fake/silent-turn")).unwrap();
    home.steer("silent", "");
    let silent = short.start_turn(&thread.id, "fixed prompt").unwrap_err();
    assert!(
        !silent.uncertain,
        "no answer to initialize: the turn was never written"
    );
    assert!(matches!(silent.error, Error::Timeout(_)));
    assert!(matches!(
        short.read_thread(&thread.id),
        Err(Error::Timeout(_))
    ));
}

#[test]
fn a_missing_program_and_a_failed_daemon_are_errors() {
    let home = Home::new("missing");
    let none = Endpoint::new(home.0.join("no-codex").to_string_lossy(), None);
    assert!(matches!(
        none.start_thread(&home.0),
        Err(Error::Spawn { .. })
    ));
    home.steer("daemon-fails", "");
    assert!(matches!(
        home.endpoint().start_daemon(),
        Err(Error::Daemon(_))
    ));
}

/// Raw JSON through `codex app-server proxy` with `input` on stdin.
fn raw(home: &Home, args: &[&str], input: &str) -> std::process::Output {
    use std::io::Write;
    let mut child = Command::new(FAKE)
        .args(args)
        .env(super::super::DIR_ENV, &home.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).unwrap();
    drop(stdin);
    child.wait_with_output().unwrap()
}

#[test]
fn the_proxy_answers_raw_json_lines_with_nothing() {
    let home = Home::new("raw");
    let init = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":"#,
        r#"{"clientInfo":{"name":"raw","version":"0"}}}"#,
        "\n"
    );
    let proxied = raw(&home, &["app-server", "proxy"], init);
    assert!(proxied.status.success());
    assert!(proxied.stdout.is_empty(), "the proxy wants WebSocket");
    let private = raw(&home, &["app-server"], init);
    let answer: Value = serde_json::from_slice(
        private
            .stdout
            .split(|b| *b == b'\n')
            .find(|l| l.starts_with(br#"{"id""#))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(answer["result"]["userAgent"], "fake", "stdio keeps lines");
}

#[test]
fn a_message_over_the_daemons_limit_is_not_sent() {
    let home = Home::new("limit");
    let app = home.endpoint();
    let thread = app.start_thread(&home.0).unwrap();
    home.steer("max-message", "400");
    let big = "x".repeat(400);
    let refused = app.start_turn(&thread.id, &big).unwrap_err();
    assert!(!refused.uncertain, "nothing was written");
    assert!(matches!(refused.error, Error::TooLarge { limit: 400, .. }));
    assert!(home.log("turns.log").is_empty());
    app.start_turn(&thread.id, "short").unwrap();
    assert_eq!(home.log("turns.log").len(), 1);
}

#[test]
fn a_request_returns_the_whole_response() {
    let home = Home::new("request");
    let app = home.endpoint();
    let started = app
        .request(
            "thread/start",
            json!({"cwd": home.0, "historyMode": "legacy"}),
        )
        .unwrap();
    let id = started["result"]["thread"]["id"].as_str().unwrap();
    assert!(ThreadId::parse(id).is_ok());
    let unknown = app
        .request("thread/read", json!({"threadId": "nope"}))
        .unwrap();
    assert!(unknown["error"]["message"].is_string());
    assert!(unknown.get("result").is_none());
}
