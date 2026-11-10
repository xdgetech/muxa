//! `plan`, `plan now`, `work pin|unpin|due`, `assist notes` and the answers
//! to `sort` and `delegate` items against a fake bridge at `work` level 10
//! (ADR 0049): what each command sends, and that a level 9 host gets nothing.
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Bridge(PathBuf);

impl Bridge {
    /// `overall`: whether the host has an overall workspace for `plan-now`.
    fn new(level: u32, overall: bool) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "muxa-cli-plans-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let script = BRIDGE
            .replace("LEVEL", &level.to_string())
            .replace("OVERALL", if overall { "True" } else { "False" });
        fs::write(dir.join("bridge.py"), script).unwrap();
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

    /// The requests that change something: not the lists.
    fn changes(&self) -> Vec<Value> {
        fs::read_to_string(self.0.join("requests"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|r| {
                !["list-inbox", "list", "list-routines"].contains(&r["type"].as_str().unwrap())
            })
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
work = {'id':'w-0b1c2d3e','rev':4,'title':'Prepare the quarterly report','state':'open',
        'createdAt':1,'updatedAt':1,'workspace':'work','requests':['q-1b2c3d4e']}
later = dict(work, id='w-8c9d0e1f', title='Renew the passport', laterAt=5, requests=[])
notes = {'text':'# Work\n\n- Reports go out on Wednesday.\n','base':'ab' * 32}
request = {'id':'q-1b2c3d4e','work':'w-0b1c2d3e','version':3,'workspace':'work','scope':'research',
           'delivery':'resulted','deliveryAt':1,'createdAt':1,'updatedAt':1,
           'results':[{'version':3,'at':9,'summary':'Done.','notes':notes}]}
row = lambda **k: dict({'why':'Pinned by you.','next':'Fill in the table.',
                        'minutes':{'low':60,'high':120,'estimated':90}}, **k)
plan = {'id':'p-0a1b2c3d','rev':2,'at':1762687800,'inputsAt':1762687800,'delivery':'resulted',
        'deliveryAt':1,'workspaces':['work'],
        'top':[row(work='w-0b1c2d3e'), row(item='i-1a2b3c4d', why='A capture waits.')],
        'dueSoon':['w-8c9d0e1f'],'stale':['q-1b2c3d4e'],
        'changed':[{'id':'w-0b1c2d3e','change':'entered'}],
        'notes':{'text':'# Overall\n','base':'cd' * 32}}
failed = dict(plan, id='p-2c3d4e5f', delivery='failed', top=[], dueSoon=[], stale=[], changed=[],
              notes=None, reason='The run did not start: cannot reach tmux on the host')
item = lambda id, kind, title: {'id':id,'kind':kind,'state':'open','question':'Approve? ' + title,
              'workspace':'secretary','title':title,'createdAt':1,'answers':['approve','reject','later']}
items = [item('i-1a2b3c4d','sort','Try the tool for business B'),
         item('i-2b3c4d5e','delegate','Compare vendor offers')]
capture = {'id':'c-4e5f6a7b','rev':2,'clientId':'x','text':'Try the tool','source':'app',
           'state':'sorted','createdAt':1,'updatedAt':1}
send({'type':'hello','protocol':1,'bridge':'fixture','tmux':'fixture',
      'capabilities':['task-records','work:LEVEL']})
for line in sys.stdin:
    req = json.loads(line)
    log.write(json.dumps(req) + '\n'); log.flush()
    kind, ident = req['type'], req['id']
    if kind == 'list-inbox':
        send({'type':'inbox-list','inbox':ident,'entries':[],'items':items,'work':[work, later],
              'requests':[request],'plans':[failed, plan],
              'assistant':{'overall':'secretary','delegates':[]}})
    elif kind == 'list-routines':
        send({'type':'routine-list','routines':ident,'entries':[]})
    elif kind == 'plan-now' and not OVERALL:
        send({'type':'result','id':ident,'error':{'code':'unsupported',
              'message':'no workspace is the overall assistant; set [assistant] overall'}})
        continue
    elif kind == 'plan-now':
        send({'type':'plan-saved','plan':ident,'entry':{'id':'p-1b2c3d4e','rev':1,'at':1762700400,
              'inputsAt':1762700400,'delivery':'pending','deliveryAt':1,'minutes':req.get('minutes')}})
    elif kind == 'update-work':
        send({'type':'work-saved','work':ident,'entry':dict(work, rev=5, pinnedAt=9)})
    elif kind == 'apply-notes' and req['source'].startswith('p-'):
        send({'type':'plan-saved','plan':ident,'entry':dict(plan, rev=3)})
    elif kind == 'apply-notes':
        send({'type':'work-saved','work':ident,'entry':work,'request':request})
    elif kind == 'update-inbox':
        it = [i for i in items if i['id'] == req['entry']][0]
        if it['kind'] == 'sort':
            send({'type':'capture-saved','capture':ident,'entry':capture,'work':dict(work, id='w-1a1a1a1a')})
        else:
            send({'type':'work-saved','work':ident,'entry':work,'request':request})
        send({'type':'inbox-saved','inbox':ident,'item':dict(it, state='answered', answer='approve', answers=[])})
    send({'type':'result','id':ident, **({'sessions':[]} if kind == 'list' else {})})
"#;

fn ok(out: &Output) -> String {
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout.clone()).unwrap()
}

#[test]
fn plan_shows_the_failed_newest_plan_then_the_last_rows() {
    let bridge = Bridge::new(10, true);
    let text = ok(&bridge.run(&["plan"]));
    for part in [
        "p-2c3d4e5f  failed  ",
        "  The run did not start: cannot reach tmux on the host\n",
        "\nLast plan with a result\np-0a1b2c3d  resulted  2025-11-09T11:30Z  [work]\n",
        "1. w-0b1c2d3e  Prepare the quarterly report  60-120 min, about 90  new\n",
        "   Why: Pinned by you.\n   Next: Fill in the table.\n",
        "2. i-1a2b3c4d  Try the tool for business B  60-120 min, about 90\n",
        "Due soon, moved down by you: w-8c9d0e1f Renew the passport\n",
        "Stale results: q-1b2c3d4e\n",
    ] {
        assert!(text.contains(part), "{part:?} in {text}");
    }
    let json: Value = serde_json::from_str(ok(&bridge.run(&["plan", "--json"])).trim()).unwrap();
    assert_eq!(json["id"], "p-2c3d4e5f");
    let inbox: Value = serde_json::from_str(ok(&bridge.run(&["inbox", "--json"])).trim()).unwrap();
    assert_eq!(inbox["plans"][1]["id"], "p-0a1b2c3d");
    assert_eq!(inbox["assistant"]["overall"], "secretary");
    assert!(bridge.changes().is_empty());
}

#[test]
fn plan_now_sends_the_minutes_and_prints_the_refusal() {
    let bridge = Bridge::new(10, true);
    let text = ok(&bridge.run(&["plan", "now", "--minutes", "45"]));
    assert!(text.starts_with("p-1b2c3d4e  pending  "), "{text}");
    assert!(text.trim_end().ends_with("(45 min)"), "{text}");
    ok(&bridge.run(&["plan", "now"]));
    let sent = bridge.changes();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0]["type"], "plan-now");
    assert_eq!(sent[0]["minutes"], 45);
    assert!(sent[1].get("minutes").is_none());

