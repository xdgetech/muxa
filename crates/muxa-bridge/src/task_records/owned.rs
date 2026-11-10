//! Codex threads muxa owns (ADR 0028): a background Codex run gets a thread
//! muxa creates before its pane starts, and the pane only views it.
use std::fmt;
use std::path::{Path, PathBuf};

use crate::agents::codex::app_server::{self, Endpoint, Thread, ThreadId};
use crate::records::Workspaces;
use crate::records::owned_thread::OwnedThread;
use crate::records::workers::{LaunchCommand, ResolvedWorker};

const PROGRAM: &str = "codex";
const HOME: &str = "CODEX_HOME";
/// A Codex that does not use the shared local daemon cannot be guarded.
const UNGUARDED: [&str; 2] = ["--no-daemon", "--remote"];
/// Characters the shell would read as more than a word.
const SHELL_SPECIAL: &[char] = &[
    '"', '\'', '\\', '$', '`', ';', '&', '|', '<', '>', '(', ')', '{', '}', '*', '?', '[', ']',
    '~', '#', '!',
];

/// How an owned run starts: the `codex` program and its flags, and the
/// workspace's agent environment. Values never reach a log.
#[derive(Clone, PartialEq, Eq)]
pub struct Launch {
    program: String,
    args: Vec<String>,
    home: Option<PathBuf>,
    env: Vec<(String, String)>,
}

impl fmt::Debug for Launch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys: Vec<&str> = self.env.iter().map(|(k, _)| k.as_str()).collect();
        f.debug_struct("Launch")
            .field("program", &self.program)
            .field("args", &self.args.len())
            .field("home", &self.home)
            .field("env", &keys)
            .finish()
    }
}

/// The owned launch of a background run, when the host allows owned threads
/// and the run starts Codex with the shared daemon. None: today's launch.
pub fn launch(workspaces: &Workspaces, resolved: &ResolvedWorker) -> Option<Launch> {
    if !workspaces.codex_threads() {
        return None;
    }
    let (program, args) = codex_argv(&resolved.command)?;
    let unguarded = args.iter().any(|a| {
        UNGUARDED
            .iter()
            .any(|f| a == f || a.strip_prefix(f).is_some_and(|rest| rest.starts_with('=')))
    });
    if unguarded {
        return None;
    }
    let env = resolved.environment.env.clone();
    let home = resolved.environment.env(HOME).map(PathBuf::from);
    Some(Launch {
        program,
        args,
        home,
        env,
    })
}

/// The program and flags of a command that starts Codex: a Codex worker, or
/// a shell line of plain words whose first is a `codex` program.
fn codex_argv(command: &LaunchCommand) -> Option<(String, Vec<String>)> {
    match command {
        LaunchCommand::Native { program, args } if *program == PROGRAM => {
            Some(((*program).to_owned(), args.clone()))
        }
        LaunchCommand::Native { .. } => None,
        LaunchCommand::Shell(line) => {
            if line.contains(SHELL_SPECIAL) {
                return None;
            }
            let mut words = line.split_whitespace().map(str::to_owned);
            let program = words.next()?;
            (Path::new(&program).file_name()? == PROGRAM).then(|| (program, words.collect()))
        }
    }
}

impl Launch {
    /// The App Server of the workspace's Codex home, with its environment.
    pub fn endpoint(&self) -> Endpoint {
        Endpoint::new(self.program.clone(), self.home.clone()).with_env(self.env.clone())
    }

    /// The pane's command: the TUI resumes the owned thread. The id comes
    /// before the flags, so a flag cannot take it as its value.
    pub fn argv(&self, thread: &ThreadId) -> Vec<String> {
        [self.program.clone(), "resume".into(), thread.to_string()]
            .into_iter()
            .chain(self.args.iter().cloned())
            .collect()
    }

    /// Blocking: starts the shared daemon if needed, then a thread in `cwd`.
    pub fn create_thread(&self, cwd: &Path) -> Result<Thread, app_server::Error> {
        let endpoint = self.endpoint();
        endpoint.start_daemon()?;
        endpoint.start_thread(cwd)
    }

    /// What the record keeps about `thread`, for `run`.
    pub fn owned(&self, run: u32, thread: &Thread) -> OwnedThread {
        OwnedThread::new(
            run,
            thread.id.as_str(),
            thread.path.as_deref(),
            &self.program,
            self.home.as_deref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::AgentSettings;

    fn workspaces(on: bool) -> Workspaces {
        let text = format!("[codex]\nowned_threads = {on}\n");
        Workspaces::parse(&text, Path::new("workspaces.toml"), None).unwrap()
    }

    fn resolved(command: LaunchCommand, env: &[(&str, &str)]) -> ResolvedWorker {
        ResolvedWorker {
            worker: None,
            command,
            instructions: None,
            environment: AgentSettings {
                command: None,
                env: env
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                    .collect(),
            },
        }
    }

    fn native(args: &[&str]) -> LaunchCommand {
        LaunchCommand::Native {
            program: "codex",
            args: args.iter().map(|a| (*a).to_owned()).collect(),
        }
    }

    const ID: &str = "01a1c48e-945c-7353-a880-253b964832f6";

    #[test]
    fn the_switch_is_off_by_default_and_only_codex_is_owned() {
        let codex = resolved(native(&[]), &[]);
        let off = Workspaces::parse("", Path::new("w.toml"), None).unwrap();
        assert!(!off.codex_threads());
        assert_eq!(launch(&off, &codex), None);
        assert_eq!(launch(&workspaces(false), &codex), None);
        assert!(launch(&workspaces(true), &codex).is_some());
        let claude = resolved(
            LaunchCommand::Native {
                program: "claude",
                args: Vec::new(),
            },
            &[],
        );
        assert_eq!(launch(&workspaces(true), &claude), None);
        assert!(
            Workspaces::parse("[codex]\nowned = true\n", Path::new("w.toml"), None).is_err(),
            "an unknown key fails config"
        );
    }

    #[test]
    fn the_pane_resumes_the_thread_with_the_worker_flags_after_the_id() {
        let on = workspaces(true);
        let thread = ThreadId::parse(ID).unwrap();
        let worker = launch(&on, &resolved(native(&["--profile", "a", "--search"]), &[])).unwrap();
        assert_eq!(
            worker.argv(&thread),
            ["codex", "resume", ID, "--profile", "a", "--search"]
        );
        let shell = LaunchCommand::Shell("/opt/bin/codex  -m  gpt".into());
        let default = launch(&on, &resolved(shell, &[("CODEX_HOME", "/h/.codex-w")])).unwrap();
        assert_eq!(
            default.argv(&thread),
            ["/opt/bin/codex", "resume", ID, "-m", "gpt"]
        );
        assert_eq!(default.endpoint().home(), Some(Path::new("/h/.codex-w")));
        assert_eq!(default.endpoint().program(), "/opt/bin/codex");
    }

    #[test]
    fn a_codex_without_the_shared_daemon_or_a_shell_line_is_not_owned() {
        let on = workspaces(true);
        for args in [&["--no-daemon"][..], &["--remote=x"], &["--remote", "x"]] {
            assert_eq!(launch(&on, &resolved(native(args), &[])), None, "{args:?}");
        }
        for line in [
            "exec codex",
            "FOO=1 codex",
            "codex $FLAGS",
            "codex; rm x",
            "codex 'a b'",
            "codexx",
        ] {
            let command = LaunchCommand::Shell(line.into());
            assert_eq!(launch(&on, &resolved(command, &[])), None, "{line}");
        }
    }
}
