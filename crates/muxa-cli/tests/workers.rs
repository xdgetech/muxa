//! Exercise CLI intent on the wire without a tmux server or native agent.
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
            "muxa-cli-workers-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("bridge.py"), BRIDGE).unwrap();
        Self(dir)
    }

    fn run(&self, workers: bool, conflict: bool, args: &[&str]) -> Output {
        self.run_recovery(workers, false, conflict, args)
    }

    fn run_recovery(&self, workers: bool, recovery: bool, conflict: bool, args: &[&str]) -> Output {
        let quote =
            |path: PathBuf| format!("'{}'", path.display().to_string().replace('\'', "'\\''"));
        let command = format!(
            "python3 {} {} {} {} {}",
            quote(self.0.join("bridge.py")),
            quote(self.0.join("requests")),
            u8::from(workers),
            u8::from(conflict),
            u8::from(recovery)
        );
        Command::new(env!("CARGO_BIN_EXE_muxa-cli"))
            .args(["--bridge", &command])
            .args(args)
            .output()
            .unwrap()
    }

    fn requests(&self) -> Vec<Value> {
        fs::read_to_string(self.0.join("requests"))
            .unwrap()
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
capture, workers, conflict = sys.argv[1], sys.argv[2] == '1', sys.argv[3] == '1'
recovery = sys.argv[4] == '1'
log = open(capture, 'w')
def send(value):
    print(json.dumps(value), flush=True)
record = {'id':'t-000001','rev':7,'workspace':'work','title':'Fixture','goal':'Wait',
          'status':'open','createdAt':1,'updatedAt':1,'runs':[], 'worker':'Reviewer'}
if recovery:
    record['restartOperation'] = {'id':'restart-fixture','phase':'replaced',
        'worker':'Reviewer',
        'actions':['finish-replacement','leave-unconfirmed','future-action']}
send({'type':'hello','protocol':1,'bridge':'fixture','tmux':'fixture',
      'capabilities':['task-records','task-handoff'] + (['workers'] if workers else [])
          + (['task-restart-recovery'] if recovery else [])})
for line in sys.stdin:
    req = json.loads(line)
    log.write(json.dumps(req) + '\n'); log.flush()
    kind, ident = req['type'], req['id']
    if kind == 'list-tasks':
        send({'type':'task-list','tasks':ident,
              'workspaces':[{'name':'work','root':'/w','repos':['/w/app','/x/tool']}],
              'entries':[record], 'workers':[{'name':'Reviewer','agent':'codex'}]})
    elif kind in ('restart-agent','create-task','recover-restart'):
        if conflict:
            send({'type':'result','id':ident,'error':{'code':'conflict','message':'fixture changed'}})
            continue
        saved = dict(record, rev=8)
        if kind == 'recover-restart': saved.pop('restartOperation', None)
        if 'worker' in req:
            if req['worker'] == 'default': saved.pop('worker', None)
            else: saved['worker'] = req['worker']
        send({'type':'task-saved','task':ident,'record':saved})
    send({'type':'result','id':ident, **({'sessions':[]} if kind == 'list' else {})})
"#;

#[test]
fn workers_restart_sends_keep_default_and_named_intent_with_revision() {
    for (flags, expected) in [
        (vec![], None),
        (vec!["--worker", "default"], Some(json!("default"))),
        (vec!["--worker", "Reviewer"], Some(json!("Reviewer"))),
    ] {
        let bridge = Bridge::new();
        let mut args = vec!["task", "restart", "t-000001"];
        args.extend(flags);
        let output = bridge.run(true, false, &args);
        assert!(output.status.success(), "{:?}", output);
        let requests = bridge.requests();
        assert_eq!(requests[0]["type"], "list-tasks");
        assert_eq!(requests[1]["type"], "restart-agent");
        assert_eq!(requests[1]["rev"], 7);
        assert_eq!(requests[1].get("worker"), expected.as_ref());
    }
}

#[test]
fn legacy_restart_omits_worker_and_revision() {
    let bridge = Bridge::new();
    assert!(
        bridge
            .run(false, false, &["task", "restart", "t-000001"])
            .status
            .success()
    );
    let requests = bridge.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["type"], "restart-agent");
    assert!(requests[0].get("rev").is_none());
    assert!(requests[0].get("worker").is_none());
}

