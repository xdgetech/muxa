use muxa_protocol::v2::{HostDisk, HostMemory, Pressure, UsageKind};
use muxa_server::owners::UsageOwner;

use super::groups::{groups, share};
use super::limits::alerts;
use super::probe::{
    Proc, parse_loadavg, parse_meminfo, parse_ps, parse_psi, parse_sysctl, parse_vm_stat,
};

const GIB: u64 = 1024 * 1024 * 1024;

// Captured on a Mac mini (24G), 2026-11-26.
const SYSCTL: &str = "25769803776
total = 13312.00M  used = 11884.31M  free = 1427.69M  (encrypted)
{ 3.18 3.19 3.38 }
2
";

const VM_STAT: &str = r#"Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                                    12824.
Pages active:                                 391938.
Pages inactive:                               388376.
Pages speculative:                              3619.
Pages throttled:                                   0.
Pages wired down:                             222334.
Pages purgeable:                                 155.
"Translation faults":                     8000881658.
File-backed pages:                            220189.
Anonymous pages:                              563744.
Pages stored in compressor:                  2283163.
Pages occupied by compressor:                 518455.
"#;

#[test]
fn macos_sysctl_and_vm_stat() {
    let (total, swap, load, pressure) = parse_sysctl(SYSCTL).expect("sysctl");
    assert_eq!(total, 24 * GIB);
    assert_eq!(swap.total, 13312 * 1024 * 1024);
    assert_eq!(swap.used / (1024 * 1024), 11884);
    assert_eq!(load, [3.18, 3.19, 3.38]);
    assert_eq!(pressure, Pressure::Warn);

    let (used, compressed) = parse_vm_stat(VM_STAT).expect("vm_stat");
    let page = 16384;
    assert_eq!(used, (563_744 - 155 + 222_334 + 518_455) * page);
    assert_eq!(compressed, 518_455 * page);
}

#[test]
fn linux_proc_files() {
    let meminfo = "MemTotal:       16000000 kB
MemFree:         1000000 kB
MemAvailable:    6000000 kB
SwapTotal:       2000000 kB
SwapFree:        1500000 kB
";
    let (total, used, swap) = parse_meminfo(meminfo).expect("meminfo");
    assert_eq!(total, 16_000_000 * 1024);
    assert_eq!(used, 10_000_000 * 1024);
    assert_eq!(swap.used, 500_000 * 1024);
    assert_eq!(
        parse_loadavg("0.50 0.40 0.30 1/200 999"),
        Some([0.5, 0.4, 0.3])
    );

    let calm = "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0";
    let some = "some avg10=12.50 avg60=3.00 avg300=1.00 total=9\nfull avg10=2.00 avg60=0.00 avg300=0.00 total=1";
    let full = "some avg10=60.00 avg60=3.00 avg300=1.00 total=9\nfull avg10=30.00 avg60=0.00 avg300=0.00 total=1";
    assert_eq!(parse_psi(calm), Pressure::Normal);
    assert_eq!(parse_psi(some), Pressure::Warn);
    assert_eq!(parse_psi(full), Pressure::Critical);
}

#[test]
fn ps_lines_keep_paths_with_spaces() {
    let procs = parse_ps(
        "    1     0   0.1  15504 /sbin/launchd
  496     1   2.5  14752 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome
 bad line
",
    );
    assert_eq!(procs.len(), 2);
    assert_eq!(procs[1].pid, 496);
    assert_eq!(procs[1].cpu, 2.5);
    assert_eq!(procs[1].rss, 14752 * 1024);
    assert_eq!(procs[1].name(), "Google Chrome");
}

fn p(pid: u32, ppid: u32, cpu: f32, mib: u64, path: &str) -> Proc {
    Proc {
        pid,
        ppid,
        cpu,
        rss: mib * 1024 * 1024,
        path: path.to_owned(),
    }
}

fn task(terminal: &str, pids: Vec<u32>) -> UsageOwner {
    UsageOwner {
        kind: UsageKind::Task,
        name: format!("Task {terminal}"),
        terminal: terminal.to_owned(),
        workspace: Some("xdge".into()),
        idle_since_ms: Some(7),
        pids,
    }
}

