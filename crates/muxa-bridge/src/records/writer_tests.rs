use std::path::Path;

use super::*;
use crate::records::Workspaces;

const WORKERS: &str = "\
[[worker]]\nname = \"lisk-delivery\"\ndescription = \"Delivery\"\nagent = \"claude-code\"\n\
[[worker]]\nname = \"lisk-research\"\ndescription = \"Research\"\nagent = \"claude-code\"\n\
[[worker]]\nname = \"lisk-writer\"\ndescription = \"Writer\"\nagent = \"claude-code\"\nworkspaces = [\"lisk\"]\n\
[[worker]]\nname = \"elsewhere\"\ndescription = \"Other\"\nagent = \"claude-code\"\nworkspaces = [\"other\"]\n\
[[worker]]\nname = \"codex-writer\"\ndescription = \"Codex\"\nagent = \"codex\"\n\
[[worker]]\nname = \"no-description\"\nagent = \"claude-code\"\n";

/// `lisk` with `assistant`, and `other`, with the host on.
fn parse(assistant: &str) -> Result<Workspaces, String> {
    let root = std::env::temp_dir();
    let root = root.to_string_lossy();
    let text = format!(
        "[assistant]\nenabled = true\n\
         [[workspace]]\nname = \"lisk\"\nroot = \"{root}\"\n[workspace.assistant]\n{assistant}\n\
         [[workspace]]\nname = \"other\"\nroot = \"{root}\"\n{WORKERS}"
    );
    Workspaces::parse(&text, Path::new("w.toml"), None).map_err(|e| e.to_string())
}

const STAFF: &str = "delegate = true\nstaff = [\"lisk-delivery\", \"lisk-research\"]";

#[test]
fn a_writer_takes_review_by_default_and_none_turns_it_off() {
    let w = parse(&format!("{STAFF}\nwriter = \"lisk-writer\"")).unwrap();
    let d = w.delegate("lisk").unwrap();
    assert_eq!(
        (d.writer.worker.as_deref(), d.writer.writes),
        (Some("lisk-writer"), Writes::Review)
    );
    let off = parse(&format!(
        "{STAFF}\nwriter = \"lisk-writer\"\nwrites = \"none\""
    ))
    .unwrap();
    let d = off.delegate("lisk").unwrap();
    assert_eq!(
        (d.writer.worker.as_deref(), d.writer.writes),
        (Some("lisk-writer"), Writes::None)
    );
    // Without a writer, a workspace takes no writes, with or without staff.
    for fields in [
        STAFF,
        "delegate = true",
        "delegate = true\nwrites = \"none\"",
    ] {
        let w = parse(fields).unwrap();
        assert_eq!(w.delegate("lisk").unwrap().writer, Writer::default());
    }
}

#[test]
fn counterexamples_name_the_workspace_and_the_writer() {
    let cases = [
        (
            format!("{STAFF}\nwriter = \"lisk-research\""),
            "writer \"lisk-research\" is on the staff",
        ),
        (
            format!("{STAFF}\nwriter = \"lisk-delivery\""),
            "writer \"lisk-delivery\" is on the staff",
        ),
        (
            format!("{STAFF}\nlead = \"lisk-research\"\nwriter = \"lisk-research\""),
            "writer \"lisk-research\" is on the staff",
        ),
        (
            "delegate = true\nworker = \"lisk-delivery\"\nwriter = \"lisk-delivery\"".into(),
            "writer \"lisk-delivery\" is also the workspace's worker",
        ),
        (
            format!("{STAFF}\nwriter = \"nobody\""),
            "writer \"nobody\" is not configured",
        ),
        (
            format!("{STAFF}\nwriter = \"elsewhere\""),
            "writer \"elsewhere\" is not allowed in this workspace",
        ),
        (
            format!("{STAFF}\nwriter = \"codex-writer\""),
            "writer \"codex-writer\" runs codex",
        ),
        (
            format!("{STAFF}\nwriter = \"no-description\""),
            "writer \"no-description\" has no description",
        ),
        (
            format!("{STAFF}\nwrites = \"review\""),
            "writes = \"review\" needs a writer",
        ),
        (
            format!("{STAFF}\nwriter = \"lisk-writer\"\nwrites = \"auto\""),
            "writes must be \"review\" or \"none\"",
        ),
    ];
    for (fields, needle) in cases {
        let err = parse(&fields).unwrap_err();
        assert!(err.contains("workspace \"lisk\""), "{fields}: {err}");
        assert!(err.contains(needle), "{fields}: {err}");
    }
    for fields in [
        "overall = true\nwriter = \"lisk-writer\"",
        "overall = true\nwrites = \"none\"",
    ] {
        let err = parse(fields).unwrap_err();
        assert!(err.contains("workspace \"lisk\""), "{fields}: {err}");
        assert!(
            err.contains("writer and writes do not apply to the overall workspace"),
            "{fields}: {err}"
        );
    }
}
