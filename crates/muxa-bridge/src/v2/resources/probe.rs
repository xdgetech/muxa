//! Reads the host's processes, memory, swap, load and disks (ADR 0105).
//! Commands and `/proc` only: no extra crate, no `unsafe`. The parsers are
//! pure so tests run them on captured text.

use std::collections::HashMap;
use std::path::Path;

use muxa_protocol::v2::{HostDisk, HostMemory, HostSwap, Pressure};
use tokio::process::Command;

/// One process as `ps` shows it. `path` is a full path on macOS and a name
/// on Linux; it never leaves the host.
#[derive(Debug, Clone, PartialEq)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    /// Percent of one core, as `ps` reports it.
    pub cpu: f32,
    /// Bytes: the footprint on macOS, resident memory elsewhere.
    pub memory: u64,
    pub path: String,
}

impl Proc {
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

/// The host-wide figures of one sample.
#[derive(Debug, Clone, PartialEq)]
pub struct System {
    pub memory: HostMemory,
    pub swap: HostSwap,
    pub load: [f32; 3],
}

/// Lines of `pid ppid %cpu rss(KiB) command`; the command may hold spaces.
pub fn parse_ps(text: &str) -> Vec<Proc> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let pid = f.next()?.parse().ok()?;
            let ppid = f.next()?.parse().ok()?;
            let cpu = f.next()?.parse().ok()?;
            let rss_kib: u64 = f.next()?.parse().ok()?;
            let path = f.collect::<Vec<_>>().join(" ");
            (!path.is_empty()).then_some(Proc {
                pid,
                ppid,
                cpu,
                memory: rss_kib * 1024,
                path,
            })
        })
        .collect()
}

/// `sysctl -n hw.memsize vm.swapusage vm.loadavg kern.memorystatus_vm_pressure_level`.
pub fn parse_sysctl(text: &str) -> Option<(u64, HostSwap, [f32; 3], Pressure)> {
    let mut lines = text.lines();
    let total = lines.next()?.trim().parse().ok()?;
    let swap = parse_swapusage(lines.next()?)?;
    let load = parse_load(
        lines
            .next()?
            .trim_matches(|c| c == '{' || c == '}' || c == ' '),
    )?;
    let pressure = match lines.next()?.trim() {
        "1" => Pressure::Normal,
        "2" => Pressure::Warn,
        "4" => Pressure::Critical,
        _ => Pressure::Unknown,
    };
    Some((total, swap, load, pressure))
}

/// `total = 13312.00M  used = 11884.31M  free = 1427.69M  (encrypted)`.
fn parse_swapusage(line: &str) -> Option<HostSwap> {
    let field = |key: &str| {
        let rest = line.split(key).nth(1)?.trim_start_matches([' ', '=']);
        megabytes(rest.split_whitespace().next()?)
    };
    Some(HostSwap {
        total: field("total")?,
        used: field("used")?,
    })
}

fn megabytes(s: &str) -> Option<u64> {
    let (n, unit) = s.split_at(s.len().checked_sub(1)?);
    let n: f64 = n.parse().ok()?;
    let scale = match unit {
        "K" => 1024.0,
        "M" => 1024.0 * 1024.0,
        "G" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((n * scale) as u64)
}

fn parse_load(text: &str) -> Option<[f32; 3]> {
    let mut f = text.split_whitespace().map(str::parse::<f32>);
    Some([f.next()?.ok()?, f.next()?.ok()?, f.next()?.ok()?])
}

/// macOS `vm_stat`: memory used as Activity Monitor counts it (app memory,
/// wired and compressed), and the compressed part.
pub fn parse_vm_stat(text: &str) -> Option<(u64, u64)> {
    let page: u64 = text
        .split("page size of ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let pages = |key: &str| -> Option<u64> {
        let line = text.lines().find(|l| l.starts_with(key))?;
        line.rsplit(':')
            .next()?
            .trim()
            .trim_end_matches('.')
            .parse()
            .ok()
    };
    let anonymous = pages("Anonymous pages").or_else(|| pages("Pages active"))?;
    let purgeable = pages("Pages purgeable").unwrap_or(0);
    let wired = pages("Pages wired down")?;
    let compressed = pages("Pages occupied by compressor")?;
    let used = (anonymous.saturating_sub(purgeable) + wired + compressed) * page;
    Some((used, compressed * page))
}

/// Linux `/proc/meminfo`, in kB.
pub fn parse_meminfo(text: &str) -> Option<(u64, u64, HostSwap)> {
    let kb = |key: &str| -> Option<u64> {
        let line = text.lines().find(|l| l.starts_with(key))?;
        Some(line.split_whitespace().nth(1)?.parse::<u64>().ok()? * 1024)
    };
    let total = kb("MemTotal:")?;
    let used = total.saturating_sub(kb("MemAvailable:")?);
    let swap_total = kb("SwapTotal:")?;
    let swap = HostSwap {
        total: swap_total,
        used: swap_total.saturating_sub(kb("SwapFree:")?),
    };
    Some((total, used, swap))
}

/// Linux `/proc/loadavg`.
pub fn parse_loadavg(text: &str) -> Option<[f32; 3]> {
    parse_load(text)
}

/// Linux `/proc/pressure/memory`: the share of the last 10 s that tasks
/// stalled on memory. `full` stalls every task, so it counts as critical.
pub fn parse_psi(text: &str) -> Pressure {
    let avg10 = |kind: &str| -> f32 {
        text.lines()
            .find(|l| l.starts_with(kind))
            .and_then(|l| l.split_whitespace().find_map(|f| f.strip_prefix("avg10=")))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0)
    };
    if avg10("full") >= PSI_STALL {
        Pressure::Critical
    } else if avg10("some") >= PSI_STALL {
        Pressure::Warn
    } else {
        Pressure::Normal
    }
}

