//! The loop's decisions (ADR 0019 section 3), as a pure function of the
//! stored loop, what this process remembers, and one tick's observation.
//! Nothing here reads tmux or the clock, so every rule is unit-tested.

use muxa_tmux::{AgentState, AgentStatus};

use crate::records::{LastCheck, Loop, LoopState, Reason, Status};

/// A finished turn must still be there this long before the check runs. Seconds.
pub const SETTLE: u64 = 5;
/// A prompt that finds no moment to be typed in this time is given up. Seconds.
pub const TYPE_WAIT: u64 = 30;
/// An old finished turn this long after a typed prompt means it did not arrive. Seconds.
pub const STALL: u64 = 60;

/// The Task's agent, as the tick found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    /// No linked session, no live agent pane, or a shell in its foreground.
    Missing,
    Present(Option<AgentStatus>),
}

/// How a check ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckEnd {
    Exited { code: i32, seconds: u64 },
    TimedOut { seconds: u64 },
}

/// The check this process runs for the Task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// None runs: after a daemon restart, a `checking` loop has none.
    Idle,
    Running,
    Ended(CheckEnd),
}

/// One tick's view of a Task. Times are Unix epoch seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    pub now: u64,
    pub status: Status,
    pub agent: Agent,
    /// The newest run's `startedAt`; an older agent state is ignored.
    pub run_started_at: Option<u64>,
    pub check: Check,
    /// No person typed on the Task's session in the last 5 seconds.
    pub quiet: bool,
}

/// What only this process knows. Lost on a restart, which costs at most a
/// rerun of the check (ADR 0019, "Daemon restart").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Memory {
    /// The finished turn's `since`, and when this process first saw it.
    pub settling: Option<(u64, u64)>,
    /// When this process typed the prompt that armed the loop.
    pub typed_at: Option<u64>,
    /// When an old finished turn was first seen after that prompt.
    pub stale_since: Option<u64>,
    /// A failed check whose prompt is not typed yet, and since when.
    pub owed: Option<(LastCheck, u64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    None,
    /// Start check number `attempt`. The loop given is already `checking`.
    RunCheck {
        attempt: u32,
    },
    /// Type the fail prompt for check `attempt`. Only once the keys are
    /// sent is `then` written, with its `armedAt` the time typing began.
    Type {
        attempt: u32,
        then: Box<Loop>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub state: Loop,
    pub memory: Memory,
    pub action: Action,
}

impl Step {
    fn keep(lp: &Loop, memory: Memory) -> Step {
        Step {
            state: lp.clone(),
            memory,
            action: Action::None,
        }
    }

    fn to(state: Loop) -> Step {
        Step {
            state,
            memory: Memory::default(),
            action: Action::None,
        }
    }
}

/// The agent state, unless it is older than the newest run: a state tag
/// survives `respawn-pane`.
fn current(agent: Option<AgentStatus>, run_started_at: Option<u64>) -> Option<AgentStatus> {
    let s = agent?;
    match (s.since, run_started_at) {
        (Some(since), Some(start)) if since < start => None,
        _ => Some(s),
    }
}

fn is_turn_end(s: AgentStatus) -> bool {
    matches!(s.state, AgentState::Idle | AgentState::Done)
}

/// `idle` or `done`, at or after `armed_at`. A state with no `since` never is.
pub fn finished_turn(s: Option<AgentStatus>, armed_at: u64) -> bool {
    s.is_some_and(|s| is_turn_end(s) && s.since.is_some_and(|t| t >= armed_at))
}

pub fn step(lp: &Loop, memory: &Memory, obs: &Observation) -> Step {
    if !lp.active() {
        return Step::to(lp.clone());
    }
    let now = obs.now;
    if obs.status != Status::Open {
        return Step::to(lp.stopped(Reason::Status, now));
    }
    let status = match obs.agent {
        Agent::Missing => return Step::to(lp.stopped(Reason::NoAgent, now)),
        Agent::Present(s) => current(s, obs.run_started_at),
    };
    if status.is_some_and(|s| s.state == AgentState::Error) {
        return Step::to(lp.stopped(Reason::Error, now));
    }
    match lp.state {
        LoopState::Paused => paused(lp, status, now),
        LoopState::Checking => checking(lp, memory, obs, status),
        _ => armed(lp, memory, status, now),
    }
}

fn paused(lp: &Loop, status: Option<AgentStatus>, now: u64) -> Step {
    if status.is_some_and(|s| s.state == AgentState::Waiting) {
        return Step::keep(lp, Memory::default());
    }
    // The turn the user's answer lets finish began after the pause.
    Step::to(Loop {
        state: LoopState::Armed,
        reason: None,
        armed_at: lp.since,
        since: now,
        ..lp.clone()
    })
}

fn armed(lp: &Loop, memory: &Memory, status: Option<AgentStatus>, now: u64) -> Step {
    let Some(s) = status else {
        return Step::keep(lp, Memory::default());
    };
    if s.state == AgentState::Waiting {
        return Step::to(Loop {
            state: LoopState::Paused,
            reason: Some(Reason::Waiting),
            since: now,
            ..lp.clone()
        });
    }
    let typed = Memory {
        typed_at: memory.typed_at,
        ..Memory::default()
    };
    if finished_turn(Some(s), lp.armed_at) {
        let since = s.since.unwrap_or(now);
        let seen_at = match memory.settling {
            Some((t, at)) if t == since => at,
            _ => now,
        };
        if now.saturating_sub(seen_at) < SETTLE {
            return Step::keep(
                lp,
                Memory {
                    settling: Some((since, seen_at)),
                    ..typed
                },
            );
        }
        let attempt = lp.attempts + 1;
        return Step {
            state: Loop {
                state: LoopState::Checking,
                since: now,
                ..lp.clone()
            },
            memory: Memory::default(),
            action: Action::RunCheck { attempt },
        };
    }
    if is_turn_end(s) && memory.typed_at.is_some() {
        let stale_since = memory.stale_since.unwrap_or(now);
        if now.saturating_sub(stale_since) > STALL {
            return Step::to(lp.stopped(Reason::Stalled, now));
        }
        return Step::keep(
            lp,
            Memory {
                stale_since: Some(stale_since),
                ..typed
            },
        );
    }
    Step::keep(lp, typed)
}

fn checking(lp: &Loop, memory: &Memory, obs: &Observation, status: Option<AgentStatus>) -> Step {
    let now = obs.now;
    if let Some((result, owed_since)) = &memory.owed {
        return owe(lp, memory, obs, status, result, *owed_since);
    }
    let end = match obs.check {
        Check::Running => return Step::keep(lp, Memory::default()),
        // No check runs here: this process restarted. The attempt does not count.
        Check::Idle => return Step::to(rearm(lp, now)),
        Check::Ended(end) => end,
    };
    // A check that hangs hangs again, whatever the agent did meanwhile.
    let timed_out = matches!(end, CheckEnd::TimedOut { .. });
    if !timed_out && !finished_turn(status, lp.armed_at) {
        // The agent moved on while the check ran; the result says nothing.
        return Step::to(rearm(lp, now));
    }
    let attempt = lp.attempts + 1;
    let (exit, seconds) = match end {
        CheckEnd::Exited { code, seconds } => (Some(code), seconds),
        CheckEnd::TimedOut { seconds } => (None, seconds),
    };
    let last = LastCheck {
        attempt,
        exit,
        seconds,
        at: now,
    };
    let with = |state, reason, attempts| Loop {
        state,
        reason,
        attempts,
        since: now,
        last_check: Some(last.clone()),
        ..lp.clone()
    };
    match exit {
        None => Step::to(with(LoopState::Stopped, Some(Reason::Timeout), lp.attempts)),
        Some(0) => Step::to(with(LoopState::Passed, None, lp.attempts)),
        Some(_) if attempt >= lp.max_attempts => {
            Step::to(with(LoopState::Stopped, Some(Reason::Limit), attempt))
        }
        Some(_) => {
            let memory = Memory {
                owed: Some((last.clone(), now)),
                ..Memory::default()
            };
            owe(lp, &memory, obs, status, &last, now)
        }
    }
}

/// A failed check's prompt: typed on a finished turn with no person typing,
/// else waited for, at most [`TYPE_WAIT`].
fn owe(
    lp: &Loop,
    memory: &Memory,
    obs: &Observation,
    status: Option<AgentStatus>,
    result: &LastCheck,
    owed_since: u64,
) -> Step {
    let now = obs.now;
    let failed = Loop {
        attempts: result.attempt,
        last_check: Some(result.clone()),
        ..lp.clone()
    };
    if finished_turn(status, lp.armed_at) && obs.quiet {
        let then = Loop {
            state: LoopState::Armed,
            armed_at: now,
            since: now,
            ..failed
        };
        return Step {
            state: lp.clone(),
            memory: memory.clone(),
            action: Action::Type {
                attempt: result.attempt,
                then: Box::new(then),
            },
        };
    }
    if now.saturating_sub(owed_since) >= TYPE_WAIT {
        return Step::to(failed.stopped(Reason::NotSent, now));
    }
    Step::keep(lp, memory.clone())
}

fn rearm(lp: &Loop, now: u64) -> Loop {
    Loop {
        state: LoopState::Armed,
        since: now,
        ..lp.clone()
    }
}

/// The memory after the prompt of a [`Action::Type`] was typed at `at`.
pub fn typed(at: u64) -> Memory {
    Memory {
        typed_at: Some(at),
        ..Memory::default()
    }
}

/// What a write of `new` over `old` also changes in the record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effects {
    /// Move the status from `open` to `review`, and from nothing else.
    pub review: bool,
    pub note: Option<String>,
}

