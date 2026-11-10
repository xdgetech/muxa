use std::path::Path;

use muxa_protocol::{AssistResult, Handoff, StepBy};

use super::*;

fn config() -> Workspaces {
    let root = std::env::temp_dir();
    let text = format!(
        "[assistant]\nenabled = true\n\
         [[workspace]]\nname = \"lisk\"\nroot = \"{}\"\n[workspace.assistant]\ndelegate = true\n\
         staff = [\"lead\", \"strategist\"]\nmax_steps = 4\n\
         [[worker]]\nname = \"lead\"\ndescription = \"Chief of staff\"\nagent = \"claude-code\"\n\
         [[worker]]\nname = \"strategist\"\ndescription = \"Strategist\"\nagent = \"claude-code\"\n",
        root.display()
    );
    Workspaces::parse(&text, Path::new("w.toml"), None).unwrap()
}

fn result(version: u64, step: u32, worker: &str, summary: &str) -> AssistResult {
    AssistResult {
        writes: Vec::new(),
        version,
        at: 1,
        summary: summary.into(),
        stale: false,
        findings: Vec::new(),
        open_questions: Vec::new(),
        next: Vec::new(),
        files: Vec::new(),
        possible_duplicates: Vec::new(),
        notes: None,
        worker: Some(worker.into()),
        step: Some(step),
    }
}

fn request() -> AssistantRequest {
    let mut q: AssistantRequest = serde_json::from_value(serde_json::json!({
        "id": "q-0000000a", "work": "w-0000000b", "version": 2, "workspace": "lisk",
        "scope": "research", "delivery": "pending", "deliveryAt": 1,
        "createdAt": 1, "updatedAt": 1,
    }))
    .unwrap();
    q.steps = steps::first(Some("lead"), StepBy::Lead, 1);
    q.steps[0].handoff = Some(Handoff {
        worker: "strategist".into(),
        purpose: "Compare the options.".into(),
    });
    q.steps
        .push(steps::fresh(2, "strategist", StepBy::Handoff, 2));
    q.results = vec![
        result(2, 1, "lead", "Two options remain."),
        result(1, 1, "lead", "An older answer."),
    ];
    q
}

#[test]
fn a_later_step_reads_the_earlier_steps_of_its_version_and_its_hand_off() {
    let b = staff_brief(&config(), &request(), Some("strategist"), 2).unwrap();
    assert_eq!((b.step, b.max_steps), (2, 4));
    assert_eq!(b.description, "Strategist");
    assert_eq!(
        b.earlier,
        [Earlier {
            step: 1,
            worker: "lead".into(),
            summary: "Two options remain.".into()
        }]
    );
    assert_eq!(
        b.asked,
        Some(("lead".into(), "Compare the options.".into()))
    );
    assert!(!b.is_lead());
}

#[test]
fn a_request_without_steps_has_no_staff_section() {
    let mut q = request();
    q.steps.clear();
    assert_eq!(staff_brief(&config(), &q, Some("lead"), 1), None);
}
