//! One JSON file per record in `<state dir>/assistant/<kind>/` (ADR 0046 §2,
//! ADR 0047 §5, ADR 0048 §1–§2), with the item store's lock and atomic write.
use std::fs::{self, DirBuilder};
use std::marker::PhantomData;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use muxa_protocol::{AssistantRequest, Capture, Correction, Plan, Policy, WorkItem};
use serde::de::{DeserializeOwned, Error as _};
use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::records::store::Lock;
use crate::routines::store::{StateError, read_json, write_json};

pub const FORMAT_VERSION: u32 = 1;
const ROOT: &str = "assistant";
const DIR_MODE: u32 = 0o700;
/// A request with 10 results of 4 KiB summaries and their findings fits.
const MAX_FILE: u64 = 1 << 20;
/// Ids tried before giving up on a free one.
const ID_TRIES: u32 = 16;

/// A file's fields that are not on the wire: muxa's own bookkeeping and a newer muxa's fields.
pub type Extra = serde_json::Map<String, serde_json::Value>;

/// A record kind: its folder and its id prefix.
pub trait Kind: Serialize + DeserializeOwned {
    const DIR: &'static str;
    const PREFIX: &'static str;
    /// The file's key for its format version.
    const FORMAT_KEY: &'static str = "version";
    fn id(&self) -> &str;
}

impl Kind for Capture {
    const DIR: &'static str = "captures";
    const PREFIX: &'static str = "c-";
    fn id(&self) -> &str {
        &self.id
    }
}

impl Kind for WorkItem {
    const DIR: &'static str = "work";
    const PREFIX: &'static str = "w-";
    fn id(&self) -> &str {
        &self.id
    }
}

impl Kind for AssistantRequest {
    const DIR: &'static str = "requests";
    const PREFIX: &'static str = "q-";
    /// A request has a `version` of its own.
    const FORMAT_KEY: &'static str = "format";
    fn id(&self) -> &str {
        &self.id
    }
}

impl Kind for Plan {
    const DIR: &'static str = "plans";
    const PREFIX: &'static str = "p-";
    fn id(&self) -> &str {
        &self.id
    }
}

impl Kind for Policy {
    const DIR: &'static str = "policies";
    const PREFIX: &'static str = "y-";
    fn id(&self) -> &str {
        &self.id
    }
}

impl Kind for Correction {
    const DIR: &'static str = "corrections";
    const PREFIX: &'static str = "x-";
    fn id(&self) -> &str {
        &self.id
    }
}

/// The name of a folder's own lock: no record id has it.
const FOLDER_LOCK: &str = "folder";

/// The file: the wire record, its format version under `T::FORMAT_KEY`, and
/// fields a newer muxa wrote, kept on rewrite.
#[derive(Debug)]
struct Stored<T> {
    version: u32,
    record: T,
    extra: serde_json::Map<String, serde_json::Value>,
}

impl<T: Kind> Serialize for Stored<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let serde_json::Value::Object(mut map) =
            serde_json::to_value(&self.record).map_err(S::Error::custom)?
        else {
            return Err(S::Error::custom("a record is not a JSON object"));
        };
        for (key, value) in &self.extra {
            if !map.contains_key(key) {
                map.insert(key.clone(), value.clone());
            }
        }
        map.insert(T::FORMAT_KEY.into(), self.version.into());
        map.serialize(s)
    }
}

impl<'de, T: Kind> Deserialize<'de> for Stored<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let mut map = serde_json::Map::deserialize(d)?;
        let version = map
            .remove(T::FORMAT_KEY)
            .and_then(|v| v.as_u64())
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| D::Error::custom(format!("no format version in {:?}", T::FORMAT_KEY)))?;
        let value = serde_json::Value::Object(map);
        let record = T::deserialize(&value).map_err(D::Error::custom)?;
        // The keys the record does not write back belong to a newer muxa.
        let known = serde_json::to_value(&record).map_err(D::Error::custom)?;
        let serde_json::Value::Object(map) = value else {
            return Err(D::Error::custom("a record is not a JSON object"));
        };
        let extra = map
            .into_iter()
            .filter(|(key, _)| known.get(key).is_none())
            .collect();
        Ok(Self {
            version,
            record,
            extra,
        })
    }
}

/// One kind's folder.
#[derive(Debug, Clone)]
pub struct Shelf<T> {
    dir: PathBuf,
    kind: PhantomData<fn() -> T>,
}

