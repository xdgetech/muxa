use std::process::Command;

use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("muxa-images-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn run(dir: &Path, program: &str, args: &[&str]) {
    let out = Command::new(program)
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{program} {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A repo with a tracked logo, screenshots in an ignored folder, an ignored
/// secret, and a symlink to an image outside the repo.
fn repo(name: &str) -> PathBuf {
    let dir = temp(name).join("app");
    std::fs::create_dir_all(&dir).unwrap();
    run(&dir, "git", &["init", "-q"]);
    write(&dir.join("docs/logo.png"), b"logo");
    write(&dir.join(".gitignore"), b"build/\n.env\n");
    run(&dir, "git", &["add", "docs/logo.png", ".gitignore"]);
    write(&dir.join("build/qa/login.png"), b"old");
    write(&dir.join("build/qa/HOME.PNG"), b"newer");
    write(&dir.join("build/qa/log.txt"), b"not an image");
    write(&dir.join(".env"), b"TOKEN=x");
    // Oldest to newest: the logo, the login, the home screen.
    run(&dir, "touch", &["-t", "202601010000", "docs/logo.png"]);
    run(&dir, "touch", &["-t", "202601020000", "build/qa/login.png"]);
    run(&dir, "touch", &["-t", "202601030000", "build/qa/HOME.PNG"]);
    let outside = dir.parent().unwrap().join("outside.png");
    write(&outside, b"outside");
    std::os::unix::fs::symlink(&outside, dir.join("escape.png")).unwrap();
    dir
}

fn paths(listing: &Listing) -> Vec<&str> {
    listing
        .images
        .iter()
        .map(|i| i.info.path.as_str())
        .collect()
}

#[test]
fn ignored_images_are_listed_newest_first_and_nothing_else() {
    let dir = repo("list");
    let listing = list(&dir.join("build"));
    assert_eq!(listing.root.as_deref(), Some("app"));
    let mut found = paths(&listing);
    // The symlink's target was written last: it comes first.
    assert_eq!(found.remove(0), "escape.png");
    assert_eq!(
        found,
        ["build/qa/HOME.PNG", "build/qa/login.png", "docs/logo.png"]
    );
    assert_eq!(listing.images[1].info.size, Some(5));
    assert!(!listing.truncated);
}

#[test]
fn reads_listed_images_inside_the_repo_only() {
    let dir = repo("read");
    let listing = list(&dir);
    let login = listing.find("build/qa/login.png").unwrap();
    assert_eq!(read(login).unwrap().0, b"old");

    assert_eq!(
        listing.find(".env"),
        Err(ReadError::NotListed(".env".into()))
    );
    assert_eq!(
        read(listing.find("escape.png").unwrap()),
        Err(ReadError::Outside("escape.png".into()))
    );

    write(
        &dir.join("build/qa/login.png"),
        &vec![0; MAX_IMAGE as usize + 1],
    );
    assert_eq!(
        read(login),
        Err(ReadError::TooLarge("build/qa/login.png".into(), MAX_IMAGE))
    );
}

#[test]
fn a_pane_outside_any_repo_lists_nothing() {
    let dir = temp("norepo");
    write(&dir.join("shot.png"), b"x");
    let listing = list(&dir);
    assert!(listing.images.is_empty());
    assert_eq!(listing.root, None);
}