#[test]
fn explicit_workers_require_capability_before_any_request() {
    for args in [
        vec!["task", "restart", "t-000001", "--worker", "Reviewer"],
        vec!["task", "restart", "t-000001", "--worker", "default"],
        vec![
            "task",
            "new",
            "--workspace",
            "work",
            "--title",
            "Fixture",
            "--goal",
            "Wait",
            "--worker",
            "Reviewer",
        ],
    ] {
        let bridge = Bridge::new();
        let output = bridge.run(false, false, &args);
        assert!(!output.status.success());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("does not advertise workers")
        );
        assert!(bridge.requests().is_empty());
    }
}

#[test]
fn selected_worker_is_sent_on_create_and_list_metadata_is_public() {
    let bridge = Bridge::new();
    let output = bridge.run(
        true,
        false,
        &[
            "task",
            "new",
            "--workspace",
            "work",
            "--title",
            "Fixture",
            "--goal",
            "Wait",
            "--worker",
            "Reviewer",
            "--json",
        ],
    );
    assert!(output.status.success());
    assert_eq!(bridge.requests()[0]["worker"], "Reviewer");
    let output = bridge.run(true, false, &["tasks", "--json"]);
    assert!(output.status.success());
    let listed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        listed["workers"],
        json!([{"name":"Reviewer", "agent":"codex"}])
    );
    assert_eq!(listed["tasks"][0]["record"]["worker"], "Reviewer");
}

#[test]
fn tasks_json_keeps_workspace_names_and_adds_their_details_and_capabilities() {
    let bridge = Bridge::new();
    let output = bridge.run(false, false, &["tasks", "--json"]);
    assert!(output.status.success(), "{:?}", output);
    let listed: Value = serde_json::from_slice(&output.stdout).unwrap();
    // Desktop clients that read names only keep working.
    assert_eq!(listed["workspaces"], json!(["work"]));
    assert_eq!(
        listed["workspaceInfo"],
        json!([{"name":"work", "root":"/w", "repos":["/w/app", "/x/tool"]}])
    );
    assert_eq!(
        listed["capabilities"],
        json!(["task-records", "task-handoff"])
    );
}

#[test]
fn conflict_is_reported_without_retrying_restart() {
    let bridge = Bridge::new();
    let output = bridge.run(
        true,
        true,
        &["task", "restart", "t-000001", "--worker", "Reviewer"],
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("conflict:")
    );
    assert_eq!(bridge.requests().len(), 2);
}

