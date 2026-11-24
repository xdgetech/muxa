use serde_json::json;

use super::*;

const NOW: u64 = 1_795_500_000;

fn w(minutes: u32, used: f64, resets_at: u64) -> Window {
    Window {
        minutes,
        used_percent: used,
        resets_at,
    }
}

#[test]
fn claude_and_codex_reports_become_windows() {
    // As the S0 probe recorded them (docs/findings/agent-usage.md).
    let claude = json!({"cwd": "/x", "rate_limits": {
        "five_hour": {"used_percentage": 3, "resets_at": 1_795_525_200u64},
        "seven_day": {"used_percentage": 76, "resets_at": 1_795_518_000u64}}});
    assert_eq!(
        from_claude(&claude),
        vec![w(300, 3.0, 1_795_525_200), w(10_080, 76.0, 1_795_518_000)]
    );
    assert!(
        from_claude(&json!({"cwd": "/x"})).is_empty(),
        "before the first reply"
    );
    let codex = json!({"limit_id": "codex", "plan_type": "plus",
        "primary": {"used_percent": 11.0, "window_minutes": 300, "resets_at": 1_795_197_934u64},
        "secondary": {"used_percent": 41.0, "window_minutes": 10080, "resets_at": 1_795_568_863u64}});
    assert_eq!(
        from_codex(&codex),
        vec![w(300, 11.0, 1_795_197_934), w(10_080, 41.0, 1_795_568_863)]
    );
}

#[test]
fn a_report_from_a_stale_agent_or_with_bad_numbers_is_dropped() {
    let old = merge(&Record::default(), &[w(300, 40.0, NOW + 3600)], NOW);
    // An older reset: another agent's report from before the reset.
    let stale = merge(&old, &[w(300, 90.0, NOW + 60)], NOW + 10);
    assert_eq!(stale.windows, vec![w(300, 40.0, NOW + 3600)]);
    // The same window, later: it wins.
    let newer = merge(&old, &[w(300, 55.0, NOW + 3600)], NOW + 20);
    assert_eq!(
        (newer.windows[0].used_percent, newer.as_of),
        (55.0, NOW + 20)
    );
    for bad in [
        w(300, 101.0, NOW + 60),
        w(300, -1.0, NOW + 60),
        w(300, f64::NAN, NOW + 60),
        w(0, 10.0, NOW + 60),
        w(300, 10.0, NOW + 30 * 24 * 3600),
    ] {
        assert_eq!(
            merge(&old, std::slice::from_ref(&bad), NOW).windows,
            old.windows,
            "{bad:?}"
        );
    }
}

#[test]
fn records_are_kept_per_subscription_and_a_bad_name_writes_nothing() {
    let dir = std::env::temp_dir().join(format!(
        "muxa-limits-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    assert_eq!(read(&dir, "work").unwrap(), Record::default());
    record(&dir, "work", &[w(300, 20.0, NOW + 60)], NOW).unwrap();
    record(&dir, "work", &[w(10_080, 70.0, NOW + 600)], NOW + 5).unwrap();
    let r = read(&dir, "work").unwrap();
    assert_eq!(r.as_of, NOW + 5);
    assert_eq!(
        r.windows,
        vec![w(300, 20.0, NOW + 60), w(10_080, 70.0, NOW + 600)]
    );
    assert!(record(&dir, "../evil", &[w(300, 1.0, NOW + 60)], NOW).is_err());
    assert_eq!(read(&dir, "../evil").unwrap(), Record::default());
    // Nothing valid: no file at all.
    record(&dir, "other", &[w(300, 500.0, NOW + 60)], NOW).unwrap();
    assert!(!dir.join(DIR).join("other.json").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_codex_login_s_newest_fresh_log_gives_its_limits() {
    let base = std::env::temp_dir().join(format!(
        "muxa-limits-codex-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    let home = base.join("codex-home");
    let old_day = home.join("sessions/2026/11/23");
    let day = home.join("sessions/2026/11/24");
    std::fs::create_dir_all(&old_day).unwrap();
    std::fs::create_dir_all(&day).unwrap();
    let event = |used: f64| {
        json!({"type": "event_msg", "payload": {"type": "token_count", "rate_limits": {
            "primary": {"used_percent": used, "window_minutes": 300, "resets_at": crate::records::now() + 600},
            "secondary": null}}})
        .to_string()
    };
    std::fs::write(old_day.join("rollout-a.jsonl"), event(99.0) + "\n").unwrap();
    std::fs::write(
        day.join("rollout-b.jsonl"),
        format!("{}\n{}\n{{\"type\":\"other\"}}\n", event(10.0), event(12.5)),
    )
    .unwrap();
    let subs = vec![subscriptions::Subscription {
        name: "chatgpt".into(),
        agent: crate::records::workers::WorkerAgent::Codex,
        home: home.clone(),
        plan: None,
        default_login: false,
    }];
    let state = base.join("state");
    scan_codex(&state, &subs, crate::records::now());
    let r = read(&state, "chatgpt").unwrap();
    assert_eq!(r.windows.len(), 1);
    assert_eq!(
        (r.windows[0].minutes, r.windows[0].used_percent),
        (300, 12.5)
    );
    // A log not touched for a while has nothing new.
    let later = crate::records::now() + 3600;
    assert!(newest_codex_log(&home, later).is_none());
    let _ = std::fs::remove_dir_all(&base);
}
