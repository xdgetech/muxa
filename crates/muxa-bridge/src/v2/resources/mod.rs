//! `host`: a sample of the host's CPU, memory, disks and what uses them
//! (ADR 0103). One sample serves every session for a few seconds.

mod groups;
mod limits;
mod probe;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use muxa_protocol::v2::{HostCpu, HostDisk, HostReport};
use muxa_server::TaskError;
use muxa_server::owners::UsageOwner;
use tokio::sync::Mutex;

use probe::{Proc, System};

/// A sample younger than this is shared instead of taken again.
const FRESH: Duration = Duration::from_secs(5);
const SYSTEM_DISK: &str = "/";

#[derive(Default)]
pub struct Resources {
    last: Mutex<Option<Sample>>,
}

#[derive(Clone)]
struct Sample {
    at: Instant,
    sampled_ms: u64,
    procs: Vec<Proc>,
    system: System,
    cpu: f32,
    /// The sample before this one was busy too.
    busy_before: bool,
}

impl Resources {
    pub async fn report(
        &self,
        config: &Arc<crate::config::Config>,
        owners: &[UsageOwner],
    ) -> Result<HostReport, TaskError> {
        let cores = cores();
        let sample = self.sample(cores).await?;
        let disks = disks(build_dir(config).await).await;
        Ok(HostReport {
            sampled_ms: sample.sampled_ms,
            cpu: HostCpu {
                percent: sample.cpu,
                cores,
            },
            load: sample.system.load,
            alerts: limits::alerts(
                sample.cpu,
                sample.busy_before,
                &sample.system.memory,
                &disks,
            ),
            memory: sample.system.memory,
            swap: sample.system.swap,
            disks,
            groups: groups::groups(&sample.procs, owners, cores),
        })
    }

    async fn sample(&self, cores: u32) -> Result<Sample, TaskError> {
        let mut last = self.last.lock().await;
        if let Some(s) = last.as_ref().filter(|s| s.at.elapsed() < FRESH) {
            return Ok(s.clone());
        }
        let failed =
            |e: String| TaskError::Failed(format!("cannot read the host's resources: {e}"));
        let procs = probe::processes().await.map_err(failed)?;
        let system = probe::system().await.map_err(failed)?;
        let cpu = groups::share(procs.iter().map(|p| p.cpu).sum(), cores);
        let busy_before = last.as_ref().is_some_and(|s| s.cpu >= limits::CPU_BUSY);
        let sample = Sample {
            at: Instant::now(),
            sampled_ms: now_ms(),
            procs,
            system,
            cpu,
            busy_before,
        };
        *last = Some(sample.clone());
        Ok(sample)
    }
}

fn cores() -> u32 {
    std::thread::available_parallelism().map_or(1, |n| n.get() as u32)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// `[build] dir`, when one is set. A broken `workspaces.toml` shows no build
/// disk rather than failing the whole report.
async fn build_dir(config: &Arc<crate::config::Config>) -> Option<PathBuf> {
    let c = Arc::clone(config);
    let dir = tokio::task::spawn_blocking(move || {
        c.load_workspaces()
            .map(|w| w.host_build().dir.clone())
            .map_err(|e| e.to_string())
    })
    .await;
    match dir {
        Ok(Ok(dir)) => dir,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "host report: cannot read the build disk setting");
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "host report: the settings read stopped");
            None
        }
    }
}

/// The system disk, and the build disk when it is a folder on another disk.
async fn disks(build: Option<PathBuf>) -> Vec<HostDisk> {
    let read = tokio::task::spawn_blocking(move || {
        let root = Path::new(SYSTEM_DISK);
        let mut out: Vec<HostDisk> = probe::disk("system", root).into_iter().collect();
        if let Some(dir) = build.filter(|d| d.is_dir()) {
            if probe::device(&dir) != probe::device(root) {
                out.extend(probe::disk("build", &dir));
            }
        }
        out
    })
    .await;
    read.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "host report: the disk read stopped");
        Vec::new()
    })
}
