//! Acceptance scenarios, in Gherkin, executed for real: the `dok` binary
//! runs inside a container built from this working tree, against a git
//! repository provisioned in that container. Given provisions, When runs
//! the command, Then reads the command's JSON report — and, for the
//! repository's state, makes one `verify-repo` call.
//!
//! Needs docker or podman. Without one the scenarios fail; they do not
//! skip, because a claim with no attempt behind it is unproven.

mod container;

use cucumber::gherkin::Step;
use cucumber::{given, then, when, World};
use serde_json::Value;

use container::Container;

#[derive(Debug, Default, World)]
pub struct DokWorld {
    container: Option<Container>,
    /// Repository state right after provisioning, from `verify-repo`.
    seed: Option<Value>,
    profile_yaml: Option<String>,
    /// The JSON report of the last `dok` invocation, when it produced one.
    report: Option<Value>,
    exit_code: Option<i32>,
    stderr: String,
}

impl DokWorld {
    fn container(&self) -> &Container {
        self.container.as_ref().expect("a git server first")
    }
    fn report(&self) -> &Value {
        self.report
            .as_ref()
            .unwrap_or_else(|| panic!("no report; stderr was: {}", self.stderr))
    }
    /// The one probe record of a `dok conform --only <id>` run.
    fn record(&self) -> &Value {
        &self.report()["records"][0]
    }
    fn run_dok(&mut self, args: &[&str]) {
        let mut full = vec!["dok"];
        full.extend_from_slice(args);
        let out = self.container().exec(&full, None);
        self.exit_code = Some(out.code);
        self.stderr = out.stderr;
        self.report = serde_json::from_str(&out.stdout).ok();
    }
}

// ---------- Given ----------

#[given("a git server with a repository holding branches main and side")]
fn git_server(w: &mut DokWorld) {
    let c = Container::start();
    let out = c.exec(&["provision-repo"], None);
    assert_eq!(out.code, 0, "provision-repo: {}", out.stderr);
    w.seed = Some(serde_json::from_str(&out.stdout).expect("verify-repo prints JSON"));
    w.container = Some(c);
}

#[given(expr = "the server refuses every push with {string}")]
fn server_refuses(w: &mut DokWorld, message: String) {
    let out = w.container().exec(&["refuse-pushes", &message], None);
    assert_eq!(out.code, 0, "refuse-pushes: {}", out.stderr);
}

#[given(expr = "the working tree contains {string}")]
fn working_tree_contains(w: &mut DokWorld, files: String) {
    let script = files
        .split(',')
        .map(str::trim)
        .map(|f| format!("mkdir -p \"$(dirname '{f}')\" && : > '{f}'"))
        .collect::<Vec<_>>()
        .join(" && ");
    let out = w
        .container()
        .exec(&["sh", "-c", &format!("cd /srv/work && {script}")], None);
    assert_eq!(out.code, 0, "{}", out.stderr);
}

#[given("a profile with")]
fn profile_with(w: &mut DokWorld, step: &Step) {
    let text = step
        .docstring
        .as_deref()
        .expect("a docstring with the profile")
        .to_string();
    let out = w
        .container()
        .exec(&["sh", "-c", "cat > /srv/profile.yaml"], Some(&text));
    assert_eq!(out.code, 0, "{}", out.stderr);
    w.profile_yaml = Some(text);
}

// ---------- When ----------

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

// ---------- Then: the report ----------

#[then(expr = "the verdict is {word}")]
fn verdict_is(w: &mut DokWorld, verdict: String) {
    let actual = if w.report().get("records").is_some() {
        &w.record()["verdict"]
    } else {
        &w.report()["verdict"]
    };
    assert_eq!(actual.as_str(), Some(verdict.as_str()), "{}", w.report());
}

#[then(expr = "the exit code is {int}")]
fn exit_code_is(w: &mut DokWorld, code: i32) {
    assert_eq!(w.exit_code, Some(code), "stderr: {}", w.stderr);
}

#[then(expr = "dok exits with code {int} and reports {string}")]
fn exits_with_error(w: &mut DokWorld, code: i32, text: String) {
    assert_eq!(w.exit_code, Some(code), "stderr: {}", w.stderr);
    assert!(
        w.stderr.contains(&text),
        "stderr does not mention {text:?}: {}",
        w.stderr
    );
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

#[then("the repository is unchanged")]
fn repository_unchanged(w: &mut DokWorld) {
    let out = w.container().exec(&["verify-repo"], None);
    assert_eq!(out.code, 0, "verify-repo: {}", out.stderr);
    let now: Value = serde_json::from_str(&out.stdout).expect("verify-repo prints JSON");
    assert_eq!(&now, w.seed.as_ref().unwrap(), "repository state changed");
}

#[then(expr = "the mode ceiling is {string}")]
fn ceiling_is(w: &mut DokWorld, ceiling: String) {
    let actual = w.report()["ceiling"].as_str().unwrap_or_default();
    let expected = match ceiling.as_str() {
        "m3-session" => "m3-session",
        "m3-staged" => "m3-staged",
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

#[tokio::main]
async fn main() {
    Container::build_image();
    DokWorld::cucumber()
        .fail_on_skipped()
        .run_and_exit("conformance/features")
        .await;
}
