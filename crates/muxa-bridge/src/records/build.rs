//! `[build]` and `[workspace.build]`: where a place's build output goes
//! (ADR 0102). File-only; the apps' Settings do not edit them.

use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use super::workspaces::{expand, parse_duration};

/// What `paths` is when a Workspace names none.
pub const DEFAULT_PATHS: [&str; 1] = ["target"];
/// Three days, in seconds.
pub const DEFAULT_DROP_AFTER: u64 = 3 * 86_400;
const MAX_PATHS: usize = 16;

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct RawHostBuild {
    dir: Option<String>,
    drop_after: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct RawWorkspaceBuild {
    dir: Option<String>,
    paths: Option<Vec<String>>,
}

/// The host's `[build]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostBuild {
    /// Absolute, as written with `~/` expanded. It may not exist (a disk
    /// that is not mounted), so it is not canonicalized.
    pub dir: Option<PathBuf>,
    /// Seconds after which an ended agent's places lose their build output.
    pub drop_after: u64,
}

impl Default for HostBuild {
    fn default() -> Self {
        Self {
            dir: None,
            drop_after: DEFAULT_DROP_AFTER,
        }
    }
}

/// A Workspace's `[workspace.build]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceBuild {
    pub dir: Option<PathBuf>,
    /// Relative, plain names only. None means [`DEFAULT_PATHS`].
    pub paths: Option<Vec<PathBuf>>,
}

/// Where one Workspace's places put their build output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildDisk {
    pub dir: PathBuf,
    pub paths: Vec<PathBuf>,
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
        Ok(HostBuild { dir, drop_after })
    }
}

impl RawWorkspaceBuild {
    pub(super) fn check(self, home: Option<&Path>) -> Result<WorkspaceBuild, String> {
        let dir = self
            .dir
            .map(|d| expand(&d, home))
            .transpose()
            .map_err(|r| format!("build dir {r}"))?;
        let paths = self.paths.map(|p| check_paths(&p)).transpose()?;
        Ok(WorkspaceBuild { dir, paths })
    }
}

/// The Workspace's own `dir`, else the host's; None turns the build disk off.
pub fn disk(host: &HostBuild, ws: &WorkspaceBuild) -> Option<BuildDisk> {
    let dir = ws.dir.clone().or_else(|| host.dir.clone())?;
    let paths = ws
        .paths
        .clone()
        .unwrap_or_else(|| DEFAULT_PATHS.iter().map(PathBuf::from).collect());
    Some(BuildDisk { dir, paths })
}

fn check_paths(raw: &[String]) -> Result<Vec<PathBuf>, String> {
    if raw.len() > MAX_PATHS {
        return Err(format!("build paths has more than {MAX_PATHS} entries"));
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
                    "build paths: {p:?} must be a relative path of plain names"
                ))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(paths: &[&str]) -> Result<WorkspaceBuild, String> {
        RawWorkspaceBuild {
            dir: None,
            paths: Some(paths.iter().map(|p| p.to_string()).collect()),
        }
        .check(None)
    }

    #[test]
    fn paths_must_be_plain_and_relative() {
        assert!(ws(&["target", "apple/MuxaKit/.build"]).is_ok());
        for bad in ["", "/abs", "../up", "a/../b", "./x"] {
            assert!(ws(&[bad]).is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn dir_must_be_absolute() {
        let raw = RawHostBuild {
            dir: Some("relative/dir".into()),
            drop_after: None,
        };
        assert!(raw.check(None).is_err());
    }

    #[test]
    fn workspace_dir_wins_and_paths_default_to_target() {
        let host = HostBuild {
            dir: Some("/ssd/host".into()),
            ..HostBuild::default()
        };
        let none = WorkspaceBuild::default();
        assert_eq!(
            disk(&host, &none),
            Some(BuildDisk {
                dir: "/ssd/host".into(),
                paths: vec!["target".into()],
            })
        );
        let own = WorkspaceBuild {
            dir: Some("/ssd/ws".into()),
            paths: None,
        };
        assert_eq!(disk(&host, &own).map(|d| d.dir), Some("/ssd/ws".into()));
        assert_eq!(disk(&HostBuild::default(), &none), None);
    }

    #[test]
    fn drop_after_defaults_to_three_days() {
        let host = RawHostBuild::default().check(None).unwrap();
        assert_eq!(host.drop_after, DEFAULT_DROP_AFTER);
        let raw = RawHostBuild {
            dir: None,
            drop_after: Some("12h".into()),
        };
        assert_eq!(raw.check(None).unwrap().drop_after, 12 * 3600);
    }
}
