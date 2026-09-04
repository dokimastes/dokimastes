//! Acceptance scenarios, in Gherkin, run in-process against the `dok`
//! library. Nothing here touches a hosting platform: `assess` runs on
//! synthetic profiles and temp trees, `conform` on a local bare
//! repository with and without a refusing hook.

mod common;

use cucumber::{given, then, when, World};

use dok::assess::measure::Measured;
use dok::assess::profile::{OracleClass, Percent, Profile, Verdict};
use dok::assess::rules::{assess, Assessment, Rating};
use dok::conform::probe::Executor;
use dok::conform::spec::{Assertion, Attempt, Identity, Probe, Refusal};
use dok::conform::verdict::{
    judge, Expect, Mechanism, Outcome, Restoration, Verdict as ProbeVerdict,
};

#[derive(Debug, Default, World)]
pub struct DokWorld {
    profile: Profile,
    measured: Measured,
    assessment: Option<Assessment>,
    oracles: Vec<(String, OracleClass)>,
    remote: Option<common::Remote>,
    outcome: Option<Outcome>,
    meaningful_as: Option<Identity>,
}

// ---------- assess ----------

#[given("a fully qualified profile on a well-formed tree")]
fn fully_qualified(w: &mut DokWorld) {
    let mut p = Profile {
        id: "p".into(),
        ..Default::default()
    };
    p.assessment.cold_build_command = Some("./gradlew build".into());
    p.assessment.test_green_on_main = Some(true);
    p.assessment.required_checks_settable_by_non_developers = Some(true);
    p.assessment.mutation_score = Some(Percent(61.0));
    p.ci.inner_loop = Some("./gradlew check".into());
    p.ci.inner_loop_p95_minutes = Some(4.0);
    p.ci.flake_rate_30d = Some(Percent(0.9));
    p.verdict_inputs.mutation = Some("pitest".into());
    w.profile = p;
    let mut m = Measured::default();
    m.build_systems.insert("gradle".into());
    m.determinism_markers.push("Dockerfile".into());
    m.codeowners = Some("CODEOWNERS".into());
    w.measured = m;
}

#[given("an empty profile on an empty tree")]
fn empty(w: &mut DokWorld) {
    w.profile = Profile {
        id: "p".into(),
        ..Default::default()
    };
    w.measured = Measured::default();
}

#[given(expr = "the inner loop p95 is {float} minutes")]
fn inner_loop(w: &mut DokWorld, minutes: f64) {
    w.profile.ci.inner_loop_p95_minutes = Some(minutes);
}

#[given("required checks cannot be set by anyone other than the developers")]
fn no_f3_boundary(w: &mut DokWorld) {
    w.profile
        .assessment
        .required_checks_settable_by_non_developers = Some(false);
}

#[given(expr = "the profile declares substrate {word}")]
fn declares_substrate(w: &mut DokWorld, verdict: String) {
    w.profile.substrate = Some(match verdict.as_str() {
        "green" => Verdict::Green,
        "amber" => Verdict::Amber,
        "red" => Verdict::Red,
        other => panic!("not a verdict: {other}"),
    });
}

#[given(expr = "the profile declares default_mode {word}")]
fn declares_mode(w: &mut DokWorld, mode: String) {
    w.profile.default_mode = Some(mode);
}

#[given(expr = "a workload {word} with oracle class {word}")]
fn declare_workload(w: &mut DokWorld, workload: String, class: String) {
    let class: OracleClass = serde_yaml::from_str(&class).expect("an oracle class");
    w.oracles.push((workload, class));
}

#[when("dok assesses the substrate")]
fn assesses(w: &mut DokWorld) {
    w.assessment = Some(assess(&w.profile, &w.measured));
}

fn assessment(w: &DokWorld) -> &Assessment {
    w.assessment.as_ref().expect("assess first")
}

#[then(expr = "the verdict is {word}")]
fn verdict_is(w: &mut DokWorld, verdict: String) {
    assert_eq!(
        assessment(w).verdict.as_str(),
        verdict,
        "{:#?}",
        assessment(w).findings
    );
}

#[then(expr = "the mode ceiling is {string}")]
fn ceiling_is(w: &mut DokWorld, ceiling: String) {
    assert_eq!(assessment(w).ceiling.as_str(), ceiling);
}

#[then("the profile is not refused")]
fn not_refused(w: &mut DokWorld) {
    assert!(
        assessment(w).refusals.is_empty(),
        "{:?}",
        assessment(w).refusals
    );
}

#[then(expr = "the profile is refused because {string}")]
fn refused_because(w: &mut DokWorld, reason: String) {
    let refusals = &assessment(w).refusals;
    assert!(
        refusals.iter().any(|r| r.contains(&reason)),
        "no refusal contains {reason:?}: {refusals:?}"
    );
}

#[then("every finding that is not ok names what would have to change")]
fn every_finding_names_change(w: &mut DokWorld) {
    for f in &assessment(w).findings {
        if f.rating != Rating::Ok {
            assert!(
                f.to_change.is_some(),
                "{:?} is {:?} and names no change",
                f.check,
                f.rating
            );
        }
    }
}

#[then(expr = "the oracle consequence for {word} contains {string}")]
fn oracle_consequence(w: &mut DokWorld, workload: String, text: String) {
    let (_, class) = w
        .oracles
        .iter()
        .find(|(name, _)| *name == workload)
        .expect("declared workload");
    assert!(
        class.consequence().contains(&text),
        "{:?}: {}",
        class,
        class.consequence()
    );
}

// ---------- conform ----------

#[given("a local repository with branches main and side")]
fn local_repository(w: &mut DokWorld) {
    w.remote = Some(common::seed());
}