impl<T: Kind> Shelf<T> {
    /// `<state dir>/assistant/<kind>`, created with mode 0700.
    pub fn open(state_dir: &Path) -> Result<Self, StateError> {
        let dir = state_dir.join(ROOT).join(T::DIR);
        DirBuilder::new()
            .recursive(true)
            .mode(DIR_MODE)
            .create(&dir)
            .map_err(|source| StateError::Io {
                action: "create",
                path: dir.clone(),
                source,
            })?;
        Ok(Self {
            dir,
            kind: PhantomData,
        })
    }

    pub fn get(&self, id: &str) -> Result<Option<T>, StateError> {
        check_id::<T>(id)?;
        Ok(self.read(id)?.map(|s| s.record))
    }

    /// [`Shelf::get`], with the file's fields that are not on the wire.
    pub fn get_with_extra(&self, id: &str) -> Result<Option<(T, Extra)>, StateError> {
        check_id::<T>(id)?;
        Ok(self.read(id)?.map(|s| (s.record, s.extra)))
    }

    /// Every readable record. An unreadable file is logged by id and skipped.
    pub fn all(&self) -> Result<Vec<T>, StateError> {
        let entries = fs::read_dir(&self.dir).map_err(|source| StateError::Io {
            action: "read",
            path: self.dir.clone(),
            source,
        })?;
        let mut out = Vec::new();
        for entry in entries {
            let name = match entry {
                Ok(e) => e.file_name(),
                Err(e) => {
                    tracing::warn!(error = %e, kind = T::DIR, "cannot read an assistant file entry");
                    continue;
                }
            };
            let Some(id) = name.to_str().and_then(|n| n.strip_suffix(".json")) else {
                continue;
            };
            if check_id::<T>(id).is_err() {
                continue;
            }
            match self.read(id) {
                Ok(Some(s)) => out.push(s.record),
                Ok(None) => {}
                Err(e) => tracing::warn!(id, error = %e, "skipping an assistant file"),
            }
        }
        Ok(out)
    }

    /// Applies `change` to record `id` under its lock and writes the result
    /// atomically. `None` when there is no such record: `change` does not
    /// run. Nothing is written when `change` fails.
    pub fn update<O, E: From<StateError>>(
        &self,
        id: &str,
        change: impl FnOnce(&mut T) -> Result<O, E>,
    ) -> Result<Option<O>, E> {
        self.update_with_extra(id, |record, _| change(record))
    }

    /// [`Shelf::update`], with the file's fields that are not on the wire:
    /// muxa's own bookkeeping and a newer muxa's fields.
    pub fn update_with_extra<O, E: From<StateError>>(
        &self,
        id: &str,
        change: impl FnOnce(&mut T, &mut serde_json::Map<String, serde_json::Value>) -> Result<O, E>,
    ) -> Result<Option<O>, E> {
        check_id::<T>(id)?;
        let _lock = Lock::acquire(&self.dir, id, true).map_err(StateError::from)?;
        let Some(mut stored) = self.read(id)? else {
            return Ok(None);
        };
        let out = change(&mut stored.record, &mut stored.extra)?;
        stored.version = FORMAT_VERSION;
        write_json(&self.dir, id, &stored, MAX_FILE)?;
        Ok(Some(out))
    }

