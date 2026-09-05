//! `assess.feature`: assessing the working tree in the container, then
//! reading verdict, ceiling, findings, refusals and the oracle map.

use cucumber::{then, when};

use super::world::DokWorld;

#[when("dok assess runs on the working tree with the profile")]
fn assess_with_profile(w: &mut DokWorld) {
    assert!(w.profile_yaml.is_some(), "no profile was given");
    w.run_dok(&[
        "assess",
        "--repo",
        "/srv/work",
        "--profile",
        "/srv/profile.yaml",
        "--format",
        "json",
    ]);
}

#[when("dok assess runs on the working tree without a profile")]
fn assess_without_profile(w: &mut DokWorld) {
    w.run_dok(&["assess", "--repo", "/srv/work", "--format", "json"]);
}

#[then(expr = "the mode ceiling is {string}")]
fn ceiling_is(w: &mut DokWorld, ceiling: String) {
    let actual = w.report()["ceiling"].as_str().unwrap_or_default();
    // The feature speaks the report's human wording; JSON carries the enum name.
    let expected = match ceiling.as_str() {
        "D2 only (m2-*)" => "d2-only",
        other => other,
    };
    assert_eq!(actual, expected, "{}", w.report());
}

#[then("the profile is not refused")]
fn not_refused(w: &mut DokWorld) {
    let refusals = w.report()["refusals"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(refusals.is_empty(), "{refusals:?}");
}

#[then(expr = "the profile is refused because {string}")]
fn refused_because(w: &mut DokWorld, reason: String) {
    let refusals = w.report()["refusals"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        refusals
            .iter()
            .any(|r| r.as_str().unwrap_or_default().contains(&reason)),
        "no refusal contains {reason:?}: {refusals:?}"
    );
}

#[then("every finding that is not ok names what would have to change")]
fn every_finding_names_change(w: &mut DokWorld) {
    for f in w.report()["findings"].as_array().expect("findings") {
        if f["rating"].as_str() != Some("ok") {
            assert!(f["to_change"].is_string(), "{f} names no change");
        }
    }
}

#[then(expr = "the finding {string} is blocking")]
fn finding_is_blocking(w: &mut DokWorld, check: String) {
    let findings = w.report()["findings"].as_array().expect("findings");
    let f = findings
        .iter()
        .find(|f| f["check"].as_str() == Some(check.as_str()))
        .unwrap_or_else(|| panic!("no finding {check}"));
    assert_eq!(f["rating"].as_str(), Some("blocking"), "{f}");
}

#[then(expr = "the oracle consequence for {word} contains {string}")]
fn oracle_consequence(w: &mut DokWorld, workload: String, text: String) {
    let oracles = w.report()["oracles"].as_array().expect("oracles");
    let o = oracles
        .iter()
        .find(|o| o["workload"].as_str() == Some(workload.as_str()))
        .unwrap_or_else(|| panic!("no workload {workload}"));
    let consequence = o["consequence"].as_str().unwrap_or_default();
    assert!(
        consequence.contains(&text),
        "{consequence:?} lacks {text:?}"
    );
}
