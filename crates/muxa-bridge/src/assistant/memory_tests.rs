use std::path::Path;

use serde_json::Value;

use super::*;

const NOW: u64 = 1_762_740_000;
const DAY: u64 = 86_400;
/// None of these may reach the section.
const HIDDEN: &str = "Zeta-hidden-9c";
const BARE: &str = "Bare-unlisted-3a";
const POLICY_MARK: &str = "policy-mark-1e4b";
const HYPOTHESIS_MARK: &str = "hypothesis-mark-6a20";
const REASON_MARK: &str = "reason-mark-c871";

fn workspaces() -> Workspaces {
    let root = std::env::temp_dir();
    let root = root.to_string_lossy();
    let ws = |name: &str, table: &str| {
        format!("[[workspace]]\nname = \"{name}\"\nroot = \"{root}\"\n{table}\n")
    };
    let text = [
        "[assistant]\nenabled = true\n".to_owned(),
        ws("Overall", "[workspace.assistant]\noverall = true"),
        ws("Sum", "[workspace.assistant]\ndelegate = true"),
        ws(
            "Out",
            "[workspace.assistant]\ndelegate = true\nshare = \"outline\"",
        ),
        ws(
            HIDDEN,
            "[workspace.assistant]\ndelegate = true\nshare = \"none\"",
        ),
        ws(BARE, ""),
    ]
    .concat();
    Workspaces::parse(&text, Path::new("w.toml"), None).unwrap()
}

fn policy(id: &str, text: &str, scope: &str, created: u64, until: Option<u64>) -> Policy {
    Policy {
        id: id.into(),
        rev: 1,
        text: text.into(),
        scope: scope.into(),
        from: None,
        until,
        source: "user".into(),
        created_at: created,
        updated_at: created,
    }
}

fn hypothesis(
    id: &str,
    text: &str,
    scope: &str,
    state: HypothesisState,
    expires: u64,
) -> Hypothesis {
    Hypothesis {
        id: id.into(),
        text: text.into(),
        scope: scope.into(),
        evidence: vec!["x-00000001".into()],
        confidence: Confidence::Low,
        state,
        plan: "p-00000001".into(),
        expires_at: expires,
        at: NOW - DAY,
    }
}

fn correction(id: &str, workspace: Option<&str>, at: u64, reason: Option<&str>) -> Correction {
    Correction {
        id: id.into(),
        kind: CorrectionKind::Deferred,
        at,
        plan: "p-00000001".into(),
        row: Some("w-00000001".into()),
        in_top: true,
        workspace: workspace.map(str::to_owned),
        before: Vec::new(),
        after: Vec::new(),
        reason: reason.map(str::to_owned),
    }
}

fn build(p: &[Policy], h: &[Hypothesis], c: &[Correction]) -> Memory {
    let records = Records {
        policies: p,
        hypotheses: h,
        corrections: c,
        now: NOW,
    };
    memory(&workspaces(), &records)
}

