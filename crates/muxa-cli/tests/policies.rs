//! `policy`, `forget`, `plan reorder|dismiss`, and `confirm` and `reject
//! --reason` on Inbox items against a fake bridge at `work` level 10
//! (ADR 0048): what each command sends, and that a level 9 host gets nothing.
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Bridge(PathBuf);

impl Bridge {
    fn new(level: u32) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "muxa-cli-policies-{}-{}",
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

    fn command(&self, args: &[&str]) -> Command {
        let quote =
            |path: PathBuf| format!("'{}'", path.display().to_string().replace('\'', "'\\''"));
        let bridge = format!(
            "python3 {} {}",
            quote(self.0.join("bridge.py")),
            quote(self.0.join("requests"))
        );
        let mut command = Command::new(env!("CARGO_BIN_EXE_muxa-cli"));
        command.args(["--bridge", &bridge]).args(args);
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
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
        'createdAt':1,'updatedAt':1,'workspace':'work'}
offers = dict(work, id='w-7b8c9d0e', title='Compare vendor offers')
row = lambda **k: dict({'why':'Pinned by you.','next':'Fill in the table.',
                        'minutes':{'low':60,'high':120,'estimated':90}}, **k)
plan = {'id':'p-3d4e5f6a','rev':3,'at':1762774200,'inputsAt':1762774200,'delivery':'resulted',
        'deliveryAt':1,'workspaces':['work'],
        'top':[row(work='w-0b1c2d3e', basis=[
                   {'kind':'policy','text':'Recruiting first.','source':'y-0a1b2c3d'},
                   {'kind':'correction','text':'You pinned it.','source':'x-0a1b2c3d'},
                   {'kind':'hypothesis','text':'Due work first.','source':'h-0f0f0f0f'}]),
               row(work='w-7b8c9d0e'), row(item='i-1a2b3c4d', why='A capture waits.')],
        'userOrder':['w-7b8c9d0e','w-0b1c2d3e','i-1a2b3c4d'],'dismissed':['i-1a2b3c4d']}
pending = dict(plan, id='p-4e5f6a7b', rev=1, delivery='processing', top=[], userOrder=[], dismissed=[])
policy = {'id':'y-0a1b2c3d','rev':2,'text':'This week, recruiting comes before sales.','scope':'all',
          'until':4102444800,'source':'user','createdAt':1,'updatedAt':1}
expired = dict(policy, id='y-3d4e5f6a', text='Tax papers first.', scope='personal', until=1762300000)
hypothesis = {'id':'h-4e5f6a7b','text':'Work due this week comes first.','scope':'all',
              'evidence':['x-0a1b2c3d','x-2e3f4a5b'],'confidence':'low','state':'proposed',
              'plan':'p-3d4e5f6a','expiresAt':1763984400,'at':1762774800}
correction = {'id':'x-0a1b2c3d','kind':'pinned','at':1762690000,'plan':'p-0a1b2c3d',
              'row':'w-0b1c2d3e','inTop':True,'workspace':'work'}
dismissed = {'id':'x-3f4a5b6c','kind':'dismissed','at':1762775000,'plan':'p-3d4e5f6a',
             'row':'i-1a2b3c4d','inTop':True,'reason':'Waiting for the vendor.'}
item = {'id':'i-5e6f7a8b','kind':'hypothesis','state':'open','question':'Is this right? Work due this week comes first.',
        'workspace':'secretary','title':'Work due this week comes first.','createdAt':1,
        'hypothesis':{'hypothesis':'h-4e5f6a7b','text':'Work due this week comes first.','scope':'all',
                      'evidence':['x-0a1b2c3d','x-2e3f4a5b'],'confidence':'low'},
        'answers':['confirm','reject','later']}
sort = {'id':'i-1a2b3c4d','kind':'sort','state':'open','question':'Sort it?','workspace':'secretary',
        'title':'Try the tool','createdAt':1,'answers':['approve','reject','later']}
send({'type':'hello','protocol':1,'bridge':'fixture','tmux':'fixture',
      'capabilities':['task-records','work:LEVEL']})
for line in sys.stdin:
    req = json.loads(line)
    log.write(json.dumps(req) + '\n'); log.flush()
    kind, ident = req['type'], req['id']
    if kind == 'list-inbox':
        send({'type':'inbox-list','inbox':ident,'entries':[],'items':[item, sort],'work':[work, offers],
              'plans':[pending, plan],'policies':[policy, expired],'hypotheses':[hypothesis],
              'corrections':[dismissed, correction]})
    elif kind == 'list-routines':
        send({'type':'routine-list','routines':ident,'entries':[]})
    elif kind == 'add-policy':
        send({'type':'policy-saved','policy':ident,'entry':dict(policy, id='y-5f6a7b8c', rev=1,
              text=req['text'], scope=req['scope'], until=req.get('until'))})
    elif kind == 'update-policy' and req['action'] == 'edit':
        send({'type':'policy-saved','policy':ident,'entry':dict(policy, rev=3, text=req.get('text', policy['text']))})
    elif kind == 'update-plan':
        send({'type':'plan-saved','plan':ident,'entry':dict(plan, rev=4)})
    elif kind == 'update-inbox' and req['action'] == 'confirm':
        send({'type':'policy-saved','policy':ident,'entry':dict(policy, id='y-6a7b8c9d', source='h-4e5f6a7b')})
        send({'type':'inbox-saved','inbox':ident,'item':dict(item, state='answered', answer='confirm', answers=[])})
    elif kind == 'update-inbox':
        it = item if req['entry'] == item['id'] else sort
        send({'type':'inbox-saved','inbox':ident,'item':dict(it, state='answered', answer=req['action'], answers=[])})
    send({'type':'result','id':ident, **({'sessions':[]} if kind == 'list' else {})})
"#;

fn ok(out: &Output) -> String {
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout.clone()).unwrap()
}

