//! Private restart-file staging. The caller holds the Task lifecycle claim.
//! No document bytes appear in manifests, Debug, errors or logs.
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path};

use rustix::fs::{AtFlags, Mode, OFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_FILE: usize = 64 * 1024;
const FILES: [&str; 2] = ["TASK.md", "HANDOFF.md"];
const STAGES: [&str; 2] = ["TASK.next", "HANDOFF.next"];
const DIR_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const READ_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC)
    .union(OFlags::NONBLOCK);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Identity {
    pub device: u64,
    pub inode: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Content {
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub name: String,
    /// None means absent, distinct from the SHA256 of empty bytes.
    pub old: Option<Content>,
    pub new: Content,
    pub stage: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub operation: String,
    pub task: Identity,
    pub managed: Identity,
    pub restarts: Identity,
    pub directory: Identity,
    pub files: [Entry; 2],
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("restart file path or manifest is invalid")]
    Invalid,
    #[error("restart file {0} must be regular, UTF-8 without NUL, and at most 64 KiB")]
    Content(&'static str),
    #[error("restart file {0} changed; refresh before continuing")]
    Conflict(&'static str),
    #[error("restart file directory changed; recovery is required")]
    DirectoryChanged,
    #[error("cannot {action} restart file {name}: {source}")]
    Io {
        action: &'static str,
        name: &'static str,
        source: io::Error,
    },
}

fn io_error(action: &'static str, name: &'static str) -> impl FnOnce(rustix::io::Errno) -> Error {
    move |error| Error::Io {
        action,
        name,
        source: error.into(),
    }
}

fn std_error(action: &'static str, name: &'static str) -> impl FnOnce(io::Error) -> Error {
    move |source| Error::Io {
        action,
        name,
        source,
    }
}

fn identity(file: &File) -> Result<Identity, Error> {
    let metadata = file.metadata().map_err(std_error("inspect", "directory"))?;
    Ok(Identity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

fn digest(bytes: &[u8]) -> Content {
    Content {
        sha256: format!("{:x}", Sha256::digest(bytes)),
        bytes: bytes.len() as u64,
    }
}

fn valid_content(content: &Content) -> bool {
    content.bytes <= MAX_FILE as u64
        && content.sha256.len() == 64
        && content
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn validate_bytes(bytes: &[u8], name: &'static str) -> Result<(), Error> {
    if bytes.len() > MAX_FILE || bytes.contains(&0) || std::str::from_utf8(bytes).is_err() {
        return Err(Error::Content(name));
    }
    Ok(())
}

fn valid_operation(operation: &str) -> bool {
    !operation.is_empty()
        && operation.len() <= 64
        && operation
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn validate_manifest(manifest: &Manifest) -> Result<(), Error> {
    if manifest.version != 1 || !valid_operation(&manifest.operation) {
        return Err(Error::Invalid);
    }
    for (index, entry) in manifest.files.iter().enumerate() {
        if entry.name != FILES[index]
            || entry.stage != STAGES[index]
            || !valid_content(&entry.new)
            || entry.old.as_ref().is_some_and(|old| !valid_content(old))
        {
            return Err(Error::Invalid);
        }
    }
    Ok(())
}

/// Walk every component without following symlinks. Task records use canonical paths.
fn open_task(path: &Path) -> Result<File, Error> {
    if !path.is_absolute() {
        return Err(Error::Invalid);
    }
    let mut directory = File::from(
        rustix::fs::open("/", DIR_FLAGS, Mode::empty())
            .map_err(io_error("open", "task directory"))?,
    );
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                directory = File::from(
                    rustix::fs::openat(&directory, name, DIR_FLAGS, Mode::empty())
                        .map_err(io_error("open", "task directory"))?,
                );
            }
            _ => return Err(Error::Invalid),
        }
    }
    Ok(directory)
}

fn child_dir(parent: &File, name: &str, create: bool, private: bool) -> Result<File, Error> {
    if create {
        match rustix::fs::mkdirat(parent, name, Mode::RUSR | Mode::WUSR | Mode::XUSR) {
            Ok(()) => rustix::fs::fsync(parent).map_err(io_error("sync", "parent directory"))?,
            Err(rustix::io::Errno::EXIST) => {}
            Err(error) => return Err(io_error("create", "staging directory")(error)),
        }
    }
    let child = File::from(
        rustix::fs::openat(parent, name, DIR_FLAGS, Mode::empty())
            .map_err(io_error("open", "staging directory"))?,
    );
    let mode = child
        .metadata()
        .map_err(std_error("inspect", "directory"))?
        .mode();
    if (private && mode & 0o077 != 0) || mode & 0o022 != 0 {
        return Err(Error::Invalid);
    }
    Ok(child)
}

fn read_at(parent: &File, name: &str, label: &'static str) -> Result<Option<Vec<u8>>, Error> {
    let fd = match rustix::fs::openat(parent, name, READ_FLAGS, Mode::empty()) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(error) => return Err(io_error("open", label)(error)),
    };
    let file = File::from(fd);
    let metadata = file.metadata().map_err(std_error("inspect", label))?;
    if !metadata.is_file() || metadata.len() > MAX_FILE as u64 {
        return Err(Error::Content(label));
    }
    let mut bytes = Vec::new();
    file.take((MAX_FILE + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(std_error("read", label))?;
    validate_bytes(&bytes, label)?;
    Ok(Some(bytes))
}

/// Strict preflight read of TASK.md or HANDOFF.md. Missing `.muxa` means absent.
pub fn read_current(task_dir: &Path, filename: &str) -> Result<Option<Vec<u8>>, Error> {
    let label = match filename {
        "TASK.md" => FILES[0],
        "HANDOFF.md" => FILES[1],
        _ => return Err(Error::Invalid),
    };
    let task = open_task(task_dir)?;
    let managed = match rustix::fs::openat(&task, ".muxa", DIR_FLAGS, Mode::empty()) {
        Ok(fd) => File::from(fd),
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(error) => return Err(io_error("open", "managed directory")(error)),
    };
    let bytes = read_at(&managed, filename, label)?;
    check_child(&task, ".muxa", &identity(&managed)?)?;
    Ok(bytes)
}

struct Anchors {
    task: File,
    managed: File,
    restarts: File,
    directory: File,
}

fn check_child(parent: &File, name: &str, expected: &Identity) -> Result<(), Error> {
    let current = child_dir(parent, name, false, false)?;
    if identity(&current)? != *expected {
        return Err(Error::DirectoryChanged);
    }
    Ok(())
}

impl Anchors {
    fn open(task_dir: &Path, manifest: &Manifest) -> Result<Self, Error> {
        validate_manifest(manifest)?;
        let task = open_task(task_dir)?;
        if identity(&task)? != manifest.task {
            return Err(Error::DirectoryChanged);
        }
        let managed = child_dir(&task, ".muxa", false, false)?;
        let restarts = child_dir(&managed, "restarts", false, true)?;
        let directory = child_dir(&restarts, &manifest.operation, false, true)?;
        let anchors = Self {
            task,
            managed,
            restarts,
            directory,
        };
        anchors.check(task_dir, manifest)?;
        Ok(anchors)
    }

    fn check(&self, task_dir: &Path, manifest: &Manifest) -> Result<(), Error> {
        if identity(&open_task(task_dir)?)? != manifest.task {
            return Err(Error::DirectoryChanged);
        }
        check_child(&self.task, ".muxa", &manifest.managed)?;
        check_child(&self.managed, "restarts", &manifest.restarts)?;
        check_child(&self.restarts, &manifest.operation, &manifest.directory)
    }
}

/// Convenience for callers that are not merging existing notes. Merge callers
/// must use `stage_from` with the exact bytes they read before rendering.
#[cfg(test)]
pub fn stage(
    task_dir: &Path,
    operation: &str,
    task_bytes: &[u8],
    handoff_bytes: &[u8],
) -> Result<Manifest, Error> {
    let old_task = read_current(task_dir, FILES[0])?;
    let old_handoff = read_current(task_dir, FILES[1])?;
    stage_from(
        task_dir,
        operation,
        old_task.as_deref(),
        old_handoff.as_deref(),
        task_bytes,
        handoff_bytes,
    )
}

/// Stage complete files only if the old bytes still match the merge baseline.
/// The caller persists this manifest before authorizing any external replacement.
pub fn stage_from(
    task_dir: &Path,
    operation: &str,
    expected_task: Option<&[u8]>,
    expected_handoff: Option<&[u8]>,
    task_bytes: &[u8],
    handoff_bytes: &[u8],
) -> Result<Manifest, Error> {
    if !valid_operation(operation) {
        return Err(Error::Invalid);
    }
    let new = [task_bytes, handoff_bytes];
    let old = [expected_task, expected_handoff];
    for index in 0..2 {
        validate_bytes(new[index], FILES[index])?;
        if let Some(bytes) = old[index] {
            validate_bytes(bytes, FILES[index])?;
        }
    }
    let task = open_task(task_dir)?;
    let managed = child_dir(&task, ".muxa", true, false)?;
    for index in 0..2 {
        if read_at(&managed, FILES[index], FILES[index])?.as_deref() != old[index] {
            return Err(Error::Conflict(FILES[index]));
        }
    }
    let restarts = child_dir(&managed, "restarts", true, true)?;
    // Never reuse another operation's private files.
    rustix::fs::mkdirat(&restarts, operation, Mode::RUSR | Mode::WUSR | Mode::XUSR)
        .map_err(io_error("create", "operation directory"))?;
    rustix::fs::fsync(&restarts).map_err(io_error("sync", "restarts directory"))?;
    let directory = child_dir(&restarts, operation, false, true)?;
    let files = std::array::from_fn(|index| Entry {
        name: FILES[index].into(),
        old: old[index].map(digest),
        new: digest(new[index]),
        stage: STAGES[index].into(),
    });
    let manifest = Manifest {
        version: 1,
        operation: operation.into(),
        task: identity(&task)?,
        managed: identity(&managed)?,
        restarts: identity(&restarts)?,
        directory: identity(&directory)?,
        files,
    };
    let anchors = Anchors {
        task,
        managed,
        restarts,
        directory,
    };
    for index in 0..2 {
        anchors.check(task_dir, &manifest)?;
        let fd = rustix::fs::openat(
            &anchors.directory,
            STAGES[index],
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(io_error("create", FILES[index]))?;
        let mut file = File::from(fd);
        file.write_all(new[index])
            .map_err(std_error("write", FILES[index]))?;
        file.sync_all().map_err(std_error("sync", FILES[index]))?;
    }
    rustix::fs::fsync(&anchors.directory).map_err(io_error("sync", "operation directory"))?;
    anchors.check(task_dir, &manifest)?;
    for index in 0..2 {
        if read_at(&anchors.managed, FILES[index], FILES[index])?.as_deref() != old[index] {
            return Err(Error::Conflict(FILES[index]));
        }
    }
    Ok(manifest)
}

fn current_hash(anchors: &Anchors, index: usize) -> Result<Option<Content>, Error> {
    Ok(read_at(&anchors.managed, FILES[index], FILES[index])?
        .as_deref()
        .map(digest))
}

fn staged_bytes(anchors: &Anchors, manifest: &Manifest, index: usize) -> Result<Vec<u8>, Error> {
    let bytes = read_at(&anchors.directory, STAGES[index], FILES[index])?
        .ok_or(Error::Conflict(FILES[index]))?;
    if digest(&bytes) != manifest.files[index].new {
        return Err(Error::Conflict(FILES[index]));
    }
    Ok(bytes)
}

/// No mutation: each live file must be its old base or already-published bytes.
pub fn verify(task_dir: &Path, manifest: &Manifest) -> Result<(), Error> {
    let anchors = Anchors::open(task_dir, manifest)?;
    for (index, name) in FILES.iter().enumerate() {
        let current = current_hash(&anchors, index)?;
        if current != manifest.files[index].old
            && current.as_ref() != Some(&manifest.files[index].new)
        {
            return Err(Error::Conflict(name));
        }
        // Published entries need no surviving staged file after their rename.
        if current.as_ref() != Some(&manifest.files[index].new) {
            staged_bytes(&anchors, manifest, index)?;
        }
    }
    anchors.check(task_dir, manifest)
}

/// Publish one file at a time. A crash may expose a mixed pair; automatic
/// prompts remain blocked until the caller durably finalizes the whole operation.
/// External editors must cooperate with the Task lock for compare/rename exclusion.
pub fn reconcile(task_dir: &Path, manifest: &Manifest) -> Result<(), Error> {
    let anchors = Anchors::open(task_dir, manifest)?;
    verify(task_dir, manifest)?;
    for index in 0..2 {
        publish_entry(&anchors, task_dir, manifest, index)?;
    }
    Ok(())
}

fn publish_entry(
    anchors: &Anchors,
    task_dir: &Path,
    manifest: &Manifest,
    index: usize,
) -> Result<(), Error> {
    anchors.check(task_dir, manifest)?;
    let current = current_hash(anchors, index)?;
    if current.as_ref() == Some(&manifest.files[index].new) {
        return Ok(());
    }
    if current != manifest.files[index].old {
        return Err(Error::Conflict(FILES[index]));
    }
    staged_bytes(anchors, manifest, index)?;
    anchors.check(task_dir, manifest)?;
    // Descriptor-relative rename cannot follow a substituted final symlink.
    rustix::fs::renameat(
        &anchors.directory,
        STAGES[index],
        &anchors.managed,
        FILES[index],
    )
    .map_err(io_error("publish", FILES[index]))?;
    rustix::fs::fsync(&anchors.managed).map_err(io_error("sync", "managed directory"))?;
    rustix::fs::fsync(&anchors.directory).map_err(io_error("sync", "operation directory"))?;
    anchors.check(task_dir, manifest)?;
    if current_hash(anchors, index)?.as_ref() != Some(&manifest.files[index].new) {
        return Err(Error::Conflict(FILES[index]));
    }
    Ok(())
}

/// Caller must verify the durable operation is still prepared under its Task
/// claim. Refuse cleanup if any live file differs from its original base.
pub fn cleanup_prepared(task_dir: &Path, manifest: &Manifest) -> Result<(), Error> {
    let anchors = Anchors::open(task_dir, manifest)?;
    for (index, name) in FILES.iter().enumerate() {
        if current_hash(&anchors, index)? != manifest.files[index].old {
            return Err(Error::Conflict(name));
        }
        staged_bytes(&anchors, manifest, index)?;
    }
    anchors.check(task_dir, manifest)?;
    for index in 0..2 {
        rustix::fs::unlinkat(&anchors.directory, STAGES[index], AtFlags::empty())
            .map_err(io_error("remove", FILES[index]))?;
    }
    rustix::fs::fsync(&anchors.directory).map_err(io_error("sync", "operation directory"))?;
    anchors.check(task_dir, manifest)?;
    rustix::fs::unlinkat(&anchors.restarts, &manifest.operation, AtFlags::REMOVEDIR)
        .map_err(io_error("remove", "operation directory"))?;
    rustix::fs::fsync(&anchors.restarts).map_err(io_error("sync", "restarts directory"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::PathBuf;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "muxa-restart-files-{}-{:08x}",
                std::process::id(),
                getrandom::u32().unwrap()
            ));
            fs::create_dir(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }
        fn live(&self, name: &str) -> PathBuf {
            self.0.join(".muxa").join(name)
        }
        fn staged(&self, operation: &str, name: &str) -> PathBuf {
            self.0.join(".muxa/restarts").join(operation).join(name)
        }
        fn write_old(&self) {
            fs::create_dir(self.0.join(".muxa")).unwrap();
            fs::write(self.live("TASK.md"), b"old task notes\n").unwrap();
            fs::write(self.live("HANDOFF.md"), b"old handoff\n").unwrap();
        }
        fn manifest(&self) -> Manifest {
            stage(&self.0, "op1", b"new task notes\n", b"new handoff\n").unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn staged_pair_is_private_and_manifest_contains_only_hashes_and_identity() {
        let fixture = Fixture::new();
        fixture.write_old();
        let manifest = fixture.manifest();
        assert_eq!(
            read_current(&fixture.0, "TASK.md").unwrap().unwrap(),
            b"old task notes\n"
        );
        assert_eq!(manifest.files[0].old, Some(digest(b"old task notes\n")));
        assert_eq!(manifest.files[1].new, digest(b"new handoff\n"));
        for name in STAGES {
            assert_eq!(
                fs::metadata(fixture.staged("op1", name))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        assert_eq!(
            fs::metadata(fixture.0.join(".muxa/restarts/op1"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(!json.contains("old task notes") && !json.contains("new handoff"));
        assert_eq!(serde_json::from_str::<Manifest>(&json).unwrap(), manifest);
        verify(&fixture.0, &manifest).unwrap();
        reconcile(&fixture.0, &manifest).unwrap();
        reconcile(&fixture.0, &manifest).unwrap();
        assert_eq!(
            read_current(&fixture.0, "TASK.md").unwrap().unwrap(),
            b"new task notes\n"
        );
        assert_eq!(
            read_current(&fixture.0, "HANDOFF.md").unwrap().unwrap(),
            b"new handoff\n"
        );
    }

    #[test]
    fn partial_publish_can_be_reconciled_without_replacing_already_published_inode() {
        let fixture = Fixture::new();
        fixture.write_old();
        let manifest = fixture.manifest();
        fs::rename(fixture.staged("op1", "TASK.next"), fixture.live("TASK.md")).unwrap();
        let before = fs::metadata(fixture.live("TASK.md")).unwrap().ino();
        verify(&fixture.0, &manifest).unwrap();
        reconcile(&fixture.0, &manifest).unwrap();
        assert_eq!(fs::metadata(fixture.live("TASK.md")).unwrap().ino(), before);
        assert_eq!(
            fs::read(fixture.live("HANDOFF.md")).unwrap(),
            b"new handoff\n"
        );
    }

    /// Only this test entry reads the probe environment. The production bridge
    /// has no fault hook; this child executes the real single-file publisher.
    #[test]
    fn publication_boundary_child() {
        let Some(task_dir) = std::env::var_os("MUXA_PUBLICATION_PROBE_DIR") else {
            return;
        };
        let task_dir = PathBuf::from(task_dir);
        let id = std::env::var("MUXA_PUBLICATION_PROBE_TASK").unwrap();
        let store = crate::records::Store::open(task_dir.join("state")).unwrap();
        let record = store.get(&id).unwrap();
        let _claim = store.claim(&id, record.rev).unwrap();
        let op = record.restart_operation().unwrap().unwrap();
        assert_eq!(op.phase, crate::records::lifecycle::Phase::Replaced);
        let manifest = op.files.unwrap();
        let anchors = Anchors::open(&task_dir, &manifest).unwrap();
        verify(&task_dir, &manifest).unwrap();
        if std::env::var("MUXA_PUBLICATION_PROBE_BOUNDARY").unwrap() == "after-task" {
            publish_entry(&anchors, &task_dir, &manifest, 0).unwrap();
        }
        println!("publication-held");
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::park();
        }
    }

    #[test]
    fn killed_publication_owner_preserves_replaced_intent_and_reconciles_or_conflicts() {
        use crate::records::lifecycle::{Operation, Phase, ProcessIdentity};
        use crate::records::{NewRecord, Store, StoreError};
        use std::io::{BufRead, BufReader};
        use std::process::{Child, Command, Stdio};

        struct Owner(Child);
        impl Drop for Owner {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        // The process identity is synthetic: this exercises the production
        // publisher and OS owner death, not a full bridge/native restart.
        for (published, edited) in [(false, false), (true, false), (true, true)] {
            let fixture = Fixture::new();
            fixture.write_old();
            let manifest = fixture.manifest();
            let store = Store::open(fixture.0.join("state")).unwrap();
            let record = store
                .create(NewRecord {
                    workspace: "fixture".into(),
                    title: "publication probe".into(),
                    dir: Some(fixture.0.clone()),
                    ..Default::default()
                })
                .unwrap();
            let op = Operation {
                version: 1,
                id: "op1".into(),
                phase: Phase::Replaced,
                source_run: None,
                reserved_run: 1,
                pane: 1,
                source_pid: 10,
                worker: None,
                session: Some(1),
                source_started: Some("fixture-old".into()),
                replacement: Some(ProcessIdentity {
                    pid: 20,
                    started: "fixture-new".into(),
                }),
                files: Some(manifest.clone()),
                since: 1,
                seen: None,
            };
            let mut claim = store.claim(&record.id, record.rev).unwrap();
            claim.write_operation(Some(op)).unwrap();
            let record = claim.record().clone();
            drop(claim);
            let record_path = fixture.0.join("state").join(format!("{}.json", record.id));
            let record_bytes = fs::read(&record_path).unwrap();
            let mut owner = Owner(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "records::restart_files::tests::publication_boundary_child",
                        "--nocapture",
                    ])
                    .env("MUXA_PUBLICATION_PROBE_DIR", &fixture.0)
                    .env("MUXA_PUBLICATION_PROBE_TASK", &record.id)
                    .env(
                        "MUXA_PUBLICATION_PROBE_BOUNDARY",
                        if published {
                            "after-task"
                        } else {
                            "before-task"
                        },
                    )
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .spawn()
                    .unwrap(),
            );
            let mut reader = BufReader::new(owner.0.stdout.take().unwrap());
            let mut line = String::new();
            loop {
                line.clear();
                assert_ne!(
                    reader.read_line(&mut line).unwrap(),
                    0,
                    "publisher exited before barrier"
                );
                if line.contains("publication-held") {
                    break;
                }
            }
            assert!(matches!(
                store.claim(&record.id, record.rev),
                Err(StoreError::Busy(_))
            ));
            assert_eq!(
                fs::read(fixture.live("TASK.md")).unwrap(),
                if published {
                    b"new task notes\n"
                } else {
                    b"old task notes\n"
                }
            );
            assert_eq!(
                fs::read(fixture.live("HANDOFF.md")).unwrap(),
                b"old handoff\n"
            );
            assert_eq!(fixture.staged("op1", "TASK.next").exists(), !published);
            assert!(fixture.staged("op1", "HANDOFF.next").exists());
            let task_inode = fs::metadata(fixture.live("TASK.md")).unwrap().ino();
            owner.0.kill().unwrap();
            assert!(!owner.0.wait().unwrap().success());
            let recovered = Store::open(fixture.0.join("state")).unwrap();
            let _claim = recovered.claim(&record.id, record.rev).unwrap();
            assert_eq!(fs::read(&record_path).unwrap(), record_bytes);
            assert_eq!(
                recovered
                    .get(&record.id)
                    .unwrap()
                    .restart_operation()
                    .unwrap()
                    .unwrap()
                    .phase,
                Phase::Replaced
            );
            if edited {
                fs::write(fixture.live("HANDOFF.md"), b"edited after crash\n").unwrap();
                assert!(matches!(
                    reconcile(&fixture.0, &manifest),
                    Err(Error::Conflict("HANDOFF.md"))
                ));
                assert_eq!(
                    fs::read(fixture.live("HANDOFF.md")).unwrap(),
                    b"edited after crash\n"
                );
            } else {
                reconcile(&fixture.0, &manifest).unwrap();
                let handoff_inode = fs::metadata(fixture.live("HANDOFF.md")).unwrap().ino();
                reconcile(&fixture.0, &manifest).unwrap();
                assert_eq!(
                    fs::metadata(fixture.live("HANDOFF.md")).unwrap().ino(),
                    handoff_inode
                );
                assert_eq!(
                    fs::read(fixture.live("HANDOFF.md")).unwrap(),
                    b"new handoff\n"
                );
            }
            if published {
                assert_eq!(
                    fs::metadata(fixture.live("TASK.md")).unwrap().ino(),
                    task_inode
                );
            }
            assert_eq!(
                fs::read(fixture.live("TASK.md")).unwrap(),
                b"new task notes\n"
            );
            assert_eq!(fs::read(&record_path).unwrap(), record_bytes);
        }
    }

    #[test]
    fn changed_merge_baseline_or_live_file_is_never_overwritten() {
        let fixture = Fixture::new();
        fixture.write_old();
        let baseline = read_current(&fixture.0, "TASK.md").unwrap().unwrap();
        fs::write(fixture.live("TASK.md"), b"edited notes\n").unwrap();
        assert!(matches!(
            stage_from(
                &fixture.0,
                "stale",
                Some(&baseline),
                Some(b"old handoff\n"),
                b"new task\n",
                b"new handoff\n"
            ),
            Err(Error::Conflict("TASK.md"))
        ));
        assert_eq!(
            fs::read(fixture.live("TASK.md")).unwrap(),
            b"edited notes\n"
        );
        let manifest = fixture.manifest();
        fs::write(fixture.live("HANDOFF.md"), b"edited handoff\n").unwrap();
        assert!(matches!(
            reconcile(&fixture.0, &manifest),
            Err(Error::Conflict("HANDOFF.md"))
        ));
        assert_eq!(
            fs::read(fixture.live("TASK.md")).unwrap(),
            b"edited notes\n"
        );
        assert_eq!(
            fs::read(fixture.live("HANDOFF.md")).unwrap(),
            b"edited handoff\n"
        );
    }

    #[test]
    fn missing_and_empty_old_files_have_distinct_manifest_bases() {
        let fixture = Fixture::new();
        assert_eq!(read_current(&fixture.0, "TASK.md").unwrap(), None);
        fs::create_dir(fixture.0.join(".muxa")).unwrap();
        fs::write(fixture.live("HANDOFF.md"), b"").unwrap();
        let manifest = fixture.manifest();
        assert_eq!(manifest.files[0].old, None);
        assert_eq!(manifest.files[1].old, Some(digest(b"")));
        reconcile(&fixture.0, &manifest).unwrap();
        assert_eq!(
            fs::read(fixture.live("TASK.md")).unwrap(),
            b"new task notes\n"
        );
    }

    #[test]
    fn invalid_bounds_utf8_nul_and_nonregular_files_fail_before_publication() {
        let fixture = Fixture::new();
        for bytes in [vec![b'x'; MAX_FILE + 1], vec![0xff], b"a\0b".to_vec()] {
            assert!(matches!(
                stage(&fixture.0, "op", &bytes, b"handoff"),
                Err(Error::Content("TASK.md"))
            ));
            assert!(!fixture.live("TASK.md").exists());
        }
        let bytes = "é".repeat(MAX_FILE / 2).into_bytes();
        let manifest = stage(&fixture.0, "bounded", &bytes, b"handoff").unwrap();
        assert_eq!(manifest.files[0].new.bytes, MAX_FILE as u64);
        fs::create_dir(fixture.live("TASK.md")).unwrap();
        assert!(matches!(
            read_current(&fixture.0, "TASK.md"),
            Err(Error::Content("TASK.md"))
        ));
        fs::remove_dir(fixture.live("TASK.md")).unwrap();
        assert!(
            std::process::Command::new("mkfifo")
                .arg(fixture.live("TASK.md"))
                .status()
                .unwrap()
                .success()
        );
        assert!(matches!(
            read_current(&fixture.0, "TASK.md"),
            Err(Error::Content("TASK.md"))
        ));
    }

    #[test]
    fn symlink_parents_and_live_files_cannot_redirect_publication() {
        let fixture = Fixture::new();
        let outside = Fixture::new();
        fs::write(outside.0.join("marker"), b"outside\n").unwrap();
        symlink(&outside.0, fixture.0.join(".muxa")).unwrap();
        assert!(stage(&fixture.0, "op1", b"task", b"handoff").is_err());
        fs::remove_file(fixture.0.join(".muxa")).unwrap();
        fixture.write_old();
        symlink(&outside.0, fixture.0.join(".muxa/restarts")).unwrap();
        assert!(stage(&fixture.0, "op1", b"task", b"handoff").is_err());
        fs::remove_file(fixture.0.join(".muxa/restarts")).unwrap();
        let manifest = fixture.manifest();
        fs::remove_file(fixture.live("TASK.md")).unwrap();
        symlink(outside.0.join("marker"), fixture.live("TASK.md")).unwrap();
        assert!(reconcile(&fixture.0, &manifest).is_err());
        assert_eq!(fs::read(outside.0.join("marker")).unwrap(), b"outside\n");
        assert_eq!(
            fs::read(fixture.live("HANDOFF.md")).unwrap(),
            b"old handoff\n"
        );
    }

    #[test]
    fn parent_identity_swaps_and_corrupt_stages_block_recovery() {
        let fixture = Fixture::new();
        fixture.write_old();
        let manifest = fixture.manifest();
        fs::write(fixture.staged("op1", "HANDOFF.next"), b"wrong stage\n").unwrap();
        assert!(reconcile(&fixture.0, &manifest).is_err());
        assert_eq!(
            fs::read(fixture.live("TASK.md")).unwrap(),
            b"old task notes\n"
        );
        fs::rename(fixture.0.join(".muxa"), fixture.0.join("moved-managed")).unwrap();
        fs::create_dir(fixture.0.join(".muxa")).unwrap();
        assert!(reconcile(&fixture.0, &manifest).is_err());
        assert_eq!(
            fs::read(fixture.0.join("moved-managed/TASK.md")).unwrap(),
            b"old task notes\n"
        );
    }

    #[test]
    fn malformed_manifest_and_stage_symlink_are_not_followed() {
        let fixture = Fixture::new();
        let manifest = fixture.manifest();
        let mut bad = manifest.clone();
        bad.version = 99;
        assert!(matches!(verify(&fixture.0, &bad), Err(Error::Invalid)));
        let mut bad = manifest.clone();
        bad.files[0].name = "../TASK.md".into();
        assert!(matches!(verify(&fixture.0, &bad), Err(Error::Invalid)));
        bad = manifest.clone();
        bad.files[0].stage = "../TASK.next".into();
        assert!(matches!(verify(&fixture.0, &bad), Err(Error::Invalid)));
        bad = manifest.clone();
        bad.files[0].new.sha256 = "0".repeat(64);
        assert!(verify(&fixture.0, &bad).is_err());
        bad = manifest.clone();
        bad.operation = "../escape".into();
        assert!(matches!(verify(&fixture.0, &bad), Err(Error::Invalid)));
        fs::remove_file(fixture.staged("op1", "TASK.next")).unwrap();
        symlink(
            fixture.staged("op1", "HANDOFF.next"),
            fixture.staged("op1", "TASK.next"),
        )
        .unwrap();
        assert!(reconcile(&fixture.0, &manifest).is_err());
        assert!(!fixture.live("TASK.md").exists());
    }

    #[test]
    fn cleanup_requires_prepared_bases_and_never_deletes_live_files() {
        let fixture = Fixture::new();
        fixture.write_old();
        let manifest = fixture.manifest();
        cleanup_prepared(&fixture.0, &manifest).unwrap();
        assert!(!fixture.0.join(".muxa/restarts/op1").exists());
        assert_eq!(
            fs::read(fixture.live("TASK.md")).unwrap(),
            b"old task notes\n"
        );
        let manifest = fixture.manifest();
        fs::rename(fixture.staged("op1", "TASK.next"), fixture.live("TASK.md")).unwrap();
        assert!(cleanup_prepared(&fixture.0, &manifest).is_err());
        assert!(fixture.staged("op1", "HANDOFF.next").exists());
    }
}
