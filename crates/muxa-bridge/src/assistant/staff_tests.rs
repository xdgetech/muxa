use std::path::Path;

use super::*;

fn config(assistant: &str) -> Workspaces {
    let root = std::env::temp_dir();
    let text = format!(
        "[assistant]\nenabled = true\n\
         [[workspace]]\nname = \"lisk\"\nroot = \"{root}\"\n[workspace.assistant]\ndelegate = true\n{assistant}\n\
         [[worker]]\nname = \"lisk-chief-of-staff\"\ndescription = \"Chief of staff\"\nagent = \"claude-code\"\n\
         [[worker]]\nname = \"lisk-strategist\"\ndescription = \"Strategist\"\nagent = \"claude-code\"\n",
        root = root.display()
    );
    Workspaces::parse(&text, Path::new("w.toml"), None).unwrap()
}

const STAFF: &str = "staff = [\"lisk-chief-of-staff\", \"lisk-strategist\"]";

#[test]
fn a_named_member_runs_else_the_lead() {
    let w = config(STAFF);
    let d = w.delegate("lisk").unwrap();
    assert_eq!(
        pick(d, "lisk", None).unwrap().as_deref(),
        Some("lisk-chief-of-staff")
    );
    assert_eq!(
        pick(d, "lisk", Some("lisk-strategist")).unwrap().as_deref(),
        Some("lisk-strategist")
    );
    let refused = pick(d, "lisk", Some("lisk-sales")).unwrap_err();
    assert_eq!(refused.code, ErrorCode::BadRequest);
    assert!(
        refused
            .message
            .contains("lisk-sales\" is not on the staff of workspace \"lisk\": lisk-chief-of-staff, lisk-strategist"),
        "{}",
        refused.message
    );
}

#[test]
fn a_staff_of_one_answers_as_before() {
    let agent = config("");
    let d = agent.delegate("lisk").unwrap();
    assert_eq!(pick(d, "lisk", None).unwrap(), None);
    assert!(
        pick(d, "lisk", Some("lisk-strategist"))
            .unwrap_err()
            .message
            .contains("has no staff")
    );
    let named = config("worker = \"lisk-strategist\"");
    let d = named.delegate("lisk").unwrap();
    assert_eq!(
        pick(d, "lisk", Some("lisk-strategist")).unwrap().as_deref(),
        Some("lisk-strategist")
    );
    assert!(members(&named, "lisk").is_empty());
    assert_eq!(notes_of(d, Some("lisk-strategist")), Place::Workspace);
}

#[test]
fn the_lead_keeps_the_workspace_notes_and_a_member_its_own() {
    let w = config(STAFF);
    let d = w.delegate("lisk").unwrap();
    assert_eq!(notes_of(d, Some("lisk-chief-of-staff")), Place::Workspace);
    assert_eq!(
        notes_of(d, Some("lisk-strategist")),
        Place::Member("lisk-strategist")
    );
    let listed = members(&w, "lisk");
    assert_eq!(
        listed
            .iter()
            .map(|m| (m.worker.as_str(), m.description.as_str(), m.lead))
            .collect::<Vec<_>>(),
        [
            ("lisk-chief-of-staff", "Chief of staff", true),
            ("lisk-strategist", "Strategist", false)
        ]
    );
}
