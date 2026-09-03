//! The four git attempts, rehearsed against a local bare repository: red
//! (no hook, every attempt goes through and is restored) then green (a
//! pre-receive hook refuses with ruleset wording, every attempt is credited
//! to it).

use std::path::{Path, PathBuf};
use std::process::Command;

use dok::conform::probe::Executor;
use dok::conform::spec::{Attempt, Refusal};
use dok::conform::verdict::{Mechanism, Outcome, Restoration};

fn git(dir: &Path, args: &[&str]) -> String {
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

struct Remote {
    _dir: tempfile::TempDir,
    bare: PathBuf,
    main: String,
    side: String,
}

fn seed() -> Remote {
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

fn refuse_with(bare: &Path, message: &str) {
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

fn executor(remote: &Remote) -> Executor {
    Executor {
        repository: "local/rehearsal".into(),
        remote: remote.bare.to_string_lossy().into_owned(),
        gh: "gh".into(),
    }
}

fn ruleset() -> Vec<Refusal> {
    vec![Refusal {
        mechanism: "ruleset".into(),
        pattern: "GH013".into(),
    }]
}

fn attempts() -> Vec<Attempt> {
    vec![
        Attempt::ForcePush {
            branch: "main".into(),
        },
        Attempt::DirectPush {
            branch: "main".into(),
            path: "docs/PROBE.md".into(),
            signed: false,
        },
        Attempt::DeleteBranch {
            branch: "side".into(),
        },
        Attempt::PushTag {
            tag: "v0.0.1".into(),
        },
    ]
}

fn ref_exists(bare: &Path, r: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(bare)
        .args(["show-ref", "--verify", "--quiet", r])
        .status()
        .unwrap()
        .success()
}

#[test]
fn red_every_attempt_succeeds_and_is_restored() {
    let remote = seed();
    let ex = executor(&remote);
    for attempt in attempts() {
        match ex.attempt(&attempt, &ruleset()) {
            Outcome::Succeeded {
                restored: Restoration::Restored,
                ..
            } => {}
            other => panic!("{attempt:?}: {other:?}"),
        }
    }
    assert_eq!(
        git(&remote.bare, &["rev-parse", "refs/heads/main"]),
        remote.main,
        "main restored"
    );
    assert_eq!(
        git(&remote.bare, &["rev-parse", "refs/heads/side"]),
        remote.side,
        "side recreated"
    );
    assert!(
        !ref_exists(&remote.bare, "refs/tags/v0.0.1"),
        "probe tag deleted"
    );
}

#[test]
fn green_every_attempt_is_refused_and_credited() {
    let remote = seed();
    refuse_with(&remote.bare, "GH013: Repository rule violations found");
    let ex = executor(&remote);
    for attempt in attempts() {
        match ex.attempt(&attempt, &ruleset()) {
            Outcome::Refused {
                mechanism: Mechanism::Named { mechanism },
            } => assert_eq!(mechanism, "ruleset"),
            other => panic!("{attempt:?}: {other:?}"),
        }
    }
    assert_eq!(
        git(&remote.bare, &["rev-parse", "refs/heads/main"]),
        remote.main
    );
}

#[test]
fn a_refusal_the_pack_did_not_name_is_a_finding_not_a_credit() {
    let remote = seed();
    refuse_with(&remote.bare, "no pushes on Fridays");
    let ex = executor(&remote);
    match ex.attempt(
        &Attempt::ForcePush {
            branch: "main".into(),
        },
        &ruleset(),
    ) {
        Outcome::Refused {
            mechanism: Mechanism::Unidentified { response },
        } => assert!(response.contains("Fridays")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn deleting_the_default_branch_is_stopped_by_git_itself_not_by_any_rule() {
    let remote = seed();
    let ex = executor(&remote);
    let refusals = vec![
        Refusal {
            mechanism: "default-branch guard".into(),
            pattern: "refusing to delete the current branch".into(),
        },
        Refusal {
            mechanism: "ruleset".into(),
            pattern: "GH013".into(),
        },
    ];
    match ex.attempt(
        &Attempt::DeleteBranch {
            branch: "main".into(),
        },
        &refusals,
    ) {
        Outcome::Refused {
            mechanism: Mechanism::Named { mechanism },
        } => assert_eq!(mechanism, "default-branch guard"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_unreachable_remote_is_an_error_not_a_refusal() {
    let ex = Executor {
        repository: "x/y".into(),
        remote: "/nonexistent/remote.git".into(),
        gh: "gh".into(),
    };
    assert!(matches!(
        ex.attempt(&Attempt::PushTag { tag: "v0".into() }, &ruleset()),
        Outcome::Errored { .. }
    ));
}
