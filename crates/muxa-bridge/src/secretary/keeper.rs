//! When the daemon starts the secretary's agent again (ADR 0056 §2): at once
//! the first time, then after a growing wait, and at most [`Limits::restarts`]
//! times in [`Limits::window`]. Past that it gives up until the agent runs
//! again, so an agent that exits at start cannot loop.

/// Seconds, so tests can make them short.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub window: u64,
    /// Starts after the first one within `window`.
    pub restarts: usize,
    /// The wait before restart n (1-based) is `backoff[n - 1]`, the last one
    /// repeating.
    pub backoff: [u64; 3],
}

impl Limits {
    pub const DEFAULT: Self = Self {
        window: 10 * 60,
        restarts: 3,
        backoff: [5, 30, 120],
    };
}

/// What the daemon saw of the secretary's agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seen {
    /// Its agent pane is alive.
    Running,
    /// Its session is gone, or its agent pane is dead.
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Nothing,
    Start,
    /// Too many starts: say so once, then wait for the agent to run again.
    GiveUp,
}

#[derive(Debug, Default)]
pub struct Keeper {
    /// The daemon's starts within the window, oldest first.
    starts: Vec<u64>,
    gave_up: bool,
}

impl Keeper {
    pub fn decide(&mut self, now: u64, seen: Seen, limits: &Limits) -> Act {
        if seen == Seen::Running {
            // The user opened it, or a start held: the cap starts over.
            self.gave_up = false;
            return Act::Nothing;
        }
        if self.gave_up {
            return Act::Nothing;
        }
        self.starts
            .retain(|at| now.saturating_sub(*at) < limits.window);
        if self.starts.len() > limits.restarts {
            self.gave_up = true;
            return Act::GiveUp;
        }
        if let Some(last) = self.starts.last() {
            let n = self.starts.len().min(limits.backoff.len());
            if now.saturating_sub(*last) < limits.backoff[n - 1] {
                return Act::Nothing;
            }
        }
        self.starts.push(now);
        Act::Start
    }

    /// Retry on the give-up item (ADR 0061): the cap starts over.
    pub fn reset(&mut self) {
        self.starts.clear();
        self.gave_up = false;
    }

    #[cfg(test)]
    fn gave_up(&self) -> bool {
        self.gave_up
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: Limits = Limits::DEFAULT;

    #[test]
    fn the_first_start_is_at_once_and_restarts_wait_longer_each_time() {
        let mut k = Keeper::default();
        assert_eq!(k.decide(1000, Seen::Stopped, &L), Act::Start);
        assert_eq!(k.decide(1004, Seen::Stopped, &L), Act::Nothing);
        assert_eq!(k.decide(1005, Seen::Stopped, &L), Act::Start);
        assert_eq!(k.decide(1034, Seen::Stopped, &L), Act::Nothing);
        assert_eq!(k.decide(1035, Seen::Stopped, &L), Act::Start);
        assert_eq!(k.decide(1154, Seen::Stopped, &L), Act::Nothing);
        assert_eq!(k.decide(1155, Seen::Stopped, &L), Act::Start);
    }

    #[test]
    fn a_crash_loop_stops_after_three_restarts_in_ten_minutes() {
        let mut k = Keeper::default();
        let mut now = 0;
        let mut acts = Vec::new();
        while now < 600 {
            let act = k.decide(now, Seen::Stopped, &L);
            if act != Act::Nothing {
                acts.push(act);
            }
            now += 1;
        }
        assert_eq!(
            acts,
            [Act::Start, Act::Start, Act::Start, Act::Start, Act::GiveUp]
        );
        assert!(k.gave_up());
        assert_eq!(
            k.decide(5000, Seen::Stopped, &L),
            Act::Nothing,
            "it stays given up"
        );
    }

    #[test]
    fn a_running_agent_ends_giving_up_and_old_starts_leave_the_window() {
        let mut k = Keeper::default();
        for at in [0, 5, 35, 155] {
            assert_eq!(k.decide(at, Seen::Stopped, &L), Act::Start);
        }
        assert_eq!(k.decide(400, Seen::Stopped, &L), Act::GiveUp);
        assert_eq!(k.decide(401, Seen::Running, &L), Act::Nothing);
        assert!(!k.gave_up());
        // 0, 5 and 35 have left the window; one restart after 155 is allowed.
        assert_eq!(k.decide(640, Seen::Stopped, &L), Act::Start);
    }

    #[test]
    fn an_agent_that_runs_long_restarts_at_once() {
        let mut k = Keeper::default();
        assert_eq!(k.decide(0, Seen::Stopped, &L), Act::Start);
        assert_eq!(k.decide(10, Seen::Running, &L), Act::Nothing);
        assert_eq!(k.decide(3600, Seen::Stopped, &L), Act::Start);
    }

    #[test]
    fn retry_starts_at_once_with_a_fresh_cap() {
        let mut k = Keeper::default();
        for at in [0, 5, 35, 155] {
            assert_eq!(k.decide(at, Seen::Stopped, &L), Act::Start);
        }
        assert_eq!(k.decide(400, Seen::Stopped, &L), Act::GiveUp);
        k.reset();
        assert!(!k.gave_up());
        let mut acts = Vec::new();
        for now in 401..1000 {
            if let a @ (Act::Start | Act::GiveUp) = k.decide(now, Seen::Stopped, &L) {
                acts.push((now, a));
            }
        }
        assert_eq!(acts[0], (401, Act::Start), "{acts:?}");
        assert_eq!(acts.last().map(|a| a.1), Some(Act::GiveUp), "{acts:?}");
        assert_eq!(acts.len(), 5, "four starts, then the cap again: {acts:?}");
    }
}
