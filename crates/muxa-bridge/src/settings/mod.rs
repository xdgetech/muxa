//! Settings in the apps (ADR 0074): the editable view of `workspaces.toml`
//! and each Workspace's `.muxa/routines.toml`, and `settings-update`, which
//! edits them in place after the loading rules accept the result. The files
//! stay the source of truth; the daemon reads them again on its next use.

pub mod edit;
pub mod files;
pub mod secrets;
pub mod view;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use muxa_protocol::settings::{SecretState, Settings, SettingsOp};
use toml_edit::DocumentMut;

use crate::records::Workspace;
use crate::records::Workspaces;
use crate::records::routines::{self, ROUTINES_FILE, RoutineGate};
use files::Revision;
use secrets::Secrets;

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("{0}")]
    Invalid(String),
    #[error("the settings changed since they were read; read them again")]
    Stale,
    #[error("the host locked its settings (--settings-read-only); change them on the host")]
    ReadOnly,
    #[error("{0}")]
    Failed(String),
}

/// What `ops` would write, checked by the loading rules.
struct Plan {
    doc: DocumentMut,
    changed: BTreeMap<String, DocumentMut>,
    effects: edit::Effects,
    /// The Workspaces after the ops.
    current: Workspaces,
    names: Vec<&'static str>,
}

pub struct SettingsStore {
    /// None: neither `--workspaces` nor `HOME` names a file.
    path: Option<PathBuf>,
    home: Option<PathBuf>,
    read_only: bool,
    secrets: Box<dyn Secrets>,
    /// One update at a time.
    writing: Mutex<()>,
}

/// For the revision only: it never asks the Keychain.
struct Unasked;

impl Secrets for Unasked {
    fn state(&self, _account: &str) -> SecretState {
        SecretState::Missing
    }
    fn set(&self, _account: &str, _secret: &str) -> Result<(), String> {
        Err("not used".into())
    }
    fn delete(&self, _account: &str) -> Result<(), String> {
        Err("not used".into())
    }
}

/// The routines a Workspace's file holds now, or as an update would leave it.
fn check_routines(ws: &Workspace, text: &str, workers: &Workspaces) -> Result<(), String> {
    let dir = routines_dir(ws)?;
    let gate = ws.routines.unwrap_or(RoutineGate {
        direct_writes: false,
    });
    routines::parse_text(text, ws, gate, &dir, workers.workers()).map(drop)
}

/// The canonical `.muxa/`, or where it will be.
fn routines_dir(ws: &Workspace) -> Result<PathBuf, String> {
    Ok(routines::routines_dir(ws)?.unwrap_or_else(|| ws.root.join(routines::WORKSPACE_DIR)))
}

fn disk_routines(ws: &Workspace) -> Result<String, String> {
    Ok(routines::read_file(ws)?.map(|(_, t)| t).unwrap_or_default())
}

/// `path` inside `dir`, also through symlinks of the part that exists.
fn inside(dir: &Path, path: &Path) -> Result<(), String> {
    let refuse = || format!("{} leads outside .muxa/", path.display());
    let Ok(real_dir) = dir.canonicalize() else {
        // `.muxa/` does not exist yet: everything under it will be new.
        return if path.starts_with(dir) {
            Ok(())
        } else {
            Err(refuse())
        };
    };
    let mut existing = path.to_path_buf();
    let mut tail = Vec::new();
    let base = loop {
        match existing.canonicalize() {
            Ok(real) => break real,
            Err(_) => {
                if existing.symlink_metadata().is_ok() {
                    return Err(refuse());
                }
                tail.push(existing.file_name().ok_or_else(refuse)?.to_owned());
                if !existing.pop() {
                    return Err(refuse());
                }
            }
        }
    };
    let real = tail.iter().rev().fold(base, |p, n| p.join(n));
    if real.starts_with(&real_dir) {
        Ok(())
    } else {
        Err(refuse())
    }
}

impl SettingsStore {
    pub fn new(
        path: Option<PathBuf>,
        home: Option<PathBuf>,
        read_only: bool,
        secrets: Box<dyn Secrets>,
    ) -> Self {
        SettingsStore {
            path,
            home,
            read_only,
            secrets,
            writing: Mutex::new(()),
        }
    }

    fn path(&self) -> Result<&Path, SettingsError> {
        self.path.as_deref().ok_or_else(|| {
            SettingsError::Invalid(
                "this host has no workspaces file; set HOME or --workspaces".into(),
            )
        })
    }

    fn read_text(&self) -> Result<String, SettingsError> {
        files::read_text(self.path()?)
            .map(Option::unwrap_or_default)
            .map_err(|e| SettingsError::Failed(e.to_string()))
    }

    /// The view and its revision. Blocking.
    pub fn view(&self) -> Result<Settings, SettingsError> {
        self.build(self.secrets.as_ref())
    }

