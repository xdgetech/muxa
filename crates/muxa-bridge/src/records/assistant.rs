//! `[assistant]` and `[workspace.assistant]` in `workspaces.toml` (ADR 0046
//! §1): whether a workspace takes assistant requests, which agent runs them,
//! and how many runs it may have. Parsing only; `assistant/` runs them.
use serde::Deserialize;

use super::workers::Worker;
use super::workspaces::parse_duration;

/// Runs a day across all workspaces when `[assistant]` gives no number.
pub const DEFAULT_HOST_RUNS_PER_DAY: u32 = 20;
pub const DEFAULT_OPEN_RUNS: u32 = 1;
pub const DEFAULT_RUNS_PER_DAY: u32 = 10;
/// Seconds: `"30m"`.
pub const DEFAULT_RUN_TIMEOUT: u64 = 30 * 60;

/// The host's switch and budget (`[assistant]`). No table: off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssistantHost {
    pub enabled: bool,
    pub max_runs_per_day: u32,
}

impl Default for AssistantHost {
    fn default() -> Self {
        Self {
            enabled: false,
            max_runs_per_day: DEFAULT_HOST_RUNS_PER_DAY,
        }
    }
}

/// A workspace that takes requests (`[workspace.assistant]` with `delegate = true`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delegate {
    /// None: the workspace's default agent (ADR 0046 §7).
    pub worker: Option<String>,
    pub max_open_runs: u32,
    pub max_runs_per_day: u32,
    /// Seconds before a run without a result is `timed-out`.
    pub run_timeout: u64,
    /// Who writes a run's proposed notes to the live file (ADR 0047 §1).
    pub notes: NotesMode,
}

/// `notes = "review" | "auto"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NotesMode {
    /// The user applies each proposal with `apply-notes`. A page or document
    /// a run read could plant text that every later run would trust.
    #[default]
    Review,
    /// The host applies a passing proposal at publish, while the base holds.
    Auto,
}