    let none = Bridge::new(10, false);
    let out = none.run(&["plan", "now"]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("unsupported: no workspace is the overall assistant"),
        "{err}"
    );
}

#[test]
fn pin_unpin_and_due_send_the_rev_they_read() {
    let bridge = Bridge::new(10, true);
    let text = ok(&bridge.run(&["work", "pin", "w-0b1c2d3e"]));
    assert!(text.starts_with("w-0b1c2d3e  open pinned  "), "{text}");
    ok(&bridge.run(&["work", "unpin", "w-0b1c2d3e"]));
    ok(&bridge.run(&[
        "work",
        "due",
        "w-0b1c2d3e",
        "--at",
        "2025-11-12 16:00",
        "--timezone",
        "Asia/Tokyo",
    ]));
    ok(&bridge.run(&[
        "work",
        "due",
        "w-0b1c2d3e",
        "--at",
        "2025-11-12T07:00:00Z",
        "--timezone",
        "UTC",
    ]));
    let sent = bridge.changes();
    assert_eq!(sent.len(), 4);
    assert_eq!(
        (sent[0]["action"].as_str(), sent[0]["rev"].as_u64()),
        (Some("pin"), Some(4))
    );
    assert_eq!(sent[1]["action"], "unpin");
    assert_eq!(sent[2]["action"], "edit");
    assert_eq!(
        sent[2]["due"],
        serde_json::json!({"at": 1_762_930_800, "timezone": "Asia/Tokyo"})
    );
    assert!(sent[2].get("title").is_none());
    assert_eq!(sent[3]["due"]["at"], 1_762_930_800);
}

