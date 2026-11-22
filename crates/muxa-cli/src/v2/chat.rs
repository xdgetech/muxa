//! `muxa-cli v2 chat`: the conversation of a Task's agent (plan F4), one line
//! per entry; `--seconds N` follows it and prints what changes.

use serde_json::{Value, json};

use super::client::{Event, State};

pub async fn run(
    bridge: &str,
    terminal: &str,
    seconds: Option<u64>,
    json_out: bool,
) -> Result<(), String> {
    let (mut c, _child) = super::open(bridge, State::default()).await?;
    let reply = c
        .request(json!({"type": "chat-open", "terminal": terminal}))
        .await?;
    print(&reply["update"], json_out);
    let Some(seconds) = seconds else {
        return Ok(());
    };
    let chat = reply["chat"].clone();
    let deadline = super::deadline(Some(seconds));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            event = c.next() => {
                let Event::Notice(msg) = event? else { continue };
                if msg["chat"] != chat {
                    continue;
                }
                match msg["type"].as_str() {
                    Some("chat") => print(&msg["update"], json_out),
                    _ => return Err(msg["reason"].as_str().unwrap_or("the chat ended").to_owned()),
                }
            }
            () = &mut deadline => return Ok(()),
        }
    }
}

fn print(update: &Value, json_out: bool) {
    if json_out {
        println!("{update}");
        return;
    }
    for e in update["entries"].as_array().into_iter().flatten() {
        println!("{}", entry_line(e));
    }
    if let Some(p) = update["prompt"].as_object() {
        let options: Vec<&str> = p["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| o["label"].as_str())
            .collect();
        println!(
            "waits ({}): {}",
            p["kind"].as_str().unwrap_or("?"),
            options.join(" | ")
        );
    }
}

/// `user: text`, `tool Bash: rm -rf build`.
fn entry_line(e: &Value) -> String {
    let kind = e["kind"].as_str().unwrap_or("?");
    match e["tool"].as_object() {
        Some(t) => format!(
            "tool {}: {}",
            t["name"].as_str().unwrap_or("?"),
            t["summary"].as_str().unwrap_or("")
        ),
        None => format!("{kind}: {}", e["text"].as_str().unwrap_or("")),
    }
}
