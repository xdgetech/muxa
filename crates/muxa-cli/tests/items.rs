//! `inbox` and `inbox answer` against a fake bridge at `work` level 3.
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Bridge(PathBuf);

impl Bridge {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "muxa-cli-items-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("bridge.py"), BRIDGE).unwrap();
        Self(dir)
    }

    fn run(&self, args: &[&str]) -> Output {
        let quote =
            |path: PathBuf| format!("'{}'", path.display().to_string().replace('\'', "'\\''"));
        let command = format!(
            "python3 {} {}",
            quote(self.0.join("bridge.py")),
            quote(self.0.join("requests"))
        );
        Command::new(env!("CARGO_BIN_EXE_muxa-cli"))
            .args(["--bridge", &command])
            .args(args)
            .output()
            .unwrap()
    }

    fn requests(&self) -> Vec<Value> {
        fs::read_to_string(self.0.join("requests"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const BRIDGE: &str = r#"
import json, sys
log = open(sys.argv[1], 'w')
def send(value):
    print(json.dumps(value), flush=True)
item = lambda i, state, **kw: dict({'id':i,'kind':'review','state':state,'question':'Ready: '+i,
    'workspace':'work','title':'Title '+i,'createdAt':1,'answers':['accept']}, **kw)
entry = json.loads('{"id": "e-1a2b3c4d", "workspace": "work", "routine": "Morning brief", "key": "linear:ENG-12:stale", "kind": "proposal", "title": "Old entry", "body": "No activity for 30 days.", "links": [{"url": "https://linear.app/example/issue/ENG-12", "title": "ENG-12"}], "createdAt": 1762157000, "run": "t-00a001", "occurrence": "2026-11-03T09:00", "disposition": "open", "proposal": {"action": "Close the issue", "target": "linear:ENG-12", "content": "Closing: no activity for 30 days.", "hash": "sha256:2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881"}}')
send({'type':'hello','protocol':1,'bridge':'fixture','tmux':'fixture',
      'capabilities':['task-records','work:1','work:3']})
for line in sys.stdin:
    req = json.loads(line)
    log.write(json.dumps(req) + '\n'); log.flush()
    kind, ident = req['type'], req['id']
    if kind == 'list-routines':
        send({'type':'routine-list','routines':ident,'entries':[]})
    elif kind == 'list-inbox':
        send({'type':'inbox-list','inbox':ident,'entries':[entry],
              'items':[item('i-00000001','open'), item('i-00000002','answered',answer='accept')]})
    elif kind == 'update-inbox':
        send({'type':'inbox-saved','inbox':ident,
              'item':item(req['entry'],'answered',answer=req['action'],answers=[])})
    send({'type':'result','id':ident, **({'sessions':[]} if kind == 'list' else {})})
"#;

#[test]
fn inbox_lists_items_at_work_level_3() {
    let bridge = Bridge::new();
    let out = bridge.run(&["inbox"]);
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("i-00000001  review  open unread"), "{text}");
    assert!(
        text.contains("Title i-00000001  Ready: i-00000001"),
        "{text}"
    );
    assert!(!text.contains("i-00000002"), "{text}");
    assert!(!text.contains("Old entry"), "{text}");

    let out = bridge.run(&["inbox", "--all"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("i-00000002  review  answered: accept"),
        "{text}"
    );

    let out = bridge.run(&["inbox", "--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["items"].as_array().unwrap().len(), 1);
    // A desktop tool tells an empty Inbox from a host without items.
    assert_eq!(value["work"], 3);
    assert_eq!(value["entries"][0]["id"], "e-1a2b3c4d");
    assert_eq!(value["runs"], json!([]));
}

#[test]
fn each_answer_sends_its_action_and_flags() {
    for (args, want) in [
        (
            vec!["reply", "--text", "try again"],
            json!({"action": "reply", "text": "try again"}),
        ),
        (
            vec!["choose", "--option", "1"],
            json!({"action": "choose", "option": 1}),
        ),
        (vec!["later"], json!({"action": "later"})),
        (vec!["keep-open"], json!({"action": "keep-open"})),
        (vec!["reject"], json!({"action": "reject"})),
    ] {
        let bridge = Bridge::new();
        let mut cli = vec!["inbox", "answer", "i-00000001"];
        cli.extend(&args);
        cli.push("--json");
        let out = bridge.run(&cli);
        assert!(out.status.success(), "{args:?} {out:?}");
        let saved: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(saved["id"], "i-00000001");
        let req = bridge.requests().pop().unwrap();
        assert_eq!(req["type"], "update-inbox");
        assert_eq!(req["entry"], "i-00000001");
        for key in ["action", "text", "option", "until"] {
            assert_eq!(req.get(key), want.get(key), "{args:?} {key}");
        }
    }
}

#[test]
fn inbox_read_is_the_read_answer() {
    let bridge = Bridge::new();
    let out = bridge.run(&["inbox", "read", "i-00000001"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(bridge.requests().pop().unwrap()["action"], "read");
}

#[test]
fn invalid_answers_exit_2_before_any_request() {
    for args in [
        &["inbox", "answer", "i-1", "reply"][..],
        &["inbox", "answer", "i-1", "redo", "--text", "x"],
        &["inbox", "answer", "i-1", "choose"],
        &["inbox", "answer", "i-1", "later", "--until", "+1h"],
        &["inbox", "answer", "i-1", "accept", "--option", "1"],
        &["inbox", "answer", "i-1", "dismiss", "--until", "+1h"],
        &["inbox", "answer", "i-1", "nope"],
    ] {
        let bridge = Bridge::new();
        let out = bridge.run(args);
        assert_eq!(out.status.code(), Some(2), "{args:?} {out:?}");
        assert!(bridge.requests().is_empty(), "{args:?}");
    }
}
