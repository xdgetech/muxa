//! When a host value is worth a warning (ADR 0103 §6). The host decides, so
//! every app colors the same values.

use muxa_protocol::v2::{HostDisk, HostMemory, Pressure};

/// CPU over this percent of all cores, two samples in a row, is busy.
pub const CPU_BUSY: f32 = 85.0;
/// A disk with less free space than this share, or than `DISK_MIN_FREE`, is
/// nearly full.
const DISK_MIN_SHARE: f64 = 0.10;
const DISK_MIN_FREE: u64 = 20 * 1024 * 1024 * 1024;

/// `cpu`, `memory` and `disk:<path>` for the values past a limit.
/// `busy_before`: the previous sample's CPU was at or over `CPU_BUSY`.
pub fn alerts(cpu: f32, busy_before: bool, memory: &HostMemory, disks: &[HostDisk]) -> Vec<String> {
    let mut out = Vec::new();
    if cpu >= CPU_BUSY && busy_before {
        out.push("cpu".to_owned());
    }
    if matches!(memory.pressure, Pressure::Warn | Pressure::Critical) {
        out.push("memory".to_owned());
    }
    for d in disks {
        let low_share = d.total > 0 && (d.free as f64) < d.total as f64 * DISK_MIN_SHARE;
        if low_share || d.free < DISK_MIN_FREE {
            out.push(format!("disk:{}", d.path));
        }
    }
    out
}
