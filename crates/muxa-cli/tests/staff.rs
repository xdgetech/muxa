//! `assist WORK --worker`, `assist staff`, step rows, and `handoff` items
//! against a fake bridge at `work` level 12 (ADR 0051): what each command
//! sends, and that a level 11 host gets nothing.
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
            "muxa-cli-staff-{}-{}",
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

    /// Every request the CLI sent.
    fn requests(&self) -> Vec<Value> {
        fs::read_to_string(self.0.join("requests"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
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
work = {'id':'w-8b9c0d1e','rev':6,'title':'Decide the Q1 roadmap','state':'in-progress',
        'createdAt':1,'updatedAt':1,'workspace':'lisk','requests':['q-2b3c4d5e']}
step = lambda n, w, by, d, **k: dict({'step':n,'worker':w,'by':by,'delivery':d,'deliveryAt':1}, **k)
result = lambda n, w, s: {'version':2,'at':1762795000 - n,'summary':s,'worker':w,'step':n}
request = {'id':'q-2b3c4d5e','work':'w-8b9c0d1e','version':2,'workspace':'lisk','scope':'compare',
           'delivery':'resulted','deliveryAt':1,'createdAt':1,'updatedAt':1,
           'results':[result(2, 'lisk-strategist', 'Option B fits.'),
                      result(1, 'lisk-chief', 'Two options remain.'),
                      {'version':1,'at':1,'summary':'Older.','stale':True}],
           'steps':[step(1, 'lisk-chief', 'lead', 'resulted',
                         handoff={'worker':'lisk-strategist','purpose':'Compare them.'}),
                    step(2, 'lisk-strategist', 'handoff', 'resulted',
                         handoff={'worker':'lisk-architect','purpose':'Check the gateway.'})]}
handoff = {'id':'i-7a8b9c0d','kind':'handoff','state':'open',
           'question':'lisk-strategist asks lisk-architect to: Check the gateway.',
           'workspace':'lisk','title':'Decide the Q1 roadmap','createdAt':1,
           'handoff':{'request':'q-2b3c4d5e','version':2,'step':2,'work':'w-8b9c0d1e',
                      'from':'lisk-strategist','worker':'lisk-architect','purpose':'Check the gateway.'},
           'answers':['approve','reject','later']}
staff = [{'worker':'lisk-chief','description':'Chief of staff.','lead':True,'notesAt':1},
         {'worker':'lisk-strategist','description':'Strategist.'}]
assistant = {'delegates':[{'workspace':'lisk','share':'summaries','overallRequests':'review','staff':staff}]}
send({'type':'hello','protocol':1,'bridge':'fixture','tmux':'fixture',
      'capabilities':['task-records','work:LEVEL']})
for line in sys.stdin:
    req = json.loads(line)
    log.write(json.dumps(req) + '\n'); log.flush()
    kind, ident = req['type'], req['id']
    if kind == 'list-inbox':
        send({'type':'inbox-list','inbox':ident,'entries':[],'items':[handoff],'work':[work],
              'requests':[request],'assistant':assistant})
    elif kind == 'list-routines':
        send({'type':'routine-list','routines':ident,'entries':[]})
    elif kind == 'request-assistant':
        first = step(1, req.get('worker') or 'lisk-chief', 'user' if req.get('worker') else 'lead', 'pending')
        send({'type':'work-saved','work':ident,'entry':dict(work, rev=7),
              'request':dict(request, version=3, delivery='pending', results=[], steps=[first])})
    elif kind == 'get-work':
        send({'type':'work-detail','work':ident,'entry':work,'captures':[],'requests':[request]})
    elif kind == 'update-inbox':
        send({'type':'work-saved','work':ident,'entry':work,
              'request':dict(request, delivery='pending', steps=request['steps'] +
                             [step(3, 'lisk-architect', 'handoff', 'pending')])})
        send({'type':'inbox-saved','inbox':ident,
              'item':dict(handoff, state='answered', answer=req['action'], answers=[])})
    send({'type':'result','id':ident})
"#;

fn ok(out: &Output) -> String {
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout.clone()).unwrap()
}

fn sent(bridge: &Bridge, kind: &str) -> Vec<Value> {
    bridge
        .requests()
        .into_iter()
        .filter(|r| r["type"] == kind)
        .collect()
}

#[test]
fn assist_worker_asks_that_member_and_without_it_the_lead() {
    let bridge = Bridge::new(12);
    let text = ok(&bridge.run(&["assist", "w-8b9c0d1e", "--worker", "lisk-strategist"]));
    assert!(
        text.contains("q-2b3c4d5e v3 compare  pending  [lisk]"),
        "{text}"
    );
    assert!(
        text.contains("1. lisk-strategist  pending  asked by you"),
        "{text}"
    );
    ok(&bridge.run(&["assist", "w-8b9c0d1e"]));
    let asked = sent(&bridge, "request-assistant");
    assert_eq!(asked[0]["worker"], "lisk-strategist");
    assert_eq!(asked[0]["rev"], 6);
    assert!(asked[1].get("worker").is_none(), "{}", asked[1]);
}

#[test]
fn a_level_11_host_gets_no_worker_and_nothing_is_sent() {
    let bridge = Bridge::new(11);
    let out = bridge.run(&["assist", "w-8b9c0d1e", "--worker", "lisk-strategist"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("update muxa-bridge"), "{err}");
    assert!(bridge.requests().is_empty(), "{:?}", bridge.requests());
    let out = bridge.run(&["assist", "staff", "lisk"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("update muxa-bridge"));
    assert!(bridge.requests().is_empty());
}

#[test]
fn worker_needs_a_command_that_takes_it() {
    let bridge = Bridge::new(12);
    let out = bridge.run(&["assist", "show", "w-8b9c0d1e", "--worker", "x"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--worker requires"));
}

#[test]
fn assist_staff_lists_the_members_and_the_lead() {
    let bridge = Bridge::new(12);
    let text = ok(&bridge.run(&["assist", "staff", "lisk"]));
    assert!(text.starts_with("lisk-chief  lead  notes "), "{text}");
    assert!(text.contains("\n  Chief of staff.\nlisk-strategist  no notes\n  Strategist.\n"));
    let json: Value =
        serde_json::from_str(ok(&bridge.run(&["assist", "staff", "lisk", "--json"])).trim())
            .unwrap();
    assert_eq!(json[0]["lead"], true);
    let out = bridge.run(&["assist", "staff", "home"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("takes no assistant requests"));
}

#[test]
fn show_lists_each_step_then_each_steps_result_newest_first() {
    let bridge = Bridge::new(12);
    let text = ok(&bridge.run(&["assist", "show", "q-2b3c4d5e"]));
    let at = |s: &str| text.find(s).unwrap_or_else(|| panic!("{s} not in {text}"));
    assert!(at("1. lisk-chief  resulted  the lead took it") < at("2. lisk-strategist"));
    assert!(text.contains("hand-off to lisk-architect: Check the gateway."));
    assert!(at("Result v2 · step 2, lisk-strategist") < at("Result v2 · step 1, lisk-chief"));
    assert!(!text.contains("Older."), "{text}");
    let work = ok(&bridge.run(&["work", "show", "w-8b9c0d1e"]));
    assert!(
        work.contains("  2. lisk-strategist  resulted  handed on"),
        "{work}"
    );
}

#[test]
fn a_handoff_item_is_listed_and_approved_through_inbox_answer() {
    let bridge = Bridge::new(12);
    let text = ok(&bridge.run(&["inbox"]));
    assert!(
        text.contains("i-7a8b9c0d  handoff  open unread  "),
        "{text}"
    );
    assert!(text.contains("lisk-strategist asks lisk-architect to: Check the gateway."));
    let answered = ok(&bridge.run(&["inbox", "answer", "i-7a8b9c0d", "approve"]));
    assert!(answered.contains("answered: approve"), "{answered}");
    let answer = &sent(&bridge, "update-inbox")[0];
    assert_eq!(
        (&answer["entry"], &answer["action"]),
        (&json!("i-7a8b9c0d"), &json!("approve"))
    );
}
