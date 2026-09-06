//! `dok baseline` — the before number. The retrospective half of the metric
//! contract, computed from the repository's own history, paired
//! symmetrically: every throughput figure next to the degradation figure
//! the evidence says moves against it. What history cannot yield is
//! listed as such, never left blank.

pub mod dates;
pub mod history;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use serde::Serialize;

use dates::{iso_date, now_unix, parse_iso_date, DAY};
use history::History;

#[derive(clap::Args)]
pub struct Args {
    /// Repository to read.
    #[arg(long, default_value = ".")]
    pub repo: PathBuf,

    /// How far back to look, in days.
    #[arg(long, default_value_t = 180)]
    pub window_days: u32,

    /// The date the first agent ran on this project, `YYYY-MM-DD`. A baseline
    /// captured after that date is not a before number and is refused.
    #[arg(long)]
    pub first_agent_run: Option<String>,

    #[arg(long, value_enum, default_value_t = crate::conform::Format::Table)]
    pub format: crate::conform::Format,
}

/// Whether a figure could be recovered from history, and if not, why.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum Status {
    /// Computed from git.
    Recovered,
    /// Git alone does not hold it; the platform API does.
    PlatformApi,
    /// Needs analysis tooling over the code, not history.
    Tooling,
    /// Did not exist before switch-on; must be collected from now on.
    CollectForward,
}

#[derive(Debug, Clone, Serialize)]
pub struct Metric {
    pub name: String,
    pub value: Option<f64>,
    pub unit: &'static str,
    /// How the figure was computed, or why it could not be.
    pub method: String,
    #[serde(flatten)]
    pub status: Status,
}

impl Metric {
    fn recovered(name: &str, value: Option<f64>, unit: &'static str, method: &str) -> Metric {
        Metric {
            name: name.into(),
            value,
            unit,
            method: method.into(),
            status: Status::Recovered,
        }
    }
    fn absent(name: &str, unit: &'static str, status: Status, why: &str) -> Metric {
        Metric {
            name: name.into(),
            value: None,
            unit,
            method: why.into(),
            status,
        }
    }
    fn shown(&self) -> String {
        match self.value {
            Some(v) if v.fract() == 0.0 => format!("{v:.0} {}", self.unit),
            Some(v) => format!("{v:.2} {}", self.unit),
            None => "—".to_string(),
        }
    }
}

/// One row of the symmetric contract.
#[derive(Debug, Clone, Serialize)]
pub struct Pair {
    pub throughput: Option<Metric>,
    pub counter: Metric,
}

#[derive(Debug, Serialize)]
pub struct Baseline {
    pub repo: String,
    pub head: String,
    pub captured_on: String,
    pub window: (String, String),
    pub window_days: u32,
    pub commits: usize,
    pub merges: usize,
    pub distinct_authors: usize,
    pub pairs: Vec<Pair>,
    /// Why the baseline cannot serve as a before number, if it cannot.
    pub refusals: Vec<String>,
}