    fn build(&self, secrets: &dyn Secrets) -> Result<Settings, SettingsError> {
        let path = self.path()?;
        let home = self.home.as_deref();
        let bytes = files::read(path).map_err(|e| SettingsError::Failed(e.to_string()))?;
        let mut revision = Revision::default();
        revision.add(path, bytes.as_deref());
        let text = bytes
            .map(String::from_utf8)
            .transpose()
            .map_err(|_| SettingsError::Failed(format!("{} is not UTF-8", path.display())))?
            .unwrap_or_default();
        let (config, mut problem) = match view::parse_config(&text) {
            Ok(c) => (c, None),
            Err(why) => (view::FileConfig::default(), Some(why)),
        };
        let loaded = Workspaces::parse(&text, path, home);
        if let Err(e) = &loaded {
            problem.get_or_insert_with(|| e.to_string());
        }
        let mut workspaces: Vec<_> = config
            .workspace
            .into_iter()
            .map(|w| view::workspace(w, home, secrets, &mut revision))
            .collect();
        if let Ok(all) = &loaded {
            for w in workspaces
                .iter_mut()
                .filter(|w| w.routines_problem.is_none())
            {
                let Some(ws) = all.get(&w.name) else { continue };
                let checked = disk_routines(ws).and_then(|t| check_routines(ws, &t, all));
                w.routines_problem = checked.err();
            }
        }
        let assistant = view::assistant(config.assistant.as_ref(), path, &mut revision);
        Ok(Settings {
            revision: revision.finish(),
            read_only: self.read_only,
            problem,
            workspaces,
            workers: config.worker,
            assistant: Some(assistant),
        })
    }

    /// Applies `ops`, all or none, when `revision` is current. Blocking.
    pub fn update(&self, revision: &str, ops: Vec<SettingsOp>) -> Result<Settings, SettingsError> {
        if self.read_only {
            return Err(SettingsError::ReadOnly);
        }
        let _one = self
            .writing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.build(&Unasked)?.revision != revision {
            return Err(SettingsError::Stale);
        }
        let path = self.path()?.to_owned();
        let original = self.read_text()?;
        let plan = self.plan(&path, &original, ops)?;
        self.write(&path, &original, &plan)?;
        tracing::info!(ops = ?plan.names, "settings changed");
        self.view()
    }

    /// Checks `ops` against the current files with the loading rules, as
    /// `update` would, and writes nothing. Blocking.
    pub fn check_ops(&self, ops: Vec<SettingsOp>) -> Result<(), SettingsError> {
        let path = self.path()?.to_owned();
        let original = self.read_text()?;
        self.plan(&path, &original, ops).map(drop)
    }

    /// The edited documents, their effects and the Workspaces after `ops`.
    fn plan(
        &self,
        path: &Path,
        original: &str,
        ops: Vec<SettingsOp>,
    ) -> Result<Plan, SettingsError> {
        let mut doc: DocumentMut = original.parse().map_err(|e| {
            SettingsError::Invalid(format!("workspaces.toml cannot be edited: {e}"))
        })?;
        // A file the loader refuses now can still be fixed by the ops.
        let before = Workspaces::parse(original, path, self.home.as_deref()).unwrap_or_default();
        // Routines that load now must still load after each op.
        let mut keep: BTreeSet<String> = before
            .all()
            .iter()
            .filter(|ws| {
                disk_routines(ws)
                    .and_then(|t| check_routines(ws, &t, &before))
                    .is_ok()
            })
            .map(|ws| ws.name.clone())
            .collect();
        let mut changed: BTreeMap<String, DocumentMut> = BTreeMap::new();
        let mut effects = edit::Effects::default();
        let mut current = before;
        let names: Vec<&'static str> = ops.iter().map(SettingsOp::name).collect();
        for (i, op) in ops.into_iter().enumerate() {
            let tag = |why: String| SettingsError::Invalid(format!("ops[{i}] {}: {why}", names[i]));
            if let Some(w) = op.workspace() {
                keep.insert(w.to_owned());
            }
            let snapshot = current.clone();
            let mut load = |name: &str| -> Result<DocumentMut, String> {
                let ws = snapshot
                    .get(name)
                    .ok_or_else(|| format!("no Workspace named {name:?}"))?;
                disk_routines(ws)?
                    .parse()
                    .map_err(|e| format!(".muxa/routines.toml cannot be edited: {e}"))
            };
            edit::apply(&mut doc, &mut load, &mut changed, &mut effects, op).map_err(tag)?;
            current = self.check(&doc, &changed, &keep).map_err(tag)?;
        }
        Ok(Plan {
            doc,
            changed,
            effects,
            current,
            names,
        })
    }

