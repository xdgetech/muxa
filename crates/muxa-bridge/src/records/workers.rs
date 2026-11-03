//! Host-owned worker definitions. Native roles remain native; resolution does not launch them.
// Runtime use remains gated until lifecycle and native acceptance pass.
#![allow(dead_code)]
use std::collections::HashSet;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{AgentSettings, LoopSpec, Workspace, Workspaces};

pub const MAX_INSTRUCTIONS: usize = 16 * 1024;
const MAX_ARGS: usize = 32;
const MAX_ARG: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerAgent {
    ClaudeCode,
    Codex,
}

impl WorkerAgent {
    pub fn id(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
        }
    }

    pub fn program(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Worker {
    pub name: String,
    pub description: Option<String>,
    pub agent: WorkerAgent,
    pub args: Vec<String>,
    pub instructions: Option<PathBuf>,
    pub workspaces: Option<Vec<String>>,
    pub loop_defaults: Option<LoopSpec>,
}

impl fmt::Debug for Worker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Worker")
            .field("name", &self.name)
            .field("agent", &self.agent)
            .field("args", &self.args.len())
            .field("instructions", &self.instructions)
            .field("workspaces", &self.workspaces)
            .field("has_loop_defaults", &self.loop_defaults.is_some())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerChoice<'a> {
    Keep,
    WorkspaceDefault,
    Named(&'a str),
}

/// Keep argv separate until muxa-tmux applies its quoting rules.
#[derive(Clone, PartialEq, Eq)]
pub enum LaunchCommand {
    Shell(String),
    Native {
        program: &'static str,
        args: Vec<String>,
    },
}

impl fmt::Debug for LaunchCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shell(_) => f.write_str("Shell(<host command>)"),
            Self::Native { program, args } => f
                .debug_struct("Native")
                .field("program", program)
                .field("args", &args.len())
                .finish(),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ResolvedWorker {
    /// None means the workspace default, not a named worker.
    pub worker: Option<String>,
    pub command: LaunchCommand,
    /// A complete validated snapshot; never logged or copied from native role files.
    pub instructions: Option<String>,
    /// Existing workspace env only. `command` is None: use `command` above.
    pub environment: AgentSettings,
}

impl fmt::Debug for ResolvedWorker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedWorker")
            .field("worker", &self.worker)
            .field("command", &self.command)
            .field(
                "instructions_bytes",
                &self.instructions.as_ref().map(String::len),
            )
            .field("environment", &self.environment)
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("workspace {0:?} is not configured")]
    Workspace(String),
    #[error("worker {0:?} is not configured; select another worker explicitly")]
    Missing(String),
    #[error("worker {worker:?} is not allowed in workspace {workspace:?}")]
    Scope { worker: String, workspace: String },
    #[error("cannot read worker instructions {path}: {source}")]
    InstructionsRead { path: PathBuf, source: io::Error },
    #[error("worker instructions {path}: {reason}")]
    InstructionsInvalid { path: PathBuf, reason: &'static str },
    #[error("the default agent command is empty or has a control character")]
    DefaultCommand,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawWorker {
    name: String,
    description: Option<String>,
    agent: String,
    #[serde(default)]
    args: Vec<String>,
    instructions: Option<String>,
    workspaces: Option<Vec<String>>,
    #[serde(rename = "loop")]
    loop_table: Option<RawLoop>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLoop {
    check: String,
    max_attempts: Option<u32>,
    timeout: Option<u64>,
}

impl RawWorker {
    pub(super) fn check(
        self,
        home: Option<&Path>,
        workspaces: &[Workspace],
    ) -> Result<Worker, String> {
        text(&self.name, 64).map_err(|reason| format!("worker name {reason}"))?;
        if let Some(description) = &self.description {
            if description.chars().count() > 200 || description.chars().any(char::is_control) {
                return Err("worker description is too long or has a control character".into());
            }
        }
        let agent = match self.agent.as_str() {
            "claude-code" => WorkerAgent::ClaudeCode,
            "codex" => WorkerAgent::Codex,
            _ => return Err("worker agent must be claude-code or codex".into()),
        };
        if self.args.len() > MAX_ARGS {
            return Err(format!("worker args has more than {MAX_ARGS} entries"));
        }
        for (index, arg) in self.args.iter().enumerate() {
            if arg.len() > MAX_ARG || arg.chars().any(char::is_control) {
                return Err(format!(
                    "worker args[{index}] is too long or has a control character"
                ));
            }
        }
        interactive_args(&self.agent, &self.args)?;
        if let Some(names) = &self.workspaces {
            let mut seen = HashSet::new();
            for name in names {
                if !workspaces.iter().any(|w| w.name == *name) {
                    return Err("worker scope contains an unknown workspace".into());
                }
                if !seen.insert(name) {
                    return Err("worker scope contains a duplicate workspace".into());
                }
            }
        }
        let instructions = self
            .instructions
            .map(|raw| instruction_path(&raw, home))
            .transpose()?;
        let loop_defaults = self.loop_table.map(|raw| LoopSpec {
            check: raw.check,
            max_attempts: raw.max_attempts,
            timeout: raw.timeout,
        });
        if let Some(spec) = &loop_defaults {
            // Validate shape even when an explicit empty scope allows no workspace.
            spec.check(Some(&super::Checks::Any))?;
            for workspace in workspaces.iter().filter(|w| {
                self.workspaces
                    .as_ref()
                    .is_none_or(|scope| scope.contains(&w.name))
            }) {
                spec.check(workspace.loop_checks.as_ref())
                    .map_err(|reason| {
                        format!("worker loop in workspace {:?}: {reason}", workspace.name)
                    })?;
            }
        }
        Ok(Worker {
            name: self.name,
            description: self.description,
            agent,
            args: self.args,
            instructions,
            workspaces: self.workspaces,
            loop_defaults,
        })
    }
}

impl Worker {
    pub fn allows(&self, workspace: &str) -> bool {
        self.workspaces
            .as_ref()
            .is_none_or(|scope| scope.iter().any(|name| name == workspace))
    }

    pub fn program(&self) -> &'static str {
        self.agent.program()
    }

    fn read_instructions(&self) -> Result<Option<String>, WorkerError> {
        let Some(path) = &self.instructions else {
            return Ok(None);
        };
        let invalid = |reason| WorkerError::InstructionsInvalid {
            path: path.clone(),
            reason,
        };
        let read = |source| WorkerError::InstructionsRead {
            path: path.clone(),
            source,
        };
        // NONBLOCK lets us reject a FIFO without hanging before its metadata check.
        let file = File::from(
            rustix::fs::open(
                path,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::NONBLOCK
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .map_err(|error| read(error.into()))?,
        );
        if !file.metadata().map_err(read)?.is_file() {
            return Err(invalid("must be a regular file"));
        }
        let mut bytes = Vec::new();
        file.take((MAX_INSTRUCTIONS + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(read)?;
        if bytes.len() > MAX_INSTRUCTIONS {
            return Err(invalid("is larger than 16 KiB"));
        }
        if bytes.contains(&0) {
            return Err(invalid("has a NUL byte"));
        }
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| invalid("is not UTF-8"))
    }
}

impl Workspaces {
    /// Resolve from one config snapshot. No native launch, file publication or loop mutation.
    pub fn resolve_worker(
        &self,
        workspace: &str,
        choice: WorkerChoice<'_>,
        current: Option<&str>,
        fallback_command: &str,
    ) -> Result<ResolvedWorker, WorkerError> {
        let workspace = self
            .get(workspace)
            .ok_or_else(|| WorkerError::Workspace(workspace.to_owned()))?;
        let name = match choice {
            WorkerChoice::Keep => current,
            WorkerChoice::WorkspaceDefault => None,
            WorkerChoice::Named(name) => Some(name),
        };
        let environment = AgentSettings {
            command: None,
            env: workspace
                .agent
                .as_ref()
                .map(|a| a.env.clone())
                .unwrap_or_default(),
        };
        let Some(name) = name else {
            let command = workspace
                .agent
                .as_ref()
                .and_then(|a| a.command.as_deref())
                .unwrap_or(fallback_command);
            if command.trim().is_empty() || command.chars().any(char::is_control) {
                return Err(WorkerError::DefaultCommand);
            }
            return Ok(ResolvedWorker {
                worker: None,
                command: LaunchCommand::Shell(command.to_owned()),
                instructions: None,
                environment,
            });
        };
        let worker = self
            .worker(name)
            .ok_or_else(|| WorkerError::Missing(name.to_owned()))?;
        if !worker.allows(&workspace.name) {
            return Err(WorkerError::Scope {
                worker: name.to_owned(),
                workspace: workspace.name.clone(),
            });
        }
        Ok(ResolvedWorker {
            worker: Some(name.to_owned()),
            command: LaunchCommand::Native {
                program: worker.program(),
                args: worker.args.clone(),
            },
            instructions: worker.read_instructions()?,
            environment,
        })
    }
}

fn text(value: &str, max: usize) -> Result<(), &'static str> {
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

fn instruction_path(raw: &str, home: Option<&Path>) -> Result<PathBuf, String> {
    if raw.chars().any(char::is_control) {
        return Err("worker instructions path has a control character".into());
    }
    let path = match raw.strip_prefix("~/") {
        Some(rest) => home
            .ok_or("worker instructions starts with ~/ but HOME is not set")?
            .join(rest),
        None => PathBuf::from(raw),
    };
    if !path.is_absolute() {
        return Err("worker instructions must be an absolute path or start with ~/".into());
    }
    Ok(path)
}

/// Reject known startup modes that violate Task ownership. Other flags remain trusted host config.
fn interactive_args(agent: &str, args: &[String]) -> Result<(), String> {
    const FORBIDDEN: &[&str] = &[
        "--print",
        "--resume",
        "--continue",
        "--fork-session",
        "--worktree",
        "--background",
        "--bg",
        "--cloud",
        "--remote",
        "--initial-prompt",
        "--prompt",
        "--input-format",
        "--output-format",
    ];
    let mut may_be_value = false;
    for (index, arg) in args.iter().enumerate() {
        let flag = arg.split('=').next().unwrap_or(arg);
        let short = match agent {
            "claude-code" => ["-p", "-r", "-c", "-w"]
                .iter()
                .any(|short| flag.starts_with(short)),
            "codex" => flag.starts_with("-r"),
            _ => false,
        };
        let compact: String = arg
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect();
        if flag == "--" || FORBIDDEN.contains(&flag) || short || compact.contains("initialprompt") {
            return Err(format!(
                "worker args[{index}] selects an unsupported startup mode"
            ));
        }
        if !arg.starts_with('-') {
            if !may_be_value {
                return Err(
                    "worker args must contain interactive flags, not a subcommand or prompt".into(),
                );
            }
            may_be_value = false;
        } else {
            // Native option names stay host-owned; only the common no-value flags
            // need special handling to avoid accepting a trailing Task prompt.
            may_be_value = !arg.contains('=')
                && !flag.starts_with("--no-")
                && !matches!(
                    flag,
                    "--verbose"
                        | "--bare"
                        | "--ide"
                        | "--chrome"
                        | "--strict-mcp-config"
                        | "--dangerously-skip-permissions"
                        | "--allow-dangerously-skip-permissions"
                        | "--full-auto"
                        | "--oss"
                        | "--search"
                        | "--no-alt-screen"
                        | "--disable-slash-commands"
                        | "--strict-config"
                );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "muxa-worker-config-{}-{:08x}",
                std::process::id(),
                getrandom::u32().unwrap()
            ));
            fs::create_dir(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }

        fn parse(
            &self,
            workers: &str,
        ) -> Result<Workspaces, super::super::workspaces::ConfigError> {
            Workspaces::parse(
                &format!(
                    "[[workspace]]\nname = 'Company'\nroot = '{}'\n[workspace.agent]\ncommand = 'default-agent'\n[workspace.agent.env]\nWORKER_TEST_KEY = 'synthetic-value'\n[workspace.loop]\nchecks = ['make check']\n[[workspace]]\nname = 'Personal'\nroot = '{}'\n{workers}",
                    self.0.display(),
                    self.0.display()
                ),
                Path::new("workspaces.toml"),
                Some(&self.0),
            )
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const REVIEWER: &str = "[[worker]]\nname = 'Reviewer'\ndescription = 'Review changes'\nagent = 'claude-code'\nargs = ['--agent', 'reviewer']\nworkspaces = ['Company']\n";

    #[test]
    fn worker_scope_selection_and_workspace_default_stay_distinct() {
        let fixture = Fixture::new();
        let config = fixture.parse(REVIEWER).unwrap();
        assert_eq!(config.workers().len(), 1);
        let named = config
            .resolve_worker("Company", WorkerChoice::Named("Reviewer"), None, "fallback")
            .unwrap();
        assert_eq!(named.worker.as_deref(), Some("Reviewer"));
        assert_eq!(
            named.command,
            LaunchCommand::Native {
                program: "claude",
                args: vec!["--agent".into(), "reviewer".into()]
            }
        );
        assert_eq!(
            named.environment.env("WORKER_TEST_KEY"),
            Some("synthetic-value")
        );
        assert_eq!(named.environment.command, None);
        assert_eq!(
            config
                .resolve_worker("Company", WorkerChoice::Keep, Some("Reviewer"), "fallback")
                .unwrap(),
            named
        );
        let cleared = config
            .resolve_worker(
                "Company",
                WorkerChoice::WorkspaceDefault,
                Some("Removed"),
                "fallback",
            )
            .unwrap();
        assert_eq!(cleared.worker, None);
        assert_eq!(
            cleared.command,
            LaunchCommand::Shell("default-agent".into())
        );
        assert_eq!(cleared.environment, named.environment);
        let missing =
            config.resolve_worker("Company", WorkerChoice::Keep, Some("Removed"), "fallback");
        assert!(matches!(missing, Err(WorkerError::Missing(_))));
        assert!(matches!(
            config.resolve_worker(
                "Personal",
                WorkerChoice::Named("Reviewer"),
                None,
                "fallback"
            ),
            Err(WorkerError::Scope { .. })
        ));
        assert_eq!(
            config
                .resolve_worker("Personal", WorkerChoice::Keep, None, "fallback")
                .unwrap()
                .command,
            LaunchCommand::Shell("fallback".into())
        );
    }

    #[test]
    fn omitted_scope_allows_all_but_empty_allows_none() {
        let fixture = Fixture::new();
        let all = fixture
            .parse("[[worker]]\nname = 'A'\nagent = 'codex'\nargs = ['--profile', 'a']")
            .unwrap();
        assert!(all.worker("A").unwrap().allows("Company"));
        assert!(all.worker("A").unwrap().allows("Personal"));
        let none = fixture
            .parse("[[worker]]\nname = 'A'\nagent = 'codex'\nworkspaces = []")
            .unwrap();
        assert!(!none.worker("A").unwrap().allows("Company"));
        assert!(!none.worker("A").unwrap().allows("Personal"));
    }

    #[test]
    fn worker_config_rejects_unknown_fields_names_agents_and_scopes() {
        let fixture = Fixture::new();
        for bad in [
            format!("{REVIEWER}\nenv = {{ KEY = 'synthetic' }}"),
            format!("{REVIEWER}\nother = true"),
            format!("{REVIEWER}\n{REVIEWER}"),
            REVIEWER.replace("name = 'Reviewer'", "name = ''"),
            REVIEWER.replace("claude-code", "unknown-agent"),
            REVIEWER.replace("['Company']", "['Missing']"),
            REVIEWER.replace("['Company']", "['Company', 'Company']"),
            REVIEWER.replace(
                "description = 'Review changes'",
                &format!("description = '{}'", "x".repeat(201)),
            ),
            REVIEWER.replace("name = 'Reviewer'", &format!("name = '{}'", "x".repeat(65))),
            REVIEWER.replace("args = ['--agent', 'reviewer']", "args = [1]"),
        ] {
            assert!(fixture.parse(&bad).is_err());
        }
    }

    #[test]
    fn native_startup_modes_are_rejected_without_quoting_argument_values() {
        let fixture = Fixture::new();
        for args in [
            "['--print']",
            "['-psecret']",
            "['--resume=synthetic-session']",
            "['--worktree', 'synthetic']",
            "['--bg']",
            "['--']",
            "['resume', 'synthetic']",
            "['--agent', 'reviewer', 'synthetic-prompt']",
            "['--verbose', 'synthetic-prompt']",
            "['--agents', '{initialPrompt: synthetic-secret}']",
            "['--agent', \"bad\\nvalue\"]",
        ] {
            let raw = REVIEWER.replace("['--agent', 'reviewer']", args);
            let error = fixture.parse(&raw).unwrap_err().to_string();
            assert!(!error.contains("synthetic-secret"));
        }
        let too_many = std::iter::repeat_n("'--verbose'", 33)
            .collect::<Vec<_>>()
            .join(",");
        assert!(
            fixture
                .parse(&REVIEWER.replace("['--agent', 'reviewer']", &format!("[{too_many}]")))
                .is_err()
        );
        assert!(
            fixture
                .parse(&REVIEWER.replace(
                    "['--agent', 'reviewer']",
                    &format!("['{}']", "x".repeat(1025))
                ))
                .is_err()
        );
    }

    #[test]
    fn loop_defaults_validate_every_permitted_workspace_and_never_start_a_loop() {
        let fixture = Fixture::new();
        let scoped = format!(
            "{REVIEWER}\n[worker.loop]\ncheck = 'make check'\nmax_attempts = 3\ntimeout = 600"
        );
        let config = fixture.parse(&scoped).unwrap();
        let defaults = config
            .worker("Reviewer")
            .unwrap()
            .loop_defaults
            .as_ref()
            .unwrap();
        assert_eq!(defaults.max_attempts, Some(3));
        config
            .resolve_worker("Company", WorkerChoice::Named("Reviewer"), None, "fallback")
            .unwrap();
        assert!(
            fixture
                .parse(&scoped.replace("workspaces = ['Company']", ""))
                .is_err()
        );
        for bad in [
            scoped.replace("make check", "not-permitted"),
            scoped.replace("max_attempts = 3", "max_attempts = 0"),
            scoped.replace("timeout = 600", "timeout = 9"),
        ] {
            assert!(fixture.parse(&bad).is_err());
        }
        assert!(
            fixture
                .parse(&scoped.replace("workspaces = ['Company']", "workspaces = []"))
                .is_ok()
        );
    }

    #[test]
    fn instruction_snapshot_is_bounded_validated_and_reread_on_each_resolution() {
        let fixture = Fixture::new();
        let path = fixture.0.join("instructions.md");
        let raw = format!("{REVIEWER}\ninstructions = '~/instructions.md'");
        let config = fixture.parse(&raw).unwrap();
        let resolve =
            || config.resolve_worker("Company", WorkerChoice::Named("Reviewer"), None, "fallback");
        assert!(matches!(
            resolve(),
            Err(WorkerError::InstructionsRead { .. })
        ));
        fs::write(&path, "first synthetic instruction").unwrap();
        assert_eq!(
            resolve().unwrap().instructions.as_deref(),
            Some("first synthetic instruction")
        );
        fs::write(&path, "edited synthetic instruction").unwrap();
        let resolved = resolve().unwrap();
        assert_eq!(
            resolved.instructions.as_deref(),
            Some("edited synthetic instruction")
        );
        assert!(!format!("{resolved:?}").contains("edited synthetic"));
        assert!(!format!("{resolved:?}").contains("synthetic-value"));
        for bytes in [
            vec![b'x'; MAX_INSTRUCTIONS + 1],
            vec![0xff],
            b"a\0b".to_vec(),
        ] {
            fs::write(&path, bytes).unwrap();
            assert!(matches!(
                resolve(),
                Err(WorkerError::InstructionsInvalid { .. })
            ));
        }
        fs::write(&path, vec![b'x'; MAX_INSTRUCTIONS]).unwrap();
        assert_eq!(
            resolve().unwrap().instructions.unwrap().len(),
            MAX_INSTRUCTIONS
        );
        assert!(
            fixture
                .parse(&raw.replace("~/instructions.md", "relative.md"))
                .is_err()
        );
    }

    #[test]
    fn nonregular_instruction_files_fail_without_waiting_for_a_fifo_writer() {
        let fixture = Fixture::new();
        let path = fixture.0.join("instructions.md");
        let config = fixture
            .parse(&format!("{REVIEWER}\ninstructions = '~/instructions.md'"))
            .unwrap();
        let resolve =
            || config.resolve_worker("Company", WorkerChoice::Named("Reviewer"), None, "fallback");
        fs::create_dir(&path).unwrap();
        assert!(matches!(
            resolve(),
            Err(WorkerError::InstructionsInvalid { .. })
        ));
        fs::remove_dir(&path).unwrap();
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        assert!(matches!(
            resolve(),
            Err(WorkerError::InstructionsInvalid { .. })
        ));
    }

    #[test]
    fn same_name_command_edits_are_resolved_from_the_current_snapshot() {
        let fixture = Fixture::new();
        let first = fixture.parse(REVIEWER).unwrap();
        let edited = fixture
            .parse(&REVIEWER.replace("'reviewer'", "'new-reviewer'"))
            .unwrap();
        let resolve = |config: &Workspaces| {
            config
                .resolve_worker("Company", WorkerChoice::Keep, Some("Reviewer"), "fallback")
                .unwrap()
        };
        assert_ne!(resolve(&first).command, resolve(&edited).command);
        assert_eq!(resolve(&first).worker, resolve(&edited).worker);
    }
}