/// The JSON lines under `title`, up to the next title.
fn block(text: &str, title: &str) -> Vec<Value> {
    let start = text.find(title).unwrap_or_else(|| panic!("{title}"));
    let rest = &text[start + title.len()..];
    let end = rest.find("\n### ").unwrap_or(rest.len());
    rest[..end]
        .lines()
        .filter(|l| l.starts_with('{'))
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn a_none_workspaces_policy_hypothesis_and_correction_are_absent() {
    let proposed = HypothesisState::Proposed;
    let m = build(
        &[
            policy("y-00000001", "All of it.", ALL, 1, None),
            policy("y-00000002", "Sum first.", "Sum", 2, None),
            policy("y-00000003", POLICY_MARK, HIDDEN, 3, None),
            policy("y-00000004", POLICY_MARK, BARE, 4, None),
            policy("y-00000005", POLICY_MARK, "Removed-workspace", 5, None),
        ],
        &[
            hypothesis("h-00000001", "Outline guess.", "Out", proposed, NOW + DAY),
            hypothesis("h-00000002", HYPOTHESIS_MARK, HIDDEN, proposed, NOW + DAY),
            hypothesis(
                "h-00000003",
                HYPOTHESIS_MARK,
                BARE,
                HypothesisState::Rejected,
                NOW + DAY,
            ),
        ],
        &[
            correction("x-00000001", None, NOW - 10, Some("Mine.")),
            correction("x-00000002", Some("Overall"), NOW - 20, None),
            correction("x-00000003", Some("Out"), NOW - 30, None),
            correction("x-00000004", Some(HIDDEN), NOW - 40, Some(REASON_MARK)),
            correction("x-00000005", Some(BARE), NOW - 50, Some(REASON_MARK)),
        ],
    );
    for mark in [
        POLICY_MARK,
        HYPOTHESIS_MARK,
        REASON_MARK,
        HIDDEN,
        BARE,
        "Removed",
    ] {
        assert!(!m.text.contains(mark), "{mark} in:\n{}", m.text);
    }
    assert_eq!(m.held, [2, 1, 0, 3]);
    assert_eq!(m.left_out, [0; 4]);
}

#[test]
fn the_section_holds_policies_then_hypotheses_then_corrections() {
    let m = build(
        &[
            policy("y-00000002", "Second.", ALL, 20, Some(NOW + DAY)),
            policy("y-00000001", "First.", "Sum", 10, None),
            policy("y-00000003", "Expired.", ALL, 5, Some(NOW - 1)),
        ],
        &[
            hypothesis(
                "h-00000001",
                "Open.",
                ALL,
                HypothesisState::Proposed,
                NOW + DAY,
            ),
            hypothesis(
                "h-00000002",
                "Turned down.",
                ALL,
                HypothesisState::Rejected,
                NOW + DAY,
            ),
            hypothesis(
                "h-00000003",
                "Old turn-down.",
                ALL,
                HypothesisState::Rejected,
                NOW - 1,
            ),
            hypothesis(
                "h-00000004",
                "Unanswered.",
                ALL,
                HypothesisState::Proposed,
                NOW,
            ),
            hypothesis(
                "h-00000005",
                "Became a policy.",
                ALL,
                HypothesisState::Confirmed,
                NOW + DAY,
            ),
            hypothesis(
                "h-00000006",
                "Ran out.",
                ALL,
                HypothesisState::Expired,
                NOW + DAY,
            ),
        ],
        &[
            correction("x-00000001", None, NOW - 31 * DAY, None),
            correction("x-00000002", None, NOW - 2 * DAY, Some("Waiting.")),
            correction("x-00000003", None, NOW - DAY, None),
        ],
    );
    let t = &m.text;
    let at = |needle: &str| {
        t.find(needle)
            .unwrap_or_else(|| panic!("{needle} in:\n{t}"))
    };
    assert!(at("### Policies") < at("### Hypotheses you proposed"));
    assert!(at("### Hypotheses you proposed") < at("### Rejected hypotheses"));
    assert!(at("### Rejected hypotheses") < at("### Corrections"));
    for rule in [
        "outranks every hypothesis",
        "Never use a rejected hypothesis",
        "not now",
    ] {
        assert!(t.contains(rule), "{rule}");
    }
    let policies = block(t, "### Policies");
    let ids: Vec<&str> = policies
        .iter()
        .map(|p| p["policy"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["y-00000001", "y-00000002"],
        "active ones, oldest first"
    );
    assert_eq!(policies[1]["until"], NOW + DAY);
    assert_eq!(policies[0]["scope"], "Sum");
    let open = block(t, "### Hypotheses you proposed");
    assert_eq!(open.len(), 1);
    assert_eq!(open[0]["evidence"][0], "x-00000001");
    let rejected = block(t, "### Rejected hypotheses");
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0]["hypothesis"], "h-00000002");
    assert!(rejected[0].get("evidence").is_none());
    let corrections = block(t, "### Corrections");
    let ids: Vec<&str> = corrections
        .iter()
        .map(|c| c["correction"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["x-00000003", "x-00000002"],
        "the last 30 days, newest first"
    );
    assert_eq!(corrections[1]["reason"], "Waiting.");
    assert_eq!(corrections[1]["inTop"], true);
    assert!(corrections[0].get("reason").is_none());
}

#[test]
fn over_16_kib_corrections_leave_oldest_first_and_the_section_says_how_many() {
    let reason = "r".repeat(400);
    let corrections: Vec<Correction> = (0..200u64)
        .map(|n| correction(&format!("x-{n:08x}"), None, NOW - n * 60, Some(&reason)))
        .collect();
    let policies = [policy("y-00000001", "Keep me.", ALL, 1, None)];
    let hypotheses = [hypothesis(
        "h-00000001",
        "Keep me too.",
        ALL,
        HypothesisState::Proposed,
        NOW + DAY,
    )];
    let m = build(&policies, &hypotheses, &corrections);
    assert!(m.text.len() <= MAX_MEMORY, "{} bytes", m.text.len());
    assert_eq!(m.held[..3], [1, 1, 0]);
    let (held, out) = (m.held[3], m.left_out[3]);
    assert!(out > 0 && held > 0);
    assert_eq!(held + out, 200);
    assert!(
        m.text
            .contains(&format!("{out} older corrections are left out"))
    );
    let kept = block(&m.text, "### Corrections");
    assert_eq!(kept[0]["correction"], "x-00000000", "the newest stays");
    let last = format!("x-{:08x}", held - 1);
    assert_eq!(
        kept[held - 1]["correction"],
        last.as_str(),
        "the oldest left"
    );
}

#[test]
fn policies_leave_only_after_every_correction_and_hypothesis() {
    let text = "p".repeat(500);
    let policies: Vec<Policy> = (0..50u64)
        .map(|n| policy(&format!("y-{n:08x}"), &text, ALL, n, None))
        .collect();
    let corrections = [correction("x-00000001", None, NOW, None)];
    let m = build(&policies, &[], &corrections);
    assert!(m.text.len() <= MAX_MEMORY);
    assert_eq!(m.left_out[3], 1);
    assert!(m.left_out[0] > 0);
    assert!(
        m.text
            .contains(&format!("{} policies are left out", m.left_out[0]))
    );
    let kept = block(&m.text, "### Policies");
    assert_eq!(
        kept.last().unwrap()["policy"],
        "y-00000031",
        "the newest stays"
    );
}

#[test]
fn an_empty_section_keeps_its_shape() {
    let m = build(&[], &[], &[]);
    assert!(m.text.starts_with("## What the user told you\n\n"));
    assert_eq!(m.text.matches("Nothing to list.").count(), 4);
    assert!(!m.text.contains("left out"));
    assert_eq!((m.held, m.left_out), ([0; 4], [0; 4]));
}
