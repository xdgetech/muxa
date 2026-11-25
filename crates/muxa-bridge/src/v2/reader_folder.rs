//! A Reader folder as one zip (ADR 0101): the documents and images the
//! Reader lists under a folder, and nothing else. Entries are stored, not
//! compressed: images are compressed already and documents are small.

use std::io::{Cursor, Write};
use std::path::Path;

use muxa_protocol::DocKind;
use muxa_server::TaskError;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use super::reader::{MAX_SENT, blocking, read_error};
use crate::docs::{self, files::Doc, images::Image};

/// The most files in a folder's zip.
const MAX_FILES: usize = 1000;

/// The listed documents (HTML too) and images under `folder`, subfolders
/// included, as one zip under a folder of the same name. `folder` is only
/// compared with listed paths, never opened.
pub async fn zip(dir: &Path, folder: &str) -> Result<Vec<u8>, TaskError> {
    let (dir, folder) = (dir.to_path_buf(), folder.trim_end_matches('/').to_owned());
    blocking(move || zip_folder(&dir, &folder)).await
}

/// A listed file under the folder, before it is read.
enum Listed<'a> {
    Doc(&'a Doc),
    Image(&'a Image),
}

/// One file of the zip: its path inside the top folder, bytes and mtime.
struct Member {
    path: String,
    bytes: Vec<u8>,
    /// Unix seconds.
    mtime: Option<u64>,
}

fn zip_folder(dir: &Path, folder: &str) -> Result<Vec<u8>, TaskError> {
    let doc_listing = docs::files::for_folder(dir, true);
    let image_listing = docs::images::for_folder(dir);
    let files = listed_under(folder, &doc_listing.docs, &image_listing.images);
    let shown = if folder.is_empty() {
        "This folder"
    } else {
        folder
    };
    check_limits(shown, &files)?;
    let members = files
        .iter()
        .map(|(path, f)| read_member(folder, path, f))
        .collect::<Result<Vec<_>, TaskError>>()?;
    let top = top_name(folder, doc_listing.root.as_deref(), dir);
    let zip = write(&top, &members).map_err(|e| {
        tracing::warn!(error = %e, files = members.len(), "a v2 Reader folder cannot be zipped");
        TaskError::Failed(format!("the folder cannot be zipped: {e}"))
    })?;
    if zip.len() as u64 > MAX_SENT {
        return Err(too_large(shown, zip.len() as u64));
    }
    Ok(zip)
}

/// The listed files whose path is under `folder`; empty `folder`: all of them.
fn listed_under<'a>(
    folder: &str,
    docs: &'a [Doc],
    images: &'a [Image],
) -> Vec<(&'a str, Listed<'a>)> {
    let under = |path: &str| {
        folder.is_empty()
            || path
                .strip_prefix(folder)
                .is_some_and(|rest| rest.starts_with('/'))
    };
    docs.iter()
        .filter(|d| d.info.kind == DocKind::Repo && under(&d.info.path))
        .map(|d| (d.info.path.as_str(), Listed::Doc(d)))
        .chain(
            images
                .iter()
                .filter(|i| under(&i.info.path))
                .map(|i| (i.info.path.as_str(), Listed::Image(i))),
        )
        .collect()
}

/// Refuses the whole folder, so a zip is never missing a file.
fn check_limits(shown: &str, files: &[(&str, Listed)]) -> Result<(), TaskError> {
    if files.is_empty() {
        return Err(TaskError::Invalid(format!(
            "{shown} has no documents or images the Reader lists"
        )));
    }
    if files.len() > MAX_FILES {
        return Err(TaskError::Invalid(format!(
            "{shown} has {} files. Save Folder sends at most {MAX_FILES}. Save a folder inside it.",
            files.len()
        )));
    }
    let size: u64 = files
        .iter()
        .map(|(_, f)| match f {
            Listed::Doc(d) => std::fs::metadata(&d.file).map_or(0, |m| m.len()),
            Listed::Image(i) => i.info.size.unwrap_or(0),
        })
        .sum();
    if size > MAX_SENT {
        return Err(too_large(shown, size));
    }
    Ok(())
}

fn too_large(shown: &str, bytes: u64) -> TaskError {
    TaskError::Invalid(format!(
        "{shown} has {} MB of files. Save Folder sends at most {} MB. Save a folder inside it.",
        bytes.div_ceil(1 << 20),
        MAX_SENT >> 20
    ))
}

/// Reads a listed file with the Reader's checks: it must stay inside its root.
fn read_member(folder: &str, path: &str, file: &Listed) -> Result<Member, TaskError> {
    let (bytes, mtime) = match file {
        Listed::Doc(d) => docs::files::read_raw(d),
        Listed::Image(i) => docs::images::read(i),
    }
    .map_err(read_error)?;
    let inside = path
        .strip_prefix(folder)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(path);
    Ok(Member {
        path: inside.to_owned(),
        bytes,
        mtime,
    })
}

/// The zip's one folder: the folder's own name, else the repo's or the
/// Task folder's.
fn top_name(folder: &str, root: Option<&str>, dir: &Path) -> String {
    folder
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .or(root)
        .map(str::to_owned)
        .or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "folder".into())
}

/// The zip of `members`, all under the folder `top`.
fn write(top: &str, members: &[Member]) -> zip::result::ZipResult<Vec<u8>> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for m in members {
        let mut options =
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        if let Some(time) = m.mtime.and_then(zip_time) {
            options = options.last_modified_time(time);
        }
        zip.start_file(format!("{top}/{}", m.path), options)?;
        zip.write_all(&m.bytes)?;
    }
    Ok(zip.finish()?.into_inner())
}

/// The host's local time, as unzip tools show it. Outside 1980-2107 or
/// with no time zone: none, and the zip's default time.
fn zip_time(unix: u64) -> Option<DateTime> {
    let seconds = i64::try_from(unix).ok()?;
    let at = jiff::Timestamp::from_second(seconds)
        .ok()?
        .to_zoned(jiff::tz::TimeZone::system());
    let part = |n: i8| u8::try_from(n).ok();
    DateTime::from_date_and_time(
        u16::try_from(at.year()).ok()?,
        part(at.month())?,
        part(at.day())?,
        part(at.hour())?,
        part(at.minute())?,
        part(at.second())?,
    )
    .ok()
}
