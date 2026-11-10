//! `inbox show`, `inbox answer` on a `write` item and `assist writes`
//! against a fake bridge at `work` level 13 (ADR 0052): what each command
//! prints and sends, and that a level 12 host gets no approve.
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Bridge(PathBuf);

impl Bridge {
    fn new(level: u32) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "muxa-cli-writes-{}-{}",
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
content = 'Release 2.4 is out.\n\n- Faster sync\n- \x1b[31mred\x1b[0m'
write = {'action':'Post to #release','target':'slack:#release','content':content,'hash':'sha256:ab'}
proposal = dict(write, request='q-5e6f7a8b', version=2, step=1, worker='lisk-delivery')
item = {'id':'i-9c0d1e2f','kind':'write','state':'open',
        'question':'lisk-delivery proposes to write to slack:#release','workspace':'lisk',
        'title':'Post to #release','createdAt':1,'summary':'The user asked for it.',
        'write':proposal,'answers':['approve','reject','later']}
rejected = dict(item, id='i-0d1e2f3a', state='answered', answer='reject', answers=[],
                write=dict(proposal, action='Comment on ENG-12', target='linear:ENG-12'))
work = {'id':'w-9c0d1e2f','rev':3,'title':'Ship release 2.4','state':'in-progress',
        'createdAt':1,'updatedAt':1,'workspace':'lisk','requests':['q-5e6f7a8b']}
request = {'id':'q-5e6f7a8b','work':'w-9c0d1e2f','version':2,'workspace':'lisk','scope':'draft',
           'delivery':'resulted','deliveryAt':1,'createdAt':1,'updatedAt':1,
           'results':[{'version':2,'at':1,'summary':'Ready.','worker':'lisk-delivery','step':1,
                       'writes':[dict(write, item='i-9c0d1e2f', task='t-00f001'),
                                 dict(write, action='Comment on ENG-12', target='linear:ENG-12',
                                      item='i-0d1e2f3a'),
                                 dict(write, action='Post again', item='i-1e2f3a4b')]}]}
assistant = {'delegates':[{'workspace':'lisk','share':'summaries','overallRequests':'review',
             'writer':{'worker':'lisk-writer','description':'Writer: posts approved text.'},
             'writes':'review'},
            {'workspace':'home','share':'outline','overallRequests':'review','writes':'none'}]}
send({'type':'hello','protocol':1,'bridge':'fixture','tmux':'fixture',
      'capabilities':['task-records','work:LEVEL']})
for line in sys.stdin:
    req = json.loads(line)
    log.write(json.dumps(req) + '\n'); log.flush()
    kind, ident = req['type'], req['id']
    if kind == 'list-inbox':
        send({'type':'inbox-list','inbox':ident,'entries':[],'items':[item, rejected],
              'work':[work],'requests':[request],'assistant':assistant})
    elif kind == 'update-inbox' and req['action'] == 'approve':
        send({'type':'task-saved','task':ident,'record':{'id':'t-00f001','rev':1,'workspace':'lisk',
              'title':'Post to #release','goal':'g','status':'open','createdAt':1,'updatedAt':1,
              'worker':'lisk-writer','work':'w-9c0d1e2f'}})
        send({'type':'inbox-saved','inbox':ident,
              'item':dict(item, state='answered', answer='approve', task='t-00f001', answers=[])})
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
fn inbox_show_prints_the_write_whole_as_plain_text() {
    let bridge = Bridge::new(13);
    let text = ok(&bridge.run(&["inbox", "show", "i-9c0d1e2f"]));
    assert!(
        text.starts_with("i-9c0d1e2f  write  open unread  "),
        "{text}"
    );
    assert!(text.contains("\nTarget  slack:#release\n"), "{text}");
    assert!(
        text.contains("From    lisk-delivery · v2 step 1\n"),
        "{text}"
    );
    assert!(
        text.contains(
            "written exactly as below:\nRelease 2.4 is out.\n\n- Faster sync\n- [31mred[0m\n"
        ),
        "{text}"
    );
    assert!(!text.contains('\u{1b}'));
    let json: Value =
        serde_json::from_str(ok(&bridge.run(&["inbox", "show", "i-9c0d1e2f", "--json"])).trim())
            .unwrap();
    assert_eq!(json["write"]["target"], "slack:#release");
    let out = bridge.run(&["inbox", "show", "i-ffffffff"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("no Inbox item i-ffffffff"));
}

#[test]
fn approve_says_the_task_started_and_reject_sends_the_reason() {
    let bridge = Bridge::new(13);
    let text = ok(&bridge.run(&["inbox", "answer", "i-9c0d1e2f", "approve"]));
    assert!(text.contains("answered: approve"), "{text}");
    assert!(text.contains("Task started: t-00f001"), "{text}");
    assert!(!text.contains("posted"), "{text}");
    ok(&bridge.run(&[
        "inbox",
        "answer",
        "i-9c0d1e2f",
        "reject",
        "--reason",
        "Wrong channel.",
    ]));
    let sent = bridge.sent("update-inbox");
    assert_eq!(
        (&sent[0]["action"], &sent[1]["action"], &sent[1]["text"]),
        (
            &json!("approve"),
            &json!("reject"),
            &json!("Wrong channel.")
        )
    );
    // Only a level 12 host is asked what the item is first.
    assert!(bridge.sent("list-inbox").is_empty());
}

#[test]
fn a_level_12_host_gets_no_approve_on_a_write_item() {
    let bridge = Bridge::new(12);
    let out = bridge.run(&["inbox", "answer", "i-9c0d1e2f", "approve"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("update muxa-bridge"), "{err}");
    assert!(bridge.sent("update-inbox").is_empty());
    let out = bridge.run(&["assist", "writes", "lisk"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("update muxa-bridge"));
    assert!(
        bridge.sent("list-inbox").len() == 1,
        "only approve's check listed"
    );
    // Other answers on a level 12 host go out as before.
    ok(&bridge.run(&["inbox", "answer", "i-9c0d1e2f", "later"]));
    assert_eq!(bridge.sent("update-inbox").len(), 1);
}

#[test]
fn assist_writes_lists_the_writer_and_each_write_with_its_state() {
    let bridge = Bridge::new(13);
    let text = ok(&bridge.run(&["assist", "writes", "lisk"]));
    assert!(
        text.starts_with("writer lisk-writer  review\n  Writer: posts approved text.\n"),
        "{text}"
    );
    assert!(
        text.contains(
            "q-5e6f7a8b v2  i-9c0d1e2f  Post to #release → slack:#release  Task t-00f001\n"
        ),
        "{text}"
    );
    assert!(
        text.contains("i-0d1e2f3a  Comment on ENG-12 → linear:ENG-12  rejected\n"),
        "{text}"
    );
    assert!(
        text.contains("i-1e2f3a4b  Post again → slack:#release  item deleted\n"),
        "{text}"
    );
    let home = ok(&bridge.run(&["assist", "writes", "home"]));
    assert!(home.contains("home has no writer"), "{home}");
    let json: Value =
        serde_json::from_str(ok(&bridge.run(&["assist", "writes", "lisk", "--json"])).trim())
            .unwrap();
    assert_eq!(
        (
            &json["writer"]["worker"],
            &json["writes"],
            json["proposed"].as_array().map(Vec::len)
        ),
        (&json!("lisk-writer"), &json!("review"), Some(3))
    );
}
