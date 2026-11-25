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

/// ADR 0091 §9: while a member's logins are all full, its step's handoffs
/// stay, held; once one has room, the step starts and the hold goes.
#[test]
fn handoffs_wait_while_a_members_logins_are_full() {
    let base = std::env::temp_dir().join(format!("muxa-team-held-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("root/.muxa")).unwrap();
    std::fs::create_dir_all(base.join("state")).unwrap();
    let base = base.canonicalize().unwrap();
    std::fs::write(base.join("root/.muxa/brief.md"), "Lead.\n").unwrap();
    std::fs::write(
        base.join("root/.muxa/routines.toml"),
        "[[routine]]\nname = \"r\"\ninstructions = \"brief.md\"\nenabled = true\nbot = \"lead\"\n\
         [[bot]]\nname = \"lead\"\nenabled = true\nmembers = [\"a\"]\n\
         [[bot]]\nname = \"a\"\nenabled = true\nsubscriptions = [\"spare\"]\n",
    )
    .unwrap();
    std::fs::write(
        base.join("workspaces.toml"),
        format!(
            "[[subscription]]\nname = \"work\"\nagent = \"claude-code\"\nhome = \"{0}/.claude-work\"\n\
             [[subscription]]\nname = \"spare\"\nagent = \"claude-code\"\nhome = \"{0}/.claude-spare\"\n\
             [[workspace]]\nname = \"w\"\nroot = \"{0}/root\"\nsubscriptions = [\"work\", \"spare\"]\n\
             [workspace.routines]\nenabled = true\n",
            base.display()
        ),
    )
    .unwrap();
    let config = crate::config::Config {
        state_dir: Some(base.join("state")),
        workspaces: Some(base.join("workspaces.toml")),
        home: None,
    };
    let lead_dir = base.join("root/routines/r-1");
    let handoffs = lead_dir.join(HANDOFFS);
    std::fs::create_dir_all(handoffs.parent().unwrap()).unwrap();
    std::fs::write(&handoffs, r#"[{"to": "a", "brief": "Read the issues."}]"#).unwrap();
    let at = now();
    let report = |percent: f64, resets_at: u64| {
        let w = crate::limits::Window {
            minutes: 300,
            used_percent: percent,
            resets_at,
        };
        crate::limits::record(&base.join("state"), "spare", &[w], at).unwrap();
    };
    report(98.0, at + 3600);

    assert!(
        plan(&config, "claude", &lead_dir, "w", "r")
            .unwrap()
            .is_empty()
    );
    assert!(handoffs.exists(), "the handoffs stay while held");
    let held = held_steps(&config);
    assert_eq!(held.len(), 1);
    assert_eq!(
        (held[0].1.workspace.as_str(), held[0].1.routine.as_str()),
        ("w", "r")
    );
    assert!(load(&lead_dir).is_err(), "no step started");

    // The next window: the member's login has room again.
    report(10.0, at + 3600 + 5 * 3600);
    let _ = plan(&config, "claude", &lead_dir, "w", "r").unwrap();
    assert!(!handoffs.exists());
    assert!(held_steps(&config).is_empty());
    assert_eq!(load(&lead_dir).unwrap().handoffs[0].to, "a");
    let _ = std::fs::remove_dir_all(base);
}
