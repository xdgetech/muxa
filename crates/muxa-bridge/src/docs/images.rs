//! Which image files a pane may view, and reading them. Unlike Markdown, files git
//! ignores are listed too: screenshots of test runs live in ignored folders. Only
//! image extensions are listed, so an ignored secret such as `.env` never is (ADR 0010).
//! Everything here blocks (git, the file system); run it on the blocking pool.

use std::path::{Path, PathBuf};
use std::process::Command;

use muxa_protocol::ImageInfo;

use super::files::{self, ReadError};

/// Images listed for one pane at most; the newest are kept.
pub const MAX_IMAGES: usize = 2000;
/// A larger image is refused. Its base64 line stays below the app's 32 MiB limit.
pub const MAX_IMAGE: u64 = 16 << 20;
/// Formats the app decodes. No SVG: it can carry scripts.
const PATTERNS: [&str; 6] = [
    ":(icase)*.png",
    ":(icase)*.jpg",
    ":(icase)*.jpeg",
    ":(icase)*.gif",
    ":(icase)*.webp",
    ":(icase)*.heic",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub info: ImageInfo,
    pub file: PathBuf,
    /// The repo; the file must stay inside it, even through symlinks.
    pub root: PathBuf,
}

#[derive(Debug, Default)]
pub struct Listing {
    pub root: Option<String>,
    pub images: Vec<Image>,
    pub truncated: bool,
}

impl Listing {
    pub fn find(&self, path: &str) -> Result<&Image, ReadError> {
        self.images
            .iter()
            .find(|i| i.info.path == path)
            .ok_or_else(|| ReadError::NotListed(path.to_owned()))
    }
}

/// The images of the repo `path` is in, newest first.
pub fn list(path: &Path) -> Listing {
    let Some(root) = files::repo_root(path) else {
        return Listing::default();
    };
    let mut images: Vec<Image> = repo_images(&root)
        .into_iter()
        .filter_map(|rel| {
            let file = root.join(&rel);
            let meta = std::fs::metadata(&file).ok()?;
            // A tracked file can be deleted in the work tree.
            meta.is_file().then(|| Image {
                info: ImageInfo {
                    path: rel,
                    mtime: files::mtime(&file),
                    size: Some(meta.len()),
                },
                file,
                root: root.clone(),
            })
        })
        .collect();
    images.sort_by(|a, b| {
        b.info
            .mtime
            .cmp(&a.info.mtime)
            .then_with(|| a.info.path.cmp(&b.info.path))
    });
    let truncated = images.len() > MAX_IMAGES;
    images.truncate(MAX_IMAGES);
    Listing {
        root: root.file_name().map(|n| n.to_string_lossy().into_owned()),
        images,
        truncated,
    }
}

/// Tracked and untracked image files, ignored ones included.
fn repo_images(root: &Path) -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--cached", "--others", "--"])
        .args(PATTERNS)
        .output();
    let out = match out {
        Ok(out) if out.status.success() => out,
        Ok(out) => {
            tracing::warn!(status = %out.status, "git ls-files failed for images");
            return Vec::new();
        }
        Err(e) => {
            tracing::warn!(error = %e, "cannot run git to list images");
            return Vec::new();
        }
    };
    let mut paths: Vec<String> = out
        .stdout
        .split(|&b| b == 0)
        .filter(|p| !p.is_empty())
        .filter_map(|p| String::from_utf8(p.to_vec()).ok())
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

/// The bytes of `image` and its mtime.
pub fn read(image: &Image) -> Result<(Vec<u8>, Option<u64>), ReadError> {
    files::read_bytes(&image.file, &image.root, &image.info.path, MAX_IMAGE)
}

#[cfg(test)]
mod tests;
