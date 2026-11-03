//! `[[routine]]` in `workspaces.toml` (ADR 0026 section 1): what runs, when,
//! as which worker, and what it may publish. Parsing only; `routines/` runs them.
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::Workspace;
use super::workers::{Worker, instruction_path};

pub const DEFAULT_ITEMS: u32 = 10;
pub const MAX_ITEMS: u32 = 50;
/// The instructions leave room in the run's goal for muxa's output section. Bytes.
pub const MAX_INSTRUCTIONS: usize = 12 * 1024;
const MAX_NAME: usize = 64;
const MAX_PROJECT: usize = 200;

/// A daily local time in an IANA time zone.
#[derive(Clone, PartialEq, Eq)]
pub struct Trigger {
    pub hour: i8,
    pub minute: i8,
    /// The IANA name, as written in the config.
    pub timezone: String,
    pub tz: jiff::tz::TimeZone,
}

impl fmt::Debug for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.at(), self.timezone)
    }
}

impl Trigger {
    /// `HH:MM`.
    pub fn at(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Routine {
    pub workspace: String,
    /// Its id within the workspace.
    pub name: String,
    /// None: run-now only.
    pub trigger: Option<Trigger>,
    /// None: the workspace default agent.
    pub worker: Option<String>,
    pub instructions: PathBuf,
    pub items: u32,
    /// The agent may write to external systems without a proposal.
    pub direct_writes: bool,
    pub project: Option<String>,
    pub enabled: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawRoutine {
    workspace: String,
    name: String,
    at: Option<String>,
    timezone: Option<String>,
    worker: Option<String>,
    instructions: String,
    project: Option<String>,
    enabled: bool,
    output: Option<RawOutput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOutput {
    items: Option<u32>,
    #[serde(default)]
    direct_writes: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum InstructionsError {
    #[error("cannot read routine instructions {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("routine instructions {path}: {reason}")]
    Invalid { path: PathBuf, reason: &'static str },
}

impl RawRoutine {
    /// Errors name the Routine and the field, never a value of the instructions.
    pub(super) fn check(
        self,
        home: Option<&Path>,
        workspaces: &[Workspace],
        workers: &[Worker],
    ) -> Result<Routine, String> {
        line(&self.name, MAX_NAME).map_err(|r| format!("routine name {r}"))?;
        let at = |field: &str| format!("routine {:?}: {field}", self.name);
        if !workspaces.iter().any(|w| w.name == self.workspace) {
            return Err(at("workspace is not configured"));
        }
        let trigger = match (self.at.as_deref(), self.timezone.as_deref()) {
            (None, None) => None,
            (Some(time), Some(zone)) => Some(trigger(time, zone).map_err(|r| at(&r))?),
            _ => return Err(at("at and timezone go together")),
        };
        let worker = match self.worker.as_deref() {
            None | Some(muxa_protocol::DEFAULT_WORKER) => None,
            Some(name) => {
                let found = workers
                    .iter()
                    .find(|w| w.name == name)
                    .ok_or_else(|| at("worker is not configured"))?;
                if !found.allows(&self.workspace) {
                    return Err(at("worker is not allowed in its workspace"));
                }
                Some(name.to_owned())
            }
        };
        let instructions = instruction_path(&self.instructions, home)
            .map_err(|r| at(&r.replace("worker ", "")))?;
        if let Some(project) = &self.project {
            line(project, MAX_PROJECT).map_err(|r| at(&format!("project {r}")))?;
        }
        let output = self.output.unwrap_or(RawOutput {
            items: None,
            direct_writes: false,
        });
        let items = output.items.unwrap_or(DEFAULT_ITEMS);
        if !(1..=MAX_ITEMS).contains(&items) {
            return Err(at(&format!("output items must be 1 to {MAX_ITEMS}")));
        }
        Ok(Routine {
            workspace: self.workspace,
            name: self.name,
            trigger,
            worker,
            instructions,
            items,
            direct_writes: output.direct_writes,
            project: self.project,
            enabled: self.enabled,
        })
    }
}

impl Routine {
    /// The instructions as they are now: read on every run, so an edit needs no restart.
    pub fn read_instructions(&self) -> Result<String, InstructionsError> {
        let path = &self.instructions;
        let read = |source| InstructionsError::Read {
            path: path.clone(),
            source,
        };
        let invalid = |reason| InstructionsError::Invalid {
            path: path.clone(),
            reason,
        };
        // NONBLOCK rejects a FIFO without waiting for a writer.
        let file = File::from(
            rustix::fs::open(
                path,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::NONBLOCK
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .map_err(|e| read(e.into()))?,
        );
        if !file.metadata().map_err(read)?.is_file() {
            return Err(invalid("must be a regular file"));
        }
        let mut bytes = Vec::new();
        file.take((MAX_INSTRUCTIONS + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(read)?;
        if bytes.len() > MAX_INSTRUCTIONS {
            return Err(invalid("is larger than 12 KiB"));
        }
        if bytes.contains(&0) {
            return Err(invalid("has a NUL byte"));
        }
        String::from_utf8(bytes).map_err(|_| invalid("is not UTF-8"))
    }
}

fn trigger(time: &str, zone: &str) -> Result<Trigger, String> {
    let bad_time = || "at must be HH:MM, 00:00 to 23:59".to_owned();
    let (h, m) = time.split_once(':').ok_or_else(bad_time)?;
    let two = |s: &str| s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit());
    if !two(h) || !two(m) {
        return Err(bad_time());
    }
    let (hour, minute): (i8, i8) = (
        h.parse().map_err(|_| bad_time())?,
        m.parse().map_err(|_| bad_time())?,
    );
    if hour > 23 || minute > 59 {
        return Err(bad_time());
    }
    if zone.is_empty() || zone.chars().any(char::is_control) || zone.len() > 64 {
        return Err("timezone must be an IANA name such as Europe/Berlin".into());
    }
    let tz = jiff::tz::TimeZone::get(zone)
        .map_err(|_| format!("timezone {zone:?} is not a known IANA time zone"))?;
    Ok(Trigger {
        hour,
        minute,
        timezone: zone.to_owned(),
        tz,
    })
}

fn line(value: &str, max: usize) -> Result<(), &'static str> {
    if value.trim().is_empty() {
        return Err("is empty");
    }
    if value.chars().count() > max {
        return Err("is too long");
    }
    if value.chars().any(char::is_control) {
        return Err("has a control character");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::Workspaces;
    use super::*;
    use std::fs;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "muxa-routine-config-{}-{:08x}",
                std::process::id(),
                getrandom::u32().unwrap()
            ));
            fs::create_dir(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }

        fn parse(&self, routines: &str) -> Result<Workspaces, String> {
            Workspaces::parse(
                &format!(
                    "[[workspace]]\nname = 'Company'\nroot = '{0}'\n[[workspace]]\nname = 'Home'\nroot = '{0}'\n\
                     [[worker]]\nname = 'Researcher'\nagent = 'claude-code'\nworkspaces = ['Company']\n{routines}",
                    self.0.display()
                ),
                Path::new("workspaces.toml"),
                Some(&self.0),
            )
            .map_err(|e| e.to_string())
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const BRIEF: &str = "[[routine]]\nworkspace = 'Company'\nname = 'Morning brief'\nat = '09:00'\n\
        timezone = 'Europe/Berlin'\nworker = 'Researcher'\ninstructions = '~/brief.md'\n\
        project = 'Ops'\nenabled = true\n[routine.output]\nitems = 5\n";

    #[test]
    fn a_full_routine_parses_with_its_defaults() {
        let f = Fixture::new();
        let config = f.parse(BRIEF).unwrap();
        let r = config.routine("Company", "Morning brief").unwrap();
        let trigger = r.trigger.as_ref().unwrap();
        assert_eq!(
            (trigger.at(), trigger.timezone.as_str()),
            ("09:00".into(), "Europe/Berlin")
        );
        assert_eq!(r.worker.as_deref(), Some("Researcher"));
        assert_eq!(r.instructions, f.0.join("brief.md"));
        assert_eq!((r.items, r.direct_writes, r.enabled), (5, false, true));
        let minimal = f
            .parse("[[routine]]\nworkspace = 'Home'\nname = 'Memory'\ninstructions = '/m.md'\nenabled = false\n")
            .unwrap();
        let m = minimal.routine("Home", "Memory").unwrap();
        assert_eq!(
            (m.trigger.is_none(), m.worker.as_deref(), m.items),
            (true, None, DEFAULT_ITEMS)
        );
        assert!(!m.enabled);
        assert!(minimal.routine("Company", "Memory").is_none());
    }

    #[test]
    fn the_same_name_in_two_workspaces_is_two_routines() {
        let f = Fixture::new();
        let other = BRIEF
            .replace("'Company'", "'Home'")
            .replace("worker = 'Researcher'\n", "");
        let config = f.parse(&format!("{BRIEF}{other}")).unwrap();
        assert_eq!(config.routines().len(), 2);
        assert!(f.parse(&format!("{BRIEF}{BRIEF}")).is_err());
    }

    #[test]
    fn invalid_routines_are_refused() {
        let f = Fixture::new();
        for bad in [
            BRIEF.replace("enabled = true\n", ""),
            BRIEF.replace("'09:00'", "'9:00'"),
            BRIEF.replace("'09:00'", "'24:00'"),
            BRIEF.replace("'09:00'", "'09:60'"),
            BRIEF.replace("'09:00'", "'09:00:00'"),
            BRIEF.replace("'Europe/Berlin'", "'Mars/Olympus'"),
            BRIEF.replace("'Europe/Berlin'", "''"),
            BRIEF.replace("timezone = 'Europe/Berlin'\n", ""),
            BRIEF.replace("at = '09:00'\n", ""),
            BRIEF.replace("workspace = 'Company'", "workspace = 'Missing'"),
            BRIEF.replace("'Researcher'", "'Missing'"),
            BRIEF.replace("workspace = 'Company'", "workspace = 'Home'"),
            BRIEF.replace("'~/brief.md'", "'relative.md'"),
            BRIEF.replace("name = 'Morning brief'", "name = ''"),
            BRIEF.replace(
                "name = 'Morning brief'",
                &format!("name = '{}'", "x".repeat(65)),
            ),
            BRIEF.replace("items = 5", "items = 0"),
            BRIEF.replace("items = 5", "items = 51"),
            BRIEF.replace("project = 'Ops'", "project = ''"),
            format!("{BRIEF}schedule = 'daily'\n"),
            BRIEF.replace("items = 5", "items = 5\nformat = 'json'"),
        ] {
            assert!(f.parse(&bad).is_err(), "accepted:\n{bad}");
        }
        assert!(f.parse(&BRIEF.replace("'Researcher'", "'default'")).is_ok());
    }

    #[test]
    fn instructions_are_read_on_each_run_and_bounded() {
        let f = Fixture::new();
        let config = f.parse(BRIEF).unwrap();
        let r = config.routine("Company", "Morning brief").unwrap();
        assert!(matches!(
            r.read_instructions(),
            Err(InstructionsError::Read { .. })
        ));
        fs::write(f.0.join("brief.md"), "first").unwrap();
        assert_eq!(r.read_instructions().unwrap(), "first");
        fs::write(f.0.join("brief.md"), "second").unwrap();
        assert_eq!(r.read_instructions().unwrap(), "second");
        for bytes in [
            vec![b'x'; MAX_INSTRUCTIONS + 1],
            vec![0xff],
            b"a\0b".to_vec(),
        ] {
            fs::write(f.0.join("brief.md"), bytes).unwrap();
            assert!(matches!(
                r.read_instructions(),
                Err(InstructionsError::Invalid { .. })
            ));
        }
    }
}
