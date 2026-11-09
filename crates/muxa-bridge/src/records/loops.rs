//! A Task's loop as the record stores it (ADR 0019): the check, its limits,
//! and where the loop stands. The engine that moves it is `loops/`. The check
//! command may hold anything: `Debug` leaves it out, and no error names it.

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{Record, Workspace};

/// Bytes.
pub const MAX_CHECK: usize = 4096;
pub const DEFAULT_MAX_ATTEMPTS: u32 = 3;
pub const MAX_ATTEMPTS: u32 = 10;
/// Seconds.
pub const DEFAULT_TIMEOUT: u64 = 600;
pub const MIN_TIMEOUT: u64 = 10;
pub const MAX_TIMEOUT: u64 = 3600;
const FIELD: &str = "loop";

/// The record's `loop`. Times are Unix epoch seconds.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Loop {
    pub check: String,
    pub max_attempts: u32,
    pub timeout: u64,
    pub state: LoopState,
    /// Failed checks so far.
    pub attempts: u32,
    /// Only a turn that finished at or after this counts.
    pub armed_at: u64,
    /// When `state` last changed.
    pub since: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<Reason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check: Option<LastCheck>,
    /// Fields written by a newer muxa; kept on rewrite.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl fmt::Debug for Loop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Loop")
            .field("check", &format_args!("<{} bytes>", self.check.len()))
            .field("max_attempts", &self.max_attempts)
            .field("timeout", &self.timeout)
            .field("state", &self.state)
            .field("attempts", &self.attempts)
            .field("armed_at", &self.armed_at)
            .field("since", &self.since)
            .field("reason", &self.reason)
            .field("last_check", &self.last_check)
            .finish()
    }
}

/// An unknown value from a newer muxa is kept as written, and means nothing here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum LoopState {
    Armed,
    Checking,
    Paused,
    Passed,
    Stopped,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Reason {
    Waiting,
    Limit,
    Timeout,
    Error,
    NoAgent,
    NotSent,
    Stalled,
    Status,
    User,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastCheck {
    pub attempt: u32,
    /// Absent when the check timed out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<i32>,
    pub seconds: u64,
    pub at: u64,
}

/// What a client asks for: `start-loop`, or `create-task` with a loop.
#[derive(Clone, PartialEq, Eq)]
pub struct LoopSpec {
    pub check: String,
    pub max_attempts: Option<u32>,
    pub timeout: Option<u64>,
}

/// A workspace's `[workspace.loop]` `checks`: the host's control over which
/// commands a client may have run (ADR 0019 section 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checks {
    Any,
    List(Vec<String>),
}

impl From<String> for LoopState {
    fn from(s: String) -> Self {
        match s.as_str() {
            "armed" => Self::Armed,
            "checking" => Self::Checking,
            "paused" => Self::Paused,
            "passed" => Self::Passed,
            "stopped" => Self::Stopped,
            _ => Self::Unknown(s),
        }
    }
}

impl From<LoopState> for String {
    fn from(s: LoopState) -> Self {
        match s {
            LoopState::Armed => "armed".into(),
            LoopState::Checking => "checking".into(),
            LoopState::Paused => "paused".into(),
            LoopState::Passed => "passed".into(),
            LoopState::Stopped => "stopped".into(),
            LoopState::Unknown(s) => s,
        }
    }
}

impl From<String> for Reason {
    fn from(s: String) -> Self {
        match s.as_str() {
            "waiting" => Self::Waiting,
            "limit" => Self::Limit,
            "timeout" => Self::Timeout,
            "error" => Self::Error,
            "no-agent" => Self::NoAgent,
            "not-sent" => Self::NotSent,
            "stalled" => Self::Stalled,
            "status" => Self::Status,
            "user" => Self::User,
            _ => Self::Unknown(s),
        }
    }
}

impl From<Reason> for String {
    fn from(r: Reason) -> Self {
        match r {
            Reason::Waiting => "waiting".into(),
            Reason::Limit => "limit".into(),
            Reason::Timeout => "timeout".into(),
            Reason::Error => "error".into(),
            Reason::NoAgent => "no-agent".into(),
            Reason::NotSent => "not-sent".into(),
            Reason::Stalled => "stalled".into(),
            Reason::Status => "status".into(),
            Reason::User => "user".into(),
            Reason::Unknown(s) => s,
        }
    }
}

