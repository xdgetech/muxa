//! `muxa-bridge`: `connect` relays protocol v2 clients to the daemon, and
//! `muxa-bridge daemon ...` runs and manages the daemon, which serves
//! protocol v2 on herdr-core (ADR 0067).

mod agents;
mod args;
mod changes;
mod config;
mod connections;
mod daemon;
mod docs;
mod launch;
mod loops;
mod records;
mod routines;
mod usage;
mod utc;
mod v2;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use args::Action;

fn main() -> ExitCode {
    match args::parse(std::env::args_os().skip(1)) {
        Ok(Action::Bare(args))
            if args::asks_for_connect(std::env::var("SSH_ORIGINAL_COMMAND").ok().as_deref()) =>
        {
            connect(args)
        }
        Ok(Action::Bare(_)) | Ok(Action::Help(_)) => {
            println!("{}", args::USAGE);
            ExitCode::SUCCESS
        }
        Ok(Action::Daemon(d)) => daemon::main(d),
        Ok(Action::Connect(a)) => connect(a),
        Ok(Action::Version) => {
            println!("muxa-bridge {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("muxa-bridge: {e}\n{}", args::USAGE);
            ExitCode::from(2)
        }
    }
}

/// What the daemon builds from the flags and the environment.
pub struct Setup {
    /// `--task-command`: the agent a Workspace without one runs.
    pub task_command: String,
    pub config: Arc<config::Config>,
    pub agents: Arc<agents::Registry>,
    /// None when neither `--daemon-dir` nor `HOME` gives one.
    pub daemon_dir: Option<PathBuf>,
}

impl Setup {
    pub fn new(args: args::Args) -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let xdg_state = std::env::var_os("XDG_STATE_HOME");
        let config = config::Config {
            state_dir: args.state_dir.or_else(|| {
                config::Config::default_state_dir(xdg_state.as_deref(), home.as_deref())
            }),
            workspaces: args.workspaces.or_else(|| {
                records::Workspaces::default_path(
                    std::env::var_os("XDG_CONFIG_HOME").as_deref(),
                    home.as_deref(),
                )
            }),
            home: home.clone(),
        };
        // Read once at start: restart the daemon after a change (ADR 0016).
        let workspace_agents: Vec<records::AgentSettings> = config
            .workspaces
            .as_deref()
            .map(|path| match records::Workspaces::load(path, home.as_deref()) {
                Ok(w) => w.all().iter().filter_map(|w| w.agent.clone()).collect(),
                Err(e) => {
                    tracing::warn!(error = %e, "cannot load workspaces; adapters use the default agent dirs");
                    Vec::new()
                }
            })
            .unwrap_or_default();
        let agents = agents::Registry::standard(&agents::data_dirs(
            &args.agent_dirs,
            home.as_deref(),
            |key| records::agent_env::dirs_of(workspace_agents.iter(), key),
        ));
        Self {
            task_command: args.task_command,
            config: Arc::new(config),
            agents: Arc::new(agents),
            daemon_dir: args
                .daemon_dir
                .or_else(|| daemon::default_dir(xdg_state.as_deref(), home.as_deref())),
        }
    }
}

/// `muxa-bridge connect`: the SSH exec command of protocol v2 clients.
fn connect(args: args::Args) -> ExitCode {
    let dir = args.daemon_dir.or_else(|| {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        daemon::default_dir(
            std::env::var_os("XDG_STATE_HOME").as_deref(),
            home.as_deref(),
        )
    });
    let Some(dir) = dir else {
        eprintln!("muxa-bridge connect: HOME is not set; pass --daemon-dir");
        return ExitCode::from(2);
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("muxa-bridge connect: cannot start the runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(v2::connect(&dir)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("muxa-bridge connect: {e}");
            ExitCode::FAILURE
        }
    }
}
