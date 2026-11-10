use std::path::PathBuf;

use super::*;

const NOW: u64 = 1_762_740_000;
const DAY: u64 = 86_400;

struct Host {
    base: PathBuf,
    config: Config,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn host() -> Host {
    let base = std::env::temp_dir().join(format!(
        "muxa-assistant-policies-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    let root = base.join("root");
    std::fs::create_dir_all(base.join("state")).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    let workspaces = base.join("workspaces.toml");
    std::fs::write(
        &workspaces,
        format!(
            "[assistant]\nenabled = true\n\
             [[workspace]]\nname = \"secretary\"\nroot = \"{0}\"\n[workspace.assistant]\noverall = true\n\
             [[workspace]]\nname = \"work\"\nroot = \"{0}\"\n",
            root.display()
        ),
    )
    .unwrap();
    let config = Config {
        state_dir: Some(base.join("state")),
        workspaces: Some(workspaces),
        home: None,
    };
    Host { base, config }
}

fn fields(text: &str, scope: &str, until: Option<u64>) -> Fields {
    Fields {
        text: Some(text.into()),
        scope: Some(scope.into()),
        from: None,
        until,
    }
}

impl Host {
    fn add(&self, text: &str, until: Option<u64>) -> Result<Policy, ErrorBody> {
        add(&self.config, fields(text, ALL, until), NOW)
    }

    fn change(
        &self,
        p: &Policy,
        rev: u64,
        action: PolicyAction,
        fields: Fields,
        now: u64,
    ) -> Result<Option<Policy>, ErrorBody> {
        update(
            &self.config,
            Change {
                policy: p.id.clone(),
                rev,
                action,
                fields,
            },
            now,
        )
    }

    fn ids(&self, now: u64) -> Vec<String> {
        list(&self.config, now)
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect()
    }
}

fn until(until: u64) -> Fields {
    Fields {
        until: Some(until),
        ..Fields::default()
    }
}

#[test]
fn a_policy_is_the_users_words_with_its_scope_and_end() {
    let h = host();
    let p = add(
        &h.config,
        Fields {
            from: Some(NOW),
            ..fields(
                "Recruiting comes before sales.",
                "work",
                Some(NOW + 5 * DAY),
            )
        },
        NOW,
    )
    .unwrap();
    assert!(p.id.starts_with("y-") && p.id.len() == 10);
    assert_eq!(
        (p.rev, p.text.as_str(), p.scope.as_str(), p.source.as_str()),
        (1, "Recruiting comes before sales.", "work", "user")
    );
    assert_eq!((p.from, p.until), (Some(NOW), Some(NOW + 5 * DAY)));
    assert_eq!(list(&h.config, NOW).unwrap(), [p]);
}

#[test]
fn text_and_scope_are_checked_before_anything_is_written() {
    let h = host();
    let refused = |f: Fields| add(&h.config, f, NOW).unwrap_err();
    for text in [
        "",
        "   ",
        "two\nlines",
        "tab\there",
        &"x".repeat(MAX_TEXT + 1),
    ] {
        let e = refused(fields(text, ALL, None));
        assert_eq!(e.code, ErrorCode::BadRequest);
        assert!(e.message.contains("1 to 500 bytes"), "{}", e.message);
    }
    // 500 bytes of a multi-byte text pass; one more byte does not.
    assert!(h.add(&"é".repeat(250), None).is_ok());
    assert!(h.add(&format!("{}x", "é".repeat(250)), None).is_err());
    let overall = refused(fields("t", "secretary", None));
    assert!(overall.message.contains("overall"), "{}", overall.message);
    assert_eq!(
        refused(fields("t", "nowhere", None)).message,
        "no workspace nowhere"
    );
    assert_eq!(
        refused(fields("t", ALL, Some(NOW - 1))).message,
        "until is past"
    );
    let backwards = Fields {
        from: Some(NOW + 10),
        ..fields("t", ALL, Some(NOW + 5))
    };
    assert!(refused(backwards).message.contains("after from"));
    assert_eq!(h.ids(NOW).len(), 1, "only the 500-byte text was stored");
}

#[test]
fn the_51st_active_policy_is_refused_with_the_limit() {
    let h = host();
    // An expired policy does not count.
    let old = h.add("Old", Some(NOW + 1)).unwrap();
    let later = NOW + 2;
    for n in 0..MAX_ACTIVE {
        add(&h.config, fields(&format!("Rule {n}"), ALL, None), later).unwrap();
    }
    let e = add(&h.config, fields("One more", ALL, None), later).unwrap_err();
    assert_eq!(e.code, ErrorCode::Conflict);
    assert_eq!(e.message, "50 active policies: delete one first");
    // Renewing the expired one would make 51 too.
    let renew = h
        .change(&old, 1, PolicyAction::Edit, until(later + DAY), later)
        .unwrap_err();
    assert_eq!(renew.code, ErrorCode::Conflict);
    assert_eq!(h.ids(later).len(), MAX_ACTIVE + 1);
    // Its text can still change while it stays expired.
    let text = Fields {
        text: Some("Old, reworded".into()),
        ..Fields::default()
    };
    let edited = h
        .change(&old, 1, PolicyAction::Edit, text, later)
        .unwrap()
        .unwrap();
    assert_eq!(edited.rev, 2);
}

#[test]
fn edit_and_delete_check_the_rev() {
    let h = host();
    let p = h
        .add("This week, recruiting comes first.", Some(NOW + DAY))
        .unwrap();
    let text = |t: &str| Fields {
        text: Some(t.into()),
        ..Fields::default()
    };
    let edited = h
        .change(
            &p,
            1,
            PolicyAction::Edit,
            text("Recruiting and support first."),
            NOW + 10,
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            edited.rev,
            edited.text.as_str(),
            edited.until,
            edited.updated_at
        ),
        (
            2,
            "Recruiting and support first.",
            Some(NOW + DAY),
            NOW + 10
        )
    );
    assert_eq!(edited.created_at, NOW);
    let stale = h
        .change(&p, 1, PolicyAction::Edit, text("Stale"), NOW)
        .unwrap_err();
    assert_eq!(stale.code, ErrorCode::Conflict);
    let stale = h
        .change(&p, 1, PolicyAction::Delete, Fields::default(), NOW)
        .unwrap_err();
    assert_eq!(stale.code, ErrorCode::Conflict);
    assert_eq!(
        h.ids(NOW),
        std::slice::from_ref(&p.id),
        "a stale delete keeps the policy"
    );

    let empty = h
        .change(&p, 2, PolicyAction::Edit, Fields::default(), NOW)
        .unwrap_err();
    assert_eq!(empty.code, ErrorCode::BadRequest);
    let with_text = h
        .change(&p, 2, PolicyAction::Delete, text("x"), NOW)
        .unwrap_err();
    assert_eq!(with_text.code, ErrorCode::BadRequest);
    let unsafe_text = h
        .change(&p, 2, PolicyAction::Edit, text(&"x".repeat(501)), NOW)
        .unwrap_err();
    assert_eq!(unsafe_text.code, ErrorCode::BadRequest);

    assert_eq!(
        h.change(&p, 2, PolicyAction::Delete, Fields::default(), NOW),
        Ok(None)
    );
    assert!(h.ids(NOW).is_empty());
    let gone = h
        .change(&p, 2, PolicyAction::Delete, Fields::default(), NOW)
        .unwrap_err();
    assert_eq!(
        (gone.code, gone.message),
        (ErrorCode::BadRequest, format!("no policy {}", p.id))
    );
    let garbage = Policy {
        id: "y-../../x".into(),
        ..p.clone()
    };
    let e = h
        .change(&garbage, 1, PolicyAction::Delete, Fields::default(), NOW)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::BadRequest);
    assert!(!e.message.contains(".."), "{}", e.message);
}

#[test]
fn an_expired_policy_is_listed_after_the_active_ones_for_30_days() {
    let h = host();
    let week = h
        .add("This week, recruiting first.", Some(NOW + 7 * DAY))
        .unwrap();
    let day = add(
        &h.config,
        fields("Today, the report.", ALL, Some(NOW + DAY)),
        NOW + 1,
    )
    .unwrap();
    let always = add(
        &h.config,
        fields("Mornings are for writing.", "work", None),
        NOW + 2,
    )
    .unwrap();
    assert_eq!(
        h.ids(NOW + 3),
        [week.id.clone(), day.id.clone(), always.id.clone()]
    );
    // Both expired: the most recently expired first, after the active one.
    let later = NOW + 8 * DAY;
    assert!(expired(&week, later) && !expired(&always, later));
    assert_eq!(
        h.ids(later),
        [always.id.clone(), week.id.clone(), day.id.clone()]
    );
    // 30 days after its end, it leaves the list, and the tick removes its file.
    let past_day = NOW + DAY + EXPIRED_KEEP + 1;
    assert_eq!(h.ids(past_day), [always.id.clone(), week.id.clone()]);
    prune(&h.config, past_day);
    let shelf = Shelf::<Policy>::open(h.config.state_dir.as_ref().unwrap()).unwrap();
    assert!(shelf.get(&day.id).unwrap().is_none());
    assert!(shelf.get(&week.id).unwrap().is_some());
    assert!(shelf.get(&always.id).unwrap().is_some());
}

#[test]
fn no_state_directory_lists_nothing_and_refuses_writes() {
    let config = Config {
        state_dir: None,
        workspaces: None,
        home: None,
    };
    assert!(list(&config, NOW).unwrap().is_empty());
    let e = add(&config, fields("t", ALL, None), NOW).unwrap_err();
    assert_eq!(e.code, ErrorCode::Unsupported);
}
