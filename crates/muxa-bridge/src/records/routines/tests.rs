use super::super::Workspaces;
use super::*;
use std::fs;

/// Two workspace roots, `company/` and `home/`, under one temp folder.
struct Fixture(PathBuf);

const GATE: &str = "[workspace.routines]\nenabled = true\n";

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "muxa-routine-config-{}-{:08x}",
            std::process::id(),
            getrandom::u32().unwrap()
        ));
        fs::create_dir(&path).unwrap();
        let path = path.canonicalize().unwrap();
        for root in ["company/.muxa", "home/.muxa"] {
            fs::create_dir_all(path.join(root)).unwrap();
        }
        Self(path)
    }

    fn muxa(&self, root: &str) -> PathBuf {
        self.0.join(root).join(WORKSPACE_DIR)
    }

    /// Writes Company's routines file, then loads the host file with `gate`
    /// on Company and the plain gate on Home.
    fn load_with(&self, gate: &str, routines: &str) -> Result<Workspaces, String> {
        fs::write(self.muxa("company").join(ROUTINES_FILE), routines).unwrap();
        Workspaces::parse(
            &format!(
                "[[workspace]]\nname = 'Company'\nroot = '{0}/company'\n{gate}\
                 [[workspace]]\nname = 'Home'\nroot = '{0}/home'\n{GATE}\
                 [[worker]]\nname = 'Researcher'\nagent = 'claude-code'\nworkspaces = ['Company']\n",
                self.0.display()
            ),
            Path::new("workspaces.toml"),
            Some(&self.0),
        )
        .map_err(|e| e.to_string())
    }

    fn load(&self, routines: &str) -> Result<Workspaces, String> {
        self.load_with(GATE, routines)
    }

    /// The single problem Company reports for `routines`.
    fn problem(&self, routines: &str) -> Option<String> {
        let config = self.load(routines).unwrap();
        assert!(config.routines().iter().all(|r| r.workspace != "Company"));
        config
            .routine_problems()
            .iter()
            .find(|p| p.workspace == "Company")
            .map(|p| p.reason.clone())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const BRIEF: &str = "[[routine]]\nname = 'Morning brief'\nat = '09:00'\n\
    timezone = 'Europe/Berlin'\nworker = 'Researcher'\ninstructions = 'routines/brief.md'\n\
    project = 'Ops'\nenabled = true\n[routine.output]\nitems = 5\n";

#[test]
fn a_full_routine_parses_with_its_defaults() {
    let f = Fixture::new();
    let config = f.load(BRIEF).unwrap();
    let r = config.routine("Company", "Morning brief").unwrap();
    let trigger = r.trigger.as_ref().unwrap();
    assert_eq!(
        (
            trigger.at(),
            trigger.schedule.as_str(),
            trigger.timezone.as_str()
        ),
        ("09:00".into(), "0 9 * * *", "Europe/Berlin")
    );
    assert_eq!(r.worker.as_deref(), Some("Researcher"));
    assert_eq!(r.instructions, f.muxa("company").join("routines/brief.md"));
    assert_eq!((r.items, r.direct_writes, r.enabled), (5, false, true));
    let minimal = f
        .load("[[routine]]\nname = 'Memory'\ninstructions = 'm.md'\nenabled = false\n")
        .unwrap();
    let m = minimal.routine("Company", "Memory").unwrap();
    assert_eq!(
        (m.trigger.is_none(), m.worker.as_deref(), m.items),
        (true, None, DEFAULT_ITEMS)
    );
    assert!(!m.enabled);
    assert!(minimal.routine("Home", "Memory").is_none());
}

#[test]
fn a_schedule_is_kept_as_written_with_single_spaces() {
    let f = Fixture::new();
    let config = f
        .load(&BRIEF.replace("at = '09:00'", "schedule = '0  8 * *  0,2-6'"))
        .unwrap();
    let t = config
        .routine("Company", "Morning brief")
        .unwrap()
        .trigger
        .clone()
        .unwrap();
    assert_eq!(
        (t.schedule.as_str(), t.at()),
        ("0 8 * * 0,2-6", "08:00".into())
    );
}

#[test]
fn routines_need_the_hosts_gate() {
    let f = Fixture::new();
    for gate in ["", "[workspace.routines]\nenabled = false\n"] {
        let config = f.load_with(gate, BRIEF).unwrap();
        assert!(config.routines().is_empty(), "gate {gate:?}");
        assert!(config.routine_problems().is_empty());
    }
}

#[test]
fn direct_writes_need_the_host_to_allow_them() {
    let f = Fixture::new();
    let direct = BRIEF.replace("items = 5", "items = 5\ndirect_writes = true");
    let reason = f.problem(&direct).unwrap();
    assert!(reason.contains("direct_writes"), "{reason}");
    let allowed = f
        .load_with(
            "[workspace.routines]\nenabled = true\ndirect_writes = true\n",
            &direct,
        )
        .unwrap();
    assert!(
        allowed
            .routine("Company", "Morning brief")
            .unwrap()
            .direct_writes
    );
}

#[test]
fn a_missing_folder_or_file_means_no_routines() {
    let f = Fixture::new();
    fs::remove_dir_all(f.muxa("home")).unwrap();
    let config = f.load(BRIEF).unwrap();
    assert_eq!(config.routines().len(), 1);
    assert!(config.routine_problems().is_empty());
    fs::remove_file(f.muxa("company").join(ROUTINES_FILE)).unwrap();
    let config = Workspaces::parse(
        &format!(
            "[[workspace]]\nname = 'Company'\nroot = '{}/company'\n{GATE}",
            f.0.display()
        ),
        Path::new("workspaces.toml"),
        None,
    )
    .unwrap();
    assert!(config.routines().is_empty() && config.routine_problems().is_empty());
}

#[test]
fn a_broken_file_breaks_only_its_workspace() {
    let f = Fixture::new();
    fs::write(
        f.muxa("home").join(ROUTINES_FILE),
        BRIEF.replace("worker = 'Researcher'\n", ""),
    )
    .unwrap();
    for bad in [
        "not toml [".to_owned(),
        BRIEF.replace("enabled = true\n", ""),
        BRIEF.replace("'09:00'", "'9:00'"),
        BRIEF.replace("'09:00'", "'24:00'"),
        BRIEF.replace("at = '09:00'", "at = '09:00'\nschedule = '0 9 * * *'"),
        BRIEF.replace("at = '09:00'", "schedule = '0 9 1 * 1'"),
        BRIEF.replace("at = '09:00'", "schedule = 'daily'"),
        BRIEF.replace("'Europe/Berlin'", "'Mars/Olympus'"),
        BRIEF.replace("timezone = 'Europe/Berlin'\n", ""),
        BRIEF.replace("at = '09:00'\n", ""),
        BRIEF.replace("'Researcher'", "'Missing'"),
        BRIEF.replace("'routines/brief.md'", "'/abs/brief.md'"),
        BRIEF.replace("'routines/brief.md'", "'~/brief.md'"),
        BRIEF.replace("'routines/brief.md'", "'../brief.md'"),
        BRIEF.replace("'routines/brief.md'", "'routines/../../x.md'"),
        BRIEF.replace("'routines/brief.md'", "''"),
        BRIEF.replace("name = 'Morning brief'", "name = ''"),
        BRIEF.replace("items = 5", "items = 0"),
        BRIEF.replace("items = 5", "items = 51"),
        BRIEF.replace("project = 'Ops'", "project = ''"),
        BRIEF.replace("name = 'Morning brief'", "name = 'B'\nworkspace = 'Home'"),
        format!("{BRIEF}{BRIEF}"),
    ] {
        let config = f.load(&bad).unwrap();
        assert!(
            config.routine("Company", "Morning brief").is_none(),
            "accepted:\n{bad}"
        );
        assert_eq!(config.routine_problems().len(), 1, "no problem for:\n{bad}");
        assert!(config.routine("Home", "Morning brief").is_some());
    }
    assert!(f.load(&BRIEF.replace("'Researcher'", "'default'")).is_ok());
}

#[test]
fn a_worker_must_be_allowed_in_the_workspace() {
    let f = Fixture::new();
    fs::write(f.muxa("home").join(ROUTINES_FILE), BRIEF).unwrap();
    let config = f.load("").unwrap();
    let home = &config.routine_problems()[0];
    assert_eq!(home.workspace, "Home");
    assert!(home.reason.contains("not allowed"), "{}", home.reason);
}

#[test]
fn routines_in_the_hosts_file_are_refused() {
    let f = Fixture::new();
    let err = Workspaces::parse(
        &format!(
            "[[workspace]]\nname = 'Company'\nroot = '{}/company'\n\
             [[routine]]\nworkspace = 'Company'\nname = 'B'\ninstructions = '/b.md'\nenabled = true\n",
            f.0.display()
        ),
        Path::new("workspaces.toml"),
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains(".muxa/routines.toml"), "{err}");
}

#[test]
fn a_folder_that_leaves_the_root_is_refused() {
    let f = Fixture::new();
    fs::remove_dir_all(f.muxa("company")).unwrap();
    std::os::unix::fs::symlink(f.muxa("home"), f.muxa("company")).unwrap();
    fs::write(f.muxa("home").join(ROUTINES_FILE), BRIEF).unwrap();
    let config = f.load_with(GATE, BRIEF);
    // load_with wrote through the link into Home's file; Company must not use it.
    let config = config.unwrap();
    assert!(config.routine("Company", "Morning brief").is_none());
    assert!(
        config
            .routine_problems()
            .iter()
            .any(|p| p.workspace == "Company")
    );
}

#[test]
fn instructions_are_read_on_each_run_and_stay_in_the_folder() {
    let f = Fixture::new();
    let config = f.load(BRIEF).unwrap();
    let r = config.routine("Company", "Morning brief").unwrap();
    assert!(matches!(
        r.read_instructions(),
        Err(InstructionsError::Read { .. })
    ));
    let dir = f.muxa("company").join("routines");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("brief.md"), "first").unwrap();
    assert_eq!(r.read_instructions().unwrap(), "first");
    fs::write(dir.join("brief.md"), "second").unwrap();
    assert_eq!(r.read_instructions().unwrap(), "second");
    for bytes in [
        vec![b'x'; MAX_INSTRUCTIONS + 1],
        vec![0xff],
        b"a\0b".to_vec(),
    ] {
        fs::write(dir.join("brief.md"), bytes).unwrap();
        assert!(matches!(
            r.read_instructions(),
            Err(InstructionsError::Invalid { .. })
        ));
    }
    fs::remove_file(dir.join("brief.md")).unwrap();
    fs::write(f.0.join("outside.md"), "secret").unwrap();
    std::os::unix::fs::symlink(f.0.join("outside.md"), dir.join("brief.md")).unwrap();
    assert!(matches!(
        r.read_instructions(),
        Err(InstructionsError::Invalid { .. })
    ));
}
