use super::*;
use crate::records::LoopSpec;

const T0: u64 = 1_000;

fn armed() -> Loop {
    Loop::start(
        LoopSpec {
            check: "make check".into(),
            max_attempts: Some(3),
            timeout: Some(60),
        },
        None,
        false,
        T0,
    )
}

fn agent(state: AgentState, since: Option<u64>) -> Agent {
    Agent::Present(Some(AgentStatus { state, since }))
}

fn obs(now: u64, agent: Agent) -> Observation {
    Observation {
        now,
        status: Status::Open,
        agent,
        run_started_at: None,
        check: Check::Idle,
        quiet: true,
    }
}

/// Steps `lp` through the observations, feeding each step's memory to the next.
fn run(lp: &Loop, list: &[Observation]) -> Step {
    let mut s = Step::to(lp.clone());
    for o in list {
        s = step(&s.state, &s.memory, o);
    }
    s
}

fn checking(lp: &Loop) -> Loop {
    Loop {
        state: LoopState::Checking,
        ..lp.clone()
    }
}

#[test]
fn a_finished_turn_is_idle_or_done_at_or_after_armed_at() {
    for state in [AgentState::Idle, AgentState::Done] {
        let s = |since| Some(AgentStatus { state, since });
        assert!(finished_turn(s(Some(T0)), T0));
        assert!(finished_turn(s(Some(T0 + 1)), T0));
        assert!(!finished_turn(s(Some(T0 - 1)), T0), "older than armedAt");
        assert!(!finished_turn(s(None), T0), "no since");
    }
    for state in [AgentState::Running, AgentState::Waiting, AgentState::Error] {
        assert!(!finished_turn(
            Some(AgentStatus {
                state,
                since: Some(T0 + 1)
            }),
            T0
        ));
    }
    assert!(!finished_turn(None, T0));
}

#[test]
fn the_check_runs_on_a_finished_turn_still_there_5_seconds_later() {
    for state in [AgentState::Done, AgentState::Idle] {
        let a = agent(state, Some(T0 + 10));
        let first = run(&armed(), &[obs(T0 + 10, a)]);
        assert_eq!(first.action, Action::None);
        assert_eq!(first.state.state, LoopState::Armed);
        let early = run(&armed(), &[obs(T0 + 10, a), obs(T0 + 14, a)]);
        assert_eq!(early.action, Action::None);
        let s = run(&armed(), &[obs(T0 + 10, a), obs(T0 + 15, a)]);
        assert_eq!(s.action, Action::RunCheck { attempt: 1 });
        assert_eq!(s.state.state, LoopState::Checking);
        assert_eq!(s.state.since, T0 + 15);
    }
}

#[test]
fn a_new_finished_turn_settles_again() {
    let s = run(
        &armed(),
        &[
            obs(T0 + 10, agent(AgentState::Done, Some(T0 + 10))),
            obs(T0 + 13, agent(AgentState::Idle, Some(T0 + 13))),
            obs(T0 + 16, agent(AgentState::Idle, Some(T0 + 13))),
        ],
    );
    assert_eq!(s.action, Action::None, "the turn of T0+13 is only 3 s old");
    let s = run(
        &armed(),
        &[
            obs(T0 + 10, agent(AgentState::Done, Some(T0 + 10))),
            obs(T0 + 12, agent(AgentState::Running, Some(T0 + 12))),
            obs(T0 + 16, agent(AgentState::Done, Some(T0 + 10))),
        ],
    );
    assert_eq!(s.action, Action::None, "running in between resets the wait");
}

#[test]
fn a_stale_since_or_no_since_never_runs_the_check() {
    let s = run(
        &armed(),
        &[
            obs(T0 + 10, agent(AgentState::Done, Some(T0 - 5))),
            obs(T0 + 30, agent(AgentState::Done, Some(T0 - 5))),
        ],
    );
    assert_eq!((s.action, s.state.state), (Action::None, LoopState::Armed));
    let s = run(
        &armed(),
        &[
            obs(T0 + 10, agent(AgentState::Idle, None)),
            obs(T0 + 30, agent(AgentState::Idle, None)),
        ],
    );
    assert_eq!((s.action, s.state.state), (Action::None, LoopState::Armed));
}

