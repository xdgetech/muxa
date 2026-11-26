//! `[build]` and `[workspace.build]`: where a place's build output goes
//! (ADR 0102). Which paths go is found per repo (`v2::build_paths`).

use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use super::workspaces::{expand, parse_duration};

/// Three days, in seconds.
pub const DEFAULT_DROP_AFTER: u64 = 3 * 86_400;
const MAX_PATHS: usize = 16;

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct RawHostBuild {
    dir: Option<String>,
    drop_after: Option<String>,
    extra_paths: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct RawWorkspaceBuild {
    dir: Option<String>,
}

/// The host's `[build]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostBuild {
    /// Absolute, as written with `~/` expanded. It may not exist (a disk
    /// that is not mounted), so it is not canonicalized.
    pub dir: Option<PathBuf>,
    /// Seconds after which an ended agent's places lose their build output.
    pub drop_after: u64,
    /// Paths every worktree links too, when its repo ignores them. Relative,
    /// plain names only.
    pub extra_paths: Vec<PathBuf>,
}

impl Default for HostBuild {
    fn default() -> Self {
        Self {
            dir: None,
            drop_after: DEFAULT_DROP_AFTER,
            extra_paths: Vec::new(),
        }
    }
}

/// A Workspace's `[workspace.build]`: another build disk, a hand edit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceBuild {
    pub dir: Option<PathBuf>,
}

/// Where one Workspace's places put their build output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildDisk {
    pub dir: PathBuf,
    /// The host's `extra_paths`, beside the ones found in each repo.
    pub extra_paths: Vec<PathBuf>,
}

impl RawHostBuild {
    pub(super) fn check(self, home: Option<&Path>) -> Result<HostBuild, String> {
        let dir = self
            .dir
            .map(|d| expand(&d, home))
            .transpose()
            .map_err(|r| format!("[build] dir {r}"))?;
        let drop_after = self
            .drop_after
            .as_deref()
            .map(parse_duration)
            .transpose()
            .map_err(|r| format!("[build] drop_after {r}"))?
            .unwrap_or(DEFAULT_DROP_AFTER);
        let extra_paths = self
            .extra_paths
            .map(|p| check_paths(&p))
            .transpose()?
            .unwrap_or_default();
        Ok(HostBuild {
            dir,
            drop_after,
            extra_paths,
        })
    }
}

impl RawWorkspaceBuild {
    pub(super) fn check(self, home: Option<&Path>) -> Result<WorkspaceBuild, String> {
        let dir = self
            .dir
            .map(|d| expand(&d, home))
            .transpose()
            .map_err(|r| format!("build dir {r}"))?;
        Ok(WorkspaceBuild { dir })
    }
}

/// The Workspace's own `dir`, else the host's; None turns the build disk off.
pub fn disk(host: &HostBuild, ws: &WorkspaceBuild) -> Option<BuildDisk> {
    let dir = ws.dir.clone().or_else(|| host.dir.clone())?;
    Some(BuildDisk {
        dir,
        extra_paths: host.extra_paths.clone(),
    })
}

fn check_paths(raw: &[String]) -> Result<Vec<PathBuf>, String> {
    if raw.len() > MAX_PATHS {
        return Err(format!(
            "build extra_paths has more than {MAX_PATHS} entries"
        ));
    }
    raw.iter()
        .map(|p| {
            let path = PathBuf::from(p);
            let plain =
                !p.is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)));
            if plain {
                Ok(path)
            } else {
                Err(format!(
                    "build extra_paths: {p:?} must be a relative path of plain names"
                ))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extra(paths: &[&str]) -> Result<HostBuild, String> {
        RawHostBuild {
            extra_paths: Some(paths.iter().map(|p| p.to_string()).collect()),
            ..RawHostBuild::default()
        }
        .check(None)
    }

    #[test]
    fn extra_paths_must_be_plain_and_relative() {
        assert!(extra(&["target", "web/.next"]).is_ok());
        for bad in ["", "/abs", "../up", "a/../b", "./x"] {
            assert!(extra(&[bad]).is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn dir_must_be_absolute() {
        let raw = RawHostBuild {
            dir: Some("relative/dir".into()),
            ..RawHostBuild::default()
        };
        assert!(raw.check(None).is_err());
    }

    #[test]
    fn workspace_dir_wins_and_extra_paths_come_from_the_host() {
        let host = HostBuild {
            dir: Some("/ssd/host".into()),
            extra_paths: vec!["out".into()],
            ..HostBuild::default()
        };
        let none = WorkspaceBuild::default();
        assert_eq!(
            disk(&host, &none),
            Some(BuildDisk {
                dir: "/ssd/host".into(),
                extra_paths: vec!["out".into()],
            })
        );
        let own = WorkspaceBuild {
            dir: Some("/ssd/ws".into()),
        };
        assert_eq!(disk(&host, &own).map(|d| d.dir), Some("/ssd/ws".into()));
        assert_eq!(disk(&HostBuild::default(), &none), None);
    }

    #[test]
    fn drop_after_defaults_to_three_days() {
        let host = RawHostBuild::default().check(None).unwrap();
        assert_eq!(host.drop_after, DEFAULT_DROP_AFTER);
        let raw = RawHostBuild {
            drop_after: Some("12h".into()),
            ..RawHostBuild::default()
        };
        assert_eq!(raw.check(None).unwrap().drop_after, 12 * 3600);
    }
}