impl NotesMode {
    fn parse(value: Option<&str>) -> Result<Self, String> {
        match value {
            None | Some("review") => Ok(Self::Review),
            Some("auto") => Ok(Self::Auto),
            Some(_) => Err("notes must be \"review\" or \"auto\"".into()),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawHost {
    enabled: bool,
    max_runs_per_day: Option<i64>,
}

impl RawHost {
    pub(super) fn check(self) -> Result<AssistantHost, String> {
        Ok(AssistantHost {
            enabled: self.enabled,
            max_runs_per_day: positive(
                "[assistant] max_runs_per_day",
                self.max_runs_per_day,
                DEFAULT_HOST_RUNS_PER_DAY,
            )?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawDelegate {
    delegate: bool,
    worker: Option<String>,
    max_open_runs: Option<i64>,
    max_runs_per_day: Option<i64>,
    run_timeout: Option<String>,
    notes: Option<String>,
}

impl RawDelegate {
    /// None when `delegate` is false. The other fields are checked anyway,
    /// so a typo fails before the day it is switched on.
    pub(super) fn check(self) -> Result<Option<Delegate>, String> {
        let delegate = Delegate {
            worker: self
                .worker
                .filter(|name| name != muxa_protocol::DEFAULT_WORKER),
            max_open_runs: positive("max_open_runs", self.max_open_runs, DEFAULT_OPEN_RUNS)?,
            max_runs_per_day: positive(
                "max_runs_per_day",
                self.max_runs_per_day,
                DEFAULT_RUNS_PER_DAY,
            )?,
            run_timeout: self
                .run_timeout
                .as_deref()
                .map(parse_duration)
                .transpose()
                .map_err(|r| format!("run_timeout {r}"))?
                .unwrap_or(DEFAULT_RUN_TIMEOUT),
            notes: NotesMode::parse(self.notes.as_deref())?,
        };
        Ok(self.delegate.then_some(delegate))
    }
}

/// The delegate's `worker` must be a configured `[[worker]]` this workspace may use.
pub(super) fn check_worker(
    delegate: &Delegate,
    workspace: &str,
    workers: &[Worker],
) -> Result<(), String> {
    let Some(name) = &delegate.worker else {
        return Ok(());
    };
    let found = workers
        .iter()
        .find(|w| w.name == *name)
        .ok_or("worker is not configured")?;
    if !found.allows(workspace) {
        return Err("worker is not allowed in this workspace".into());
    }
    Ok(())
}

/// TOML integers are signed; reading them as such lets the error name the field.
fn positive(field: &str, value: Option<i64>, default: u32) -> Result<u32, String> {
    match value {
        None => Ok(default),
        Some(n) => u32::try_from(n)
            .ok()
            .filter(|n| *n >= 1)
            .ok_or_else(|| format!("{field} must be a whole number from 1 to {}", u32::MAX)),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::records::Workspaces;

    fn parse(extra: &str, ws_extra: &str) -> Result<Workspaces, String> {
        let root = std::env::temp_dir();
        let root = root.to_string_lossy();
        let text = format!(
            "{extra}\n[[workspace]]\nname = \"w\"\nroot = \"{root}\"\n{ws_extra}\n\
             [[worker]]\nname = \"Elsewhere\"\nagent = \"claude-code\"\nworkspaces = [\"w2\"]\n\
             [[worker]]\nname = \"Researcher\"\nagent = \"claude-code\"\n\
             [[workspace]]\nname = \"w2\"\nroot = \"{root}\"\n"
        );
        Workspaces::parse(&text, Path::new("w.toml"), None).map_err(|e| e.to_string())
    }

    #[test]
    fn without_tables_the_host_is_off_and_no_workspace_delegates() {
        let w = parse("", "").unwrap();
        assert!(!w.assistant().enabled);
        assert_eq!(w.assistant().max_runs_per_day, 20);
        assert_eq!(w.all()[0].assistant, None);
        assert_eq!(w.delegate("w"), None);
    }

    #[test]
    fn a_delegate_gets_the_defaults() {
        let w = parse(
            "[assistant]\nenabled = true",
            "[workspace.assistant]\ndelegate = true",
        )
        .unwrap();
        let d = w.delegate("w").unwrap();
        assert_eq!(
            (d.worker.as_deref(), d.max_open_runs, d.max_runs_per_day),
            (None, 1, 10)
        );
        assert_eq!(d.run_timeout, 1800);
        assert_eq!(d.notes, super::NotesMode::Review);
        assert_eq!(w.delegate("w2"), None);
    }

    #[test]
    fn values_are_kept_and_the_host_switch_gates_delegates() {
        let ws = "[workspace.assistant]\ndelegate = true\nworker = \"Researcher\"\n\
                  max_open_runs = 2\nmax_runs_per_day = 4\nrun_timeout = \"2h\"\nnotes = \"auto\"";
        let w = parse("[assistant]\nenabled = true\nmax_runs_per_day = 7", ws).unwrap();
        assert_eq!(w.assistant().max_runs_per_day, 7);
        let d = w.delegate("w").unwrap();
        assert_eq!(d.worker.as_deref(), Some("Researcher"));
        assert_eq!(
            (d.max_open_runs, d.max_runs_per_day, d.run_timeout),
            (2, 4, 7200)
        );
        assert_eq!(d.notes, super::NotesMode::Auto);
        let off = parse("[assistant]\nenabled = false", ws).unwrap();
        assert!(off.all()[0].assistant.is_some());
        assert_eq!(off.delegate("w"), None);
        let no = parse(
            "[assistant]\nenabled = true",
            "[workspace.assistant]\ndelegate = false",
        )
        .unwrap();
        assert_eq!(no.delegate("w"), None);
    }

    #[test]
    fn counterexamples_are_refused_by_field() {
        let on = "[assistant]\nenabled = true";
        let cases = [
            ("[assistant]\nenabled = true\nlimit = 3", "", "limit"),
            ("[assistant]\nmax_runs_per_day = 3", "", "enabled"),
            (
                "[assistant]\nenabled = true\nmax_runs_per_day = 0",
                "",
                "max_runs_per_day",
            ),
            (on, "[workspace.assistant]\nmax_open_runs = 1", "delegate"),
            (
                on,
                "[workspace.assistant]\ndelegate = true\nworkers = \"x\"",
                "workers",
            ),
            (
                on,
                "[workspace.assistant]\ndelegate = true\nmax_open_runs = 0",
                "max_open_runs",
            ),
            (
                on,
                "[workspace.assistant]\ndelegate = true\nmax_open_runs = -1",
                "max_open_runs",
            ),
            (
                on,
                "[workspace.assistant]\ndelegate = true\nmax_runs_per_day = 0",
                "max_runs_per_day",
            ),
            (
                on,
                "[workspace.assistant]\ndelegate = true\nrun_timeout = \"soon\"",
                "run_timeout",
            ),
            (
                on,
                "[workspace.assistant]\ndelegate = true\nrun_timeout = \"0m\"",
                "run_timeout",
            ),
            (
                on,
                "[workspace.assistant]\ndelegate = false\nrun_timeout = \"30\"",
                "run_timeout",
            ),
            (
                on,
                "[workspace.assistant]\ndelegate = true\nnotes = \"always\"",
                "notes must be",
            ),
            (
                on,
                "[workspace.assistant]\ndelegate = true\nworker = \"Nobody\"",
                "not configured",
            ),
            (
                on,
                "[workspace.assistant]\ndelegate = true\nworker = \"Elsewhere\"",
                "not allowed",
            ),
        ];
        for (host, ws, needle) in cases {
            let err = parse(host, ws).unwrap_err();
            assert!(err.contains(needle), "{host:?} {ws:?}: {err}");
        }
    }

    #[test]
    fn the_default_worker_name_means_the_workspace_agent() {
        let w = parse(
            "[assistant]\nenabled = true",
            &format!(
                "[workspace.assistant]\ndelegate = true\nworker = \"{}\"",
                muxa_protocol::DEFAULT_WORKER
            ),
        )
        .unwrap();
        assert_eq!(w.delegate("w").unwrap().worker, None);
    }
}