#[test]
fn a_state_older_than_the_newest_run_is_ignored() {
    let run_at = |o: Observation| Observation {
        run_started_at: Some(T0 + 50),
        ..o
    };
    let old_done = agent(AgentState::Done, Some(T0 + 20));
    let s = run(
        &armed(),
        &[
            run_at(obs(T0 + 60, old_done)),
            run_at(obs(T0 + 70, old_done)),
        ],
    );
    assert_eq!((s.action, s.state.state), (Action::None, LoopState::Armed));
    let old_error = agent(AgentState::Error, Some(T0 + 20));
    assert_eq!(
        run(&armed(), &[run_at(obs(T0 + 60, old_error))])
            .state
            .state,
        LoopState::Armed
    );
    let old_waiting = agent(AgentState::Waiting, Some(T0 + 20));
    assert_eq!(
        run(&armed(), &[run_at(obs(T0 + 60, old_waiting))])
            .state
            .state,
        LoopState::Armed
    );
    let new_error = agent(AgentState::Error, Some(T0 + 55));
    assert_eq!(
        run(&armed(), &[run_at(obs(T0 + 60, new_error))])
            .state
            .reason,
        Some(Reason::Error)
    );
}

#[test]
fn a_pass_ends_the_loop_and_asks_for_review() {
    let lp = checking(&armed());
    let a = agent(AgentState::Done, Some(T0 + 10));
    let o = Observation {
        check: Check::Ended(CheckEnd::Exited {
            code: 0,
            seconds: 7,
        }),
        ..obs(T0 + 30, a)
    };
    let s = step(&lp, &Memory::default(), &o);
    assert_eq!(s.state.state, LoopState::Passed);
    assert_eq!(s.state.attempts, 0);
    assert_eq!(
        s.state.last_check,
        Some(LastCheck {
            attempt: 1,
            exit: Some(0),
            seconds: 7,
            at: T0 + 30
        })
    );
    let e = effects(&lp, &s.state);
    assert!(e.review);
    assert_eq!(e.note.as_deref(), Some("The check passed on attempt 1."));
}

#[test]
fn a_fail_types_the_prompt_and_only_then_arms_again() {
    let lp = checking(&armed());
    let a = agent(AgentState::Done, Some(T0 + 10));
    let o = Observation {
        check: Check::Ended(CheckEnd::Exited {
            code: 2,
            seconds: 3,
        }),
        ..obs(T0 + 30, a)
    };
    let s = step(&lp, &Memory::default(), &o);
    assert_eq!(s.state, lp, "nothing is written before the prompt is typed");
    let Action::Type { attempt, then } = &s.action else {
        panic!("expected a prompt, got {:?}", s.action);
    };
    assert_eq!(*attempt, 1);
    assert_eq!(then.state, LoopState::Armed);
    assert_eq!((then.attempts, then.armed_at), (1, T0 + 30));
    assert_eq!(then.last_check.as_ref().and_then(|c| c.exit), Some(2));
    assert_eq!(
        effects(&lp, then),
        Effects {
            review: false,
            note: None
        }
    );
    // Typing failed: the next tick tries again.
    let again = step(&s.state, &s.memory, &obs(T0 + 32, a));
    assert!(matches!(again.action, Action::Type { attempt: 1, .. }));
}

#[test]
fn the_prompt_waits_while_a_person_types_and_gives_up_after_30_s() {
    let lp = checking(&armed());
    let a = agent(AgentState::Done, Some(T0 + 10));
    let busy = |now, check| Observation {
        check,
        quiet: false,
        ..obs(now, a)
    };
    let s = step(
        &lp,
        &Memory::default(),
        &busy(
            T0 + 30,
            Check::Ended(CheckEnd::Exited {
                code: 1,
                seconds: 3,
            }),
        ),
    );
    assert_eq!(s.action, Action::None);
    let s = step(&s.state, &s.memory, &busy(T0 + 45, Check::Idle));
    assert_eq!(s.action, Action::None);
    let quiet = step(&s.state, &s.memory, &obs(T0 + 50, a));
    assert!(matches!(quiet.action, Action::Type { attempt: 1, .. }));
    let late = step(&s.state, &s.memory, &busy(T0 + 60, Check::Idle));
    assert_eq!(late.state.state, LoopState::Stopped);
    assert_eq!(late.state.reason, Some(Reason::NotSent));
    assert_eq!(late.state.attempts, 1, "the failed check counts");
}

