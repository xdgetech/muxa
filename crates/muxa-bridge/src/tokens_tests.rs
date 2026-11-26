use std::sync::Arc;

use super::*;

fn utc_day() -> Day {
    Day::new(jiff::civil::date(2026, 11, 25), &TimeZone::UTC).unwrap()
}

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("muxa-tokens-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    fn write(&self, rel: &str, lines: &[String]) -> PathBuf {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        path
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn claude(id: &str, at: &str, cwd: &str, input: u64, read: u64, output: u64) -> String {
    serde_json::json!({
        "type": "assistant", "timestamp": at, "cwd": cwd,
        "message": {"id": id, "content": [{"type": "text", "text": "never kept"}],
            "usage": {"input_tokens": input, "cache_creation_input_tokens": 10,
                "cache_read_input_tokens": read, "output_tokens": output}}
    })
    .to_string()
}

fn codex(at: &str, input: u64, cached: u64, output: u64) -> String {
    serde_json::json!({
        "timestamp": at, "type": "event_msg",
        "payload": {"type": "token_count", "info": {"total_token_usage": {
            "input_tokens": input, "cached_input_tokens": cached, "output_tokens": output,
            "reasoning_output_tokens": 1, "total_tokens": input + output}}}
    })
    .to_string()
}

#[test]
fn a_claude_message_counts_once_and_only_on_its_day() {
    let d = Dir::new("claude");
    let path = d.write(
        "projects/-w/s.jsonl",
        &[
            claude("m1", "2026-11-25T09:00:00Z", "/w/t", 100, 1000, 5),
            // The same message again, for its second content block.
            claude("m1", "2026-11-25T09:00:01Z", "/w/t", 100, 1000, 5),
            claude("m2", "2026-11-24T23:59:59Z", "/w/t", 7, 7, 7),
            claude("m3", "2026-11-25T10:00:00Z", "/w", 1, 0, 2),
            r#"{"type":"user","timestamp":"2026-11-25T09:00:00Z","cwd":"/w/t"}"#.into(),
            "not json".into(),
        ],
    );
    let got = claude_file(&path, &utc_day());
    assert_eq!(
        got["/w/t"],
        Count {
            input: 110,
            cached: 1000,
            output: 5
        }
    );
    assert_eq!(got["/w"].output, 2);
}

#[test]
fn a_codex_session_counts_what_its_totals_grew_by_on_the_day() {
    let d = Dir::new("codex");
    let meta =
        r#"{"timestamp":"2026-11-24T22:00:00Z","type":"session_meta","payload":{"cwd":"/w/c"}}"#;
    let path = d.write(
        "sessions/2026/11/24/rollout-a.jsonl",
        &[
            meta.into(),
            codex("2026-11-24T23:00:00Z", 1000, 800, 50),
            r#"{"timestamp":"2026-11-25T01:00:00Z","type":"event_msg","payload":{"type":"token_count","info":null}}"#.into(),
            codex("2026-11-25T02:00:00Z", 1500, 1100, 80),
            codex("2026-11-26T01:00:00Z", 9000, 9000, 900),
        ],
    );
    assert_eq!(
        codex_file(&path, &utc_day()),
        Some((
            "/w/c".to_owned(),
            Count {
                input: 200,
                cached: 300,
                output: 30
            }
        ))
    );
    let quiet = d.write(
        "sessions/2026/11/25/rollout-b.jsonl",
        &[meta.into(), codex("2026-11-24T23:00:00Z", 10, 0, 1)],
    );
    assert_eq!(codex_file(&quiet, &utc_day()), None);
}

#[test]
fn folders_go_to_their_workspace_and_others_stay_on_the_host() {
    let by = HashMap::from([
        (
            "/w".to_owned(),
            Count {
                input: 1,
                cached: 0,
                output: 1,
            },
        ),
        (
            "/w/tasks/fix".to_owned(),
            Count {
                input: 10,
                cached: 5,
                output: 10,
            },
        ),
        (
            "/w/tasks/big".to_owned(),
            Count {
                input: 50,
                cached: 0,
                output: 50,
            },
        ),
        (
            "/w/inner/x".to_owned(),
            Count {
                input: 3,
                cached: 0,
                output: 0,
            },
        ),
        (
            "/private/elsewhere".to_owned(),
            Count {
                input: 99,
                cached: 0,
                output: 99,
            },
        ),
    ]);
    let roots = [
        ("w".to_owned(), PathBuf::from("/w")),
        ("inner".to_owned(), PathBuf::from("/w/inner")),
    ];
    let got = assign(by, &roots, &utc_day());
    assert_eq!(got.day, "2026-11-25");
    let ws: Vec<(&str, u64)> = got
        .workspaces
        .iter()
        .map(|w| (w.workspace.as_str(), w.tokens.input))
        .collect();
    assert_eq!(ws, [("w", 61), ("inner", 3)]);
    let tasks: Vec<(&str, &str)> = got
        .tasks
        .iter()
        .map(|t| (t.title.as_str(), t.workspace.as_str()))
        .collect();
    assert_eq!(tasks, [("big", "w"), ("fix", "w"), ("x", "inner")]);
    assert!(!format!("{got:?}").contains("elsewhere"));
}

#[test]
fn the_dirs_are_searched_for_files_of_the_day() {
    let claude_dir = Dir::new("claude-dirs");
    claude_dir.write(
        "projects/-w/s.jsonl",
        &[claude("m1", "2026-11-25T09:00:00Z", "/w/t", 1, 0, 1)],
    );
    claude_dir.write(
        "projects/-w/s/subagents/agent-1.jsonl",
        &[claude("m9", "2026-11-25T09:30:00Z", "/w/t", 2, 0, 2)],
    );
    let codex_dir = Dir::new("codex-dirs");
    codex_dir.write(
        "sessions/2026/11/25/rollout-c.jsonl",
        &[
            r#"{"timestamp":"2026-11-25T08:00:00Z","type":"session_meta","payload":{"cwd":"/w/t"}}"#.into(),
            codex("2026-11-25T08:10:00Z", 10, 4, 3),
        ],
    );
    // A file written today counts; the test's day is today in UTC.
    let today = Day::new(
        jiff::Timestamp::now().to_zoned(TimeZone::UTC).date(),
        &TimeZone::UTC,
    )
    .unwrap();
    let fix = |s: &str| s.replace("2026-11-25", &today.date.to_string());
    for path in [
        "projects/-w/s.jsonl",
        "projects/-w/s/subagents/agent-1.jsonl",
    ] {
        let p = claude_dir.0.join(path);
        std::fs::write(&p, fix(&std::fs::read_to_string(&p).unwrap())).unwrap();
    }
    let d = today.date;
    let rollout = codex_dir.0.join("sessions/2026/11/25/rollout-c.jsonl");
    let moved = codex_dir.0.join(format!(
        "sessions/{:04}/{:02}/{:02}/rollout-c.jsonl",
        d.year(),
        d.month(),
        d.day()
    ));
    let text = fix(&std::fs::read_to_string(&rollout).unwrap());
    std::fs::remove_file(&rollout).unwrap();
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::write(&moved, text).unwrap();

    let got = by_folder(
        std::slice::from_ref(&claude_dir.0),
        std::slice::from_ref(&codex_dir.0),
        &today,
    );
    assert_eq!(
        got["/w/t"],
        Count {
            // Each Claude line adds 10 cache writes; Codex input less its cached.
            input: 11 + 12 + 6,
            cached: 4,
            output: 1 + 2 + 3
        }
    );
}

#[tokio::test]
async fn token_usage_reads_the_hosts_logins_for_its_workspaces() {
    use muxa_server::{Host, TaskError};
    let d = Dir::new("host");
    let base = d.0.canonicalize().unwrap();
    std::fs::create_dir_all(base.join("root/tasks/fix")).unwrap();
    std::fs::write(
        base.join("workspaces.toml"),
        format!(
            "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n",
            base.join("root").display()
        ),
    )
    .unwrap();
    let today = jiff::Timestamp::now().to_zoned(TimeZone::system()).date();
    let at = today
        .at(12, 0, 0, 0)
        .to_zoned(TimeZone::system())
        .unwrap()
        .timestamp()
        .to_string();
    let task = base.join("root/tasks/fix").display().to_string();
    d.write(
        ".claude/projects/-fix/s.jsonl",
        &[claude("m1", &at, &task, 5, 0, 7)],
    );
    let config = crate::config::Config {
        state_dir: Some(base.join("state")),
        workspaces: Some(base.join("workspaces.toml")),
        home: Some(base.clone()),
    };
    let host = crate::v2::BridgeHost::new("claude".into(), Arc::new(config));

    let got = host.token_usage(None).await.unwrap();
    assert_eq!(got.day, today.to_string());
    assert_eq!(got.workspaces[0].workspace, "w");
    assert_eq!(
        (got.tasks[0].title.as_str(), got.tasks[0].tokens.output),
        ("fix", 7)
    );
    assert!(matches!(
        host.token_usage(Some("yesterday".into())).await,
        Err(TaskError::Invalid(_))
    ));
}