#[given(expr = "the remote refuses every push with {string}")]
fn remote_refuses(w: &mut DokWorld, message: String) {
    common::refuse_with(&w.remote.as_ref().expect("a repository").bare, &message);
}

fn executor(w: &DokWorld) -> Executor {
    let remote = w.remote.as_ref().expect("a repository");
    Executor {
        repository: "local/rehearsal".into(),
        remote: remote.bare.to_string_lossy().into_owned(),
        gh: "gh".into(),
    }
}

fn refusals() -> Vec<Refusal> {
    vec![
        Refusal {
            mechanism: "ruleset".into(),
            pattern: "GH013".into(),
        },
        Refusal {
            mechanism: "default-branch guard".into(),
            pattern: "refusing to delete the current branch".into(),
        },
    ]
}

fn attempt_named(kind: &str, branch: &str) -> Attempt {
    match kind {
        "force-push" => Attempt::ForcePush {
            branch: branch.into(),
        },
        "delete-branch" => Attempt::DeleteBranch {
            branch: branch.into(),
        },
        "direct-push" => Attempt::DirectPush {
            branch: branch.into(),
            path: "docs/PROBE.md".into(),
            signed: false,
        },
        "push-tag" => Attempt::PushTag {
            tag: "v0.0.1".into(),
        },
        other => panic!("no attempt kind {other}"),
    }
}

#[when(expr = "the {word} attempt is tried")]
fn attempt_tried(w: &mut DokWorld, kind: String) {
    // Deletion is rehearsed on `side`: git itself refuses to delete the
    // branch HEAD points at, and that is a separate scenario.
    let branch = if kind == "delete-branch" {
        "side"
    } else {
        "main"
    };
    w.outcome = Some(executor(w).attempt(&attempt_named(&kind, branch), &refusals()));
}

#[when(expr = "the {word} attempt is tried on {word}")]
fn attempt_tried_on(w: &mut DokWorld, kind: String, branch: String) {
    w.outcome = Some(executor(w).attempt(&attempt_named(&kind, &branch), &refusals()));
}

fn outcome(w: &DokWorld) -> &Outcome {
    w.outcome.as_ref().expect("an attempt first")
}

#[then("the attempt went through and the previous state was restored")]
fn went_through_and_restored(w: &mut DokWorld) {
    match outcome(w) {
        Outcome::Succeeded {
            restored: Restoration::Restored,
            ..
        } => {}
        other => panic!("{other:?}"),
    }
    let remote = w.remote.as_ref().unwrap();
    assert_eq!(
        common::git(&remote.bare, &["rev-parse", "refs/heads/main"]),
        remote.main,
        "main restored"
    );
    assert_eq!(
        common::git(&remote.bare, &["rev-parse", "refs/heads/side"]),
        remote.side,
        "side restored"
    );
    assert!(
        !common::ref_exists(&remote.bare, "refs/tags/v0.0.1"),
        "probe tag deleted"
    );
}

#[then(expr = "the attempt was refused by {string}")]
fn refused_by(w: &mut DokWorld, mechanism: String) {
    match outcome(w) {
        Outcome::Refused {
            mechanism: Mechanism::Named { mechanism: m },
        } => assert_eq!(*m, mechanism),
        other => panic!("{other:?}"),
    }
}

#[then("the attempt was refused by an unidentified mechanism")]
fn refused_unidentified(w: &mut DokWorld) {
    assert!(
        matches!(
            outcome(w),
            Outcome::Refused {
                mechanism: Mechanism::Unidentified { .. }
            }
        ),
        "{:?}",
        outcome(w)
    );
}

#[given(expr = "a probe meaningful only as {word}")]
fn probe_meaningful_only_as(w: &mut DokWorld, identity: String) {
    w.meaningful_as = Some(serde_yaml::from_str(&identity).expect("an identity"));
}

#[when(expr = "it is run as {word}")]
fn run_as(w: &mut DokWorld, identity: String) {
    let running: Identity = serde_yaml::from_str(&identity).expect("an identity");
    let meaningful = w.meaningful_as.expect("a probe first");
    let probe = Probe {
        id: "P".into(),
        claim: "c".into(),
        run_as: vec![meaningful],
        attempt: None,
        refused_by: vec![],
        assert: Some(Assertion::CodeownersResolve),
    };
    w.outcome = Some(match dok::conform::identity_gate(&probe, running) {
        Some(reason) => Outcome::NotRun { reason },
        None => Outcome::Holds,
    });
}

#[then("the probe was not run")]
fn probe_not_run(w: &mut DokWorld) {
    assert!(
        matches!(outcome(w), Outcome::NotRun { .. }),
        "{:?}",
        outcome(w)
    );
}

fn expect(word: &str) -> Expect {
    match word {
        "red" => Expect::Red,
        "green" => Expect::Green,
        other => panic!("not an expectation: {other}"),
    }
}

#[then(expr = "under expectation {word} the verdict is {word}")]
fn verdict_under(w: &mut DokWorld, expectation: String, verdict: String) {
    let (v, note) = judge(expect(&expectation), outcome(w));
    let want = match verdict.as_str() {
        "pass" => ProbeVerdict::Pass,
        "fail" => ProbeVerdict::Fail,
        "unproven" => ProbeVerdict::Unproven,
        other => panic!("not a verdict: {other}"),
    };
    assert_eq!(v, want, "{note}");
}

#[then(expr = "under expectation {word} the verdict is pass with a finding")]
fn pass_with_finding(w: &mut DokWorld, expectation: String) {
    let (v, note) = judge(expect(&expectation), outcome(w));
    assert_eq!(v, ProbeVerdict::Pass, "{note}");
    assert!(note.contains("finding"), "{note}");
}

#[tokio::main]
async fn main() {
    DokWorld::cucumber()
        .fail_on_skipped()
        .run_and_exit("conformance/features")
        .await;
}
