use std::collections::BTreeMap;

use super::*;
use crate::records::connections::Access;

const SECRET: &str = "lin_api_s3cret";

fn connection(name: &str, env: &str) -> Connection {
    Connection {
        name: name.into(),
        env: env.into(),
        account: None,
        access: Access::Read,
        default: false,
    }
}

fn server(name: &str, transport: Transport, connections: &[&str], default: bool) -> McpServer {
    McpServer {
        name: name.into(),
        transport,
        env: BTreeMap::new(),
        headers: BTreeMap::new(),
        connections: connections.iter().map(|c| (*c).to_owned()).collect(),
        default,
    }
}

fn ws(root: &Path) -> Workspace {
    let mut linear = server(
        "linear",
        Transport::Stdio {
            command: "npx".into(),
            args: vec!["-y".into(), "linear-mcp".into()],
        },
        &["linear"],
        false,
    );
    linear.env.insert("LOG".into(), "info".into());
    let mut docs = server(
        "docs",
        Transport::Http {
            url: "https://docs.example/mcp".into(),
        },
        &["docs"],
        true,
    );
    docs.headers
        .insert("Authorization".into(), "Bearer ${DOCS_TOKEN}".into());
    let slack = server(
        "slack",
        Transport::Sse {
            url: "https://slack.example/sse".into(),
        },
        &[],
        false,
    );
    Workspace {
        name: "w".into(),
        root: root.to_owned(),
        repos: Vec::new(),
        missing_repos: Vec::new(),
        agent: None,
        loop_checks: None,
        park_after: None,
        routines: None,
        connections: vec![
            connection("linear", "LINEAR_API_KEY"),
            connection("docs", "DOCS_TOKEN"),
        ],
        mcp: vec![linear, docs, slack],
    }
}

fn names(servers: &[McpServer]) -> Vec<&str> {
    servers.iter().map(|s| s.name.as_str()).collect()
}

#[test]
fn an_agent_gets_the_default_and_the_named_servers_only() {
    let w = ws(Path::new("/w"));
    assert_eq!(names(&chosen(&w, &[]).unwrap()), ["docs"]);
    assert_eq!(
        names(&chosen(&w, &["linear".into()]).unwrap()),
        ["linear", "docs"]
    );
    let e = chosen(&w, &["github".into()]).unwrap_err();
    assert!(e.contains("no MCP server \"github\""), "{e}");
}

#[test]
fn a_servers_connections_join_the_named_ones_once() {
    let w = ws(Path::new("/w"));
    let servers = chosen(&w, &["linear".into()]).unwrap();
    assert_eq!(
        with_connections(&["linear".into()], &servers),
        ["linear", "docs"]
    );
}

#[test]
fn the_config_has_only_the_chosen_servers_and_references_for_secrets() {
    let w = ws(Path::new("/w"));
    let text = config_text(&chosen(&w, &["linear".into()]).unwrap(), &w.connections);
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        value,
        json!({"mcpServers": {
            "linear": {"type": "stdio", "command": "npx", "args": ["-y", "linear-mcp"],
                       "env": {"LOG": "info", "LINEAR_API_KEY": "${LINEAR_API_KEY}"}},
            "docs": {"type": "http", "url": "https://docs.example/mcp",
                     "headers": {"Authorization": "Bearer ${DOCS_TOKEN}"}},
        }})
    );
    let sse = config_text(&chosen(&w, &["slack".into()]).unwrap()[1..], &w.connections);
    assert!(sse.contains("\"type\": \"sse\""), "{sse}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&config_text(&[], &[])).unwrap(),
        json!({"mcpServers": {}})
    );
}

#[test]
fn the_written_config_never_holds_a_secret() {
    let root = std::env::temp_dir().join(format!("muxa-mcp-secret-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let w = ws(&root);
    let servers = chosen(&w, &["linear".into()]).unwrap();
    let path = write(&root, &servers, &w.connections).unwrap();
    assert_eq!(path, root.join(".muxa/mcp.json"));
    let named = with_connections(&[], &servers);
    let env = crate::connections::env_with(&w, &named, |_| Ok(SECRET.to_owned())).unwrap();
    let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        keys,
        ["LINEAR_API_KEY", "DOCS_TOKEN"],
        "the agent gets them"
    );
    for entry in std::fs::read_dir(root.join(".muxa")).unwrap() {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(!text.contains(SECRET), "{text}");
        assert!(text.contains("${LINEAR_API_KEY}"));
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn only_claude_code_takes_servers() {
    let w = ws(Path::new("/w"));
    assert!(only_claude(Some("codex"), &[]).is_ok());
    let e = only_claude(Some("codex"), &chosen(&w, &[]).unwrap()).unwrap_err();
    assert!(e.contains("(docs)") && e.contains("codex"), "{e}");
}

#[test]
fn the_flags_follow_the_command_and_are_quoted_for_the_shell() {
    let config = Path::new("/w/it's/.muxa/mcp.json");
    assert_eq!(
        shell_flags(config),
        "--mcp-config '/w/it'\\''s/.muxa/mcp.json' --strict-mcp-config"
    );
    let LaunchCommand::Native { args, .. } = with_flags(
        LaunchCommand::Native {
            program: "claude",
            args: vec!["--agent".into(), "r".into()],
        },
        config,
    ) else {
        panic!("native stays native");
    };
    assert_eq!(
        args,
        [
            "--agent",
            "r",
            "--mcp-config",
            "/w/it's/.muxa/mcp.json",
            "--strict-mcp-config"
        ]
    );
}
