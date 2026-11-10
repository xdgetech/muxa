use std::path::Path;

use super::*;
use crate::records::Workspaces;

const WORKERS: &str = "\
[[worker]]\nname = \"lisk-chief-of-staff\"\ndescription = \"Chief of staff\"\nagent = \"claude-code\"\n\
[[worker]]\nname = \"lisk-strategist\"\ndescription = \"Strategist\"\nagent = \"claude-code\"\n\
[[worker]]\nname = \"lisk-architect\"\ndescription = \"Architect\"\nagent = \"claude-code\"\nworkspaces = [\"lisk\"]\n\
[[worker]]\nname = \"elsewhere\"\ndescription = \"Other\"\nagent = \"claude-code\"\nworkspaces = [\"other\"]\n\
[[worker]]\nname = \"codex-helper\"\ndescription = \"Codex\"\nagent = \"codex\"\n\
[[worker]]\nname = \"no-description\"\nagent = \"claude-code\"\n\
[[worker]]\nname = \"a/b\"\ndescription = \"Slash\"\nagent = \"claude-code\"\n";

/// `lisk` with `assistant`, and `other`, with the host on.
fn parse(assistant: &str, host: &str) -> Result<Workspaces, String> {
    let root = std::env::temp_dir();
    let root = root.to_string_lossy();
    let text = format!(
        "[assistant]\nenabled = true\n{host}\n\
         [[workspace]]\nname = \"lisk\"\nroot = \"{root}\"\n[workspace.assistant]\n{assistant}\n\
         [[workspace]]\nname = \"other\"\nroot = \"{root}\"\n{WORKERS}"
    );
    Workspaces::parse(&text, Path::new("w.toml"), None).map_err(|e| e.to_string())
}

#[test]
fn a_codex_staff_member_needs_owned_threads() {
    let staff = "delegate = true\nstaff = [\"lisk-strategist\", \"codex-helper\"]";
    let off = parse(staff, "").unwrap_err();
    assert!(off.contains("owned_threads"), "{off}");
    let w = parse(staff, "[codex]\nowned_threads = true").unwrap();
    assert_eq!(
        w.delegate("lisk").unwrap().staff.members,
        ["lisk-strategist", "codex-helper"]
    );
}

#[test]
fn a_workspace_without_staff_resolves_as_before() {
    let w = parse("delegate = true\nworker = \"lisk-strategist\"", "").unwrap();
    let d = w.delegate("lisk").unwrap();
    assert!(d.staff.members.is_empty());
    assert_eq!(d.lead(), Some("lisk-strategist"));
    assert_eq!(
        (d.staff.handoffs, d.staff.max_steps),
        (Handoffs::Review, DEFAULT_MAX_STEPS)
    );
    let agent = parse("delegate = true", "").unwrap();
    assert_eq!(agent.delegate("lisk").unwrap().lead(), None);
    assert_eq!(
        agent.assistant().max_auto_handoffs_per_day,
        DEFAULT_AUTO_HANDOFFS_PER_DAY
    );
}

#[test]
fn the_first_member_leads_unless_lead_names_another() {
    let staff = "delegate = true\nstaff = [\"lisk-chief-of-staff\", \"lisk-strategist\", \"lisk-architect\"]";
    let w = parse(staff, "").unwrap();
    let d = w.delegate("lisk").unwrap();
    assert_eq!(d.lead(), Some("lisk-chief-of-staff"));
    assert_eq!(d.staff.members.len(), 3);
    assert!(d.staff.members.iter().any(|m| m == "lisk-architect"));
    let set = parse(
        &format!("{staff}\nlead = \"lisk-strategist\"\nhandoffs = \"auto\"\nmax_steps = 5"),
        "max_auto_handoffs_per_day = 4",
    )
    .unwrap();
    let d = set.delegate("lisk").unwrap();
    assert_eq!(d.lead(), Some("lisk-strategist"));
    assert_eq!((d.staff.handoffs, d.staff.max_steps), (Handoffs::Auto, 5));
    assert_eq!(set.assistant().max_auto_handoffs_per_day, 4);
    let none = parse(&format!("{staff}\nhandoffs = \"none\""), "").unwrap();
    assert_eq!(
        none.delegate("lisk").unwrap().staff.handoffs,
        Handoffs::None
    );
}

#[test]
fn counterexamples_name_the_workspace_and_the_worker() {
    let nine: Vec<String> = (0..9).map(|i| format!("\"w{i}\"")).collect();
    let cases = [
        (
            "staff = [\"lisk-strategist\", \"lisk-strategist\"]",
            "\"lisk-strategist\" appears twice",
        ),
        ("staff = [\"nobody\"]", "\"nobody\" is not configured"),
        (
            "staff = [\"elsewhere\"]",
            "\"elsewhere\" is not allowed in this workspace",
        ),
        ("staff = [\"codex-helper\"]", "\"codex-helper\" runs codex"),
        (
            "staff = [\"no-description\"]",
            "\"no-description\" has no description",
        ),
        (
            "staff = [\"lisk-strategist\"]\nlead = \"lisk-architect\"",
            "lead \"lisk-architect\" is not in staff",
        ),
        (
            "staff = [\"lisk-strategist\"]\nworker = \"lisk-strategist\"",
            "list the worker in staff and name it lead",
        ),
        ("staff = [\"a/b\"]", "\"a/b\" is not a staff name"),
        ("staff = [\"..\"]", "\"..\" is not a staff name"),
        ("staff = []", "staff is empty"),
        (&format!("staff = [{}]", nine.join(", ")), "more than 8"),
        ("lead = \"lisk-strategist\"", "lead needs staff"),
        (
            "staff = [\"lisk-strategist\"]\nmax_steps = 6",
            "max_steps must be",
        ),
        (
            "staff = [\"lisk-strategist\"]\nmax_steps = 0",
            "max_steps must be",
        ),
        (
            "staff = [\"lisk-strategist\"]\nhandoffs = \"always\"",
            "handoffs must be",
        ),
    ];
    for (fields, needle) in cases {
        let err = parse(&format!("delegate = true\n{fields}"), "").unwrap_err();
        assert!(err.contains("workspace \"lisk\""), "{fields}: {err}");
        assert!(err.contains(needle), "{fields}: {err}");
    }
    let overall = parse("overall = true\nstaff = [\"lisk-strategist\"]", "").unwrap_err();
    assert!(
        overall.contains("do not apply to the overall workspace"),
        "{overall}"
    );
    let host = parse("delegate = true", "max_auto_handoffs_per_day = 0").unwrap_err();
    assert!(host.contains("max_auto_handoffs_per_day"), "{host}");
}

#[test]
fn every_worker_may_describe_its_job_in_1_kib() {
    let with = |text: &str| {
        parse(
            "delegate = true",
            &format!(
                "[[worker]]\nname = \"long\"\nagent = \"claude-code\"\ndescription = \"{text}\""
            ),
        )
    };
    // Multi-byte text counts in bytes.
    let kib = "é".repeat(MAX_DESCRIPTION / 2);
    assert!(with(&kib).is_ok());
    let err = with(&format!("{kib}x")).unwrap_err();
    assert!(err.contains("\"long\"") && err.contains("1 KiB"), "{err}");
    assert!(with("line\\nbreak").is_err());
}
