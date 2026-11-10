use super::*;

fn chain(step: u32) -> Chain {
    let staff: Vec<String> = ["lead", "strategist", "architect", "security"]
        .map(String::from)
        .into();
    Chain {
        workers: staff[..step as usize].to_vec(),
        staff,
        step,
        max_steps: 3,
        handoffs: Handoffs::Review,
    }
}

fn raw(worker: &str, purpose: &str) -> RawHandoff {
    RawHandoff {
        worker: worker.into(),
        purpose: purpose.into(),
    }
}

fn reasons(raw: RawHandoff, chain: &Chain) -> Vec<String> {
    let mut out = Vec::new();
    let found = check(raw, chain, &mut out);
    assert_eq!(found.is_some(), out.is_empty());
    out
}

#[test]
fn a_hand_off_to_a_member_not_yet_in_the_chain_passes() {
    let mut out = Vec::new();
    let h = check(raw("architect", "Check the gateway."), &chain(2), &mut out).unwrap();
    assert_eq!(
        (h.worker.as_str(), h.purpose.as_str()),
        ("architect", "Check the gateway.")
    );
}

#[test]
fn counterexamples_are_refused_without_quoting_the_agent() {
    let secret = "secret-purpose-mark";
    let cases = [
        (raw("strategist", secret), chain(2), "this run's own worker"),
        (raw("lead", secret), chain(2), "already took a step"),
        (
            raw("sales-mark", secret),
            chain(2),
            "must be a member of the staff",
        ),
        (
            raw("security", secret),
            chain(3),
            "the last step (3 of at most 3)",
        ),
        (
            raw("architect", ""),
            chain(2),
            "handoff.purpose must be 1 to 1024 bytes",
        ),
        (
            raw("architect", &"x".repeat(MAX_PURPOSE + 1)),
            chain(2),
            "handoff.purpose",
        ),
        (raw("architect", "a\u{7}b"), chain(2), "handoff.purpose"),
        (
            raw("architect", secret),
            Chain {
                handoffs: Handoffs::None,
                ..chain(2)
            },
            "hand-offs are off",
        ),
        (raw("architect", secret), Chain::default(), "no staff"),
    ];
    for (r, c, needle) in cases {
        let out = reasons(r, &c);
        assert!(out.iter().any(|r| r.contains(needle)), "{needle}: {out:?}");
        assert!(out.iter().all(|r| !r.contains("-mark")), "{out:?}");
    }
}
