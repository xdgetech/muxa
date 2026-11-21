//! `muxa-cli v2 reader` and `read`: the Reader of a terminal's folder (plan
//! F2). `read --seconds N` follows the file and prints each new version.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use super::client::{Client, Event, State};

#[derive(Debug, PartialEq, Eq)]
pub struct Read {
    pub terminal: String,
    /// `docs`, `changes` or `images`.
    pub kind: String,
    pub path: String,
    pub html: bool,
    pub branch: bool,
    pub seconds: Option<u64>,
}

pub fn kind(word: &str) -> Result<String, lexopt::Error> {
    match word {
        "docs" | "changes" | "images" => Ok(word.to_owned()),
        _ => Err(format!("{word:?} is not docs, changes or images").into()),
    }
}

/// One line per file: the path, and what the kind adds.
pub async fn list(
    bridge: &str,
    terminal: &str,
    kind: &str,
    html: bool,
    json: bool,
) -> Result<(), String> {
    let (mut c, _child) = super::open(bridge, State::default()).await?;
    let msg = json!({"type": "reader-list", "terminal": terminal, "kind": kind, "html": html});
    let reply = c.request(msg).await?;
    if json {
        println!("{}", reply["list"]);
        return Ok(());
    }
    for e in reply["list"]["entries"].as_array().into_iter().flatten() {
        println!("{}", row(e));
    }
    if let Some(b) = reply["list"]["branch"].as_object() {
        println!(
            "branch: {} commits since {}",
            b["commits"],
            b["base"].as_str().unwrap_or("?")
        );
        for e in b["entries"].as_array().into_iter().flatten() {
            println!("  {}", row(e));
        }
    }
    Ok(())
}

fn row(e: &Value) -> String {
    let path = e["path"].as_str().unwrap_or("?");
    match (e["status"].as_str(), e["title"].as_str()) {
        (Some(status), _) => format!("{status:<10} {path}"),
        (None, Some(title)) => format!("{path}  {title}"),
        (None, None) => path.to_owned(),
    }
}

/// Prints the file; with `seconds`, again each time it changes.
pub async fn read(bridge: &str, r: &Read) -> Result<(), String> {
    let (mut c, _child) = super::open(bridge, State::default()).await?;
    let scope = if r.branch { "branch" } else { "uncommitted" };
    let msg = json!({"type": "reader-open", "terminal": r.terminal, "kind": r.kind,
                     "path": r.path, "html": r.html, "scope": scope});
    let reply = c.request(msg).await?;
    print(&reply["content"])?;
    let Some(seconds) = r.seconds else {
        return Ok(());
    };
    follow(&mut c, &reply["reader"], seconds).await
}

async fn follow<R, W>(c: &mut Client<R, W>, reader: &Value, seconds: u64) -> Result<(), String>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let deadline = super::deadline(Some(seconds));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            event = c.next() => {
                let Event::Notice(msg) = event? else { continue };
                if msg["reader"] != *reader {
                    continue;
                }
                match msg["type"].as_str() {
                    Some("reader") => print(&msg["content"])?,
                    _ => return Err(msg["reason"].as_str().unwrap_or("the file ended").to_owned()),
                }
            }
            () = &mut deadline => return Ok(()),
        }
    }
}

/// Text as is; an image's bytes raw, for a redirect to a file.
fn print(content: &Value) -> Result<(), String> {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    if let Some(data) = content["data"].as_str() {
        let bytes = STANDARD.decode(data).map_err(|e| e.to_string())?;
        out.write_all(&bytes).map_err(|e| e.to_string())?;
    } else if content["binary"] == true {
        writeln!(out, "(a binary file)").map_err(|e| e.to_string())?;
    } else {
        let text = content["text"].as_str().unwrap_or("");
        out.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    }
    out.flush().map_err(|e| e.to_string())
}
