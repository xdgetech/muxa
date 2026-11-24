//! `<state dir>/subscriptions/<name>.json`: the plan limits a subscription's
//! agent last reported (ADR 0091 §6). Percentages and times only. The input
//! comes from an agent's status line or log, so it is checked as untrusted.

use std::fs::DirBuilder;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::records::lock::Lock;
use crate::records::subscriptions;
use crate::routines::store::{StateError, read_json, write_json};

pub const DIR: &str = "subscriptions";
const VERSION: u32 = 1;
const DIR_MODE: u32 = 0o700;
const MAX_FILE: u64 = 16 * 1024;
/// The longest plan window, a week, with a day to spare.
const MAX_AHEAD: u64 = 8 * 24 * 3600;
const MAX_WINDOWS: usize = 8;
/// Claude Code's two windows, in minutes.
const FIVE_HOURS: u32 = 300;
const SEVEN_DAYS: u32 = 10_080;

/// One plan window as the agent reported it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub minutes: u32,
    /// 0 to 100.
    pub used_percent: f64,
    /// Unix seconds.
    pub resets_at: u64,
}

/// What a subscription's agents last reported.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Record {
    #[serde(default)]
    version: u32,
    /// Unix seconds of the newest report.
    #[serde(default)]
    pub as_of: u64,
    #[serde(default)]
    pub windows: Vec<Window>,
}

/// Claude Code's status line input: `rate_limits.five_hour` and
/// `seven_day`, each `{used_percentage, resets_at}` (docs/findings/agent-usage.md).
pub fn from_claude(input: &Value) -> Vec<Window> {
    let limits = &input["rate_limits"];
    [("five_hour", FIVE_HOURS), ("seven_day", SEVEN_DAYS)]
        .into_iter()
        .filter_map(|(key, minutes)| {
            let w = &limits[key];
            Some(Window {
                minutes,
                used_percent: w["used_percentage"].as_f64()?,
                resets_at: w["resets_at"].as_u64()?,
            })
        })
        .collect()
}

/// A Codex `token_count` event's `rate_limits`: `primary` and `secondary`,
/// each `{used_percent, window_minutes, resets_at}`.
pub fn from_codex(rate_limits: &Value) -> Vec<Window> {
    ["primary", "secondary"]
        .into_iter()
        .filter_map(|key| {
            let w = &rate_limits[key];
            Some(Window {
                minutes: u32::try_from(w["window_minutes"].as_u64()?).ok()?,
                used_percent: w["used_percent"].as_f64()?,
                resets_at: w["resets_at"].as_u64()?,
            })
        })
        .collect()
}

/// Windows a report may hold: a sane size, a percentage, and a reset that
/// is not long past or far ahead.
fn valid(w: &Window, now: u64) -> bool {
    (1..=SEVEN_DAYS * 2).contains(&w.minutes)
        && w.used_percent.is_finite()
        && (0.0..=100.0).contains(&w.used_percent)
        && w.resets_at + MAX_AHEAD >= now
        && w.resets_at <= now + MAX_AHEAD
}

/// Pure. `old` with `fresh` laid over it, window by window: a report with an
/// older reset than the stored one is dropped, since it came from a stale
/// agent (ADR 0091 §6).
pub fn merge(old: &Record, fresh: &[Window], now: u64) -> Record {
    let mut windows: Vec<Window> = old.windows.clone();
    for w in fresh.iter().filter(|w| valid(w, now)) {
        match windows.iter_mut().find(|o| o.minutes == w.minutes) {
            Some(o) if o.resets_at > w.resets_at => {}
            Some(o) => *o = w.clone(),
            None => windows.push(w.clone()),
        }
    }
    windows.sort_by_key(|w| w.minutes);
    windows.truncate(MAX_WINDOWS);
    Record {
        version: VERSION,
        as_of: now,
        windows,
    }
}

/// Pure. What a client sees of `sub` and its record.
pub fn usage(
    sub: &subscriptions::Subscription,
    found: &Record,
) -> muxa_protocol::v2::SubscriptionUsage {
    muxa_protocol::v2::SubscriptionUsage {
        name: sub.name.clone(),
        agent: sub.agent.id().to_owned(),
        plan: sub.plan.clone(),
        windows: found
            .windows
            .iter()
            .map(|w| muxa_protocol::v2::UsageWindow {
                minutes: w.minutes,
                // Checked to be 0 to 100 when recorded.
                used_percent: w.used_percent.round() as u32,
                resets_at: w.resets_at,
            })
            .collect(),
        as_of: (found.as_of > 0).then_some(found.as_of),
    }
}

fn folder(state_dir: &Path) -> PathBuf {
    state_dir.join(DIR)
}