#[test]
fn the_prompt_is_not_typed_into_a_waiting_agent() {
    let lp = checking(&armed());
    let o = Observation {
        check: Check::Ended(CheckEnd::Exited {
            code: 1,
            seconds: 3,
        }),
        ..obs(T0 + 30, agent(AgentState::Done, Some(T0 + 10)))
    };
    let s = step(&lp, &Memory::default(), &o);
    let s = step(
        &s.state,
        &s.memory,
        &obs(T0 + 31, agent(AgentState::Waiting, Some(T0 + 31))),
    );
    assert_eq!(s.action, Action::None);
}

#[test]
fn the_last_allowed_fail_stops_with_limit() {
    let lp = Loop {
        attempts: 2,
        ..checking(&armed())
    };
    let o = Observation {
        check: Check::Ended(CheckEnd::Exited {
            code: 1,
            seconds: 3,
        }),
        ..obs(T0 + 30, agent(AgentState::Idle, Some(T0 + 10)))
    };
    let s = step(&lp, &Memory::default(), &o);
    assert_eq!(s.action, Action::None);
    assert_eq!(
        (s.state.state.clone(), s.state.reason.clone()),
        (LoopState::Stopped, Some(Reason::Limit))
    );
    assert_eq!(s.state.attempts, 3);
    let note = effects(&lp, &s.state).note.unwrap_or_default();
    assert!(note.contains("3 times"));
    assert!(!note.contains("make check"), "no command in the note");
}

#[test]
fn a_timeout_stops_without_an_exit_code() {
    let lp = checking(&armed());
    let o = Observation {
        check: Check::Ended(CheckEnd::TimedOut { seconds: 60 }),
        ..obs(T0 + 90, agent(AgentState::Running, Some(T0 + 80)))
    };
    let s = step(&lp, &Memory::default(), &o);
    assert_eq!(s.state.reason, Some(Reason::Timeout));
    let c = s.state.last_check.unwrap();
    assert_eq!((c.exit, c.seconds, c.attempt), (None, 60, 1));
}

#[test]
fn a_result_after_the_agent_moved_on_is_dropped() {
    let lp = checking(&armed());
    let o = Observation {
        check: Check::Ended(CheckEnd::Exited {
            code: 1,
            seconds: 3,
        }),
        ..obs(T0 + 30, agent(AgentState::Running, Some(T0 + 25)))
    };
    let s = step(&lp, &Memory::default(), &o);
    assert_eq!(s.action, Action::None);
    assert_eq!(s.state.state, LoopState::Armed);
    assert_eq!((s.state.attempts, s.state.armed_at), (0, T0));
}

#[test]
fn a_checking_loop_with_no_check_here_arms_again_and_the_attempt_does_not_count() {
    let lp = Loop {
        attempts: 1,
        ..checking(&armed())
    };
    let s = step(
        &lp,
        &Memory::default(),
        &obs(T0 + 30, agent(AgentState::Done, Some(T0 + 10))),
    );
    assert_eq!(s.state.state, LoopState::Armed);
    assert_eq!(s.state.attempts, 1);
    let running = Observation {
        check: Check::Running,
        ..obs(T0 + 30, agent(AgentState::Done, Some(T0 + 10)))
    };
    assert_eq!(step(&lp, &Memory::default(), &running).state, lp);
}

#[test]
fn waiting_pauses_and_the_next_finished_turn_runs_the_check() {
    let lp = Loop {
        attempts: 1,
        ..armed()
    };
    let s = run(
        &lp,
        &[obs(T0 + 20, agent(AgentState::Waiting, Some(T0 + 20)))],
    );
    assert_eq!(s.state.state, LoopState::Paused);
    assert_eq!(s.state.reason, Some(Reason::Waiting));
    assert_eq!(s.action, Action::None);
    let still = step(
        &s.state,
        &s.memory,
        &obs(T0 + 200, agent(AgentState::Waiting, Some(T0 + 20))),
    );
    assert_eq!(
        (still.state.state.clone(), still.action.clone()),
        (LoopState::Paused, Action::None)
    );
    let back = step(
        &still.state,
        &still.memory,
        &obs(T0 + 210, agent(AgentState::Running, Some(T0 + 205))),
    );
    assert_eq!(back.state.state, LoopState::Armed);
    assert_eq!(
        back.state.armed_at,
        T0 + 20,
        "armedAt is the time it paused"
    );
    assert_eq!(back.state.attempts, 1);
    let done = agent(AgentState::Done, Some(T0 + 220));
    let s = run(&back.state, &[obs(T0 + 220, done), obs(T0 + 225, done)]);
    assert_eq!(s.action, Action::RunCheck { attempt: 2 });
}