impl Loop {
    /// The loop works on it: not ended, and a state this muxa knows.
    pub fn active(&self) -> bool {
        matches!(
            self.state,
            LoopState::Armed | LoopState::Checking | LoopState::Paused
        )
    }

    /// `start-loop`: armed from `now`. The attempts go on from `previous`
    /// unless `reset`.
    pub fn start(spec: LoopSpec, previous: Option<&Loop>, reset: bool, now: u64) -> Loop {
        let attempts = match previous {
            Some(p) if !reset => p.attempts,
            _ => 0,
        };
        Loop {
            check: spec.check,
            max_attempts: spec.max_attempts.unwrap_or(DEFAULT_MAX_ATTEMPTS),
            timeout: spec.timeout.unwrap_or(DEFAULT_TIMEOUT),
            state: LoopState::Armed,
            attempts,
            armed_at: now,
            since: now,
            reason: None,
            last_check: previous.and_then(|p| p.last_check.clone()),
            extra: previous.map(|p| p.extra.clone()).unwrap_or_default(),
        }
    }

    /// Stopped with `not-sent` before any check ran: the start prompt was
    /// never typed (`handoff::give_up`), and the start's failure says so.
    pub fn never_started(&self) -> bool {
        self.state == LoopState::Stopped
            && self.reason == Some(Reason::NotSent)
            && self.last_check.is_none()
    }

    /// Ended for `reason`. An ended loop stays as it is.
    pub fn stopped(&self, reason: Reason, now: u64) -> Loop {
        if !self.active() {
            return self.clone();
        }
        Loop {
            state: LoopState::Stopped,
            reason: Some(reason),
            since: now,
            ..self.clone()
        }
    }
}

impl LoopSpec {
    /// Checks the limits, and that `checks` allows the command. The messages
    /// never quote the command.
    pub fn check(&self, checks: Option<&Checks>) -> Result<(), String> {
        let checks = checks.ok_or_else(|| {
            "this workspace has no loops: add a [workspace.loop] table to workspaces.toml \
             on the host"
                .to_owned()
        })?;
        if let Some(n) = self.max_attempts
            && !(1..=MAX_ATTEMPTS).contains(&n)
        {
            return Err(format!("maxAttempts must be 1 to {MAX_ATTEMPTS}"));
        }
        if let Some(t) = self.timeout
            && !(MIN_TIMEOUT..=MAX_TIMEOUT).contains(&t)
        {
            return Err(format!(
                "timeout must be {MIN_TIMEOUT} to {MAX_TIMEOUT} seconds"
            ));
        }
        command_shape(&self.check)?;
        match checks {
            Checks::Any => Ok(()),
            Checks::List(list) if list.contains(&self.check) => Ok(()),
            Checks::List(_) => {
                Err("the check is not one of the workspace's checks in workspaces.toml".to_owned())
            }
        }
    }
}

/// Not empty, at most [`MAX_CHECK`] bytes, no control character.
pub fn command_shape(check: &str) -> Result<(), String> {
    if check.trim().is_empty() {
        return Err("the check command is empty".into());
    }
    if check.len() > MAX_CHECK {
        return Err(format!(
            "the check command is longer than {MAX_CHECK} bytes"
        ));
    }
    if check.chars().any(char::is_control) {
        return Err("the check command has a control character".into());
    }
    Ok(())
}

impl Workspace {
    /// Only a Task inside its workspace can have a loop: a repo Task in one
    /// of its repos (ADR 0031), or a Task without a repo under the root.
    pub fn may_loop(&self, record: &Record) -> Result<(), String> {
        let inside = match (&record.repo, &record.dir) {
            (Some(repo), _) => self.has_repo(&canonical(repo)),
            (None, Some(dir)) => canonical(dir).starts_with(&self.root),
            (None, None) => false,
        };
        if inside {
            Ok(())
        } else {
            Err(format!(
                "task {} is outside workspace {:?}: a loop needs one of its repos, \
                 or a directory under its root",
                record.id, self.name
            ))
        }
    }
}

