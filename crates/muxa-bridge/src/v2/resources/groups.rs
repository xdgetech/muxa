//! Groups the host's processes by what the user can act on (ADR 0105 §4):
//! each muxa owner takes its terminals' process trees, the rest go by name.

use std::collections::HashMap;

use muxa_protocol::v2::{UsageGroup, UsageKind};
use muxa_server::owners::UsageOwner;

use super::probe::Proc;

/// Other apps beyond this many, by memory, count as `other`.
const MAX_APPS: usize = 8;
/// A parent chain longer than this is broken; stop walking it.
const MAX_DEPTH: usize = 64;
const CLAUDE: &str = "claude";
const VM: &str = "com.apple.Virtualization.VirtualMachine";

/// Percent of all `cores` together, to one decimal.
pub fn share(per_core: f32, cores: u32) -> f32 {
    let all = per_core / cores.max(1) as f32;
    (all.min(100.0) * 10.0).round() / 10.0
}

/// Owners first, biggest first; then the rest, biggest first.
pub fn groups(procs: &[Proc], owners: &[UsageOwner], cores: u32) -> Vec<UsageGroup> {
    let by_pid: HashMap<u32, &Proc> = procs.iter().map(|p| (p.pid, p)).collect();
    let owned = claim(procs, owners);
    let mut mine: Vec<Sum> = owners.iter().map(|_| Sum::default()).collect();
    let mut rest: HashMap<(UsageKind, String), Sum> = HashMap::new();
    let mut classes: HashMap<u32, (UsageKind, String)> = HashMap::new();
    for p in procs {
        match owned.get(&p.pid) {
            Some(&i) => mine[i].add(p),
            None => {
                let key = class_of(p, &by_pid, &owned, &mut classes);
                rest.entry(key).or_default().add(p);
            }
        }
    }
    let mut out: Vec<UsageGroup> = owners
        .iter()
        .zip(mine)
        .map(|(o, s)| UsageGroup {
            kind: o.kind,
            name: o.name.clone(),
            terminal: Some(o.terminal.clone()),
            workspace: o.workspace.clone(),
            cpu: share(s.cpu, cores),
            memory: s.memory,
            processes: s.count,
            idle_since_ms: o.idle_since_ms,
        })
        .collect();
    out.sort_by_key(|g| std::cmp::Reverse(g.memory));
    out.extend(others(rest, cores));
    out
}

/// Each owner's process trees, from its terminals' pids. A pid goes to the
/// first owner that reaches it.
fn claim(procs: &[Proc], owners: &[UsageOwner]) -> HashMap<u32, usize> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in procs {
        children.entry(p.ppid).or_default().push(p.pid);
    }
    let mut owned = HashMap::new();
    for (i, o) in owners.iter().enumerate() {
        let mut todo: Vec<u32> = o.pids.clone();
        while let Some(pid) = todo.pop() {
            if owned.contains_key(&pid) {
                continue;
            }
            owned.insert(pid, i);
            todo.extend(children.get(&pid).into_iter().flatten());
        }
    }
    owned
}

/// A process's own kind, or the kind of an ancestor that takes its children:
/// Claude Code's tools and MCP servers count with it, as a simulator's apps do.
fn class_of(
    p: &Proc,
    by_pid: &HashMap<u32, &Proc>,
    owned: &HashMap<u32, usize>,
    memo: &mut HashMap<u32, (UsageKind, String)>,
) -> (UsageKind, String) {
    if let Some(k) = memo.get(&p.pid) {
        return k.clone();
    }
    let own = own_class(p);
    let found = if matches!(own.0, UsageKind::App | UsageKind::Other) {
        ancestor_class(p, by_pid, owned).unwrap_or(own)
    } else {
        own
    };
    memo.insert(p.pid, found.clone());
    found
}

fn ancestor_class(
    p: &Proc,
    by_pid: &HashMap<u32, &Proc>,
    owned: &HashMap<u32, usize>,
) -> Option<(UsageKind, String)> {
    let mut at = p.ppid;
    for _ in 0..MAX_DEPTH {
        // launchd, init and muxa's own terminals end the walk.
        if at <= 1 || owned.contains_key(&at) {
            return None;
        }
        let parent = by_pid.get(&at)?;
        let class = own_class(parent);
        if matches!(class.0, UsageKind::ClaudeOutside | UsageKind::Simulator) {
            return Some(class);
        }
        at = parent.ppid;
    }
    None
}

fn own_class(p: &Proc) -> (UsageKind, String) {
    let name = p.name();
    if p.path.contains("CoreSimulator") || p.path.contains("/Simulator.app/") {
        return (UsageKind::Simulator, "Simulator".to_owned());
    }
    if p.path.contains("/Docker.app/") || name.starts_with("com.docker") {
        return (UsageKind::Docker, "Docker".to_owned());
    }
    if name == VM {
        return (UsageKind::Vm, "Virtual machines".to_owned());
    }
    if name == CLAUDE {
        return (UsageKind::ClaudeOutside, "Claude Code".to_owned());
    }
    match app_bundle(&p.path) {
        Some(app) => (UsageKind::App, app.to_owned()),
        None => (UsageKind::Other, "Other".to_owned()),
    }
}

/// `/Applications/Google Chrome.app/…/Helper.app/…` is `Google Chrome`:
/// an installed app's helpers count with it. System apps count as other.
fn app_bundle(path: &str) -> Option<&str> {
    let rest = path.split("/Applications/").nth(1)?;
    rest.split_once(".app/")
        .map(|(app, _)| app)
        .filter(|app| !app.contains('/'))
}

fn others(rest: HashMap<(UsageKind, String), Sum>, cores: u32) -> Vec<UsageGroup> {
    let mut rows: Vec<((UsageKind, String), Sum)> = rest.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1.memory));
    let mut kept = Vec::new();
    let mut other = Sum::default();
    let mut apps = 0;
    for ((kind, name), sum) in rows {
        let spill = kind == UsageKind::Other || (kind == UsageKind::App && apps >= MAX_APPS);
        if kind == UsageKind::App {
            apps += 1;
        }
        if spill {
            other.merge(&sum);
        } else {
            kept.push(group(kind, name, &sum, cores));
        }
    }
    if other.count > 0 {
        kept.push(group(UsageKind::Other, "Other".to_owned(), &other, cores));
    }
    kept
}

fn group(kind: UsageKind, name: String, s: &Sum, cores: u32) -> UsageGroup {
    UsageGroup {
        kind,
        name,
        terminal: None,
        workspace: None,
        cpu: share(s.cpu, cores),
        memory: s.memory,
        processes: s.count,
        idle_since_ms: None,
    }
}

#[derive(Debug, Default, Clone)]
struct Sum {
    cpu: f32,
    memory: u64,
    count: u32,
}

impl Sum {
    fn add(&mut self, p: &Proc) {
        self.cpu += p.cpu;
        self.memory += p.memory;
        self.count += 1;
    }

    fn merge(&mut self, o: &Sum) {
        self.cpu += o.cpu;
        self.memory += o.memory;
        self.count += o.count;
    }
}
