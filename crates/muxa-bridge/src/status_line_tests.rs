use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("muxa-status-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn hook() -> Hook {
    Hook {
        exe: PathBuf::from("/opt/muxa/bin/muxa-bridge"),
        state_dir: PathBuf::from("/state dir"),
        subscription: "work".into(),
    }
}

fn settings(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join(".muxa").join(SETTINGS_FILE)).unwrap())
        .unwrap()
}

#[test]
fn the_value_after_the_last_flag_is_read_as_the_shell_would() {
    assert_eq!(
        last_shell_value("claude --settings '/a b/x.json' --y", "--settings").as_deref(),
        Some("/a b/x.json")
    );
    assert_eq!(
        last_shell_value("claude --settings '/a'\\''b'", "--settings").as_deref(),
        Some("/a'b")
    );
    assert_eq!(
        last_shell_value("claude --settings /one --settings /two", "--settings").as_deref(),
        Some("/two")
    );
    assert_eq!(last_shell_value("claude --model x", "--settings"), None);
}

#[test]
fn a_claude_agent_gets_the_status_line_in_new_settings() {
    let dir = temp("new");
    let line = add_to_line("claude --model opus".into(), &dir, &hook()).unwrap();
    let file = dir.join(".muxa").join(SETTINGS_FILE);
    assert_eq!(
        line,
        format!("claude --model opus --settings '{}'", file.display())
    );
    assert_eq!(
        settings(&dir)["statusLine"],
        json!({"type": "command",
            "command": "'/opt/muxa/bin/muxa-bridge' statusline --subscription 'work' --state-dir '/state dir'"})
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_status_line_joins_the_settings_muxa_already_passes() {
    let dir = temp("merge");
    let file = crate::v2::git::write_file(
        &dir,
        SETTINGS_FILE,
        r#"{"permissions":{"deny":["Edit(//x/**)"]}}"#,
    )
    .unwrap();
    let native = LaunchCommand::Native {
        program: "claude",
        args: vec![SETTINGS_FLAG.into(), file.to_string_lossy().into_owned()],
    };
    let out = add(native.clone(), &dir, &hook()).unwrap();
    assert_eq!(out, native, "no second --settings");
    let merged = settings(&dir);
    assert_eq!(merged["permissions"]["deny"][0], "Edit(//x/**)");
    assert_eq!(merged["statusLine"]["type"], "command");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_users_own_settings_and_other_agents_are_left_alone() {
    let dir = temp("alone");
    let own = "claude --settings ~/my-settings.json".to_owned();
    assert_eq!(add_to_line(own.clone(), &dir, &hook()).unwrap(), own);
    assert_eq!(add_to_line("codex".into(), &dir, &hook()).unwrap(), "codex");
    assert!(!dir.join(".muxa").exists(), "nothing written");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_users_status_line_gets_the_same_input_and_prints_its_output() {
    assert_eq!(run_own("tr a-z A-Z", b"model opus"), "MODEL OPUS");
    assert_eq!(run_own("exit 3", b"{}"), "");
}