/// Blocking. The record of `name`; empty when there is none or the name is
/// not a subscription name.
pub fn read(state_dir: &Path, name: &str) -> Result<Record, StateError> {
    if subscriptions::name(name).is_err() {
        return Ok(Record::default());
    }
    Ok(read_json(
        &folder(state_dir).join(format!("{name}.json")),
        name,
        MAX_FILE,
    )?
    .unwrap_or_default())
}

/// Blocking, under the record's lock. Keeps `fresh` for `name` at `now`. A
/// report with no valid window changes nothing.
pub fn record(state_dir: &Path, name: &str, fresh: &[Window], now: u64) -> Result<(), StateError> {
    subscriptions::name(name).map_err(|_| {
        StateError::Unreadable("subscription".into(), "not a subscription name".into())
    })?;
    if !fresh.iter().any(|w| valid(w, now)) {
        return Ok(());
    }
    let dir = folder(state_dir);
    DirBuilder::new()
        .recursive(true)
        .mode(DIR_MODE)
        .create(&dir)
        .map_err(|source| StateError::Io {
            action: "create",
            path: dir.clone(),
            source,
        })?;
    let _lock = Lock::acquire(&dir, name, true)?;
    let old = read(state_dir, name)?;
    write_json(&dir, name, &merge(&old, fresh, now), MAX_FILE)
}

/// A log older than this has nothing new: its limits were recorded before.
const FRESH_LOG: u64 = 15 * 60;
/// The end of a log that holds its last `token_count` event.
const LOG_TAIL: u64 = 512 * 1024;

/// Blocking. The newest Codex log under `<home>/sessions/YYYY/MM/DD/`, when
/// it changed in the last [`FRESH_LOG`], and its time.
fn newest_codex_log(home: &Path, now: u64) -> Option<(PathBuf, u64)> {
    let mut dir = home.join("sessions");
    for _ in 0..3 {
        let newest = std::fs::read_dir(&dir)
            .ok()?
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name())
            .max()?;
        dir = dir.join(newest);
    }
    let (path, at) = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("rollout-") && name.ends_with(".jsonl")
        })
        .filter_map(|e| {
            let at = e.metadata().ok()?.modified().ok()?;
            let at = at.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
            Some((e.path(), at))
        })
        .max_by_key(|(_, at)| *at)?;
    (at + FRESH_LOG >= now).then_some((path, at))
}

/// Blocking. The `rate_limits` of the last `token_count` event in the end of
/// `log`. Nothing else of the log is kept.
fn last_codex_limits(log: &Path) -> Option<Vec<Window>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(log).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(LOG_TAIL)))
        .ok()?;
    let mut bytes = Vec::new();
    file.take(LOG_TAIL).read_to_end(&mut bytes).ok()?;
    String::from_utf8_lossy(&bytes)
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|e| e["payload"]["type"] == "token_count" && e["payload"]["rate_limits"].is_object())
        .map(|e| from_codex(&e["payload"]["rate_limits"]))
}

/// Blocking. Records the limits in each Codex subscription's newest log, as
/// of the log's time (ADR 0091 §6). A failure is logged by name only.
pub fn scan_codex(state_dir: &Path, subs: &[subscriptions::Subscription], now: u64) {
    for s in subs
        .iter()
        .filter(|s| s.agent == crate::records::workers::WorkerAgent::Codex)
    {
        let Some((log, at)) = newest_codex_log(&s.home, now) else {
            continue;
        };
        let Some(windows) = last_codex_limits(&log) else {
            continue;
        };
        if let Err(e) = record(state_dir, &s.name, &windows, at) {
            tracing::info!(subscription = %s.name, error = %e, "codex limits not recorded");
        }
    }
}

/// How often the daemon reads the Codex logs again.
const CODEX_SCAN: std::time::Duration = std::time::Duration::from_secs(60);

/// Reads the Codex subscriptions' logs every [`CODEX_SCAN`]. Claude Code's
/// limits come by its status line instead.
pub fn spawn_codex_scan(config: std::sync::Arc<crate::config::Config>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(CODEX_SCAN);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let config = std::sync::Arc::clone(&config);
            let scanned = tokio::task::spawn_blocking(move || {
                let (Some(state), Some(file)) = (&config.state_dir, &config.workspaces) else {
                    return;
                };
                match crate::records::Workspaces::load(file, config.home.as_deref()) {
                    Ok(all) => scan_codex(state, all.subscriptions(), crate::records::now()),
                    Err(e) => tracing::info!(error = %e, "codex limits not read: no workspaces"),
                }
            })
            .await;
            if let Err(e) = scanned {
                tracing::warn!(error = %e, "codex limits scan stopped");
            }
        }
    });
}

#[cfg(test)]
#[path = "limits_tests.rs"]
mod tests;
