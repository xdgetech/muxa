//! What an agent gets of its workspace's connections (ADR 0069): the ones
//! on by default and the ones its Task or Routine names, each as one
//! environment variable whose value comes from the macOS Keychain. Less
//! ambient access, not a boundary: every agent runs as the same user.
//! Secrets never reach a log or an error message.

use std::process::{Command, Stdio};

use crate::records::connections::Connection;
use crate::records::{AgentSettings, Workspace};

/// The Keychain items' service; the account is `<workspace>/<connection>`.
pub const SERVICE: &str = "muxa";
const SECURITY: &str = "/usr/bin/security";

pub fn account(workspace: &str, connection: &str) -> String {
    format!("{workspace}/{connection}")
}

/// The connections an agent gets: the default ones and `named`, once each,
/// in the workspace's order. An unknown name is an error.
pub fn chosen<'a>(ws: &'a Workspace, named: &[String]) -> Result<Vec<&'a Connection>, String> {
    if let Some(unknown) = named
        .iter()
        .find(|n| !ws.connections.iter().any(|c| &c.name == *n))
    {
        return Err(format!(
            "Workspace {} has no connection {unknown:?}; add it as [[workspace.connection]] in workspaces.toml",
            ws.name
        ));
    }
    Ok(ws
        .connections
        .iter()
        .filter(|c| c.default || named.contains(&c.name))
        .collect())
}

/// The variables for [`chosen`], each read with `secret`. Blocking.
pub fn env_with(
    ws: &Workspace,
    named: &[String],
    secret: impl Fn(&str) -> Result<String, String>,
) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for c in chosen(ws, named)? {
        let value = secret(&account(&ws.name, &c.name))
            .map_err(|why| format!("connection {:?}: {why}", c.name))?;
        out.push((c.env.clone(), value));
    }
    Ok(out)
}

/// [`env_with`] from the Keychain. Blocking.
pub fn env(ws: &Workspace, named: &[String]) -> Result<Vec<(String, String)>, String> {
    let out = env_with(ws, named, keychain)?;
    if !out.is_empty() {
        let names: Vec<&str> = chosen(ws, named)?.iter().map(|c| c.name.as_str()).collect();
        tracing::info!(workspace = %ws.name, connections = ?names, "connections given");
    }
    Ok(out)
}

/// What a v1 Task or a Routine run gets: the default connections, and the
/// ones its Routine names. Blocking.
pub fn for_run(
    workspaces: &crate::records::Workspaces,
    workspace: &str,
    routine: Option<&str>,
) -> Result<Vec<(String, String)>, String> {
    let Some(ws) = workspaces.get(workspace) else {
        return Ok(Vec::new());
    };
    let named = routine
        .and_then(|name| workspaces.routine(workspace, name))
        .map(|r| r.connections.clone())
        .unwrap_or_default();
    env(ws, &named)
}

/// A launch on the workspace's default agent: the workspace's command, and
/// `env`, the variables resolved for it (its own and the connections).
pub fn default_agent(workspace: &Workspace, env: &[(String, String)]) -> Option<AgentSettings> {
    let command = workspace.agent.as_ref().and_then(|a| a.command.clone());
    (command.is_some() || !env.is_empty()).then(|| AgentSettings {
        command,
        env: env.to_vec(),
    })
}

/// Adds `extra` to an agent's variables, keeping them sorted by key.
pub fn merge(env: &mut Vec<(String, String)>, extra: Vec<(String, String)>) {
    env.extend(extra);
    env.sort_by(|a, b| a.0.cmp(&b.0));
}

