//! Which paths of a worktree are build output (ADR 0102 §1): the folder each
//! build tool's marker file implies, kept only when the repo ignores it.
//! Blocking.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A marker deeper than this many folders is a fixture, not a project.
const MAX_DEPTH: usize = 3;
const MAX_PATHS: usize = 32;

/// A marker file and the build folder its tool makes beside it.
const MARKERS: [(&str, &str); 6] = [
    ("Cargo.toml", "target"),
    ("Package.swift", ".build"),
    ("build.gradle", "build"),
    ("build.gradle.kts", "build"),
    ("settings.gradle", ".gradle"),
    ("settings.gradle.kts", ".gradle"),
];
const NEXT_MARKER: &str = "package.json";

/// The build paths of `place`, relative: the found ones and `extra`, each
/// only when git ignores it. An error leaves the place building in place.
pub fn of(place: &Path, extra: &[PathBuf]) -> Vec<PathBuf> {
    let files = match tracked_markers(place) {
        Ok(files) => files,
        Err(e) => {
            tracing::warn!(place = %place.display(), error = %e, "cannot list build markers");
            return Vec::new();
        }
    };
    let mut found = candidates(&files, |f| uses_next(&place.join(f)));
    found.extend(extra.iter().cloned());
    let found: Vec<PathBuf> = found.into_iter().take(MAX_PATHS).collect();
    match ignored(place, &found) {
        Ok(kept) => kept,
        Err(e) => {
            tracing::warn!(place = %place.display(), error = %e, "cannot check build paths against git");
            Vec::new()
        }
    }
}

/// The build folders `files` (tracked, relative) imply. Cargo builds a
/// workspace into one `target`, so only the outermost `Cargo.toml` counts.
fn candidates(files: &[PathBuf], uses_next: impl Fn(&Path) -> bool) -> BTreeSet<PathBuf> {
    let cargo_dirs: Vec<&Path> = files
        .iter()
        .filter(|f| f.file_name().is_some_and(|n| n == "Cargo.toml"))
        .filter_map(|f| f.parent())
        .collect();
    let mut out = BTreeSet::new();
    for file in files {
        let (Some(dir), Some(name)) = (file.parent(), file.file_name()) else {
            continue;
        };
        if dir.components().count() > MAX_DEPTH {
            continue;
        }
        let inner_crate =
            name == "Cargo.toml" && cargo_dirs.iter().any(|d| *d != dir && dir.starts_with(d));
        if inner_crate {
            continue;
        }
        if let Some((_, folder)) = MARKERS.iter().find(|(m, _)| name == *m) {
            out.insert(dir.join(folder));
        } else if name == NEXT_MARKER && uses_next(file) {
            out.insert(dir.join(".next"));
        }
    }
    out
}

/// A `package.json` that depends on Next.js.
fn uses_next(file: &Path) -> bool {
    std::fs::read_to_string(file).is_ok_and(|t| t.contains("\"next\":"))
}

fn tracked_markers(place: &Path) -> Result<Vec<PathBuf>, String> {
    let specs = MARKERS
        .iter()
        .map(|(m, _)| *m)
        .chain([NEXT_MARKER])
        .map(|m| format!(":(glob)**/{m}"));
    let out = git(place)
        .args(["ls-files", "-z", "--"])
        .args(specs)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("git ls-files exited with {}", out.status));
    }
    Ok(out
        .stdout
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| PathBuf::from(String::from_utf8_lossy(p).into_owned()))
        .collect())
}

/// The paths git ignores. A link muxa made earlier is kept as it is: its
/// `info/exclude` line names it, and git refuses paths beyond a link.
fn ignored(place: &Path, paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let is_link = |p: &Path| {
        place
            .join(p)
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
    };
    let (links, ask): (Vec<&PathBuf>, Vec<&PathBuf>) = paths.iter().partition(|p| is_link(p));
    let mut kept: BTreeSet<PathBuf> = links.into_iter().cloned().collect();
    if !ask.is_empty() {
        // A trailing `/` lets a folder rule like `target/` match a folder
        // that does not exist yet.
        let out = git(place)
            .args(["check-ignore", "--"])
            .args(ask.iter().map(|p| format!("{}/", p.display())))
            .output()
            .map_err(|e| e.to_string())?;
        // 1: none is ignored.
        if !matches!(out.status.code(), Some(0 | 1)) {
            return Err(format!("git check-ignore exited with {}", out.status));
        }
        kept.extend(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|l| PathBuf::from(l.trim_end_matches('/'))),
        );
    }
    Ok(kept.into_iter().collect())
}

fn git(place: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(place)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null());
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2::adopt::tests::{git as run, repo, temp};

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn markers_name_their_build_folders() {
        let files = paths(&[
            "Cargo.toml",
            "crates/a/Cargo.toml",
            "apple/Kit/Package.swift",
            "android/settings.gradle.kts",
            "android/app/build.gradle.kts",
            "web/package.json",
            "tools/package.json",
            "a/b/c/d/Cargo.toml",
        ]);
        let found = candidates(&files, |f| f.starts_with("web"));
        assert_eq!(
            found.into_iter().collect::<Vec<_>>(),
            paths(&[
                "android/.gradle",
                "android/app/build",
                "apple/Kit/.build",
                "target",
                "web/.next",
            ])
        );
    }

    #[test]
    fn only_ignored_paths_stay() {
        let place = temp("build-paths").join("r");
        repo(&place);
        std::fs::create_dir_all(place.join("apple/Kit")).unwrap();
        std::fs::create_dir_all(place.join("web")).unwrap();
        std::fs::write(place.join("Cargo.toml"), "[package]\n").unwrap();
        std::fs::write(place.join("apple/Kit/Package.swift"), "// swift\n").unwrap();
        std::fs::write(
            place.join("web/package.json"),
            "{\"dependencies\": {\"next\": \"15\"}}\n",
        )
        .unwrap();
        // `.next` is not ignored, so it is not build output here.
        std::fs::write(place.join(".gitignore"), "target/\n.build/\n/out\n").unwrap();
        run(&place, &["add", "."]);
        let found = of(&place, &paths(&["out", "docs"]));
        assert_eq!(found, paths(&["apple/Kit/.build", "out", "target"]));
        // A link made earlier stays a build path.
        std::os::unix::fs::symlink(place.join("README.md"), place.join("web/.next")).unwrap();
        assert!(of(&place, &[]).contains(&PathBuf::from("web/.next")));
    }
}
