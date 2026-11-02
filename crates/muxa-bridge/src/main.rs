//! `muxa-bridge`: serves the muxa protocol on stdin/stdout, started over SSH.

mod agents;
mod args;
mod changes;
mod chat;
mod convert;
mod docs;
mod lines;
mod records;
mod server;
mod sessions;
mod states;
mod task_records;
mod tasks;
mod utc;
mod watch;

use std::process::ExitCode;

use args::Action;

fn main() -> ExitCode {
    let args = match args::parse(std::env::args_os().skip(1)) {
        Ok(Action::Serve(args)) => args,
        Ok(Action::Version) => {
            println!("muxa-bridge {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Ok(Action::Help) => {
            println!("{}", args::USAGE);
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("muxa-bridge: {e}\n{}", args::USAGE);
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

    let path = std::env::var("PATH").ok();
    let Some(tmux) = args::find_tmux(args.tmux, path.as_deref()) else {
        eprintln!("muxa-bridge: tmux not found; pass --tmux PATH");
        return ExitCode::from(2);
    };
    let tasks = tasks::Config {
        launcher: args.task_launcher,
        command: args.task_command,
        tmux: tmux.clone(),
        socket: args.socket.clone(),
    };
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let records = task_records::Config {
        state_dir: args.state_dir.or_else(|| {
            records::Store::default_dir(
                std::env::var_os("XDG_STATE_HOME").as_deref(),
                home.as_deref(),
            )
        }),
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
    let config = server::Config {
        tmux: muxa_tmux::Options {
            tmux,
            socket: args.socket,
        },
        tasks: std::sync::Arc::new(tasks),
        records: std::sync::Arc::new(records),
        idle: args.idle,
        agents: std::sync::Arc::new(agents::Registry::standard(&agents::data_dirs(
            &args.agent_dirs,
            home.as_deref(),
            |key| records::agent_env::dirs_of(workspace_agents.iter(), key),
        ))),
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