/// Percent of 10 s stalled on memory that counts as pressure.
const PSI_STALL: f32 = 10.0;

pub async fn processes() -> Result<Vec<Proc>, String> {
    let text = run("ps", &["-A", "-o", "pid=,ppid=,%cpu=,rss=,comm="]).await?;
    let procs = parse_ps(&text);
    if !cfg!(target_os = "macos") {
        return Ok(procs);
    }
    // macOS resident memory counts shared pages in every process: 240
    // simulator processes showed 24G of RSS on a 24G host, and 3G of
    // footprint. `ps` has no footprint column; `top` has.
    let footprints = parse_top_mem(&run("top", &["-l", "1", "-stats", "pid,mem"]).await?);
    Ok(with_footprints(procs, &footprints))
}

/// Each process's footprint where `top` listed it, else its resident memory.
pub fn with_footprints(procs: Vec<Proc>, footprints: &HashMap<u32, u64>) -> Vec<Proc> {
    procs
        .into_iter()
        .map(|p| Proc {
            memory: footprints.get(&p.pid).copied().unwrap_or(p.memory),
            ..p
        })
        .collect()
}

/// `top -l 1 -stats pid,mem`: after the header, `PID MEM` lines such as
/// `99671  6178K` or `38460  2691M+`.
pub fn parse_top_mem(text: &str) -> HashMap<u32, u64> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let pid = f.next()?.parse().ok()?;
            let mem = top_size(f.next()?)?;
            f.next().is_none().then_some((pid, mem))
        })
        .collect()
}

fn top_size(s: &str) -> Option<u64> {
    let s = s.trim_end_matches(['+', '-']);
    let (n, scale) = match s.chars().last()? {
        'B' => (&s[..s.len() - 1], 1),
        'K' => (&s[..s.len() - 1], 1 << 10),
        'M' => (&s[..s.len() - 1], 1 << 20),
        'G' => (&s[..s.len() - 1], 1 << 30),
        _ => (s, 1),
    };
    let n: f64 = n.parse().ok()?;
    Some((n * scale as f64) as u64)
}

pub async fn system() -> Result<System, String> {
    if cfg!(target_os = "macos") {
        let sysctl = run(
            "sysctl",
            &[
                "-n",
                "hw.memsize",
                "vm.swapusage",
                "vm.loadavg",
                "kern.memorystatus_vm_pressure_level",
            ],
        )
        .await?;
        let (total, swap, load, pressure) =
            parse_sysctl(&sysctl).ok_or("sysctl gave an unexpected answer")?;
        let vm = run("vm_stat", &[]).await?;
        let (used, compressed) = parse_vm_stat(&vm).ok_or("vm_stat gave an unexpected answer")?;
        Ok(System {
            memory: HostMemory {
                total,
                used,
                compressed: Some(compressed),
                pressure,
            },
            swap,
            load,
        })
    } else {
        let meminfo = read("/proc/meminfo").await?;
        let (total, used, swap) = parse_meminfo(&meminfo).ok_or("unexpected /proc/meminfo")?;
        let load =
            parse_loadavg(&read("/proc/loadavg").await?).ok_or("unexpected /proc/loadavg")?;
        // Kernels before 4.20 have no PSI: no pressure is known.
        let pressure = match read("/proc/pressure/memory").await {
            Ok(text) => parse_psi(&text),
            Err(_) => Pressure::Unknown,
        };
        Ok(System {
            memory: HostMemory {
                total,
                used,
                compressed: None,
                pressure,
            },
            swap,
            load,
        })
    }
}

/// Free and total space of the file system that holds `path`.
pub fn disk(name: &str, path: &Path) -> Option<HostDisk> {
    let s = rustix::fs::statvfs(path).ok()?;
    Some(HostDisk {
        name: name.to_owned(),
        path: path.to_string_lossy().into_owned(),
        free: s.f_bavail.saturating_mul(s.f_frsize),
        total: s.f_blocks.saturating_mul(s.f_frsize),
    })
}

/// The device of `path`, to tell whether two paths share a disk.
pub fn device(path: &Path) -> Option<u64> {
    rustix::fs::stat(path).ok().map(|s| s.st_dev as u64)
}

async fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{program} exited with {}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `/proc` files are made in memory on read, so this does not block.
async fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))
}
