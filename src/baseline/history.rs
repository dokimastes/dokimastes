//! Reading a repository's history and computing the retrospective half of
//! the metric contract from it. Only counts and sizes leave this module:
//! no author, no email, no login — the telemetry firewall applies to the
//! baseline as much as to anything else the framework records.

use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;

use super::dates::DAY;

/// A line added and removed again within this many days counts as churn.
pub const CHURN_WINDOW_DAYS: i64 = 14;

#[derive(Debug, Clone, Serialize)]
pub struct FileChange {
    pub added: u64,
    pub deleted: u64,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Commit {
    pub sha: String,
    pub time: i64,
    pub parents: Vec<String>,
    pub subject: String,
    pub files: Vec<FileChange>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Merge {
    pub sha: String,
    pub time: i64,
    /// Lines added plus deleted against the first parent.
    pub size_lines: u64,
    /// Seconds from the oldest commit merged to the merge itself.
    pub lead_time_secs: Option<i64>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct History {
    pub head: String,
    pub window_start: i64,
    pub window_end: i64,
    pub commits: Vec<Commit>,
    pub merges: Vec<Merge>,
    /// Tag creation times for `v*` tags inside the window.
    pub release_tags: Vec<i64>,
    pub distinct_authors: usize,
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .context("cannot start git")?;
    if !out.status.success() {
        return Err(anyhow!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn read(repo: &Path, window_start: i64, window_end: i64) -> Result<History> {
    let head = git(repo, &["rev-parse", "HEAD"])
        .map(|s| s.trim().to_string())
        .context("no commits to baseline")?;
    let since = format!("--since=@{window_start}");
    let until = format!("--until=@{window_end}");

    let log = git(
        repo,
        &[
            "log",
            "--no-merges",
            &since,
            &until,
            "--format=%x1e%H%x1f%ct%x1f%P%x1f%s",
            "--numstat",
        ],
    )?;
    let mut commits = parse_log(&log);
    commits.sort_by_key(|c| c.time);

    let authors = git(
        repo,
        &["log", "--no-merges", &since, &until, "--format=%aN"],
    )?;
    let distinct_authors = authors
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .len();

    let merge_log = git(
        repo,
        &[
            "log",
            "--merges",
            "--first-parent",
            &since,
            &until,
            "--format=%H %ct %P",
        ],
    )?;
    let mut merges = Vec::new();
    for line in merge_log.lines() {
        let mut it = line.split_whitespace();
        let (Some(sha), Some(time), Some(p1), Some(p2)) =
            (it.next(), it.next(), it.next(), it.next())
        else {
            continue;
        };
        let time: i64 = time.parse().unwrap_or(0);
        let numstat = git(repo, &["diff", "--numstat", p1, sha])?;
        let size_lines = parse_numstat(&numstat)
            .iter()
            .map(|f| f.added + f.deleted)
            .sum();
        let times = git(repo, &["log", "--format=%ct", &format!("{p1}..{p2}")])?;
        let oldest = times
            .lines()
            .filter_map(|l| l.trim().parse::<i64>().ok())
            .min();
        merges.push(Merge {
            sha: sha.to_string(),
            time,
            size_lines,
            lead_time_secs: oldest.map(|o| time - o),
        });
    }

    let tags = git(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short) %(creatordate:unix)",
            "refs/tags",
        ],
    )?;
    let release_tags = tags
        .lines()
        .filter_map(|l| {
            let (name, t) = l.split_once(' ')?;
            let t: i64 = t.trim().parse().ok()?;
            (name.starts_with('v') && t >= window_start && t <= window_end).then_some(t)
        })
        .collect();

    Ok(History {
        head,
        window_start,
        window_end,
        commits,
        merges,
        release_tags,
        distinct_authors,
    })
}

fn parse_log(text: &str) -> Vec<Commit> {
    let mut commits = Vec::new();
    for record in text.split('\u{1e}').filter(|r| !r.trim().is_empty()) {
        let mut lines = record.lines();
        let Some(header) = lines.next() else { continue };
        let fields: Vec<&str> = header.split('\u{1f}').collect();
        if fields.len() < 4 {
            continue;
        }
        let files = parse_numstat(&lines.collect::<Vec<_>>().join("\n"));
        commits.push(Commit {
            sha: fields[0].to_string(),
            time: fields[1].parse().unwrap_or(0),
            parents: fields[2].split_whitespace().map(str::to_string).collect(),
            subject: fields[3].to_string(),
            files,
        });
    }
    commits
}

fn parse_numstat(text: &str) -> Vec<FileChange> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            let added = it.next()?.trim();
            let deleted = it.next()?.trim();
            let path = it.next()?.trim();
            // Binary files show "-"; they are not lines.
            Some(FileChange {
                added: added.parse().unwrap_or(0),
                deleted: deleted.parse().unwrap_or(0),
                path: path.to_string(),
            })
        })
        .collect()
}

/// Lines added in the window that were deleted again within
/// `CHURN_WINDOW_DAYS`, approximated at file granularity: a later deletion
/// in the same file is attributed to the most recent additions still inside
/// the window, never beyond what they added. An upper bound, and stated as one.
pub fn churn(commits: &[Commit]) -> (u64, u64) {
    let window = CHURN_WINDOW_DAYS * DAY;
    let mut recent: BTreeMap<&str, VecDeque<(i64, u64)>> = BTreeMap::new();
    let mut added_total = 0;
    let mut churned = 0;
    for c in commits {
        for f in &c.files {
            let queue = recent.entry(f.path.as_str()).or_default();
            while queue.front().is_some_and(|(t, _)| c.time - *t > window) {
                queue.pop_front();
            }
            let mut to_attribute = f.deleted;
            for (_, remaining) in queue.iter_mut() {
                if to_attribute == 0 {
                    break;
                }
                let take = (*remaining).min(to_attribute);
                *remaining -= take;
                to_attribute -= take;
                churned += take;
            }
            if f.added > 0 {
                queue.push_back((c.time, f.added));
                added_total += f.added;
            }
        }
    }
    (churned, added_total)
}

pub fn reverts(commits: &[Commit]) -> usize {
    commits
        .iter()
        .filter(|c| c.subject.starts_with("Revert "))
        .count()
}

pub fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    Some(if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(time_days: i64, files: &[(u64, u64, &str)], subject: &str) -> Commit {
        Commit {
            sha: format!("{time_days:040}"),
            time: time_days * DAY,
            parents: vec![],
            subject: subject.into(),
            files: files
                .iter()
                .map(|(a, d, p)| FileChange {
                    added: *a,
                    deleted: *d,
                    path: p.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn churn_counts_deletions_within_fourteen_days_only() {
        let commits = vec![
            commit(0, &[(100, 0, "a.rs")], "add"),
            commit(5, &[(0, 30, "a.rs")], "trim"), // churn: 30 of the 100
            commit(30, &[(0, 50, "a.rs")], "much later"), // outside the window: not churn
        ];
        assert_eq!(churn(&commits), (30, 100));
    }

    #[test]
    fn churn_never_exceeds_what_was_added() {
        let commits = vec![
            commit(0, &[(10, 0, "a.rs")], "add"),
            commit(1, &[(0, 500, "a.rs")], "delete a lot"),
        ];
        assert_eq!(churn(&commits), (10, 10));
    }

    #[test]
    fn churn_is_per_file() {
        let commits = vec![
            commit(0, &[(10, 0, "a.rs")], "add"),
            commit(1, &[(0, 10, "b.rs")], "delete elsewhere"),
        ];
        assert_eq!(churn(&commits), (0, 10));
    }

    #[test]
    fn reverts_are_counted_by_subject() {
        let commits = vec![
            commit(0, &[], "Add thing"),
            commit(1, &[], "Revert \"Add thing\""),
            commit(2, &[], "revert lowercase is not git's wording"),
        ];
        assert_eq!(reverts(&commits), 1);
    }

    #[test]
    fn numstat_parsing_skips_binaries_as_zero_lines() {
        let files = parse_numstat("3\t1\tsrc/a.rs\n-\t-\timg.png\n");
        assert_eq!(files.len(), 2);
        assert_eq!((files[1].added, files[1].deleted), (0, 0));
    }

    #[test]
    fn median_of_even_and_odd() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&mut [4.0, 1.0, 3.0, 2.0]), Some(2.5));
        assert_eq!(median(&mut []), None);
    }
}
