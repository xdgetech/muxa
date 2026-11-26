//! A worktree's build output on the build disk (ADR 0102 §1): each build
//! path (`build_paths`) becomes a link to `<dir>/<workspace>/<place name>/<path>`. Blocking.

use std::io;
use std::path::{Path, PathBuf};

use crate::records::build::BuildDisk;
use crate::records::build_access::{self, Access};

/// Where `rel` of `place` builds: `<dir>/<workspace>/<place name>/<rel, '/' as '-'>`.
pub fn target(disk: &BuildDisk, workspace: &str, place: &Path, rel: &Path) -> Option<PathBuf> {
    let name = place.file_name()?;
    let flat = rel.to_string_lossy().replace('/', "-");
    Some(disk.dir.join(workspace).join(name).join(flat))
}

/// Links each build path of `place` to the build disk. Returns the build
/// folders it links to. A path that is a real folder, or whose parent is
/// not in this place, stays as it is. With no build disk mounted, nothing.
/// A disk muxa may not write to, or that does not answer in time,
/// links nothing and returns the message for the user.
pub fn link(disk: &BuildDisk, workspace: &str, place: &Path) -> Result<Vec<PathBuf>, String> {
    match build_access::check(&disk.dir, build_access::WAIT)? {
        Access::Writable => {}
        Access::Missing => {
            tracing::warn!(dir = %disk.dir.display(), "the build disk is not there; builds stay in the place");
            return Ok(Vec::new());
        }
    }
    let mut out = Vec::new();
    for rel in &super::build_paths::of(place, &disk.extra_paths) {
        let Some(to) = target(disk, workspace, place, rel) else {
            continue;
        };
        match link_one(&place.join(rel), &to) {
            Ok(true) => {
                if let Err(e) = super::git::exclude_line(place, &exclude_entry(rel)) {
                    tracing::warn!(error = %e, "cannot exclude a build link from git");
                }
                out.push(to);
            }
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(path = %place.join(rel).display(), error = %e, "cannot link a build folder")
            }
        }
    }
    Ok(out)
}

/// `/target`: the link itself, which a `target/` rule in `.gitignore` does
/// not match.
fn exclude_entry(rel: &Path) -> String {
    format!("/{}", rel.to_string_lossy())
}

/// True when `at` links to `to` now.
fn link_one(at: &Path, to: &Path) -> io::Result<bool> {
    match at.symlink_metadata() {
        Ok(m) if m.file_type().is_symlink() => Ok(std::fs::read_link(at)? == to),
        Ok(_) => {
            tracing::info!(path = %at.display(), "a build folder is already there; it stays");
            Ok(false)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if !at.parent().is_some_and(Path::is_dir) {
                return Ok(false);
            }
            std::fs::create_dir_all(to)?;
            std::os::unix::fs::symlink(to, at)?;
            Ok(true)
        }
        Err(e) => Err(e),
    }
}

/// Removes build folders muxa recorded, and their place folder on the
/// build disk when it becomes empty. A link or a relative path is skipped.
pub fn remove(build: &[PathBuf]) -> Result<(), String> {
    let mut problems = Vec::new();
    for dir in build {
        let is_link = dir
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink());
        if !dir.is_absolute() || is_link {
            tracing::warn!(path = %dir.display(), "not a build folder muxa made; kept");
            continue;
        }
        match std::fs::remove_dir_all(dir) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => problems.push(format!("cannot remove {}: {e}", dir.display())),
        }
        if let Some(parent) = dir.parent() {
            // Fails, as it should, while another build folder is there.
            let _ = std::fs::remove_dir(parent);
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2::adopt::tests::{git, repo, temp};

    fn disk(dir: &Path) -> BuildDisk {
        BuildDisk {
            dir: dir.to_owned(),
            extra_paths: Vec::new(),
        }
    }

    /// A repo with a Cargo crate and a Swift package, whose build folders
    /// it ignores.
    fn project(place: &Path) {
        repo(place);
        std::fs::create_dir_all(place.join("apple/Kit")).unwrap();
        std::fs::write(place.join("Cargo.toml"), "[package]\n").unwrap();
        std::fs::write(place.join("apple/Kit/Package.swift"), "// swift\n").unwrap();
        std::fs::write(place.join(".gitignore"), "target/\n.build/\n").unwrap();
        git(place, &["add", "."]);
        git(
            place,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "-m",
                "p",
            ],
        );
    }

    #[test]
    fn a_worktree_links_its_build_paths_and_stays_clean() {
        let root = temp("build-link");
        let ssd = root.join("ssd");
        std::fs::create_dir_all(&ssd).unwrap();
        let place = root.join("r");
        project(&place);
        let made = link(&disk(&ssd), "w", &place).unwrap();
        assert_eq!(
            made,
            vec![ssd.join("w/r/apple-Kit-.build"), ssd.join("w/r/target")]
        );
        assert_eq!(std::fs::read_link(place.join("target")).unwrap(), made[1]);
        assert!(made[0].is_dir());
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&place)
            .args(["status", "--porcelain"])
            .output()
            .unwrap();
        assert!(
            status.stdout.is_empty(),
            "{:?}",
            String::from_utf8_lossy(&status.stdout)
        );
        // A second start finds the same links.
        assert_eq!(link(&disk(&ssd), "w", &place).unwrap(), made);
    }

    #[test]
    fn a_real_folder_or_a_missing_parent_stays_and_no_disk_means_no_link() {
        let root = temp("build-keep");
        let ssd = root.join("ssd");
        std::fs::create_dir_all(&ssd).unwrap();
        let place = root.join("r");
        project(&place);
        std::fs::create_dir_all(place.join("target")).unwrap();
        assert_eq!(
            link(&disk(&ssd), "w", &place).unwrap(),
            vec![ssd.join("w/r/apple-Kit-.build")]
        );
        assert!(
            !place
                .join("target")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let other = root.join("o");
        project(&other);
        assert!(
            link(&disk(&root.join("not-mounted")), "w", &other)
                .unwrap()
                .is_empty()
        );
        assert!(other.join("target").symlink_metadata().is_err());
        git(&other, &["status"]);
    }

    #[test]
    fn a_disk_muxa_may_not_write_to_links_nothing_and_says_why() {
        use std::os::unix::fs::PermissionsExt;
        let root = temp("build-denied");
        let ssd = root.join("ssd");
        std::fs::create_dir_all(&ssd).unwrap();
        let place = root.join("r");
        project(&place);
        std::fs::set_permissions(&ssd, std::fs::Permissions::from_mode(0o500)).unwrap();
        let got = link(&disk(&ssd), "w", &place);
        std::fs::set_permissions(&ssd, std::fs::Permissions::from_mode(0o700)).unwrap();
        let why = got.unwrap_err();
        assert!(why.contains(&ssd.display().to_string()), "{why}");
        assert!(place.join("target").symlink_metadata().is_err());
    }

    #[test]
    fn remove_takes_the_folder_and_its_empty_parent_but_not_a_link() {
        let root = temp("build-remove");
        let ssd = root.join("ssd");
        let made = ssd.join("w/r/target");
        std::fs::create_dir_all(made.join("debug")).unwrap();
        let kept = root.join("kept");
        std::fs::create_dir_all(&kept).unwrap();
        let link = root.join("link");
        std::os::unix::fs::symlink(&kept, &link).unwrap();
        remove(&[made.clone(), link.clone()]).unwrap();
        assert!(!made.exists());
        assert!(!ssd.join("w/r").exists());
        assert!(kept.exists() && link.exists());
    }
}