pub fn run(args: Args) -> Result<ExitCode> {
    if args.window_days == 0 {
        bail!("--window-days must be at least 1");
    }
    let now = now_unix();
    let start = now - args.window_days as i64 * DAY;
    let first_agent_run = match &args.first_agent_run {
        Some(text) => Some(
            parse_iso_date(text)
                .with_context(|| format!("--first-agent-run {text:?} is not a YYYY-MM-DD date"))?,
        ),
        None => None,
    };
    let history = history::read(&args.repo, start, now)
        .with_context(|| format!("reading {}", args.repo.display()))?;
    let baseline = compute(
        &args.repo.display().to_string(),
        &history,
        args.window_days,
        now,
        first_agent_run,
    );

    match args.format {
        crate::conform::Format::Json => println!("{}", serde_json::to_string_pretty(&baseline)?),
        crate::conform::Format::Table => print!("{}", markdown(&baseline)),
    }
    Ok(if baseline.refusals.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

/// `first_agent_run` is the start of the day the first agent ran, if known.
/// A baseline captured on or after that day is refused: it is not a before
/// number.
pub fn compute(
    repo: &str,
    h: &History,
    window_days: u32,
    now: i64,
    first_agent_run: Option<i64>,
) -> Baseline {
    let weeks = window_days as f64 / 7.0;
    let (churned, added) = history::churn(&h.commits);
    let reverts = history::reverts(&h.commits);
    let mut sizes: Vec<f64> = h.merges.iter().map(|m| m.size_lines as f64).collect();
    let mut lead_hours: Vec<f64> = h
        .merges
        .iter()
        .filter_map(|m| m.lead_time_secs)
        .map(|s| s as f64 / 3600.0)
        .collect();

    // A linear or squash-merged history has no merge commits. Its
    // pull-request figures are then not measured-as-zero; they live on the
    // platform, and the status says so in a form a consumer can read.
    const LINEAR: &str = "no merge commits in the window: a linear or squash-merged history leaves none, so this figure needs the platform API";
    let merge_metric = |name: &str, value: Option<f64>, unit: &'static str, method: &str| {
        if h.merges.is_empty() {
            Metric::absent(name, unit, Status::PlatformApi, LINEAR)
        } else {
            Metric::recovered(name, value, unit, method)
        }
    };

    let pairs = vec![
        Pair {
            throughput: Some(Metric::recovered("commits per week", Some(h.commits.len() as f64 / weeks), "commits/week", "non-merge commits in the window, divided by weeks")),
            counter: Metric::recovered(
                "code churn within 14 days",
                (added > 0).then(|| churned as f64 / added as f64 * 100.0),
                "% of added lines",
                "lines added in the window and deleted again in the same file within 14 days, at file granularity and following renames — an upper bound on line-level churn",
            ),
        },
        Pair {
            throughput: Some(merge_metric("merges per week", Some(h.merges.len() as f64 / weeks), "merges/week", "merge commits on the first-parent line, divided by weeks")),
            counter: Metric::recovered("revert commits", Some(reverts as f64), "commits", "commits whose subject starts with \"Revert \""),
        },
        Pair {
            throughput: Some(merge_metric("median merge size", history::median(&mut sizes), "lines", "lines added plus deleted per merge against its first parent")),
            counter: Metric::absent("incidents per merge", "incidents/merge", Status::CollectForward, "history does not record incidents; link the incident tracker and collect from now on"),
        },
        Pair {
            throughput: Some(merge_metric("median lead time", history::median(&mut lead_hours), "hours", "oldest merged commit, by author time so a rebase does not reset it, over every non-first parent, to the merge commit — bounded by the window")),
            counter: Metric::absent("merges with no review", "% of merges", Status::PlatformApi, "review state lives on the platform, not in git"),
        },
        Pair {
            throughput: Some(Metric::recovered("releases per week", Some(h.release_tags.len() as f64 / weeks), "tags/week", "tags named v or V followed by a digit, created in the window, divided by weeks — a proxy for deploys; deploy records themselves are not in git")),
            counter: Metric::absent("review depth", "comments/merge", Status::PlatformApi, "review comments live on the platform, not in git"),
        },
        Pair {
            throughput: None,
            counter: Metric::absent("block duplication and cross-file calls", "ratio", Status::Tooling, "needs a code-analysis pass over the tree, not history"),
        },
        Pair {
            throughput: None,
            counter: Metric::absent("mutation score", "%", Status::Tooling, "needs one run of the stack's mutation tool; record it as assessment.mutation_score"),
        },
        Pair {
            throughput: None,
            counter: Metric::absent("escaped-defect rate per lane", "defects/change", Status::CollectForward, "does not exist before switch-on; attribute every escaped defect to the lane in force when its change was made"),
        },
    ];

    Baseline {
        repo: repo.to_string(),
        head: h.head.clone(),
        captured_on: iso_date(now),
        window: (iso_date(h.window_start), iso_date(h.window_end)),
        window_days,
        commits: h.commits.len(),
        merges: h.merges.len(),
        distinct_authors: h.distinct_authors,
        pairs,
        refusals: refusals(now, first_agent_run),
    }
}

fn refusals(now: i64, first_agent_run: Option<i64>) -> Vec<String> {
    match first_agent_run {
        Some(first_run) if now >= first_run => vec![format!(
            "captured on {} but the first agent ran on {}: this is not a before number, and a project without one cannot be marked ready",
            iso_date(now),
            iso_date(first_run)
        )],
        _ => Vec::new(),
    }
}

fn markdown(b: &Baseline) -> String {
    let mut out = format!(
        "## Baseline — `{}` at `{}`, captured {}\n\nWindow {} to {} ({} days): {} commits, {} merges, {} distinct authors. Counts only — no person is named.\n\n",
        b.repo,
        &b.head[..b.head.len().min(10)],
        b.captured_on,
        b.window.0,
        b.window.1,
        b.window_days,
        b.commits,
        b.merges,
        b.distinct_authors
    );
    out.push_str("| Throughput (the case for) | Value | Paired counter-metric (what degrades) | Value |\n|---|---|---|---|\n");
    for p in &b.pairs {
        let (tn, tv) = p
            .throughput
            .as_ref()
            .map(|m| (m.name.as_str(), m.shown()))
            .unwrap_or(("—", String::new()));
        out.push_str(&format!(
            "| {tn} | {tv} | {} | {} |\n",
            p.counter.name,
            p.counter.shown()
        ));
    }
    out.push_str("\n### How each figure was computed\n\n");
    for m in b
        .pairs
        .iter()
        .flat_map(|p| p.throughput.iter().chain(std::iter::once(&p.counter)))
    {
        if matches!(m.status, Status::Recovered) {
            out.push_str(&format!("- **{}** — {}.\n", m.name, m.method));
        }
    }
    out.push_str("\n### Not recoverable from history — collect from now on\n\n");
    for m in b.pairs.iter().map(|p| &p.counter) {
        let label = match m.status {
            Status::Recovered => continue,
            Status::PlatformApi => "platform API",
            Status::Tooling => "tooling",
            Status::CollectForward => "collect forward",
        };
        out.push_str(&format!("- **{}** ({label}) — {}.\n", m.name, m.method));
    }
    if !b.refusals.is_empty() {
        out.push_str("\n### Refused\n\n");
        for r in &b.refusals {
            out.push_str(&format!("- {r}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use history::{Commit, FileChange, Merge};

    fn history() -> History {
        let file = |a, d| FileChange {
            added: a,
            deleted: d,
            path: "a.rs".into(),
            renamed_from: None,
        };
        History {
            head: "abc".into(),
            window_start: 0,
            window_end: 70 * DAY,
            commits: vec![
                Commit {
                    sha: "1".into(),
                    time: DAY,
                    author_time: DAY,
                    parents: vec![],
                    subject: "add".into(),
                    files: vec![file(100, 0)],
                },
                Commit {
                    sha: "2".into(),
                    time: 3 * DAY,
                    author_time: 3 * DAY,
                    parents: vec![],
                    subject: "Revert \"x\"".into(),
                    files: vec![file(0, 25)],
                },
            ],
            merges: vec![
                Merge {
                    sha: "m1".into(),
                    time: 5 * DAY,
                    size_lines: 40,
                    lead_time_secs: Some(7200),
                },
                Merge {
                    sha: "m2".into(),
                    time: 9 * DAY,
                    size_lines: 60,
                    lead_time_secs: Some(3600),
                },
            ],
            release_tags: vec![6 * DAY],
            distinct_authors: 3,
        }
    }

    fn metric<'a>(b: &'a Baseline, name: &str) -> &'a Metric {
        b.pairs
            .iter()
            .flat_map(|p| p.throughput.iter().chain(std::iter::once(&p.counter)))
            .find(|m| m.name == name)
            .unwrap()
    }

    #[test]
    fn figures_are_paired_and_computed() {
        let b = compute("r", &history(), 70, 70 * DAY, None);
        assert_eq!(metric(&b, "commits per week").value, Some(0.2));
        assert_eq!(metric(&b, "code churn within 14 days").value, Some(25.0));
        assert_eq!(metric(&b, "revert commits").value, Some(1.0));
        assert_eq!(metric(&b, "median merge size").value, Some(50.0));
        assert_eq!(metric(&b, "median lead time").value, Some(1.5));
        assert_eq!(metric(&b, "releases per week").value, Some(0.1));
        assert!(b.pairs.iter().all(|p| p.throughput.is_none()
            || !matches!(p.counter.status, Status::Recovered)
            || p.counter.value.is_some()
            || p.counter.name.contains("churn")));
    }

    #[test]
    fn what_history_cannot_yield_is_named_not_blank() {
        let b = compute("r", &history(), 70, 70 * DAY, None);
        let absent: Vec<&Metric> = b
            .pairs
            .iter()
            .map(|p| &p.counter)
            .filter(|m| !matches!(m.status, Status::Recovered))
            .collect();
        assert_eq!(absent.len(), 6);
        assert!(absent
            .iter()
            .all(|m| m.value.is_none() && !m.method.is_empty()));
    }

    #[test]
    fn a_linear_history_is_not_measured_as_zero() {
        let mut h = history();
        h.merges.clear();
        let b = compute("r", &h, 70, 70 * DAY, None);
        for name in ["merges per week", "median merge size", "median lead time"] {
            let m = metric(&b, name);
            assert!(
                matches!(m.status, Status::PlatformApi),
                "{name}: {:?}",
                m.status
            );
            assert_eq!(m.value, None, "{name}");
            assert!(m.method.contains("platform API"), "{name}");
        }
    }

    #[test]
    fn a_baseline_on_or_after_the_first_agent_run_is_refused() {
        let first_run = 50 * DAY; // start of the day the first agent ran
        assert!(
            compute("r", &history(), 70, first_run - 1, Some(first_run))
                .refusals
                .is_empty(),
            "the day before"
        );
        assert_eq!(
            compute("r", &history(), 70, first_run, Some(first_run))
                .refusals
                .len(),
            1,
            "on the day, at 00:00"
        );
        assert_eq!(
            compute("r", &history(), 70, first_run + DAY - 1, Some(first_run))
                .refusals
                .len(),
            1,
            "on the day, at 23:59:59"
        );
        assert_eq!(
            compute("r", &history(), 70, first_run + 30 * DAY, Some(first_run))
                .refusals
                .len(),
            1,
            "later"
        );
        assert!(
            compute("r", &history(), 70, first_run + 30 * DAY, None)
                .refusals
                .is_empty(),
            "no date given, no refusal"
        );
        assert!(
            compute("r", &history(), 70, first_run, Some(first_run)).refusals[0]
                .contains("not a before number")
        );
    }

    #[test]
    fn output_names_no_person() {
        let b = compute("r", &history(), 70, 70 * DAY, None);
        let json = serde_json::to_string(&b).unwrap();
        assert!(!json.contains("author\"") || json.contains("distinct_authors"));
        assert!(!json.contains('@'));
    }
}