#[test]
fn worker_selectors_are_rejected_when_empty_removed_or_out_of_scope() {
    for args in [
        // ADR 0027 replaced this flag with `--worker default`.
        vec!["task", "restart", "t-000001", "--workspace-default"],
        vec!["task", "restart", "t-000001", "--worker", " "],
        vec!["tasks", "--worker", "Reviewer"],
        vec!["task", "done", "t-000001", "--worker", "default"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_muxa-cli"))
            .args(["--bridge", "exit 99"])
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
    }
}

#[test]
fn recovery_inspection_does_not_mutate_and_unknown_phase_has_no_action() {
    let bridge = Bridge::new();
    let output = bridge.run_recovery(
        false,
        true,
        false,
        &["task", "recover", "t-000001", "--json"],
    );
    assert!(output.status.success());
    let record: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(record["rev"], 7);
    assert_eq!(record["restartOperation"]["id"], "restart-fixture");
    assert_eq!(bridge.requests().len(), 1);
    assert_eq!(bridge.requests()[0]["type"], "list-tasks");
    fs::write(
        bridge.0.join("bridge.py"),
        BRIDGE.replace("'phase':'replaced'", "'phase':'future-phase'"),
    )
    .unwrap();
    let output = bridge.run_recovery(false, true, false, &["task", "recover", "t-000001"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("future-phase"));
    assert!(text.contains("no recovery action is available"));
    assert!(!text.contains("actions:"));
    assert_eq!(bridge.requests().len(), 1);
}

#[test]
fn recovery_stays_available_without_workers_and_shows_the_requested_worker() {
    let bridge = Bridge::new();
    let output = bridge.run_recovery(
        false,
        true,
        false,
        &["task", "recover", "t-000001", "--json"],
    );
    assert!(output.status.success(), "{output:?}");
    let record: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(record["restartOperation"]["worker"], "Reviewer");
    assert_eq!(bridge.requests(), [json!({"type":"list-tasks","id":1})]);
    let output = bridge.run_recovery(false, true, false, &["task", "recover", "t-000001"]);
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("requested/unconfirmed"));
    assert!(text.contains("requested worker: Reviewer"));
    assert!(text.contains("actions: finish-replacement, leave-unconfirmed"));
    assert!(!text.contains("future-action"));
    assert_eq!(bridge.requests(), [json!({"type":"list-tasks","id":1})]);
    let output = bridge.run_recovery(
        false,
        true,
        false,
        &[
            "task",
            "recover",
            "t-000001",
            "--operation",
            "restart-fixture",
            "--rev",
            "7",
            "--action",
            "leave-unconfirmed",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        bridge.requests(),
        [json!({"type":"recover-restart","id":1,
        "task":"t-000001","operation":"restart-fixture","rev":7,"action":"leave-unconfirmed"})]
    );
}

#[test]
fn explicit_recovery_sends_the_displayed_identity_without_substitution_or_retry() {
    for action in ["cancel-prepared", "finish-replacement", "leave-unconfirmed"] {
        let bridge = Bridge::new();
        let output = bridge.run_recovery(
            false,
            true,
            false,
            &[
                "task",
                "recover",
                "t-000001",
                "--operation",
                "displayed-operation",
                "--rev",
                "123",
                "--action",
                action,
            ],
        );
        assert!(output.status.success(), "{output:?}");
        let requests = bridge.requests();
        assert_eq!(
            requests,
            [
                json!({"type":"recover-restart","id":1,"task":"t-000001","rev":123,"operation":"displayed-operation","action":action})
            ]
        );
    }
    let bridge = Bridge::new();
    let output = bridge.run_recovery(
        false,
        true,
        true,
        &[
            "task",
            "recover",
            "t-000001",
            "--operation",
            "displayed-operation",
            "--rev",
            "123",
            "--action",
            "leave-unconfirmed",
        ],
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("conflict:")
    );
    assert_eq!(bridge.requests().len(), 1);
}

#[test]
fn recovery_needs_its_own_capability_and_all_explicit_identity_flags() {
    let bridge = Bridge::new();
    let output = bridge.run(
        true,
        false,
        &[
            "task",
            "recover",
            "t-000001",
            "--operation",
            "displayed-operation",
            "--rev",
            "123",
            "--action",
            "leave-unconfirmed",
        ],
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("does not advertise task-restart-recovery")
    );
    assert!(bridge.requests().is_empty());
    for flags in [
        vec!["--operation", "fixture"],
        vec!["--rev", "7", "--action", "cancel-prepared"],
        vec![
            "--operation",
            "fixture",
            "--rev",
            "0",
            "--action",
            "cancel-prepared",
        ],
        vec![
            "--operation",
            "fixture",
            "--rev",
            "7",
            "--action",
            "future-action",
        ],
        vec![
            "--operation",
            " ",
            "--rev",
            "7",
            "--action",
            "cancel-prepared",
        ],
    ] {
        let mut args = vec!["task", "recover", "t-000001"];
        args.extend(flags);
        let output = bridge.run_recovery(false, true, false, &args);
        assert_eq!(output.status.code(), Some(2));
    }
}
