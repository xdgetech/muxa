use std::path::PathBuf;

use muxa_protocol::{Delivery, Notes};

use super::*;
use crate::assistant::apply::{Applied, apply_notes};
use crate::assistant::notes::{self, base_of};
use crate::assistant::store::Extra;
use crate::routines::store::StateError;

struct Host {
    base: PathBuf,
    root: PathBuf,
    config: Config,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn host() -> Host {
    let base = std::env::temp_dir().join(format!(
        "muxa-plans-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    let root = base.join("overall");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(base.join("state")).unwrap();
    let root = root.canonicalize().unwrap();
    let file = base.join("workspaces.toml");
    std::fs::write(
        &file,
        format!(
            "[assistant]\nenabled = true\n[[workspace]]\nname = \"Overall\"\nroot = \"{}\"\n\
             [workspace.assistant]\noverall = true\n",
            root.display()
        ),
    )
    .unwrap();
    Host {
        config: Config {
            state_dir: Some(base.join("state")),
            workspaces: Some(file),
            home: None,
        },
        base,
        root,
    }
}

impl Host {
    fn plan(&self, inputs_at: u64, delivery: Delivery, notes: Option<Notes>) -> Plan {
        shelf(&self.config)
            .unwrap()
            .create_with_extra::<StateError>(|id| {
                let mut extra = Extra::new();
                extra.insert(WORKSPACE_FIELD.into(), "Overall".into());
                Ok((
                    Plan {
                        id,
                        rev: 2,
                        at: inputs_at,
                        inputs_at,
                        delivery,
                        delivery_at: inputs_at,
                        minutes: None,
                        run: None,
                        workspaces: Vec::new(),
                        top: Vec::new(),
                        due_soon: Vec::new(),
                        unanswered: Vec::new(),
                        stale: Vec::new(),
                        changed: Vec::new(),
                        user_order: Vec::new(),
                        dismissed: Vec::new(),
                        notes,
                        reason: None,
                    },
                    extra,
                ))
            })
            .unwrap()
    }

    fn listed(&self) -> Vec<Plan> {
        list(&self.config, &[], &[]).unwrap()
    }
}

#[test]
fn plans_are_listed_newest_first_and_seven_are_kept() {
    let h = host();
    let made: Vec<Plan> = (1..=9)
        .map(|n| h.plan(n * 100, Delivery::Resulted, None))
        .collect();
    // A plan still being made is kept even when it is old.
    let making = h.plan(50, Delivery::Pending, None);
    let ids = |plans: &[Plan]| plans.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
    let listed = h.listed();
    assert_eq!(listed.len(), 10);
    assert_eq!(listed[0].id, made[8].id);
    start::prune(&shelf(&h.config).unwrap());
    let kept = h.listed();
    let mut want: Vec<String> = made.iter().rev().take(KEEP).map(|p| p.id.clone()).collect();
    want.push(making.id.clone());
    assert_eq!(ids(&kept), want);
}

#[test]
fn a_plans_notes_apply_to_the_overall_root_at_its_rev() {
    let h = host();
    let text = "# Overall\n- Mornings are for the report.\n";
    let proposal = Notes {
        text: text.into(),
        base: base_of(b""),
    };
    let p = h.plan(100, Delivery::Resulted, Some(proposal.clone()));
    let stale = apply_notes(&h.config, &p.id, 1, &proposal.base).unwrap_err();
    assert_eq!(stale.code, ErrorCode::Conflict);
    let bare = h.plan(200, Delivery::Resulted, None);
    let none = apply_notes(&h.config, &bare.id, 2, &proposal.base).unwrap_err();
    assert_eq!(none.code, ErrorCode::BadRequest);
    let Applied::Plan(entry) = apply_notes(&h.config, &p.id, 2, &proposal.base).unwrap() else {
        panic!("a plan's notes reply with the plan")
    };
    assert_eq!(entry.id, p.id);
    assert_eq!(notes::read(&h.root).unwrap().text, text);
    let again = apply_notes(&h.config, &p.id, 2, &proposal.base).unwrap_err();
    assert_eq!(again.code, ErrorCode::Conflict, "the live file changed");
}
