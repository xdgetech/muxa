//! `inbox show`, `inbox answer` on a `memory` item and `memory WORKSPACE`
//! against a fake bridge at `work` level 14 (ADR 0053): what each command
//! prints and sends, and that a level 13 host gets no approve.
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Bridge(PathBuf);

impl Bridge {
    fn new(level: u32) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "muxa-cli-memory-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(dir.join("root/.muxa")).unwrap();
        fs::write(
            dir.join("bridge.py"),
            BRIDGE
                .replace("LEVEL", &level.to_string())
                .replace("ROOT", &dir.join("root").display().to_string()),
        )
        .unwrap();
        Self(dir)
    }

    fn run(&self, args: &[&str]) -> Output {
        let quote =
            |path: PathBuf| format!("'{}'", path.display().to_string().replace('\'', "'\\''"));
        let bridge = format!(
            "python3 {} {}",
            quote(self.0.join("bridge.py")),
            quote(self.0.join("requests"))
        );
        Command::new(env!("CARGO_BIN_EXE_muxa-cli"))
            .args(["--bridge", &bridge])
            .args(args)
            .output()
            .unwrap()
    }

    fn sent(&self, kind: &str) -> Vec<Value> {
        fs::read_to_string(self.0.join("requests"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|r| r["type"] == kind)
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
text = '- Keys live in Vault.\n- \x1b[31mred\x1b[0m\n'
memory = {'task':'t-00b001','title':'Rotate the keys','text':text,'hash':'sha256:4f'}
item = {'id':'i-4e5f6a7b','kind':'memory','state':'open',
        'question':'Rotate the keys proposes to add to the workspace memory','workspace':'lisk',
        'title':'Rotate the keys','createdAt':1,'task':'t-00b001','memory':memory,
        'answers':['approve','reject','later']}
workspace = {'name':'lisk','root':'ROOT'}
if LEVEL >= 14:
    workspace.update(memory='review', memoryAt=1)
send({'type':'hello','protocol':1,'bridge':'fixture','tmux':'fixture',
      'capabilities':['task-records','work:LEVEL']})
for line in sys.stdin:
    req = json.loads(line)
    log.write(json.dumps(req) + '\n'); log.flush()
    kind, ident = req['type'], req['id']
    if kind == 'list-inbox':
        send({'type':'inbox-list','inbox':ident,'entries':[],'items':[item]})
    elif kind == 'list-tasks':
        send({'type':'task-list','tasks':ident,'workspaces':[workspace],'entries':[]})
    elif kind == 'update-inbox':
        send({'type':'inbox-saved','inbox':ident,
              'item':dict(item, state='answered', answer=req['action'], answers=[])})
    send({'type':'result','id':ident})
"#;

fn ok(out: &Output) -> String {
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout.clone()).unwrap()
}

#[test]
fn inbox_show_prints_the_memory_text_whole_as_plain_text() {
    let bridge = Bridge::new(14);
    let text = ok(&bridge.run(&["inbox", "show", "i-4e5f6a7b"]));
    assert!(
        text.starts_with("i-4e5f6a7b  memory  open unread  "),
        "{text}"
    );
    assert!(
        text.contains("From    Task t-00b001 · Rotate the keys\n"),
        "{text}"
    );
    assert!(
        text.contains("exactly as below:\n- Keys live in Vault.\n- [31mred[0m\n"),
        "{text}"
    );
}

#[test]
fn approve_shows_the_text_then_sends_its_hash() {
    let bridge = Bridge::new(14);
    let out = ok(&bridge.run(&["inbox", "answer", "i-4e5f6a7b", "approve"]));
    assert!(out.contains("- Keys live in Vault."), "{out}");
    let sent = bridge.sent("update-inbox");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["action"], "approve");
    assert_eq!(sent[0]["hash"], "sha256:4f");
    ok(&bridge.run(&["inbox", "answer", "i-4e5f6a7b", "reject"]));
    assert!(bridge.sent("update-inbox")[1].get("hash").is_none());
}

#[test]
fn a_level_13_host_gets_no_approve_and_no_memory() {
    let bridge = Bridge::new(13);
    let out = bridge.run(&["inbox", "answer", "i-4e5f6a7b", "approve"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("update muxa-bridge"), "{err}");
    assert!(bridge.sent("update-inbox").is_empty(), "nothing was sent");
    let out = bridge.run(&["memory", "lisk"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("update muxa-bridge"));
    assert!(bridge.sent("list-tasks").is_empty());
}

#[test]
fn memory_prints_the_mode_and_the_live_file() {
    let bridge = Bridge::new(14);
    let out = ok(&bridge.run(&["memory", "lisk"]));
    assert!(out.starts_with("lisk  memory review  changed "), "{out}");
    assert_eq!(out.lines().count(), 1, "no file yet: {out}");
    fs::write(
        bridge.0.join("root/.muxa/WORKSPACE.md"),
        "# Lisk\n\n## Rotate the keys (t-00b001, 2026-11-11)\n\n- Keys live in Vault.\n",
    )
    .unwrap();
    let out = ok(&bridge.run(&["memory", "lisk"]));
    assert!(
        out.ends_with(
            "\n# Lisk\n\n## Rotate the keys (t-00b001, 2026-11-11)\n\n- Keys live in Vault.\n"
        ),
        "{out}"
    );
    let json: Value =
        serde_json::from_str(&ok(&bridge.run(&["memory", "lisk", "--json"]))).unwrap();
    assert_eq!(json["memory"], "review");
    assert!(json["text"].as_str().unwrap().starts_with("# Lisk"));
    let missing = bridge.run(&["memory", "other"]);
    assert!(!missing.status.success());
}