fn canonical(p: &Path) -> std::path::PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_owned())
}

impl Record {
    /// The record's `loop`, None when the Task has none.
    pub fn task_loop(&self) -> Result<Option<Loop>, serde_json::Error> {
        self.extra.get(FIELD).map(Loop::deserialize).transpose()
    }

    pub fn set_loop(&mut self, value: Option<&Loop>) -> Result<(), serde_json::Error> {
        match value {
            Some(l) => {
                self.extra.insert(FIELD.into(), serde_json::to_value(l)?);
            }
            None => {
                self.extra.remove(FIELD);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(check: &str) -> LoopSpec {
        LoopSpec {
            check: check.into(),
            max_attempts: None,
            timeout: None,
        }
    }

    #[test]
    fn the_record_shape_round_trips_and_keeps_unknown_values() {
        let json = r#"{"check":"make check","maxAttempts":3,"timeout":600,"state":"later","attempts":1,"armedAt":10,"since":12,"reason":"newer","lastCheck":{"attempt":1,"exit":2,"seconds":4,"at":11},"more":1}"#;
        let l: Loop = serde_json::from_str(json).unwrap();
        assert_eq!(l.state, LoopState::Unknown("later".into()));
        assert!(!l.active());
        assert_eq!(serde_json::to_string(&l).unwrap(), json);
    }

    #[test]
    fn timed_out_checks_have_no_exit() {
        let c = LastCheck {
            attempt: 2,
            exit: None,
            seconds: 10,
            at: 5,
        };
        assert_eq!(
            serde_json::to_string(&c).unwrap(),
            r#"{"attempt":2,"seconds":10,"at":5}"#
        );
    }

    #[test]
    fn debug_leaves_the_command_out() {
        let l = Loop::start(spec("echo secret-token"), None, false, 1);
        assert!(!format!("{l:?}").contains("secret"));
    }

    #[test]
    fn starting_again_keeps_the_attempts_unless_reset() {
        let mut first = Loop::start(spec("x"), None, false, 1);
        first.attempts = 2;
        let first = first.stopped(Reason::Limit, 5);
        let again = Loop::start(spec("x"), Some(&first), false, 9);
        assert_eq!((again.attempts, again.armed_at), (2, 9));
        assert_eq!(again.state, LoopState::Armed);
        assert_eq!(again.reason, None);
        assert_eq!(Loop::start(spec("x"), Some(&first), true, 9).attempts, 0);
        assert_eq!(again.max_attempts, DEFAULT_MAX_ATTEMPTS);
        assert_eq!(again.timeout, DEFAULT_TIMEOUT);
    }

    #[test]
    fn an_ended_loop_does_not_stop_again() {
        let l = Loop::start(spec("x"), None, false, 1).stopped(Reason::Limit, 5);
        assert_eq!(l.stopped(Reason::User, 9).reason, Some(Reason::Limit));
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("muxa-loops-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    fn parse(text: &str) -> Result<super::super::Workspaces, String> {
        super::super::Workspaces::parse(text, Path::new("w.toml"), None).map_err(|e| e.to_string())
    }

    #[test]
    fn the_loop_table_is_optional_and_checked() {
        let dir = temp("table");
        let root = dir.display();
        let none = parse(&format!("[[workspace]]\nname = \"a\"\nroot = \"{root}\"\n")).unwrap();
        assert_eq!(none.all()[0].loop_checks, None);
        let any = parse(&format!(
            "[[workspace]]\nname = \"a\"\nroot = \"{root}\"\n[workspace.loop]\nchecks = \"any\"\n"
        ))
        .unwrap();
        assert_eq!(any.all()[0].loop_checks, Some(Checks::Any));
        let list = parse(&format!(
            "[[workspace]]\nname = \"a\"\nroot = \"{root}\"\n[workspace.loop]\nchecks = [\"make check\"]\n"
        ))
        .unwrap();
        assert_eq!(
            list.all()[0].loop_checks,
            Some(Checks::List(vec!["make check".into()]))
        );
        for bad in ["checks = \"all\"", "checks = [\"\"]", "other = 1", ""] {
            let e = parse(&format!(
                "[[workspace]]\nname = \"a\"\nroot = \"{root}\"\n[workspace.loop]\n{bad}\n"
            ));
            assert!(e.is_err(), "{bad}");
        }
    }

    fn record(repo: Option<&Path>, dir: Option<&Path>) -> Record {
        Record {
            version: 1,
            id: "t-000001".into(),
            rev: 1,
            workspace: "w".into(),
            title: "T".into(),
            goal: String::new(),
            status: super::super::Status::Open,
            created_at: 0,
            updated_at: 0,
            project: None,
            repo: repo.map(Path::to_owned),
            branch: None,
            dir: dir.map(Path::to_owned),
            session: None,
            note: None,
            runs: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }

    #[test]
    fn only_a_task_inside_its_workspace_may_loop() {
        let dir = temp("inside");
        let (root, repo, other) = (dir.join("root"), dir.join("repo"), dir.join("other"));
        for d in [&root, &repo, &other, &root.join("tasks/x")] {
            std::fs::create_dir_all(d).unwrap();
        }
        let ws = Workspace {
            name: "w".into(),
            root: root.clone(),
            repos: vec![repo.clone()],
            agent: None,
            loop_checks: Some(Checks::Any),
            park_after: None,
            routines: None,
            assistant: None,
        };
        assert!(
            ws.may_loop(&record(Some(&repo), Some(&other))).is_ok(),
            "repos may lie outside the root"
        );
        assert!(
            ws.may_loop(&record(Some(&other), Some(&root.join("tasks/x"))))
                .is_err()
        );
        assert!(
            ws.may_loop(&record(None, Some(&root.join("tasks/x"))))
                .is_ok()
        );
        assert!(ws.may_loop(&record(None, Some(&other))).is_err());
        assert!(ws.may_loop(&record(None, None)).is_err());
    }

    #[test]
    fn the_loop_lives_in_the_record_and_survives_older_code() {
        let mut r = record(None, None);
        assert_eq!(r.task_loop().unwrap(), None);
        let l = Loop::start(spec("x"), None, false, 3);
        r.set_loop(Some(&l)).unwrap();
        let text = serde_json::to_string(&r).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["loop"]["check"], "x");
        assert_eq!(v["loop"]["state"], "armed");
        let back: Record = serde_json::from_str(&text).unwrap();
        assert_eq!(back.task_loop().unwrap(), Some(l));
        r.set_loop(None).unwrap();
        assert_eq!(r.task_loop().unwrap(), None);
    }

    #[test]
    fn the_workspace_decides_which_checks_run() {
        let list = Checks::List(vec!["make check".into()]);
        assert!(spec("make check").check(Some(&list)).is_ok());
        let e = spec("rm -rf ~").check(Some(&list)).unwrap_err();
        assert!(!e.contains("rm"), "the message never quotes the command");
        assert!(
            spec("make check")
                .check(None)
                .unwrap_err()
                .contains("[workspace.loop]")
        );
        assert!(spec("anything").check(Some(&Checks::Any)).is_ok());
        assert!(spec("a\nb").check(Some(&Checks::Any)).is_err());
        assert!(spec(" ").check(Some(&Checks::Any)).is_err());
        assert!(
            spec(&"x".repeat(MAX_CHECK + 1))
                .check(Some(&Checks::Any))
                .is_err()
        );
        let limits = |m, t| LoopSpec {
            check: "x".into(),
            max_attempts: m,
            timeout: t,
        };
        assert!(limits(Some(0), None).check(Some(&Checks::Any)).is_err());
        assert!(limits(Some(11), None).check(Some(&Checks::Any)).is_err());
        assert!(limits(Some(10), Some(10)).check(Some(&Checks::Any)).is_ok());
        assert!(limits(None, Some(9)).check(Some(&Checks::Any)).is_err());
        assert!(limits(None, Some(3601)).check(Some(&Checks::Any)).is_err());
    }
}