    /// The loading rules on the edited documents; the new Workspaces.
    fn check(
        &self,
        doc: &DocumentMut,
        changed: &BTreeMap<String, DocumentMut>,
        keep: &BTreeSet<String>,
    ) -> Result<Workspaces, String> {
        let path = self.path().map_err(|e| e.to_string())?;
        let text = doc.to_string();
        same_meaning(doc, &text)?;
        let all =
            Workspaces::parse(&text, path, self.home.as_deref()).map_err(|e| e.to_string())?;
        for (name, routines_doc) in changed {
            let routines_text = routines_doc.to_string();
            same_meaning(routines_doc, &routines_text)?;
            if let Some(ws) = all.get(name) {
                check_routines(ws, &routines_text, &all)?;
            }
        }
        for ws in all.all().iter().filter(|w| keep.contains(&w.name)) {
            if !changed.contains_key(&ws.name) {
                check_routines(ws, &disk_routines(ws)?, &all)?;
            }
        }
        Ok(all)
    }

    /// Secrets first, then the files, then the secrets that went.
    fn write(&self, path: &Path, original: &str, plan: &Plan) -> Result<(), SettingsError> {
        let (doc, changed, effects, all) = (&plan.doc, &plan.changed, &plan.effects, &plan.current);
        let failed = |e: String| SettingsError::Failed(e);
        // All or none: a memory that cannot come back stops the whole update.
        for (name, bot, version) in &effects.memory_restores {
            let kept = all
                .bot(name, bot)
                .map(|b| b.read_memory_version(*version as usize));
            if !matches!(kept, Some(Ok(text)) if !text.is_empty()) {
                return Err(SettingsError::Invalid(format!(
                    "bot {bot:?} in {name:?} has no kept memory {version}"
                )));
            }
        }
        let mut writes: Vec<(PathBuf, String)> = Vec::new();
        for (name, rel, text) in &effects.instructions {
            let ws = all
                .get(name)
                .ok_or_else(|| SettingsError::Invalid(format!("no Workspace named {name:?}")))?;
            let dir = routines_dir(ws).map_err(SettingsError::Invalid)?;
            let target = dir.join(rel);
            inside(&dir, &target).map_err(SettingsError::Invalid)?;
            writes.push((target, text.clone()));
        }
        for (name, routines_doc) in changed {
            let Some(ws) = all.get(name) else { continue };
            let dir = routines_dir(ws).map_err(SettingsError::Invalid)?;
            let target = dir.join(ROUTINES_FILE);
            inside(&dir, &target).map_err(SettingsError::Invalid)?;
            writes.push((target, routines_doc.to_string()));
        }
        if let Some(soul) = &effects.assistant_soul {
            let files = crate::records::assistant::files(path).ok_or_else(|| {
                SettingsError::Invalid("the assistant's folder cannot be resolved".into())
            })?;
            let target = files.folder.join(crate::records::bots::SOUL_FILE);
            inside(&files.bound, &target).map_err(SettingsError::Invalid)?;
            writes.push((target, soul.clone()));
        }
        let text = doc.to_string();
        if text != original {
            writes.push((path.to_owned(), text));
        }
        for (account, secret) in &effects.secrets {
            self.secrets.set(account, secret.expose()).map_err(failed)?;
        }
        for (target, text) in &writes {
            files::write_atomic(target, text).map_err(|e| failed(e.to_string()))?;
        }
        for (name, bot, version) in &effects.memory_restores {
            restore_memory(all, name, bot, *version).map_err(failed)?;
        }
        for account in &effects.forget {
            if let Err(e) = self.secrets.delete(account) {
                tracing::warn!(account = %account, error = %e, "a removed connection's secret stays in the Keychain");
            }
        }
        Ok(())
    }
}

/// Puts kept memory `version` back; the current memory is kept in turn.
fn restore_memory(
    all: &Workspaces,
    workspace: &str,
    bot: &str,
    version: u32,
) -> Result<(), String> {
    let found = all
        .bot(workspace, bot)
        .ok_or_else(|| format!("there is no bot {bot:?} in {workspace:?}"))?;
    let text = found
        .read_memory_version(version as usize)
        .map_err(|e| e.to_string())?;
    if text.is_empty() {
        return Err(format!("bot {bot:?} has no kept memory {version}"));
    }
    found.write_memory(&text).map_err(|e| e.to_string())?;
    tracing::info!(bot, version, "bot memory restored");
    Ok(())
}

/// Printing the edited document and reading it back must give the same
/// values; a table printed under the wrong `[[workspace]]` would not.
fn same_meaning(doc: &DocumentMut, text: &str) -> Result<(), String> {
    let back: toml::Table = toml::from_str(text).map_err(|e| e.message().to_owned())?;
    if back == edit::plain(doc) {
        Ok(())
    } else {
        Err("muxa cannot write this change safely into the file's layout; edit it by hand".into())
    }
}

#[cfg(test)]
mod tests;
