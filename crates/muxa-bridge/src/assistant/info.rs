//! `assistant` in `inbox-list` (ADR 0047 §7): who assists on this host.
//! Clients leave `overall` out of their New Task and Capture pickers.
use muxa_protocol::{AssistantInfo, DelegateInfo};

use crate::records::Workspaces;
use crate::records::assistant::Role;
use crate::records::workspaces::Workspace;

/// Blocking: reads the time of each delegate's notes, never their text.
/// None while the host is off, or when no workspace has `[workspace.assistant]`.
pub fn info(workspaces: &Workspaces) -> Option<AssistantInfo> {
    if !workspaces.assistant().enabled {
        return None;
    }
    let overall = workspaces.overall().map(|w| w.name.clone());
    let delegates: Vec<DelegateInfo> = workspaces
        .all()
        .iter()
        .filter(|w| matches!(w.role, Role::Member { .. }))
        .map(|w| DelegateInfo {
            workspace: w.name.clone(),
            share: workspaces.share(&w.name),
            overall_requests: workspaces.overall_requests(&w.name),
            notes_at: notes_at(w),
        })
        .collect();
    (overall.is_some() || !delegates.is_empty()).then_some(AssistantInfo { overall, delegates })
}

/// An unusable notes file shows as none; a run's start reports it in full.
fn notes_at(w: &Workspace) -> Option<u64> {
    super::notes::modified(&w.root).unwrap_or_else(|e| {
        tracing::warn!(workspace = %w.name, error = %e, "cannot read the time of the notes");
        None
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use muxa_protocol::{OverallRequests, Share};

    use super::*;

    fn roots(names: &[&str]) -> Vec<std::path::PathBuf> {
        names
            .iter()
            .map(|n| {
                let dir = std::env::temp_dir().join(format!(
                    "muxa-info-{}-{n}-{:08x}",
                    std::process::id(),
                    getrandom::u32().unwrap()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                dir.canonicalize().unwrap()
            })
            .collect()
    }

    #[test]
    fn the_overall_workspace_and_every_member_are_listed() {
        let r = roots(&["o", "a", "b", "c"]);
        let text = format!(
            "[assistant]\nenabled = true\n\
             [[workspace]]\nname = \"Overall\"\nroot = \"{}\"\n\
             [workspace.assistant]\noverall = true\n\
             [[workspace]]\nname = \"A\"\nroot = \"{}\"\n\
             [workspace.assistant]\ndelegate = true\noverall_requests = \"auto\"\n\
             [[workspace]]\nname = \"B\"\nroot = \"{}\"\n\
             [workspace.assistant]\ndelegate = false\nshare = \"outline\"\n\
             [[workspace]]\nname = \"C\"\nroot = \"{}\"\n",
            r[0].display(),
            r[1].display(),
            r[2].display(),
            r[3].display()
        );
        std::fs::create_dir_all(r[1].join(".muxa/assistant")).unwrap();
        std::fs::write(r[1].join(super::super::notes::NOTES), "# A\n").unwrap();
        let w = Workspaces::parse(&text, Path::new("w.toml"), None).unwrap();
        let info = info(&w).unwrap();
        assert_eq!(info.overall.as_deref(), Some("Overall"));
        let shown: Vec<_> = info
            .delegates
            .iter()
            .map(|d| (d.workspace.as_str(), d.share, d.overall_requests))
            .collect();
        assert_eq!(
            shown,
            [
                ("A", Share::Summaries, OverallRequests::Auto),
                ("B", Share::Outline, OverallRequests::None),
            ]
        );
        assert!(info.delegates[0].notes_at.is_some());
        assert_eq!(info.delegates[1].notes_at, None);

        let off = Workspaces::parse(
            &text.replace("enabled = true", "enabled = false"),
            Path::new("w.toml"),
            None,
        )
        .unwrap();
        assert_eq!(super::info(&off), None);
        for dir in r {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
