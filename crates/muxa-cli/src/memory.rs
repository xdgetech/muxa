//! Workspace memory (ADR 0053), at `work` level 14: a `memory` item's text
//! whole in `inbox show`, the check before `inbox answer ITEM approve` on a
//! `memory` item, and `memory WORKSPACE`. Texts go to stdout only, as plain
//! text.

use std::io::Read;
use std::path::Path;

use muxa_protocol::{MemoryMode, MemoryProposal, WORK_MEMORY, work_level};
use serde_json::json;

use crate::assist::plain;
use crate::conn::Conn;

pub const NEEDS_MEMORY: &str =
    "this host keeps no workspace memory; update muxa-bridge on the host";
/// The file's cap on the host, in bytes.
const MAX_FILE: u64 = 16 << 10;

pub fn has_memory(conn: &Conn) -> bool {
    work_level(conn.capabilities()) >= WORK_MEMORY
}

/// The hash an `approve` on a `memory` item sends, once its text is printed
/// whole (`writes::approve_hash` finds the item). A host below level 14 gets
/// nothing.
pub fn shown_hash(conn: &Conn, m: &MemoryProposal, as_json: bool) -> Result<String, String> {
    if !has_memory(conn) {
        return Err(NEEDS_MEMORY.into());
    }
    // With --json, stdout carries the item only.
    if as_json {
        eprint!("{}", memory_text(m));
    } else {
        print!("{}", memory_text(m));
    }
    Ok(m.hash.clone())
}

/// A proposal as the user approves it: the Task, then the text whole.
pub fn memory_text(m: &MemoryProposal) -> String {
    format!(
        "From    Task {} · {}\nHash    {}\nAdded to .muxa/WORKSPACE.md exactly as below:\n{}\n",
        m.task,
        plain(&m.title),
        m.hash,
        plain(m.text.trim_end())
    )
}

/// `memory WORKSPACE`: the mode, when the file changed, and the live file.
/// The file is read where this command runs: on the host.
pub async fn show(conn: &mut Conn, workspace: &str, as_json: bool) -> Result<String, String> {
    if !has_memory(conn) {
        return Err(NEEDS_MEMORY.into());
    }
    let (workspaces, ..) = crate::tasks::fetch(conn).await?;
    let w = workspaces
        .iter()
        .find(|w| w.name == workspace)
        .ok_or_else(|| format!("no workspace {workspace}; see muxa-cli task list"))?;
    let root = w
        .root
        .as_deref()
        .ok_or("the host sent no root for the workspace")?;
    let text = read_live(Path::new(root))?;
    let mode = w.memory.unwrap_or(MemoryMode::Unknown);
    if as_json {
        println!(
            "{}",
            json!({"workspace": w.name, "memory": mode, "memoryAt": w.memory_at, "text": text})
        );
        return Ok(String::new());
    }
    let changed = match w.memory_at {
        Some(at) => format!(
            "changed {} ago",
            crate::items::age(crate::when::now().saturating_sub(at))
        ),
        None => "no file yet".into(),
    };
    println!(
        "{}  memory {}  {changed}",
        w.name,
        crate::items::wire(json!(mode))
    );
    if let Some(t) = text {
        print!("{}", plain(&t));
        if !t.ends_with('\n') {
            println!();
        }
    }
    Ok(String::new())
}

/// The live file under `root`; None without one. A symlink, a file past the
/// cap or one that is not UTF-8 is an error.
fn read_live(root: &Path) -> Result<Option<String>, String> {
    if !root.is_dir() {
        return Err(format!(
            "{} is not on this machine; run muxa-cli memory on the host",
            root.display()
        ));
    }
    let path = root.join(".muxa/WORKSPACE.md");
    match std::fs::symlink_metadata(&path) {
        Ok(m) if m.is_file() => {}
        Ok(_) => return Err(format!("{} is not a regular file", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    }
    let file =
        std::fs::File::open(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(format!("{} is over 16 KiB", path.display()));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("{} is not UTF-8 text", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proposal_shows_its_task_and_its_text_whole_as_plain_text() {
        let m = MemoryProposal {
            task: "t-00b001".into(),
            title: "Rotate the keys".into(),
            text: "- Keys live in Vault.\n- \u{1b}[31mred\u{1b}[0m\n".into(),
            hash: "sha256:4f".into(),
        };
        assert_eq!(
            memory_text(&m),
            "From    Task t-00b001 · Rotate the keys\nHash    sha256:4f\n\
             Added to .muxa/WORKSPACE.md exactly as below:\n- Keys live in Vault.\n- [31mred[0m\n"
        );
    }
}
