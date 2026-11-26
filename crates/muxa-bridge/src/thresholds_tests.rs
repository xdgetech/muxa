use std::collections::HashMap;

use super::*;

const NOW: u64 = 1_800_000_000;

fn record(windows: &[(u32, f64, u64)]) -> Record {
    let fresh: Vec<Window> = windows
        .iter()
        .map(|&(minutes, used_percent, resets_at)| Window {
            minutes,
            used_percent,
            resets_at,
        })
        .collect();
    limits::merge(&Record::default(), &fresh, NOW)
}

#[test]
fn a_login_is_full_from_95_percent_until_its_latest_full_window_resets() {
    let limits = Snapshot::new(
        HashMap::from([
            (
                "work".to_owned(),
                record(&[(300, 96.0, NOW + 600), (10_080, 99.5, NOW + 9_000)]),
            ),
            ("spare".to_owned(), record(&[(300, 94.9, NOW + 600)])),
            ("old".to_owned(), record(&[(300, 100.0, NOW - 1)])),
        ]),
        NOW,
    );
    assert_eq!(limits.full_until("work"), Some(NOW + 9_000));
    assert_eq!(limits.full_until("spare"), None);
    // A window past its reset says nothing about now.
    assert_eq!(limits.full_until("old"), None);
    assert_eq!(limits.full_until("unknown"), None);
}

#[test]
fn each_window_past_80_percent_warns_once_per_reset() {
    let r = record(&[(300, 82.4, NOW + 600), (10_080, 40.0, NOW + 9_000)]);
    let items = warnings("work", &r, NOW);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].kind, ItemKind::Read);
    assert_eq!(items[0].title, "work at 82% of its 5-hour limit");
    assert!(items[0].once);
    let again = warnings("work", &record(&[(300, 90.0, NOW + 620)]), NOW);
    assert_eq!(
        again[0].key, items[0].key,
        "a few seconds of drift is one window"
    );
    let next = warnings("work", &record(&[(300, 85.0, NOW + 600 + 5 * 3600)]), NOW);
    assert_ne!(next[0].key, items[0].key, "the next window warns again");
    assert!(
        warnings("work", &r, NOW + 600).is_empty(),
        "reset: nothing to warn"
    );
    let weekly = warnings("work", &record(&[(10_080, 96.0, NOW + 9_000)]), NOW);
    assert_eq!(weekly[0].title, "work at 96% of its weekly limit");
    assert!(
        weekly[0]
            .note
            .as_deref()
            .unwrap()
            .contains("wait for the reset")
    );
}
