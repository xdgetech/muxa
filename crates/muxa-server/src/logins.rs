//! A login terminal (ADR 0103 §7): the agent's own login for one
//! subscription, run on the host so a user can log in from a phone. muxa
//! starts it, labels it, and never reads what is typed into it.

use muxa_backend::{Labels, Launch, SessionBackend, TerminalInfo};
use muxa_protocol::v2::ErrorCode;

use crate::session::{code_of, task_code};
use crate::task_actions::remove_terminal;
use crate::tasks::{Host, KIND, SUBSCRIPTION};

const LOGIN: &str = "login";

type Refusal = (ErrorCode, String);

/// What the host runs for a subscription's login: the agent's own login
/// command with the subscription's folder in its environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoginLaunch {
    pub argv: Vec<String>,
    pub cwd: Option<std::path::PathBuf>,
    pub env: std::collections::BTreeMap<String, String>,
}

fn is_login_of(t: &TerminalInfo, name: &str) -> bool {
    t.labels.get(KIND).map(String::as_str) == Some(LOGIN)
        && t.labels.get(SUBSCRIPTION).map(String::as_str) == Some(name)
}

/// The live login terminal of `name`, or a new one. Ended ones of the same
/// subscription go first. The bool says whether it was already running.
pub(crate) async fn start<B: SessionBackend, H: Host>(
    backend: &B,
    host: &H,
    name: &str,
) -> Result<(TerminalInfo, bool), Refusal> {
    let terminals = backend
        .list()
        .await
        .map_err(|e| (code_of(&e), e.to_string()))?;
    if let Some(live) = terminals.iter().find(|t| t.live && is_login_of(t, name)) {
        return Ok((live.clone(), true));
    }
    for ended in terminals.iter().filter(|t| !t.live && is_login_of(t, name)) {
        remove_terminal(backend, &ended.id).await;
    }
    let login = host
        .login_launch(name)
        .await
        .map_err(|e| (task_code(&e), e.to_string()))?;
    let launch = Launch {
        argv: login.argv,
        cwd: login.cwd,
        name: Some(format!("login-{name}")),
        labels: Labels::from([
            (KIND.to_owned(), LOGIN.to_owned()),
            (SUBSCRIPTION.to_owned(), name.to_owned()),
        ]),
        env: login.env,
    };
    let started = backend
        .run(&launch)
        .await
        .map_err(|e| (code_of(&e), e.to_string()))?;
    tracing::info!(terminal = %started.id, subscription = name, "login terminal");
    Ok((started, false))
}
