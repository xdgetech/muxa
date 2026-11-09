//! `capture`, `work` and `task new --work` against a fake bridge at `work`
//! level 8 (ADR 0046): what each command sends, with the `rev` it read.
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Bridge(PathBuf);

impl Bridge {
    fn new(level: u32) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "muxa-cli-work-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        fs::write(
            dir.join("bridge.py"),
            BRIDGE.replace("LEVEL", &level.to_string()),
        )
        .unwrap();
        Self(dir)
    }

    fn run(&self, args: &[&str], stdin: &str) -> Output {
        let quote =
            |path: PathBuf| format!("'{}'", path.display().to_string().replace('\'', "'\\''"));
        let command = format!(
            "python3 {} {}",
            quote(self.0.join("bridge.py")),
            quote(self.0.join("requests"))
        );
        let mut child = Command::new(env!("CARGO_BIN_EXE_muxa-cli"))
            .args(["--bridge", &command])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    /// The requests that change something: not `list-inbox`.
    fn changes(&self) -> Vec<Value> {
        fs::read_to_string(self.0.join("requests"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|r| r["type"] != "list-inbox" && r["type"] != "list")
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
log = open(sys.argv[1], 'a')
def send(value):
    print(json.dumps(value), flush=True)
work = {'id':'w-0000abcd','rev':4,'title':'Shorten onboarding','state':'open',
        'createdAt':1,'updatedAt':1,'workspace':'work'}
capture = {'id':'c-0000abcd','rev':2,'clientId':'x','text':'Ring the bank\nsecond','source':'app',
           'state':'received','createdAt':1,'updatedAt':1}
send({'type':'hello','protocol':1,'bridge':'fixture','tmux':'fixture',
      'capabilities':['task-records','work:LEVEL']})
for line in sys.stdin:
    req = json.loads(line)
    log.write(json.dumps(req) + '\n'); log.flush()
    kind, ident = req['type'], req['id']
    if kind == 'list-inbox':
        send({'type':'inbox-list','inbox':ident,'entries':[],'items':[],
              'work':[work, dict(work, id='w-0000dead', state='done')],'captures':[capture]})
    elif kind == 'capture':
        send({'type':'capture-saved','capture':ident,'entry':dict(capture, text=req['text'], source=req['source'])})
    elif kind == 'sort-capture':
        send({'type':'capture-saved','capture':ident,'entry':dict(capture, state='sorted'),'work':work})
    elif kind == 'update-work':
        send({'type':'work-saved','work':ident,'entry':dict(work, rev=5, laterAt=9)})
    elif kind == 'create-task':
        send({'type':'task-saved','task':ident,'record':{'id':'t-0000aa','rev':1,'workspace':'work',
              'title':req['title'],'goal':req['goal'],'status':'open','createdAt':1,'updatedAt':1,'work':req.get('work')}})
    send({'type':'result','id':ident, **({'sessions':[]} if kind == 'list' else {})})
"#;

fn ok(out: &Output) -> String {
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout.clone()).unwrap()
}

#[test]
fn capture_sends_the_text_once_with_its_source() {
    let bridge = Bridge::new(8);
    let text = ok(&bridge.run(
        &[
            "capture",
            "-",
            "--source",
            "tmux",
            "--client-id",
            "tmux-1",
            "--json",
        ],
        "Renew the visa\nbefore June",
    ));
    let saved: Value = serde_json::from_str(text.trim()).unwrap();
    assert_eq!(saved["capture"]["text"], "Renew the visa\nbefore June");
    let sent = bridge.changes();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["type"], "capture");
    assert_eq!(sent[0]["clientId"], "tmux-1");
    assert_eq!(sent[0]["source"], "tmux");
    assert!(sent[0].get("assist").is_none());
}

#[test]
fn work_lists_open_items_then_unsorted_captures() {
    let bridge = Bridge::new(8);
    let text = ok(&bridge.run(&["work"], ""));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert!(lines[0].starts_with("w-0000abcd  open  "), "{text}");
    assert!(lines[0].ends_with("Shorten onboarding  [work]"), "{text}");
    assert!(lines[1].starts_with("c-0000abcd  unsorted  "), "{text}");
    assert!(!text.contains("second"), "only the first line: {text}");
    assert_eq!(ok(&bridge.run(&["work", "--all"], "")).lines().count(), 3);
}

#[test]
fn changes_send_the_rev_they_read() {
    let bridge = Bridge::new(8);
    ok(&bridge.run(&["work", "later", "w-0000abcd"], ""));
    ok(&bridge.run(&["work", "merge", "w-0000abcd", "w-0000dead"], ""));
    ok(&bridge.run(&["work", "sort", "c-0000abcd", "link", "w-0000abcd"], ""));
    ok(&bridge.run(
        &[
            "task",
            "new",
            "--workspace",
            "work",
            "--title",
            "Do it",
            "--goal",
            "Go",
            "--work",
            "w-0000abcd",
        ],
        "",
    ));
    let sent = bridge.changes();
    assert_eq!(sent[0]["action"], "later");
    assert_eq!(sent[0]["rev"], 4);
    assert_eq!(sent[1]["action"], "merge");
    assert_eq!(sent[1]["into"], "w-0000dead");
    assert_eq!(sent[2]["type"], "sort-capture");
    assert_eq!(
        (sent[2]["rev"].clone(), sent[2]["work"].clone()),
        (2.into(), "w-0000abcd".into())
    );
    assert_eq!(sent[3]["type"], "create-task");
    assert_eq!(sent[3]["work"], "w-0000abcd");
    assert_eq!(sent[3]["workRev"], 4);
}

#[test]
fn wrong_arguments_and_an_older_host_send_nothing() {
    let bridge = Bridge::new(8);
    for args in [
        &["work", "edit", "w-0000abcd"][..],
        &["work", "later", "w-0000abcd", "--title", "x"],
        &["work", "merge", "w-0000abcd"],
        &["work", "sort", "c-0000abcd", "note", "--title", "x"],
        &["work", "sort", "c-0000abcd", "link"],
        &["work", "later", "w-0000ffff"],
        &["capture", "x", "--source", "phone"],
        &["tasks", "--work", "w-0000abcd"],
    ] {
        let out = bridge.run(args, "");
        assert!(!out.status.success(), "{args:?}");
    }
    assert!(bridge.changes().is_empty());
    let old = Bridge::new(7);
    let out = old.run(&["capture", "x"], "");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("update muxa-bridge"));
    assert!(old.changes().is_empty());
}