#[test]
fn assist_notes_shows_the_proposal_and_applies_it_with_its_base() {
    let bridge = Bridge::new(10, true);
    let text = ok(&bridge.run(&["assist", "notes", "w-0b1c2d3e"]));
    assert_eq!(
        text,
        "q-1b2c3d4e v3 proposes NOTES.md (base abababababab)\n# Work\n\n- Reports go out on Wednesday.\n"
    );
    assert!(bridge.changes().is_empty(), "showing sends nothing");
    let text = ok(&bridge.run(&["assist", "notes", "q-1b2c3d4e", "--apply"]));
    assert_eq!(
        text.trim(),
        "notes applied: q-1b2c3d4e v3 research  resulted  [work]"
    );
    let text = ok(&bridge.run(&["assist", "notes", "p-0a1b2c3d", "--apply"]));
    assert!(
        text.starts_with("notes applied: p-0a1b2c3d  resulted"),
        "{text}"
    );
    let sent = bridge.changes();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0]["type"], "apply-notes");
    assert_eq!(
        (sent[0]["source"].as_str(), sent[0]["version"].as_u64()),
        (Some("q-1b2c3d4e"), Some(3))
    );
    assert_eq!(sent[0]["base"], "ab".repeat(32));
    assert_eq!(
        (sent[1]["source"].as_str(), sent[1]["version"].as_u64()),
        (Some("p-0a1b2c3d"), Some(2))
    );
    assert_eq!(sent[1]["base"], "cd".repeat(32));
    let out = bridge.run(&["assist", "notes", "p-2c3d4e5f", "--apply"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("proposes no notes"));
    assert_eq!(bridge.changes().len(), 2);
}

#[test]
fn sort_and_delegate_items_list_and_approve_like_other_items() {
    let bridge = Bridge::new(10, true);
    let text = ok(&bridge.run(&["inbox"]));
    assert!(text.contains("i-1a2b3c4d  sort  open unread  "), "{text}");
    assert!(
        text.contains("i-2b3c4d5e  delegate  open unread  "),
        "{text}"
    );
    for id in ["i-1a2b3c4d", "i-2b3c4d5e"] {
        let text = ok(&bridge.run(&["inbox", "answer", id, "approve"]));
        assert!(
            text.contains(&format!("{id}  ")) && text.contains("answered: approve"),
            "{text}"
        );
    }
    let sent = bridge.changes();
    assert_eq!(sent.len(), 2);
    assert!(
        sent.iter()
            .all(|r| r["type"] == "update-inbox" && r["action"] == "approve")
    );
}

#[test]
fn wrong_arguments_and_a_level_9_host_send_nothing() {
    let bridge = Bridge::new(10, true);
    for args in [
        &["plan", "--minutes", "5"][..],
        &["plan", "now", "--minutes", "x"],
        &["work", "due", "w-0b1c2d3e"],
        &[
            "work",
            "due",
            "w-0b1c2d3e",
            "--at",
            "Wednesday",
            "--timezone",
            "UTC",
        ],
        &[
            "work",
            "due",
            "w-0b1c2d3e",
            "--at",
            "2025-11-12 16:00",
            "--timezone",
            "Mars/Base",
        ],
        &[
            "work",
            "due",
            "w-0b1c2d3e",
            "--at",
            "2025-11-12 16:00",
            "--title",
            "x",
        ],
        &["work", "pin", "w-0b1c2d3e", "--at", "2025-11-12 16:00"],
        &["work", "pin", "w-0000ffff"],
        &["assist", "show", "w-0b1c2d3e", "--apply"],
        &["assist", "notes", "w-0000ffff"],
    ] {
        let out = bridge.run(args);
        assert!(!out.status.success(), "{args:?}");
    }
    assert!(bridge.changes().is_empty());
    let old = Bridge::new(9, true);
    for args in [
        &["plan"][..],
        &["plan", "now"],
        &["work", "pin", "w-0b1c2d3e"],
        &["work", "unpin", "w-0b1c2d3e"],
        &[
            "work",
            "due",
            "w-0b1c2d3e",
            "--at",
            "2025-11-12T07:00:00Z",
            "--timezone",
            "UTC",
        ],
        &["assist", "notes", "w-0b1c2d3e", "--apply"],
    ] {
        let out = old.run(args);
        assert!(!out.status.success(), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("update muxa-bridge"),
            "{args:?}"
        );
    }
    assert!(old.changes().is_empty());
}
