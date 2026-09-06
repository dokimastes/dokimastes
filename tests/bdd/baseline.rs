//! `baseline.feature`: the before number from a repository with a known
//! month of history, provisioned in the container.

use cucumber::{given, then, when};
use serde_json::Value;

use super::world::DokWorld;

#[given("a repository with a known month of history")]
fn known_history(w: &mut DokWorld) {
    let out = w.container().exec(&["provision-history"], None);
    assert_eq!(out.code, 0, "provision-history: {}", out.stderr);
}

#[when(expr = "dok baseline runs over the last {int} days")]
fn baseline_runs(w: &mut DokWorld, days: i64) {
    let days = days.to_string();
    w.run_dok(&[
        "baseline",
        "--repo",
        "/srv/history",
        "--window-days",
        &days,
        "--format",
        "json",
    ]);
}

#[when(expr = "dok baseline runs over the last {int} days with the first agent run on {word}")]
fn baseline_runs_with_first_run(w: &mut DokWorld, days: i64, date: String) {
    let days = days.to_string();
    w.run_dok(&[
        "baseline",
        "--repo",
        "/srv/history",
        "--window-days",
        &days,
        "--first-agent-run",
        &date,
        "--format",
        "json",
    ]);
}

fn metric<'a>(report: &'a Value, name: &str) -> &'a Value {
    report["pairs"]
        .as_array()
        .expect("pairs")
        .iter()
        .flat_map(|p| [&p["throughput"], &p["counter"]])
        .find(|m| m["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("no metric {name:?} in {report}"))
}

#[then(expr = "the metric {string} is {float}")]
fn metric_is(w: &mut DokWorld, name: String, value: f64) {
    let actual = metric(w.report(), &name)["value"]
        .as_f64()
        .unwrap_or_else(|| panic!("{name} has no value"));
    assert!((actual - value).abs() < 0.01, "{name}: {actual} vs {value}");
}

#[then(expr = "the metric {string} has no value")]
fn metric_has_no_value(w: &mut DokWorld, name: String) {
    assert!(
        metric(w.report(), &name)["value"].is_null(),
        "{name} has a value"
    );
}

#[then(expr = "the metric {string} is not recoverable because of {word}")]
fn metric_not_recoverable(w: &mut DokWorld, name: String, status: String) {
    let m = metric(w.report(), &name);
    assert_eq!(m["status"].as_str(), Some(status.as_str()), "{m}");
    assert!(m["value"].is_null(), "{m}");
    assert!(
        m["method"].as_str().is_some_and(|s| !s.is_empty()),
        "{m} gives no reason"
    );
}

#[then("no person is named in the report")]
fn no_person_named(w: &mut DokWorld) {
    let text = w.report().to_string();
    // The history's author is `h <h@example.invalid>`; neither may appear.
    assert!(
        !text.contains("example.invalid") && !text.contains("\"h\""),
        "{text}"
    );
}

#[then(expr = "the baseline is refused because {string}")]
fn baseline_refused(w: &mut DokWorld, reason: String) {
    let refusals = w.report()["refusals"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        refusals
            .iter()
            .any(|r| r.as_str().unwrap_or_default().contains(&reason)),
        "{refusals:?}"
    );
}

#[then("the baseline is not refused")]
fn baseline_not_refused(w: &mut DokWorld) {
    let refusals = w.report()["refusals"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(refusals.is_empty(), "{refusals:?}");
}
