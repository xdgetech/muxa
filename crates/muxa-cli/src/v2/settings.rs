//! `settings` and `settings-set`: the host's editable settings (ADR 0074),
//! for reading them and scripting ops from a JSON file or stdin.

use std::io::Read;

use serde_json::{Value, json};

use super::client::State;
use super::open;

#[derive(Debug, PartialEq, Eq)]
pub struct Set {
    /// `-` or none: stdin.
    pub file: Option<String>,
    /// None: the current one, read first. A hand edit in between is then
    /// overwritten, so a script that read the settings should pass it.
    pub revision: Option<String>,
    pub json: bool,
}

pub async fn show(bridge: &str, json: bool) -> Result<(), String> {
    let (mut c, _child) = open(bridge, State::default()).await?;
    let reply = c.request(json!({"type": "settings"})).await?;
    print(&reply["settings"], json);
    Ok(())
}

/// The ops: a list, or `{"revision"?, "ops"}`. A revision given on the
/// command line wins.
fn ops_of(input: &str, revision: Option<String>) -> Result<(Option<String>, Value), String> {
    let value: Value = serde_json::from_str(input).map_err(|e| {
        format!(
            "the ops are not JSON: line {}, column {}",
            e.line(),
            e.column()
        )
    })?;
    let (from_file, ops) = match value {
        Value::Array(_) => (None, value),
        Value::Object(mut o) => (
            o.remove("revision")
                .and_then(|r| r.as_str().map(str::to_owned)),
            o.remove("ops").ok_or("the JSON object has no \"ops\"")?,
        ),
        _ => return Err("the ops are a JSON list, or an object with \"ops\"".into()),
    };
    if !ops.is_array() {
        return Err("\"ops\" is not a list".into());
    }
    Ok((revision.or(from_file), ops))
}

pub async fn set(bridge: &str, args: Set) -> Result<(), String> {
    let mut input = String::new();
    match args.file.as_deref() {
        None | Some("-") => std::io::stdin()
            .read_to_string(&mut input)
            .map_err(|e| format!("cannot read stdin: {e}"))?,
        Some(path) => {
            input =
                std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
            input.len()
        }
    };
    let (revision, ops) = ops_of(&input, args.revision)?;
    let (mut c, _child) = open(bridge, State::default()).await?;
    let revision = match revision {
        Some(r) => r,
        None => {
            let reply = c.request(json!({"type": "settings"})).await?;
            reply["settings"]["revision"]
                .as_str()
                .ok_or("the host sent no revision")?
                .to_owned()
        }
    };
    let reply = c
        .request(json!({"type": "settings-update", "revision": revision, "ops": ops}))
        .await?;
    print(&reply["settings"], args.json);
    Ok(())
}

fn print(settings: &Value, json: bool) {
    if json {
        println!("{settings}");
        return;
    }
    let text = |v: &Value| v.as_str().unwrap_or("").to_owned();
    println!("revision {}", text(&settings["revision"]));
    if settings["read_only"] == true {
        println!("read only (--settings-read-only)");
    }
    if let Some(p) = settings["problem"].as_str() {
        println!("problem: {p}");
    }
    let names = |v: &Value, key: &str| -> String {
        let list: Vec<String> = v
            .as_array()
            .into_iter()
            .flatten()
            .map(|x| text(&x[key]))
            .collect();
        list.join(",")
    };
    for w in settings["workspaces"].as_array().into_iter().flatten() {
        let connections: Vec<String> = w["connections"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| format!("{}({})", text(&c["name"]), text(&c["secret"])))
            .collect();
        let subscriptions: Vec<String> = w["subscriptions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(text)
            .collect();
        println!(
            "{}\t{}\tenv:{}\tconnections:{}\tmcp:{}\troutines:{}\tsubscriptions:{}",
            text(&w["name"]),
            text(&w["root"]),
            names(&w["agent"]["env"], "key"),
            connections.join(","),
            names(&w["mcp"], "name"),
            names(&w["routines"], "name"),
            subscriptions.join(","),
        );
        if let Some(p) = w["routines_problem"].as_str() {
            println!("  routines: {p}");
        }
    }
    for worker in settings["workers"].as_array().into_iter().flatten() {
        println!(
            "worker {}\t{}",
            text(&worker["name"]),
            text(&worker["agent"])
        );
    }
    for sub in settings["subscriptions"].as_array().into_iter().flatten() {
        println!(
            "subscription {}\t{}\t{}\t{}",
            text(&sub["name"]),
            text(&sub["agent"]),
            text(&sub["home"]),
            text(&sub["plan"])
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ops_come_as_a_list_or_with_their_revision() {
        let (rev, ops) = ops_of(r#"[{"op":"worker-remove","name":"r"}]"#, None).unwrap();
        assert_eq!((rev, ops[0]["op"].as_str()), (None, Some("worker-remove")));
        let (rev, _) = ops_of(r#"{"revision":"ab","ops":[]}"#, None).unwrap();
        assert_eq!(rev.as_deref(), Some("ab"));
        let (rev, _) = ops_of(r#"{"revision":"ab","ops":[]}"#, Some("cd".into())).unwrap();
        assert_eq!(rev.as_deref(), Some("cd"));
        let err = ops_of(r#"{"secret":"s3cret"#, None).unwrap_err();
        assert!(!err.contains("s3cret"), "{err}");
        assert!(ops_of(r#"{"ops":{}}"#, None).is_err());
    }
}
