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

/// The same known month of history `tests/docker/provision-history` builds,
/// in a temp directory. Returns the repository path and the `now` used.
pub fn seed_history() -> (tempfile::TempDir, PathBuf, i64) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("history");
    std::fs::create_dir_all(&repo).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let at = |days: i64| format!("@{} +0000", now - days * 86_400);
    let commit = |days: i64, msg: &str| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args([
                "-c",
                "user.name=h",
                "-c",
                "user.email=h@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                msg,
            ])
            .env("GIT_AUTHOR_DATE", at(days))
            .env("GIT_COMMITTER_DATE", at(days))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    let write = |name: &str, lines: std::ops::RangeInclusive<u32>, append: bool| {
        let body: String = lines.map(|n| format!("{n}\n")).collect();
        let path = repo.join(name);
        if append {
            let mut existing = std::fs::read_to_string(&path).unwrap_or_default();
            existing.push_str(&body);
            std::fs::write(&path, existing).unwrap();
        } else {
            std::fs::write(&path, body).unwrap();
        }
    };
    git(&repo, &["init", "--quiet", "-b", "main"]);
    write("a.rs", 1..=100, false);
    git(&repo, &["add", "a.rs"]);
    commit(20, "add a");
    write("a.rs", 1..=70, false);
    git(&repo, &["add", "a.rs"]);
    commit(15, "trim a");
    git(&repo, &["checkout", "--quiet", "-b", "feature"]);
    write("b.rs", 1..=10, false);
    git(&repo, &["add", "b.rs"]);
    commit(10, "feature part 1");
    write("b.rs", 11..=20, true);
    git(&repo, &["add", "b.rs"]);
    commit(9, "feature part 2");
    git(&repo, &["checkout", "--quiet", "main"]);
    let out = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args([
            "-c",
            "user.name=h",
            "-c",
            "user.email=h@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "merge",
            "--quiet",
            "--no-ff",
            "--no-edit",
            "feature",
        ])
        .env("GIT_AUTHOR_DATE", at(8))
        .env("GIT_COMMITTER_DATE", at(8))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["-c", "tag.gpgsign=false", "tag", "v0.1.0"])
        .env("GIT_COMMITTER_DATE", at(8))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    write("c.rs", 1..=1, false);
    git(&repo, &["add", "c.rs"]);
    commit(3, "Revert \"something\"");
    (dir, repo, now)
}