pub fn effects(old: &Loop, new: &Loop) -> Effects {
    if old.state == new.state && old.reason == new.reason {
        return Effects {
            review: false,
            note: None,
        };
    }
    let attempt = new.last_check.as_ref().map_or(new.attempts, |c| c.attempt);
    let note = match (&new.state, &new.reason) {
        (LoopState::Passed, _) => Some(format!("The check passed on attempt {attempt}.")),
        (LoopState::Stopped, Some(reason)) => stop_note(reason, new),
        _ => None,
    };
    Effects {
        review: new.state == LoopState::Passed,
        note,
    }
}

/// One sentence, with no check output (ADR 0019 section 3).
fn stop_note(reason: &Reason, lp: &Loop) -> Option<String> {
    Some(match reason {
        Reason::Limit => format!(
            "The loop stopped: the check failed {} times. Its output is in .muxa/CHECK.md.",
            lp.attempts
        ),
        Reason::Timeout => format!(
            "The loop stopped: the check ran longer than {} seconds.",
            lp.timeout
        ),
        Reason::Error => "The loop stopped: the agent reported an error.".into(),
        Reason::NoAgent => "The loop stopped: the Task has no running agent.".into(),
        Reason::NotSent => {
            "The loop stopped: the fail prompt could not be typed within 30 seconds.".into()
        }
        Reason::Stalled => {
            "The loop stopped: the agent did not start a turn after the prompt.".into()
        }
        Reason::User => "The loop was stopped.".into(),
        Reason::Status | Reason::Waiting | Reason::Unknown(_) => return None,
    })
}

#[cfg(test)]
#[path = "step_tests.rs"]
mod tests;
