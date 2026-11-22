//! Where the daemon's state and the workspaces config live.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::records::Workspaces;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Routine and Inbox state. None when neither `--state-dir` nor `HOME`
    /// gives one.
    pub state_dir: Option<PathBuf>,
    /// None means no workspaces.
    pub workspaces: Option<PathBuf>,
    /// Expands `~/` in `workspaces.toml`.
    pub home: Option<PathBuf>,
}

impl Config {
    /// `$XDG_STATE_HOME/muxa/tasks`, else `~/.local/state/muxa/tasks`: the
    /// folder older muxa kept Task records in, so Routine and Inbox state stay.
    pub fn default_state_dir(
        xdg_state_home: Option<&OsStr>,
        home: Option<&Path>,
    ) -> Option<PathBuf> {
        let base = xdg_state_home
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|h| h.join(".local/state")))?;
        Some(base.join("muxa/tasks"))
    }

    /// The workspaces config; none when no file is set. Blocking.
    pub fn load_workspaces(&self) -> Result<Workspaces, String> {
        let Some(path) = &self.workspaces else {
            return Ok(Workspaces::default());
        };
        Workspaces::load(path, self.home.as_deref()).map_err(|e| {
            tracing::warn!(error = %e, "cannot load workspaces");
            format!("{e}. Fix the workspaces file on the host.")
        })
    }
}
