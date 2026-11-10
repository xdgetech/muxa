//! A newer host lists values this CLI does not know: an Inbox item kind, a
//! Work state, a delivery, a basis kind. `inbox`, `work` and `plan` still
//! print the list and show each new value as `unknown`.
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

const BRIDGE: &str = r#"
import json, sys
def send(value):
    print(json.dumps(value), flush=True)
work = {'id':'w-0b1c2d3e','rev':4,'title':'Prepare the quarterly report','state':'blocked',
        'createdAt':1,'updatedAt':1,'workspace':'work','requests':['q-1b2c3d4e']}
request = {'id':'q-1b2c3d4e','work':'w-0b1c2d3e','version':1,'workspace':'work','scope':'summarize',
           'delivery':'queued','deliveryAt':1,'createdAt':1,'updatedAt':1}
row = {'work':'w-0b1c2d3e','why':'The calendar says so.','next':'Fill in the table.',
       'minutes':{'low':60,'high':120,'estimated':90},
       'basis':[{'kind':'calendar','text':'Review on Friday','source':'k-0a1b2c3d'}]}
newest = {'id':'p-2c3d4e5f','rev':1,'at':1762687800,'inputsAt':1762687800,'delivery':'queued',
          'deliveryAt':1}
plan = {'id':'p-0a1b2c3d','rev':2,'at':1762687800,'inputsAt':1762687800,'delivery':'resulted',
        'deliveryAt':1,'workspaces':['work'],'top':[row],'changed':[{'id':'w-0b1c2d3e','change':'split'}]}
item = {'id':'i-1a2b3c4d','kind':'reminder','state':'open','question':'Renew it?',
        'workspace':'secretary','title':'Renew the passport','createdAt':1,
        'answers':['snooze','later']}
capture = {'id':'c-4e5f6a7b','rev':1,'clientId':'x','text':'From a mail','source':'mail',
           'state':'received','createdAt':1,'updatedAt':1}
correction = {'id':'x-0c1d2e3f','kind':'snoozed','at':1,'plan':'p-0a1b2c3d','inTop':True}
send({'type':'hello','protocol':1,'bridge':'fixture','tmux':'fixture',
      'capabilities':['task-records','work:11']})
for line in sys.stdin:
    req = json.loads(line)
    kind, ident = req['type'], req['id']
    if kind == 'list-inbox':
        send({'type':'inbox-list','inbox':ident,'entries':[],'items':[item],'work':[work],
              'captures':[capture],'requests':[request],'plans':[newest, plan],
              'assistant':{'overall':'secretary','delegates':[
                  {'workspace':'work','share':'everything','overallRequests':'ask-first'}]},
              'corrections':[correction]})
    elif kind == 'list-routines':
        send({'type':'routine-list','routines':ident,'entries':[]})
    send({'type':'result','id':ident, **({'sessions':[]} if kind == 'list' else {})})
"#;

struct Bridge(PathBuf);

impl Bridge {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("muxa-cli-newer-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("bridge.py"), BRIDGE).unwrap();
        Self(dir)
    }

    fn run(&self, args: &[&str]) -> Output {
        let script = self.0.join("bridge.py").display().to_string();
        let command = format!("python3 '{}'", script.replace('\'', "'\\''"));
        Command::new(env!("CARGO_BIN_EXE_muxa-cli"))
            .args(["--bridge", &command])
            .args(args)
            .output()
            .unwrap()
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn ok(out: &Output) -> String {
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout.clone()).unwrap()
}

#[test]
fn inbox_work_and_plan_show_a_newer_hosts_values_as_unknown() {
    let bridge = Bridge::new();
    let inbox = ok(&bridge.run(&["inbox"]));
    assert!(
        inbox.contains("i-1a2b3c4d  unknown  open unread  "),
        "{inbox}"
    );
    let work = ok(&bridge.run(&["work"]));
    assert!(
        work.contains("w-0b1c2d3e  unknown  ") && work.contains("Prepare the quarterly report"),
        "{work}"
    );
    let plan = ok(&bridge.run(&["plan"]));
    for part in [
        "p-2c3d4e5f  unknown  ",
        "\nLast plan with a result\np-0a1b2c3d  resulted  ",
        "1. w-0b1c2d3e  Prepare the quarterly report  60-120 min, about 90\n",
        "   Why: The calendar says so.\n",
    ] {
        assert!(plan.contains(part), "{part:?} in {plan}");
    }
    let json = ok(&bridge.run(&["inbox", "--json"]));
    assert!(json.contains("\"kind\":\"unknown\""), "{json}");
}
