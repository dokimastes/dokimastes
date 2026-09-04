//! The four git attempts, rehearsed against a local bare repository: red
//! (no hook, every attempt goes through and is restored) then green (a
//! pre-receive hook refuses with ruleset wording, every attempt is credited
//! to it).

mod common;

use common::{git, ref_exists, refuse_with, seed, Remote};

use dok::conform::probe::Executor;
use dok::conform::spec::{Attempt, Refusal};
use dok::conform::verdict::{Mechanism, Outcome, Restoration};

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
