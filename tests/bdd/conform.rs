//! `conform.feature`: running one probe against the containerised server,
//! then reading its record and the repository's state.

use cucumber::{then, when};
use serde_json::Value;

use super::world::DokWorld;

#[when(expr = "dok conform runs the {word} probe as {word} expecting {word}")]
fn conform_runs(w: &mut DokWorld, probe: String, identity: String, expect: String) {
    w.run_dok(&[
        "conform",
        "--pack",
        "/srv/pack.yaml",
        "--remote",
        "/srv/repo.git",
        "--format",
        "json",
        "--as",
        &identity,
        "--expect",
        &expect,
        "--only",
        &probe,
    ]);
}

#[then(expr = "the note contains {string}")]
fn note_contains(w: &mut DokWorld, text: String) {
    let note = w.record()["note"].as_str().unwrap_or_default();
    assert!(note.contains(&text), "note {note:?} lacks {text:?}");
}

#[then("the outcome is succeeded and the previous state was restored")]
fn outcome_succeeded_restored(w: &mut DokWorld) {
    let r = w.record();
    assert_eq!(r["outcome"].as_str(), Some("succeeded"), "{r}");
    assert_eq!(r["restored"]["kind"].as_str(), Some("restored"), "{r}");
}

#[then(expr = "the outcome is refused by {string}")]
fn outcome_refused_by(w: &mut DokWorld, mechanism: String) {
    let r = w.record();
    assert_eq!(r["outcome"].as_str(), Some("refused"), "{r}");
    assert_eq!(r["mechanism"]["kind"].as_str(), Some("named"), "{r}");
    assert_eq!(
        r["mechanism"]["mechanism"].as_str(),
        Some(mechanism.as_str()),
        "{r}"
    );
}

#[then("the outcome is refused by an unidentified mechanism")]
fn outcome_refused_unidentified(w: &mut DokWorld) {
    let r = w.record();
    assert_eq!(r["outcome"].as_str(), Some("refused"), "{r}");
    assert_eq!(r["mechanism"]["kind"].as_str(), Some("unidentified"), "{r}");
}

#[then("the outcome is not-run")]
fn outcome_not_run(w: &mut DokWorld) {
    assert_eq!(
        w.record()["outcome"].as_str(),
        Some("not-run"),
        "{}",
        w.record()
    );
}

/// One call: `verify-repo` prints where main and side point and which
/// tags exist; it must equal what provisioning recorded.
#[then("the repository is unchanged")]
fn repository_unchanged(w: &mut DokWorld) {
    let out = w.container().exec(&["verify-repo"], None);
    assert_eq!(out.code, 0, "verify-repo: {}", out.stderr);
    let now: Value = serde_json::from_str(&out.stdout).expect("verify-repo prints JSON");
    assert_eq!(&now, w.seed.as_ref().unwrap(), "repository state changed");
}
