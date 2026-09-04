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
