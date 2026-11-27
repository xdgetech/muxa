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
mod limits;
mod loops;
mod records;
mod routines;
mod settings;
mod status_line;
mod thresholds;
mod tokens;
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
        Ok(Action::StatusLine {
            subscription,
            state_dir,
        }) => status_line::main(&subscription, &state_dir),
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
    /// Claude Code's config dirs from `--agent-dir` or the default, for
    /// resuming a conversation in an adopted place (ADR 0076).
    pub claude_dirs: Vec<PathBuf>,
    /// None when neither `--daemon-dir` nor `HOME` gives one.
    pub daemon_dir: Option<PathBuf>,
    pub settings_read_only: bool,
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
        // Read at start and after each settings change: Workspace env and
        // subscription folders (ADR 0016, 0103).
        let (flags, path, home_dir) = (
            args.agent_dirs.clone(),
            config.workspaces.clone(),
            home.clone(),
        );
        let agents = agents::Registry::reloadable(Box::new(move || {
            let workspaces = path
                .as_deref()
                .map(|p| match records::Workspaces::load(p, home_dir.as_deref()) {
                    Ok(w) => w,
                    Err(e) => {
                        tracing::warn!(error = %e, "cannot load workspaces; adapters use the default agent dirs");
                        records::Workspaces::default()
                    }
                })
                .unwrap_or_default();
            agents::data_dirs(&flags, home_dir.as_deref(), |key| {
                workspaces.agent_dirs(key)
            })
        }));
        let claude_dirs = agents::data_dirs(&args.agent_dirs, home.as_deref(), |_| Vec::new())
            .remove(agents::claude::ID)
            .unwrap_or_default();
        Self {
            settings_read_only: args.settings_read_only,
            task_command: args.task_command,
            config: Arc::new(config),
            agents: Arc::new(agents),
            claude_dirs,
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
