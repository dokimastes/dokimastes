//! A local bare repository to rehearse push attempts against, shared by the
//! integration tests and the acceptance scenarios.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[derive(Debug)]
pub struct Remote {
    _dir: tempfile::TempDir,
    pub bare: PathBuf,
    pub main: String,
    pub side: String,
}

/// A bare repository with one commit on `main` and the same commit on `side`.
pub fn seed() -> Remote {
    let dir = tempfile::tempdir().unwrap();
    let bare = dir.path().join("remote.git");
    let work = dir.path().join("work");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "--quiet", "--bare", "-b", "main"]);
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "--quiet", "-b", "main"]);
    std::fs::write(work.join("README.md"), "seed\n").unwrap();
    git(&work, &["add", "."]);
    git(&work, &["commit", "--quiet", "-m", "seed"]);
    git(&work, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&work, &["push", "--quiet", "origin", "main:main"]);
    git(&work, &["push", "--quiet", "origin", "main:side"]);
    let main = git(&bare, &["rev-parse", "refs/heads/main"]);
    let side = git(&bare, &["rev-parse", "refs/heads/side"]);
    Remote {
        _dir: dir,
        bare,
        main,
        side,
    }
}

/// Install a pre-receive hook that refuses every push with `message`.
pub fn refuse_with(bare: &Path, message: &str) {
    let hook = bare.join("hooks").join("pre-receive");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\necho \"{message}\" >&2\nexit 1\n"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

pub fn ref_exists(bare: &Path, r: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(bare)
        .args(["show-ref", "--verify", "--quiet", r])
        .status()
        .unwrap()
        .success()
}

/// The known month of history that `tests/docker/provision-history` builds,
/// in a temp directory — the same script, so there is one fixture.
/// Returns the repository path and the `now` the dates are relative to.
pub fn seed_history() -> (tempfile::TempDir, PathBuf, i64) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("history");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let script = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/docker/provision-history"
    );
    let out = Command::new("sh")
        .arg(script)
        .arg(&repo)
        .env("DOK_NOW", now.to_string())
        .output()
        .expect("sh");
    assert!(
        out.status.success(),
        "provision-history: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    (dir, repo, now)
}
