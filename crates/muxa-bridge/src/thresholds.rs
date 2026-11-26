//! What a subscription's last report means for new work (ADR 0103 §9): a
//! warning at 80%, and from 95% new agents take the next login while
//! unattended work waits for the reset. A window past its reset means nothing.

use std::collections::HashMap;
use std::path::Path;

use muxa_inbox::NewItem;
use muxa_protocol::v2::ItemKind;

use crate::limits::{self, Record, Window};
use crate::records::subscriptions::Subscription;

pub const WARN: f64 = 80.0;
pub const SWITCH: f64 = 95.0;
/// Two reports of one window may differ by seconds in their reset: one
/// warning per window per 10 minutes of reset time.
const RESET_ROUNDING: u64 = 600;

/// The subscriptions' last reports, read once for one decision.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    records: HashMap<String, Record>,
    now: u64,
}

impl Snapshot {
    pub fn new(records: HashMap<String, Record>, now: u64) -> Self {
        Self { records, now }
    }

    /// Blocking. Each subscription's record. One that cannot be read counts
    /// as no data, which never holds work back.
    pub fn read(state_dir: Option<&Path>, subs: &[Subscription], now: u64) -> Self {
        let Some(dir) = state_dir else {
            return Self::new(HashMap::new(), now);
        };
        let records = subs
            .iter()
            .filter_map(|s| match limits::read(dir, &s.name) {
                Ok(r) => Some((s.name.clone(), r)),
                Err(e) => {
                    tracing::info!(subscription = %s.name, error = %e, "limits not read");
                    None
                }
            })
            .collect();
        Self::new(records, now)
    }

    /// Until when `name` takes no new agents: the latest reset among its
    /// windows at or above [`SWITCH`]. None below it or without current data.
    pub fn full_until(&self, name: &str) -> Option<u64> {
        self.records
            .get(name)?
            .windows
            .iter()
            .filter(|w| w.resets_at > self.now && w.used_percent >= SWITCH)
            .map(|w| w.resets_at)
            .max()
    }
}

/// Pure. One Read item per window of `name` at or above [`WARN`] that has
/// not reset. The key makes it once per window and reset.
pub fn warnings(name: &str, record: &Record, now: u64) -> Vec<NewItem> {
    record
        .windows
        .iter()
        .filter(|w| w.resets_at > now && w.used_percent >= WARN)
        .map(|w| warning(name, w))
        .collect()
}

fn warning(name: &str, w: &Window) -> NewItem {
    let percent = w.used_percent.round() as u32;
    let note = if w.used_percent >= SWITCH {
        "New agents take the next subscription in their list. Routines and Bot teams wait for the reset when none is left."
    } else {
        "From 95%, new agents take the next subscription in their list."
    };
    NewItem {
        kind: ItemKind::Read,
        title: format!(
            "{name} at {percent}% of its {} limit",
            window_name(w.minutes)
        ),
        note: Some(format!("It resets at {}. {note}", clock(w.resets_at))),
        workspace: None,
        due: None,
        source: "limits".into(),
        key: Some(format!(
            "limit:{name}:{}:{}",
            w.minutes,
            w.resets_at / RESET_ROUNDING
        )),
        once: true,
        terminal: None,
        routine: None,
        place: None,
        extra: Default::default(),
    }
}

/// `5-hour`, `weekly`, `45-minute`.
fn window_name(minutes: u32) -> String {
    match minutes {
        10_080 => "weekly".into(),
        m if m % 60 == 0 => format!("{}-hour", m / 60),
        m => format!("{m}-minute"),
    }
}

/// The host's local time, `14:00`, or `Thu 14:00` when not today.
fn clock(at: u64) -> String {
    let Ok(ts) = jiff::Timestamp::from_second(at as i64) else {
        return at.to_string();
    };
    let zone = jiff::tz::TimeZone::system();
    let when = ts.to_zoned(zone.clone());
    let today = jiff::Timestamp::now().to_zoned(zone).date();
    if when.date() == today {
        when.strftime("%H:%M").to_string()
    } else {
        when.strftime("%a %H:%M").to_string()
    }
}

/// How often the daemon looks for windows to warn about.
const EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// Every minute: reads the Codex logs (Claude Code reports by its status
/// line) and adds a warning for each window that crossed [`WARN`].
pub fn spawn(
    inbox: std::sync::Arc<muxa_inbox::Inbox>,
    config: std::sync::Arc<crate::config::Config>,
) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let config = std::sync::Arc::clone(&config);
            let found = tokio::task::spawn_blocking(move || due_warnings(&config)).await;
            match found {
                Ok(items) => add(&inbox, items),
                Err(e) => tracing::warn!(error = %e, "the limits watch stopped"),
            }
        }
    });
}

/// Blocking. Scans the Codex logs, then the warnings every record asks for.
fn due_warnings(config: &crate::config::Config) -> Vec<NewItem> {
    let (Some(state), Some(file)) = (&config.state_dir, &config.workspaces) else {
        return Vec::new();
    };
    let all = match crate::records::Workspaces::load(file, config.home.as_deref()) {
        Ok(all) => all,
        Err(e) => {
            tracing::info!(error = %e, "limits not watched: no workspaces");
            return Vec::new();
        }
    };
    let now = crate::records::now();
    limits::scan_codex(state, all.subscriptions(), now);
    all.subscriptions()
        .iter()
        .filter_map(|s| limits::read(state, &s.name).ok().map(|r| (s, r)))
        .flat_map(|(s, r)| warnings(&s.name, &r, now))
        .collect()
}

fn add(inbox: &muxa_inbox::Inbox, items: Vec<NewItem>) {
    if items.is_empty() {
        return;
    }
    let added = inbox.change(|b, at| {
        for item in items {
            b.add(item, at)?;
        }
        Ok(())
    });
    if let Err(e) = added {
        tracing::warn!(error = %e, "no limit warning");
    }
}

#[cfg(test)]
#[path = "thresholds_tests.rs"]
mod tests;
