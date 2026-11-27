//! `host` and `host watch` (ADR 0105): the host's CPU, memory and disks,
//! and what uses them.

use serde_json::{Value, json};

use super::client::{Event, State};
use super::{deadline, open};

pub async fn run(bridge: &str, json: bool) -> Result<(), String> {
    let (mut c, _child) = open(bridge, State::default()).await?;
    let reply = c.request(json!({"type": "host"})).await?;
    show(&reply["host"], json);
    Ok(())
}

/// Prints each sample until `seconds` pass, or for ever.
pub async fn watch(bridge: &str, seconds: Option<u64>, json: bool) -> Result<(), String> {
    let (mut c, _child) = open(bridge, State::default()).await?;
    c.request(json!({"type": "host-watch"})).await?;
    let deadline = deadline(seconds);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            event = c.next() => {
                if let Event::Notice(msg) = event? {
                    if msg["type"] == "host" {
                        show(&msg["host"], json);
                    }
                }
            }
            () = &mut deadline => return Ok(()),
        }
    }
}

fn show(host: &Value, json: bool) {
    if json {
        println!("{host}");
    } else {
        print!("{}", text(host));
    }
}

/// A summary line, then one line per group. Pure.
pub fn text(h: &Value) -> String {
    let alerts: Vec<&str> = h["alerts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let mark = |key: &str| if alerts.contains(&key) { " !" } else { "" };
    let m = &h["memory"];
    let mut out = format!(
        "CPU {:.0}%{} of {} cores, load {}  Memory {} of {}{} ({}, {} compressed)  Swap {}\n",
        h["cpu"]["percent"].as_f64().unwrap_or(0.0),
        mark("cpu"),
        h["cpu"]["cores"],
        h["load"][0],
        size(m["used"].as_u64()),
        size(m["total"].as_u64()),
        mark("memory"),
        m["pressure"].as_str().unwrap_or("?"),
        size(m["compressed"].as_u64()),
        size(h["swap"]["used"].as_u64()),
    );
    for d in h["disks"].as_array().into_iter().flatten() {
        let path = d["path"].as_str().unwrap_or("?");
        out.push_str(&format!(
            "Disk {} {} free of {}{}\n",
            path,
            size(d["free"].as_u64()),
            size(d["total"].as_u64()),
            mark(&format!("disk:{path}")),
        ));
    }
    for g in h["groups"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "{:>7} {:>5.1}% {:>5}  {:<14} {}{}\n",
            size(g["memory"].as_u64()),
            g["cpu"].as_f64().unwrap_or(0.0),
            g["processes"].as_u64().unwrap_or(0),
            g["kind"].as_str().unwrap_or("?"),
            g["name"].as_str().unwrap_or("?"),
            idle(&g["idle_since_ms"], h["sampled_ms"].as_u64()),
        ));
    }
    out
}

fn idle(since: &Value, now: Option<u64>) -> String {
    match (since.as_u64(), now) {
        (Some(s), Some(n)) => format!("  (idle {})", age(n.saturating_sub(s) / 1000)),
        _ => String::new(),
    }
}

/// Seconds as `40s`, `12m`, `5h`, `3d`.
fn age(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86_400),
    }
}

fn size(bytes: Option<u64>) -> String {
    let Some(b) = bytes else { return "-".into() };
    let gib = b as f64 / (1u64 << 30) as f64;
    if gib >= 1.0 {
        format!("{gib:.1}G")
    } else {
        format!("{:.0}M", b as f64 / (1u64 << 20) as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_marks_values_past_a_limit_and_idle_tasks() {
        let host = json!({
            "sampled_ms": 7_200_000, "cpu": {"percent": 12.4, "cores": 10}, "load": [2.1, 2.4, 2.6],
            "memory": {"total": 25_769_803_776u64, "used": 24_696_061_952u64,
                       "compressed": 8_160_437_862u64, "pressure": "warn"},
            "swap": {"used": 0, "total": 0},
            "disks": [{"name": "system", "path": "/", "free": 10_737_418_240u64, "total": 494_384_795_648u64}],
            "groups": [{"kind": "task", "name": "Fix it", "terminal": "t1", "cpu": 0.5,
                        "memory": 419_430_400, "processes": 3, "idle_since_ms": 0}],
            "alerts": ["memory", "disk:/"],
        });
        let out = text(&host);
        assert!(out.starts_with("CPU 12% of 10 cores, load 2.1  Memory 23.0G of 24.0G ! (warn"));
        assert!(out.contains("Disk / 10.0G free of 460.4G !"));
        assert!(
            out.contains("   400M   0.5%     3  task           Fix it  (idle 2h)"),
            "{out}"
        );
    }
}
