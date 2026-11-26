//! `disk` and `disk clean` (ADR 0102 §5): the host's Task places, other
//! worktrees and build folders, and their removal. A dry run unless `--yes`.

use serde_json::{Value, json};

use super::client::State;
use super::open;

#[derive(Debug, PartialEq, Eq)]
pub enum Disk {
    Report {
        json: bool,
    },
    Clean {
        paths: Vec<String>,
        yes: bool,
        force: bool,
        json: bool,
    },
}

/// The request for `d`. Pure.
pub fn request(d: &Disk) -> Value {
    match d {
        Disk::Report { .. } => json!({"type": "disk"}),
        Disk::Clean {
            paths, yes, force, ..
        } => json!({"type": "disk-clean", "paths": paths, "force": force, "dry_run": !yes}),
    }
}

pub async fn run(bridge: &str, d: &Disk) -> Result<(), String> {
    let (mut c, _child) = open(bridge, State::default()).await?;
    let reply = c.request(request(d)).await?;
    match d {
        Disk::Report { json: true } => println!("{}", reply["rows"]),
        Disk::Report { json: false } => {
            let rows = reply["rows"].as_array().cloned().unwrap_or_default();
            for r in &rows {
                println!("{}", row(r));
            }
            println!("{}", total(&rows));
        }
        Disk::Clean { json: true, .. } => println!("{reply}"),
        Disk::Clean { json: false, .. } => print!("{}", cleaned(&reply)),
    }
    Ok(())
}

/// One line per row: size, build size, kind, state, path. Pure.
fn row(r: &Value) -> String {
    let state = match (r["held"].as_str(), r["cleanable"].as_bool()) {
        (Some(h), _) => format!("held ({h})"),
        (None, Some(true)) => "cleanable".to_owned(),
        _ => match (r["clean"].as_bool(), r["merged"].as_bool()) {
            (Some(false), _) => "uncommitted".to_owned(),
            (_, Some(false)) => "unmerged".to_owned(),
            _ => "kept".to_owned(),
        },
    };
    format!(
        "{:>8} {:>8}  {:<15} {:<12} {}",
        size(r["bytes"].as_u64()),
        size(r["build_bytes"].as_u64()),
        r["kind"].as_str().unwrap_or("?"),
        state,
        r["path"].as_str().unwrap_or("?"),
    )
}

fn total(rows: &[Value]) -> String {
    let sum = |key: &str, only: bool| -> u64 {
        rows.iter()
            .filter(|r| !only || r["cleanable"].as_bool() == Some(true))
            .map(|r| r[key].as_u64().unwrap_or(0) + r["build_bytes"].as_u64().unwrap_or(0))
            .sum()
    };
    format!(
        "{} in {} rows; {} cleanable. Remove with: muxa-cli disk clean --yes",
        size(Some(sum("bytes", false))),
        rows.len(),
        size(Some(sum("bytes", true))),
    )
}

fn cleaned(reply: &Value) -> String {
    let dry = reply["dry_run"].as_bool().unwrap_or(true);
    let verb = if dry { "would remove" } else { "removed" };
    let mut out = String::new();
    for p in reply["removed"].as_array().into_iter().flatten() {
        out.push_str(&format!("{verb} {}\n", p.as_str().unwrap_or("?")));
    }
    for k in reply["kept"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "kept {}: {}\n",
            k["path"].as_str().unwrap_or("?"),
            k["why"].as_str().unwrap_or("?")
        ));
    }
    if dry {
        out.push_str("Dry run: nothing changed. Add --yes to remove.\n");
    }
    out
}

/// Bytes as `1.2G`, `340M`, `12K`. Pure.
fn size(bytes: Option<u64>) -> String {
    let Some(b) = bytes else { return "-".into() };
    const UNITS: [(u64, &str); 3] = [(1 << 30, "G"), (1 << 20, "M"), (1 << 10, "K")];
    for (scale, unit) in UNITS {
        if b >= scale {
            let n = b as f64 / scale as f64;
            return if n < 10.0 {
                format!("{n:.1}{unit}")
            } else {
                format!("{n:.0}{unit}")
            };
        }
    }
    format!("{b}B")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_is_a_dry_run_until_yes() {
        let dry = Disk::Clean {
            paths: vec![],
            yes: false,
            force: false,
            json: false,
        };
        assert_eq!(
            request(&dry),
            json!({"type": "disk-clean", "paths": [], "force": false, "dry_run": true})
        );
        let real = Disk::Clean {
            paths: vec!["/w/r-x".into()],
            yes: true,
            force: true,
            json: false,
        };
        assert_eq!(
            request(&real),
            json!({"type": "disk-clean", "paths": ["/w/r-x"], "force": true, "dry_run": false})
        );
    }

    #[test]
    fn rows_and_sizes_read_well() {
        assert_eq!(size(Some(3 << 30)), "3.0G");
        assert_eq!(size(Some(340 << 20)), "340M");
        assert_eq!(size(None), "-");
        let r = json!({"path": "/w/.worktrees/r-x", "kind": "task-worktree", "bytes": 2048,
                       "clean": true, "merged": false, "cleanable": false});
        assert_eq!(
            row(&r),
            "    2.0K        -  task-worktree   unmerged     /w/.worktrees/r-x"
        );
        let out = cleaned(
            &json!({"dry_run": true, "removed": ["/a"], "kept": [{"path": "/b", "why": "held"}]}),
        );
        assert!(out.starts_with("would remove /a\nkept /b: held\nDry run"));
    }
}