#[test]
fn policy_lists_active_then_expired_then_hypotheses_and_corrections() {
    let bridge = Bridge::new(10);
    let text = ok(&bridge.run(&["policy"]));
    for part in [
        "Policies\ny-0a1b2c3d  all  until 2100-01-01T00:00Z  This week, recruiting comes before sales.\n",
        "Expired\ny-3d4e5f6a  personal  until 2025-11-04T23:46Z  expired  Tax papers first.\n",
        "Hypotheses\nh-4e5f6a7b  proposed  low  all  2 corrections  Work due this week comes first.\n",
        "Recent corrections\nx-3f4a5b6c  dismissed  i-1a2b3c4d  2025-11-10T11:43Z  Waiting for the vendor.\n",
        "x-0a1b2c3d  pinned  w-0b1c2d3e  ",
    ] {
        assert!(text.contains(part), "{part:?} in {text}");
    }
    let json: Value = serde_json::from_str(ok(&bridge.run(&["policy", "--json"])).trim()).unwrap();
    assert_eq!(json["policies"][1]["id"], "y-3d4e5f6a");
    assert_eq!(json["hypotheses"][0]["id"], "h-4e5f6a7b");
    assert_eq!(json["corrections"][0]["id"], "x-3f4a5b6c");
    let inbox: Value = serde_json::from_str(ok(&bridge.run(&["inbox", "--json"])).trim()).unwrap();
    assert_eq!(inbox["work"], 10);
    assert_eq!(inbox["policies"][0]["id"], "y-0a1b2c3d");
    assert_eq!(inbox["hypotheses"][0]["id"], "h-4e5f6a7b");
    assert_eq!(inbox["corrections"][1]["id"], "x-0a1b2c3d");
    assert!(bridge.changes().is_empty());
}