/// The secret of `account` from the login Keychain, through `security`.
/// Blocking.
pub fn keychain(account: &str) -> Result<String, String> {
    // A debug build's tests keep secrets in files, never in the Keychain.
    if cfg!(debug_assertions) {
        if let Some(dir) = std::env::var_os("MUXA_TEST_SECRETS_DIR") {
            let file = std::path::Path::new(&dir).join(account.replace('/', "__"));
            return std::fs::read_to_string(file)
                .map(|s| s.trim_end_matches('\n').to_owned())
                .map_err(|_| "no secret in the test folder".into());
        }
    }
    if !cfg!(target_os = "macos") {
        return Err("muxa keeps secrets in the macOS Keychain; this host has none".into());
    }
    let out = Command::new(SECURITY)
        .args(["find-generic-password", "-s", SERVICE, "-a", account, "-w"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {SECURITY}: {e}"))?;
    if !out.status.success() {
        // 44: no such item.
        return Err(match out.status.code() {
            Some(44) => format!(
                "no secret in the Keychain; add it with: security add-generic-password -s {SERVICE} -a '{account}' -w"
            ),
            code => format!(
                "the Keychain refused (security exited with {code:?}); if it is locked, log in on the host"
            ),
        });
    }
    let mut value =
        String::from_utf8(out.stdout).map_err(|_| "the secret is not UTF-8".to_owned())?;
    if value.ends_with('\n') {
        value.pop();
    }
    if value.is_empty() {
        return Err("the secret in the Keychain is empty".into());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::connections::Access;

    fn ws() -> Workspace {
        let c = |name: &str, env: &str, default: bool| Connection {
            name: name.into(),
            env: env.into(),
            account: None,
            access: Access::Read,
            default,
        };
        Workspace {
            name: "work".into(),
            root: "/w".into(),
            repos: Vec::new(),
            missing_repos: Vec::new(),
            agent: None,
            loop_checks: None,
            park_after: None,
            routines: None,
            connections: vec![
                c("docs", "DOCS_TOKEN", true),
                c("linear", "LINEAR_API_KEY", false),
                c("slack", "SLACK_TOKEN", false),
            ],
        }
    }

    fn fake(account: &str) -> Result<String, String> {
        match account {
            "work/slack" => Err("no secret".into()),
            other => Ok(format!("secret-of-{other}")),
        }
    }

    #[test]
    fn an_agent_gets_the_default_and_the_named_connections_only() {
        let env = env_with(&ws(), &["linear".into()], fake).unwrap();
        assert_eq!(
            env,
            [
                ("DOCS_TOKEN".to_owned(), "secret-of-work/docs".to_owned()),
                (
                    "LINEAR_API_KEY".to_owned(),
                    "secret-of-work/linear".to_owned()
                ),
            ]
        );
        let none_named = env_with(&ws(), &[], fake).unwrap();
        assert_eq!(none_named.len(), 1, "the default one alone");
    }

    #[test]
    fn an_unknown_name_and_a_missing_secret_say_which_connection() {
        let e = env_with(&ws(), &["github".into()], fake).unwrap_err();
        assert!(e.contains("no connection \"github\""), "{e}");
        let e = env_with(&ws(), &["slack".into()], fake).unwrap_err();
        assert_eq!(e, "connection \"slack\": no secret");
    }

    #[test]
    fn the_default_agent_keeps_its_command_and_gets_the_resolved_variables() {
        let mut w = ws();
        w.agent = Some(AgentSettings {
            command: Some("claude --model opus".into()),
            env: vec![("A".into(), "1".into())],
        });
        let env = vec![
            ("A".to_owned(), "1".to_owned()),
            ("DOCS_TOKEN".to_owned(), "t".to_owned()),
        ];
        let agent = default_agent(&w, &env).unwrap();
        assert_eq!(agent.command.as_deref(), Some("claude --model opus"));
        assert_eq!(agent.env, env);
        w.agent = None;
        assert!(default_agent(&w, &[]).is_none());
    }

    #[test]
    fn merged_variables_stay_sorted() {
        let mut env = vec![
            ("A".to_owned(), "1".to_owned()),
            ("Z".to_owned(), "2".to_owned()),
        ];
        merge(&mut env, vec![("M".to_owned(), "3".to_owned())]);
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["A", "M", "Z"]);
    }
}
