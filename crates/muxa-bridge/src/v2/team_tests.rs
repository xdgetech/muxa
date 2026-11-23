use super::*;

fn pending() -> Pending {
    Pending {
        workspace: "w".into(),
        routine: "r".into(),
        lead: "lead".into(),
        step: 1,
        handoffs: vec![
            Handoff {
                to: "a".into(),
                brief: "Read the issues.".into(),
            },
            Handoff {
                to: "b".into(),
                brief: "Read the PRs.".into(),
            },
        ],
        ..Pending::default()
    }
}

#[test]
fn a_step_completes_when_every_member_has_a_result() {
    let mut p = pending();
    p.results.insert("a".into(), "Two issues.".into());
    assert!(!p.complete());
    p.results.insert("b".into(), "One PR.".into());
    assert!(p.complete());
    let next = p.follow_up();
    assert_eq!(next.step, 2);
    assert!(next.text.contains("step 2 of 3") && next.text.contains("One PR."));
}

#[test]
fn a_team_starts_at_most_its_handoffs_a_day() {
    let dir = std::env::temp_dir().join(format!("muxa-team-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let config = crate::config::Config {
        state_dir: Some(dir.clone()),
        workspaces: None,
        home: None,
    };
    assert_eq!(take_budget(&config, "w", "lead", 3, 2), 2);
    assert_eq!(take_budget(&config, "w", "lead", 3, 2), 1);
    assert_eq!(take_budget(&config, "w", "lead", 3, 2), 0);
    assert_eq!(take_budget(&config, "w", "other", 3, 2), 2);
    let _ = std::fs::remove_dir_all(dir);
}
