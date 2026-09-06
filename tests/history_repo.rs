//! The baseline read from a real repository with a known month of history.

mod common;

use dok::baseline::dates::DAY;
use dok::baseline::history;
use dok::baseline::{compute, Baseline, Metric, Status};

fn metric<'a>(b: &'a Baseline, name: &str) -> &'a Metric {
    b.pairs
        .iter()
        .flat_map(|p| p.throughput.iter().chain(std::iter::once(&p.counter)))
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("no metric {name}"))
}

fn close(a: Option<f64>, b: f64) -> bool {
    a.is_some_and(|a| (a - b).abs() < 0.01)
}

#[test]
fn known_history_yields_the_expected_figures() {
    let (_dir, repo, now) = common::seed_history();
    let h = history::read(&repo, now - 60 * DAY, now).unwrap();
    assert_eq!(h.commits.len(), 5, "non-merge commits");
    assert_eq!(h.merges.len(), 1);
    assert_eq!(h.release_tags.len(), 1);
    assert_eq!(h.distinct_authors, 1);

    let b = compute("history", &h, 60, now, None);
    assert!(close(
        metric(&b, "commits per week").value,
        5.0 / (60.0 / 7.0)
    ));
    assert!(
        close(
            metric(&b, "code churn within 14 days").value,
            30.0 / 121.0 * 100.0
        ),
        "{:?}",
        metric(&b, "code churn within 14 days")
    );
    assert!(close(metric(&b, "revert commits").value, 1.0));
    assert!(close(metric(&b, "median merge size").value, 20.0));
    assert!(close(metric(&b, "median lead time").value, 48.0));
    assert!(close(
        metric(&b, "releases per week").value,
        1.0 / (60.0 / 7.0)
    ));
    assert!(matches!(
        metric(&b, "merges with no review").status,
        Status::PlatformApi
    ));
}

#[test]
fn a_window_that_misses_the_history_is_empty_not_wrong() {
    let (_dir, repo, now) = common::seed_history();
    let h = history::read(&repo, now - DAY, now).unwrap();
    assert!(h.commits.is_empty() && h.merges.is_empty() && h.release_tags.is_empty());
    let b = compute("history", &h, 1, now, None);
    assert_eq!(
        metric(&b, "code churn within 14 days").value,
        None,
        "no added lines, no ratio"
    );
    assert_eq!(metric(&b, "commits per week").value, Some(0.0));
    assert!(
        matches!(metric(&b, "merges per week").status, Status::PlatformApi),
        "no merges: not measured as zero"
    );
}

#[test]
fn a_directory_without_commits_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    common::git(dir.path(), &["init", "--quiet", "-b", "main"]);
    assert!(history::read(dir.path(), 0, i64::MAX / 2).is_err());
}