    /// Deletes record `id` under its lock; a missing one is no error.
    pub fn remove(&self, id: &str) -> Result<(), StateError> {
        check_id::<T>(id)?;
        let _lock = Lock::acquire(&self.dir, id, true)?;
        let path = self.dir.join(format!("{id}.json"));
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(StateError::Io {
                action: "remove",
                path,
                source,
            }),
        }
    }

    /// Removes record `id` under its lock when `check` passes on it. `None`
    /// when there is no such record: `check` does not run.
    pub fn remove_if<E: From<StateError>>(
        &self,
        id: &str,
        check: impl FnOnce(&T) -> Result<(), E>,
    ) -> Result<Option<T>, E> {
        check_id::<T>(id)?;
        let _lock = Lock::acquire(&self.dir, id, true).map_err(StateError::from)?;
        let Some(stored) = self.read(id)? else {
            return Ok(None);
        };
        check(&stored.record)?;
        let path = self.dir.join(format!("{id}.json"));
        match fs::remove_file(&path) {
            Ok(()) => Ok(Some(stored.record)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(StateError::Io {
                action: "remove",
                path,
                source,
            }
            .into()),
        }
    }

    /// The whole folder's lock, for a write that checks a limit across its
    /// records. Take it before any record's lock.
    pub(crate) fn lock_folder(&self) -> Result<Lock, StateError> {
        Ok(Lock::acquire(&self.dir, FOLDER_LOCK, true)?)
    }

    /// Writes a new record under a fresh random id. `make` gets the id.
    pub fn create<E: From<StateError>>(
        &self,
        make: impl FnOnce(String) -> Result<T, E>,
    ) -> Result<T, E> {
        self.create_with_extra(|id| make(id).map(|r| (r, Extra::new())))
    }

    /// [`Shelf::create`], with fields that are not on the wire, in the same write.
    pub fn create_with_extra<E: From<StateError>>(
        &self,
        make: impl FnOnce(String) -> Result<(T, Extra), E>,
    ) -> Result<T, E> {
        for _ in 0..ID_TRIES {
            let id = format!(
                "{}{:08x}",
                T::PREFIX,
                getrandom::u32().map_err(StateError::from)?
            );
            let _lock = Lock::acquire(&self.dir, &id, true).map_err(StateError::from)?;
            if self.read(&id)?.is_some() {
                continue;
            }
            let (record, extra) = make(id.clone())?;
            return self.write_new(&id, record, extra);
        }
        Err(StateError::Unreadable(T::DIR.to_owned(), "no free id".into()).into())
    }

    /// Writes `record` as the new file `id`; the caller holds `id`'s lock.
    fn write_new<E: From<StateError>>(&self, id: &str, record: T, extra: Extra) -> Result<T, E> {
        if record.id() != id {
            return Err(
                StateError::Unreadable(id.to_owned(), "a record under another id".into()).into(),
            );
        }
        let stored = Stored {
            version: FORMAT_VERSION,
            record,
            extra,
        };
        write_json(&self.dir, id, &stored, MAX_FILE)?;
        Ok(stored.record)
    }

    fn read(&self, id: &str) -> Result<Option<Stored<T>>, StateError> {
        let path = self.dir.join(format!("{id}.json"));
        let Some(stored) = read_json::<Stored<T>>(&path, id, MAX_FILE)? else {
            return Ok(None);
        };
        if stored.version != FORMAT_VERSION {
            return Err(StateError::Unreadable(
                id.to_owned(),
                format!("format version {} is not {FORMAT_VERSION}", stored.version),
            ));
        }
        Ok(Some(stored))
    }
}

/// What [`Shelf::<Capture>::capture`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Captured {
    New(Capture),
    /// The client sent this `clientId` before; nothing was written.
    Known(Capture),
}

impl Shelf<Capture> {
    /// Stores the Capture `make` builds, or returns the one already stored
    /// for `client_id`. The id comes from `client_id`'s hash, so a resend
    /// reads one file under that id's lock, and two resends at once make one
    /// Capture.
    pub fn capture<E: From<StateError>>(
        &self,
        client_id: &str,
        make: impl FnOnce(String) -> Result<Capture, E>,
    ) -> Result<Captured, E> {
        for attempt in 0..ID_TRIES {
            let id = capture_id(client_id, attempt);
            let _lock = Lock::acquire(&self.dir, &id, true).map_err(StateError::from)?;
            match self.read(&id)? {
                Some(s) if s.record.client_id == client_id => return Ok(Captured::Known(s.record)),
                // Another clientId has this hash prefix; try the next id.
                Some(_) => continue,
                None => {
                    let record = make(id.clone())?;
                    return self.write_new(&id, record, Extra::new()).map(Captured::New);
                }
            }
        }
        Err(StateError::Unreadable(Capture::DIR.to_owned(), "no free id".into()).into())
    }
}

/// `c-` and the first 8 hex digits of the SHA-256 of the client id and the attempt.
fn capture_id(client_id: &str, attempt: u32) -> String {
    let mut hash = Sha256::new();
    hash.update(client_id.as_bytes());
    hash.update([0]);
    hash.update(attempt.to_be_bytes());
    let hex: String = hash.finalize()[..4]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{}{hex}", Capture::PREFIX)
}

/// An id names a file, so only the kind's prefix and 8 hex digits pass.
fn check_id<T: Kind>(id: &str) -> Result<(), StateError> {
    let ok = id
        .strip_prefix(T::PREFIX)
        .is_some_and(|h| h.len() == 8 && h.bytes().all(|b| b.is_ascii_hexdigit()));
    if ok {
        Ok(())
    } else {
        Err(StateError::Unreadable(
            id.to_owned(),
            format!("not a {} id", T::DIR),
        ))
    }
}

#[cfg(test)]
mod tests;
