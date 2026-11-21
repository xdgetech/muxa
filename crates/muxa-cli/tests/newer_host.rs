//! A newer host lists an Inbox item kind this CLI does not know, and an
//! older one still sends the lists of the removed assistant layer (ADR 0066).
//! `inbox` prints the list, shows the new kind as `unknown`, and ignores the rest.
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

const BRIDGE: &str = r#"
import json, sys
def send(value):
    print(json.dumps(value), flush=True)
item = {'id':'i-1a2b3c4d','kind':'reminder','state':'open','question':'Renew it?',
        'workspace':'secretary','title':'Renew the passport','createdAt':1,
        'answers':['snooze','later']}
send({'type':'hello','protocol':1,'bridge':'fixture','tmux':'fixture',
      'capabilities':['task-records','work:12']})
for line in sys.stdin:
    req = json.loads(line)
    kind, ident = req['type'], req['id']
    if kind == 'list-inbox':
        send({'type':'inbox-list','inbox':ident,'entries':[],'items':[item],
              'work':[{'id':'w-0b1c2d3e','state':'open'}],'plans':[{'id':'p-0a1b2c3d'}],
              'assistant':{'overall':'secretary'},'corrections':[]})
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
fn inbox_shows_a_newer_kind_as_unknown_and_ignores_the_assistant_lists() {
    let bridge = Bridge::new();
    let inbox = ok(&bridge.run(&["inbox"]));
    assert!(
        inbox.contains("i-1a2b3c4d  unknown  open unread  "),
        "{inbox}"
    );
    let json = ok(&bridge.run(&["inbox", "--json"]));
    assert!(json.contains("\"kind\":\"unknown\""), "{json}");
    assert!(!json.contains("workItems"), "{json}");
}