#[test]
fn the_engine_never_answers_a_menu() {
    // Whatever a waiting agent shows, the only actions are a check or the fail prompt,
    // and neither is ever chosen while the agent waits.
    let w = agent(AgentState::Waiting, Some(T0 + 20));
    for lp in [
        armed(),
        checking(&armed()),
        Loop {
            state: LoopState::Paused,
            ..armed()
        },
    ] {
        for check in [Check::Idle, Check::Running] {
            for now in [T0 + 20, T0 + 40, T0 + 100] {
                let o = Observation {
                    check,
                    ..obs(now, w)
                };
                assert_eq!(step(&lp, &Memory::default(), &o).action, Action::None);
            }
        }
    }
}

#[test]
fn error_and_a_missing_agent_stop() {
    let s = step(
        &armed(),
        &Memory::default(),
        &obs(T0 + 5, agent(AgentState::Error, Some(T0 + 5))),
    );
    assert_eq!(s.state.reason, Some(Reason::Error));
    let s = step(&armed(), &Memory::default(), &obs(T0 + 5, Agent::Missing));
    assert_eq!(s.state.reason, Some(Reason::NoAgent));
    let note = effects(&armed(), &s.state).note;
    assert!(note.is_some());
}

#[test]
fn a_status_other_than_open_stops_quietly() {
    for status in [Status::Review, Status::Done, Status::Dropped] {
        let o = Observation {
            status,
            ..obs(T0 + 5, agent(AgentState::Running, Some(T0 + 5)))
        };
        let s = step(&checking(&armed()), &Memory::default(), &o);
        assert_eq!(s.state.reason, Some(Reason::Status));
        assert_eq!(effects(&armed(), &s.state).note, None);
        assert!(!effects(&armed(), &s.state).review);
    }
}

#[test]
fn an_old_finished_turn_after_a_typed_prompt_stalls_after_60_s() {
    let lp = Loop {
        armed_at: T0 + 100,
        ..armed()
    };
    let old = agent(AgentState::Done, Some(T0 + 50));
    let s = step(&lp, &typed(T0 + 100), &obs(T0 + 101, old));
    assert_eq!(s.state.state, LoopState::Armed);
    let s = step(&s.state, &s.memory, &obs(T0 + 161, old));
    assert_eq!(
        s.state.state,
        LoopState::Armed,
        "60 s is not more than 60 s"
    );
    let s = step(&s.state, &s.memory, &obs(T0 + 162, old));
    assert_eq!(s.state.reason, Some(Reason::Stalled));
    // A turn that started resets the clock.
    let s = run(
        &lp,
        &[
            obs(T0 + 101, old),
            obs(T0 + 140, agent(AgentState::Running, Some(T0 + 140))),
            obs(T0 + 200, old),
        ],
    );
    assert_eq!(s.state.state, LoopState::Armed);
}

#[test]
fn a_loop_started_on_a_finished_agent_waits_without_stalling() {
    let old = agent(AgentState::Idle, Some(T0 - 100));
    let s = run(&armed(), &[obs(T0 + 1, old), obs(T0 + 500, old)]);
    assert_eq!((s.state.state, s.action), (LoopState::Armed, Action::None));
}

#[test]
fn an_ended_loop_does_nothing() {
    let passed = Loop {
        state: LoopState::Passed,
        ..armed()
    };
    let unknown = Loop {
        state: LoopState::Unknown("later".into()),
        ..armed()
    };
    for lp in [passed, armed().stopped(Reason::User, T0), unknown] {
        let s = step(
            &lp,
            &Memory::default(),
            &obs(T0 + 50, agent(AgentState::Done, Some(T0 + 40))),
        );
        assert_eq!((s.state, s.action), (lp, Action::None));
    }
}
