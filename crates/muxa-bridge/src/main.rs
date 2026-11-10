//! `muxa-bridge`: serves the muxa protocol on stdin/stdout, started over SSH.
//! `muxa-bridge daemon ...` runs and manages the host daemon (ADR 0019).

mod agents;
mod args;
mod assistant;
mod changes;
mod chat;
mod convert;
mod daemon;
mod docs;
mod items;
mod lines;
mod loops;
mod records;
mod routines;
mod server;
mod sessions;
mod sizer;
mod states;
mod task_records;
mod task_watch;
mod tasks;
mod utc;
mod viewport;
mod watch;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use args::Action;

fn main() -> ExitCode {
    let rpc = std::env::args_os().nth(1).is_some_and(|a| a == "codex-rpc");
    let usage = if rpc {
        args::CODEX_RPC_USAGE
    } else {
        args::USAGE
    };
    let args = match args::parse(std::env::args_os().skip(1)) {
        Ok(Action::Serve(args)) => args,
        Ok(Action::Daemon(d)) => return daemon::main(d),
        Ok(Action::CodexRpc(c)) => return codex_rpc(c),
        Ok(Action::Version) => {
            println!("muxa-bridge {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Ok(Action::Help(text)) => {
            println!("{text}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("muxa-bridge: {e}\n{usage}");
            return ExitCode::from(2);
        }
    };
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("MUXA_LOG")
                .unwrap_or_else(|_| "warn".into()),
        )
        .init();

    let idle = args.idle;
    let setup = match Setup::new(args) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("muxa-bridge: {e}");
            return ExitCode::from(2);
        }
    };
    let config = server::Config {
        tmux: setup.tmux_options(),
        tasks: setup.tasks,
        records: setup.records,
        idle,
        agents: setup.agents,
        daemon_dir: setup.daemon_dir,
    };

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("muxa-bridge: cannot start: {e}");
            return ExitCode::FAILURE;
        }
    };
    let end = runtime.block_on(server::serve(
        config,
        tokio::io::stdin(),
        tokio::io::stdout(),
    ));
    tracing::info!(?end, "session ended");
    // tokio's stdin reads on a blocking thread that cannot be cancelled;
    // waiting for it would keep the process alive after the session ended.
    runtime.shutdown_background();
    match end {
        server::End::TmuxExited | server::End::WriterFailed => ExitCode::FAILURE,
        _ => ExitCode::SUCCESS,
    }
}

/// `muxa-bridge codex-rpc`: one request through muxa's own App Server client.
/// The output goes to the developer's terminal, never to a log.
fn codex_rpc(c: args::CodexRpc) -> ExitCode {
    let params = match serde_json::from_str::<serde_json::Value>(&c.params) {
        Ok(v) if v.is_object() => v,
        _ => {
            eprintln!("muxa-bridge codex-rpc: JSON_PARAMS must be a JSON object");
            return ExitCode::from(2);
        }
    };
    let endpoint = agents::codex::app_server::Endpoint::new(c.program, c.home);
    let response = match endpoint.request(&c.method, params) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("muxa-bridge codex-rpc: {e}");
            return ExitCode::FAILURE;
        }
    };
    let pretty = |v: &serde_json::Value| serde_json::to_string_pretty(v).unwrap_or_default();
    match (response.get("result"), response.get("error")) {
        (Some(result), _) => {
            println!("{}", pretty(result));
            ExitCode::SUCCESS
        }
        (None, Some(error)) => {
            eprintln!("muxa-bridge codex-rpc: error answer\n{}", pretty(error));
            ExitCode::FAILURE
        }
        (None, None) => {
            eprintln!("muxa-bridge codex-rpc: the answer has no result or error");
            ExitCode::FAILURE
        }
    }
}

/// What serve mode and the daemon both build from the flags and the environment.
pub struct Setup {
    pub tmux: PathBuf,
    pub socket: Option<String>,
    pub tasks: Arc<tasks::Config>,
    pub records: Arc<task_records::Config>,
    pub agents: Arc<agents::Registry>,
    /// None when neither `--daemon-dir` nor `HOME` gives one: no daemon.
    pub daemon_dir: Option<PathBuf>,
}

impl Setup {
    pub fn new(args: args::Args) -> Result<Self, String> {
        let path = std::env::var("PATH").ok();
        let tmux = args::find_tmux(args.tmux, path.as_deref())
            .ok_or("tmux not found; pass --tmux PATH")?;
        let tasks = tasks::Config {
            launcher: args.task_launcher,
            command: args.task_command,
            tmux: tmux.clone(),
            socket: args.socket.clone(),
        };
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let xdg_state = std::env::var_os("XDG_STATE_HOME");
        let records = task_records::Config {
            state_dir: args
                .state_dir
                .or_else(|| records::Store::default_dir(xdg_state.as_deref(), home.as_deref())),
            workspaces: args.workspaces.or_else(|| {
                records::Workspaces::default_path(
                    std::env::var_os("XDG_CONFIG_HOME").as_deref(),
                    home.as_deref(),
                )
            }),
            home: home.clone(),
        };
        // Read once: a bridge lives for one SSH connection (ADR 0016).
        let workspace_agents: Vec<records::AgentSettings> = records
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
        Ok(Self {
            tmux,
            socket: args.socket,
            tasks: Arc::new(tasks),
            records: Arc::new(records),
            agents: Arc::new(agents),
            daemon_dir: args
                .daemon_dir
                .or_else(|| daemon::default_dir(xdg_state.as_deref(), home.as_deref())),
        })
    }

    pub fn tmux_options(&self) -> muxa_tmux::Options {
        muxa_tmux::Options {
            tmux: self.tmux.clone(),
            socket: self.socket.clone(),
        }
    }
}