#[test]
fn processes_group_by_task_tree_then_by_kind() {
    let procs = vec![
        p(1, 0, 0.0, 10, "/sbin/launchd"),
        // Task a: agent 100 (claude under a shell) and shell 200.
        p(100, 50, 0.0, 5, "/bin/zsh"),
        p(101, 100, 10.0, 400, "claude"),
        p(102, 101, 0.0, 50, "node"),
        p(200, 50, 0.0, 5, "/bin/zsh"),
        // Task b.
        p(300, 50, 0.0, 600, "claude"),
        // Claude Code outside muxa, with an MCP server under it.
        p(400, 1, 2.0, 300, "claude"),
        p(401, 400, 0.0, 100, "node"),
        // A simulator and an app it runs.
        p(
            500,
            1,
            0.0,
            20,
            "/Library/Developer/CoreSimulator/launchd_sim",
        ),
        p(
            501,
            500,
            1.0,
            200,
            "/private/var/x/SpringBoard.app/SpringBoard",
        ),
        p(
            600,
            1,
            5.0,
            230,
            "/Applications/Docker.app/Contents/MacOS/com.docker.backend",
        ),
        p(
            700,
            1,
            40.0,
            1700,
            "/System/Library/Frameworks/Virtualization.framework/x/com.apple.Virtualization.VirtualMachine",
        ),
        p(
            800,
            1,
            1.0,
            900,
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        ),
        p(
            801,
            800,
            1.0,
            300,
            "/Applications/Google Chrome.app/Contents/Frameworks/x/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper",
        ),
        p(
            900,
            1,
            0.0,
            40,
            "/System/Library/CoreServices/Dock.app/Contents/MacOS/Dock",
        ),
    ];
    let owners = vec![task("t1", vec![100, 200]), task("t2", vec![300])];
    let got = groups(&procs, &owners, 10);

    let find = |kind: UsageKind| got.iter().find(|g| g.kind == kind).expect("group");
    assert_eq!(
        got[0].terminal.as_deref(),
        Some("t2"),
        "the bigger Task first"
    );
    let a = got
        .iter()
        .find(|g| g.terminal.as_deref() == Some("t1"))
        .expect("t1");
    assert_eq!(a.processes, 4);
    assert_eq!(a.rss, 460 * 1024 * 1024);
    assert_eq!(a.cpu, 1.0);
    assert_eq!(a.idle_since_ms, Some(7));

    let outside = find(UsageKind::ClaudeOutside);
    assert_eq!(outside.processes, 2, "its MCP server counts with it");
    assert_eq!(find(UsageKind::Simulator).processes, 2);
    assert_eq!(find(UsageKind::Docker).rss, 230 * 1024 * 1024);
    assert_eq!(find(UsageKind::Vm).cpu, 4.0);
    let chrome = find(UsageKind::App);
    assert_eq!(
        (chrome.name.as_str(), chrome.processes),
        ("Google Chrome", 2)
    );
    let other = find(UsageKind::Other);
    assert_eq!(other.processes, 2, "launchd and a system app");
    assert!(
        got.iter().all(|g| !g.name.contains('/')),
        "only names cross the wire"
    );
}

#[test]
fn apps_past_the_limit_count_as_other() {
    let procs: Vec<Proc> = (0..10)
        .map(|i| {
            p(
                10 + i,
                1,
                0.0,
                100 + u64::from(i),
                &format!("/Applications/App{i}.app/Contents/MacOS/App{i}"),
            )
        })
        .collect();
    let got = groups(&procs, &[], 4);
    assert_eq!(got.iter().filter(|g| g.kind == UsageKind::App).count(), 8);
    let other = got.last().expect("other");
    assert_eq!((other.kind, other.processes), (UsageKind::Other, 2));
    assert_eq!(other.rss, (100 + 101) * 1024 * 1024, "the two smallest");
}

#[test]
fn cpu_is_a_share_of_all_cores() {
    assert_eq!(share(250.0, 10), 25.0);
    assert_eq!(share(3000.0, 10), 100.0);
    assert_eq!(share(1.0, 0), 1.0);
}

fn memory(pressure: Pressure) -> HostMemory {
    HostMemory {
        total: 24 * GIB,
        used: 20 * GIB,
        compressed: None,
        pressure,
    }
}

fn disk(path: &str, free: u64, total: u64) -> HostDisk {
    HostDisk {
        name: "system".into(),
        path: path.into(),
        free,
        total,
    }
}

#[test]
fn limits() {
    let roomy = [disk("/", 100 * GIB, 500 * GIB)];
    assert!(alerts(50.0, true, &memory(Pressure::Normal), &roomy).is_empty());
    assert!(
        alerts(90.0, false, &memory(Pressure::Normal), &roomy).is_empty(),
        "one busy sample"
    );
    assert_eq!(
        alerts(90.0, true, &memory(Pressure::Normal), &roomy),
        ["cpu"]
    );
    assert_eq!(
        alerts(0.0, false, &memory(Pressure::Critical), &roomy),
        ["memory"]
    );
    let full = [
        disk("/", 19 * GIB, 100 * GIB),
        disk("/Volumes/b", 150 * GIB, 2000 * GIB),
    ];
    assert_eq!(
        alerts(0.0, false, &memory(Pressure::Normal), &full),
        ["disk:/", "disk:/Volumes/b"]
    );
}

#[tokio::test]
async fn this_host_reports_itself() {
    let config = std::sync::Arc::new(crate::config::Config {
        state_dir: None,
        workspaces: None,
        home: None,
    });
    let report = super::Resources::default()
        .report(&config, &[])
        .await
        .expect("report");
    assert!(report.cpu.cores > 0);
    assert!(report.memory.total > 0 && report.memory.used <= report.memory.total);
    assert_eq!(
        report.disks.first().map(|d| d.name.as_str()),
        Some("system")
    );
    assert!(report.groups.iter().map(|g| g.processes).sum::<u32>() > 1);
}