#[test]
fn policy_add_edit_and_delete_send_what_the_user_gave() {
    let bridge = Bridge::new(10);
    let text = ok(&bridge.run(&[
        "policy",
        "add",
        "Mornings are for the report.",
        "--scope",
        "work",
        "--from",
        "2030-11-10 09:00",
        "--until",
        "2030-11-16T14:59:59Z",
        "--timezone",
        "UTC",
    ]));
    assert!(
        text.starts_with("y-5f6a7b8c  work  until 2030-11-16T14:59Z  Mornings are for the report."),
        "{text}"
    );
    let mut child = bridge
        .command(&["policy", "add", "-", "--until", "week", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"Recruiting first.")
        .unwrap();
    let saved: Value = serde_json::from_str(ok(&child.wait_with_output().unwrap()).trim()).unwrap();
    assert_eq!(saved["text"], "Recruiting first.");
    ok(&bridge.run(&[
        "policy",
        "edit",
        "y-0a1b2c3d",
        "--text",
        "Recruiting and support first.",
    ]));
    let text = ok(&bridge.run(&["policy", "delete", "y-0a1b2c3d"]));
    assert_eq!(text, "deleted y-0a1b2c3d\n");
    let sent = bridge.changes();
    assert_eq!(sent.len(), 4);
    assert_eq!(
        sent[0],
        json!({"type": "add-policy", "id": sent[0]["id"], "text": "Mornings are for the report.",
            "scope": "work", "from": 1_920_531_600, "until": 1_921_071_599})
    );
    assert_eq!(sent[1]["scope"], "all");
    assert!(sent[1]["until"].as_u64().unwrap() < sent[0]["until"].as_u64().unwrap());
    assert!(sent[1].get("from").is_none());
    assert_eq!(
        sent[2],
        json!({"type": "update-policy", "id": sent[2]["id"], "policy": "y-0a1b2c3d", "rev": 2,
            "action": "edit", "text": "Recruiting and support first."})
    );
    assert_eq!(
        sent[3],
        json!({"type": "update-policy", "id": sent[3]["id"], "policy": "y-0a1b2c3d", "rev": 2,
            "action": "delete"})
    );
}

#[test]
fn plan_reorder_and_dismiss_change_the_newest_plan_with_a_result() {
    let bridge = Bridge::new(10);
    let text = ok(&bridge.run(&["plan"]));
    for part in [
        "1. w-7b8c9d0e  Compare vendor offers  60-120 min, about 90\n",
        "2. w-0b1c2d3e  Prepare the quarterly report  60-120 min, about 90\n",
        "   Based on: policy y-0a1b2c3d  This week, recruiting comes before sales.\n",
        "   Based on: correction x-0a1b2c3d  pinned w-0b1c2d3e\n",
        "   Based on: hypothesis h-0f0f0f0f  forgotten\n",
        "3. i-1a2b3c4d  Try the tool  not today\n",
    ] {
        assert!(text.contains(part), "{part:?} in {text}");
    }
    let text = ok(&bridge.run(&["plan", "reorder", "w-0b1c2d3e", "i-1a2b3c4d", "w-7b8c9d0e"]));
    assert!(text.starts_with("p-3d4e5f6a  resulted  "), "{text}");
    ok(&bridge.run(&[
        "plan",
        "dismiss",
        "w-7b8c9d0e",
        "--reason",
        "Waiting for the vendor.",
    ]));
    ok(&bridge.run(&["plan", "dismiss", "w-0b1c2d3e", "--json"]));
    let sent = bridge.changes();
    assert_eq!(sent.len(), 3);
    assert_eq!(
        sent[0],
        json!({"type": "update-plan", "id": sent[0]["id"], "plan": "p-3d4e5f6a", "rev": 3,
            "action": "reorder", "order": ["w-0b1c2d3e", "i-1a2b3c4d", "w-7b8c9d0e"]})
    );
    assert_eq!(
        sent[1],
        json!({"type": "update-plan", "id": sent[1]["id"], "plan": "p-3d4e5f6a", "rev": 3,
            "action": "dismiss", "row": "w-7b8c9d0e", "text": "Waiting for the vendor."})
    );
    assert!(sent[2].get("text").is_none());
}

#[test]
fn hypothesis_items_confirm_and_reject_with_a_reason() {
    let bridge = Bridge::new(10);
    let text = ok(&bridge.run(&["inbox"]));
    assert!(
        text.contains("i-5e6f7a8b  hypothesis  open unread  "),
        "{text}"
    );
    let text = ok(&bridge.run(&["inbox", "answer", "i-5e6f7a8b", "confirm"]));
    assert!(text.contains("answered: confirm"), "{text}");
    ok(&bridge.run(&[
        "inbox",
        "answer",
        "i-5e6f7a8b",
        "reject",
        "--reason",
        "Sales still matters on Mondays.",
    ]));
    ok(&bridge.run(&["inbox", "answer", "i-1a2b3c4d", "reject"]));
    let text = ok(&bridge.run(&["forget", "x-0a1b2c3d"]));
    assert_eq!(text, "forgot x-0a1b2c3d\n");
    let sent = bridge.changes();
    assert_eq!(sent.len(), 4);
    assert_eq!(
        (sent[0]["action"].as_str(), sent[0]["entry"].as_str()),
        (Some("confirm"), Some("i-5e6f7a8b"))
    );
    assert_eq!(sent[1]["text"], "Sales still matters on Mondays.");
    assert!(sent[2].get("text").is_none());
    assert_eq!(
        sent[3],
        json!({"type": "forget", "id": sent[3]["id"], "record": "x-0a1b2c3d"})
    );
}

#[test]
fn wrong_arguments_and_a_level_9_host_send_nothing() {
    let bridge = Bridge::new(10);
    let long = "x".repeat(501);
    for args in [
        &["policy", "add", "two\nlines"][..],
        &["policy", "add", &long],
        &["policy", "add", "x", "--until", "Sunday"],
        &[
            "policy",
            "add",
            "x",
            "--from",
            "2025-11-16 09:00",
            "--until",
            "2025-11-10 09:00",
        ],
        &["policy", "add", "x", "--text", "y"],
        &["policy", "add", "x", "--timezone", "UTC"],
        &["policy", "edit", "y-0a1b2c3d"],
        &["policy", "edit", "y-0000ffff", "--text", "x"],
        &["policy", "delete", "y-0000ffff"],
        &["plan", "reorder", "w-0b1c2d3e", "w-7b8c9d0e"],
        &["plan", "reorder", "w-0b1c2d3e", "w-0b1c2d3e", "w-7b8c9d0e"],
        &["plan", "dismiss", "w-0000ffff"],
        &["plan", "dismiss", "w-0b1c2d3e", "--reason", &long],
        &["plan", "--reason", "x"],
        &["forget", "y-0a1b2c3d"],
        &["inbox", "answer", "i-5e6f7a8b", "confirm", "--reason", "x"],
        &["work", "pin", "w-0b1c2d3e", "--from", "2025-11-10 09:00"],
    ] {
        let out = bridge.run(args);
        assert!(!out.status.success(), "{args:?}");
    }
    assert!(bridge.changes().is_empty());
    let old = Bridge::new(9);
    for args in [
        &["policy"][..],
        &["policy", "add", "Recruiting first."],
        &["policy", "edit", "y-0a1b2c3d", "--text", "x"],
        &["policy", "delete", "y-0a1b2c3d"],
        &["plan", "reorder", "w-7b8c9d0e", "w-0b1c2d3e", "i-1a2b3c4d"],
        &["plan", "dismiss", "w-0b1c2d3e"],
        &["forget", "x-0a1b2c3d"],
        &["inbox", "answer", "i-5e6f7a8b", "confirm"],
        &["inbox", "answer", "i-1a2b3c4d", "reject", "--reason", "x"],
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
