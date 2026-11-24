//! What an agent gets of its workspace's connections (ADR 0069, 0075):
//! every one, each as one environment variable whose value comes from the
//! macOS Keychain. Every agent runs as the same user, so this was never a
//! boundary. Secrets never reach a log or an error message.

use std::process::{Command, Stdio};

use crate::records::Workspace;

/// The Keychain items' service; the account is `<workspace>/<connection>`.
pub const SERVICE: &str = "muxa";
const SECURITY: &str = "/usr/bin/security";

pub fn account(workspace: &str, connection: &str) -> String {
    format!("{workspace}/{connection}")
}

/// The variables of every connection of `ws`, each read with `secret`. Blocking.
pub fn env_with(
    ws: &Workspace,
    secret: impl Fn(&str) -> Result<String, String>,
) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for c in &ws.connections {
        let value = secret(&account(&ws.name, &c.name))
            .map_err(|why| format!("connection {:?}: {why}", c.name))?;
        out.push((c.env.clone(), value));
    }
    Ok(out)
}

/// [`env_with`] from the Keychain. Blocking.
pub fn env(ws: &Workspace) -> Result<Vec<(String, String)>, String> {
    let out = env_with(ws, keychain)?;
    if !out.is_empty() {
        let names: Vec<&str> = ws.connections.iter().map(|c| c.name.as_str()).collect();
        tracing::info!(workspace = %ws.name, connections = ?names, "connections given");
    }
    Ok(out)
}

/// The names of every connection of `ws`.
pub fn names(ws: &Workspace) -> Vec<String> {
    ws.connections.iter().map(|c| c.name.clone()).collect()
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
        let c = |name: &str, env: &str| crate::records::connections::Connection {
            name: name.into(),
            env: env.into(),
            account: None,
            access: Access::Read,
        };
        Workspace {
            name: "work".into(),
            root: "/w".into(),
            repos: Vec::new(),
            missing_repos: Vec::new(),
            hidden_repos: Vec::new(),
            agent: None,
            loop_checks: None,
            park_after: None,
            routines: None,
            connections: vec![c("docs", "DOCS_TOKEN"), c("linear", "LINEAR_API_KEY")],
            mcp: Vec::new(),
            claude_connectors: false,
            bot_rules: Default::default(),
            subscriptions: Vec::new(),
        }
    }

    fn fake(account: &str) -> Result<String, String> {
        match account {
            "work/slack" => Err("no secret".into()),
            other => Ok(format!("secret-of-{other}")),
        }
    }

    #[test]
    fn every_agent_gets_every_connection_of_its_workspace() {
        let env = env_with(&ws(), fake).unwrap();
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
        assert_eq!(names(&ws()), ["docs", "linear"]);
    }

    #[test]
    fn a_missing_secret_says_which_connection() {
        let mut w = ws();
        w.connections[1].name = "slack".into();
        let e = env_with(&w, fake).unwrap_err();
        assert_eq!(e, "connection \"slack\": no secret");
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
