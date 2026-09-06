//! Reading a repository's history and computing the retrospective half of
//! the metric contract from it. Only counts and sizes leave this module:
//! no author, no email, no login — the telemetry firewall applies to the
//! baseline as much as to anything else the framework records.
//!
//! Two `git log` passes cover the window, whatever its size: one over every
//! commit (graph, times, numstat, authors counted in memory), one over the
//! first-parent merges with their diff against the first parent.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
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
    /// The path after the change. For a rename, the new path.
    pub path: String,
    /// For a rename, the path before it.
    pub renamed_from: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Commit {
    pub sha: String,
    /// Committer time: when it landed.
    pub time: i64,
    /// Author time: when it was written. Survives a rebase; committer time does not.
    pub author_time: i64,
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
    /// Seconds from the oldest merged commit (by author time, over every
    /// non-first parent) to the merge itself. Commits older than the window
    /// are not seen, so this is a lower bound for very long-lived branches.
    pub lead_time_secs: Option<i64>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct History {
    pub head: String,
    pub window_start: i64,
    pub window_end: i64,
    /// Non-merge commits in the window, oldest first.
    pub commits: Vec<Commit>,
    /// First-parent merge commits in the window.
    pub merges: Vec<Merge>,
    /// Creation times of release tags (`v` or `V` followed by a digit) inside the window.
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

/// One parsed record of `git log --format=%x1e%H%x1f%ct%x1f%at%x1f%P%x1f%aN%x1f%s --numstat`.
struct Record {
    commit: Commit,
    author: String,
}

pub fn read(repo: &Path, window_start: i64, window_end: i64) -> Result<History> {
    let head = git(repo, &["rev-parse", "HEAD"])
        .map(|s| s.trim().to_string())
        .context("no commits to baseline")?;
    let since = format!("--since=@{window_start}");
    let until = format!("--until=@{window_end}");

    // Pass 1: every commit in the window, with numstat (empty for merges).
    let log = git(
        repo,
        &[
            "log",
            &since,
            &until,
            "--format=%x1e%H%x1f%ct%x1f%at%x1f%P%x1f%aN%x1f%s",
            "--numstat",
        ],
    )?;
    let records = parse_log(&log);
    let distinct_authors = records
        .iter()
        .map(|r| r.author.as_str())
        .filter(|a| !a.is_empty())
        .collect::<BTreeSet<_>>()
        .len();
    let graph: HashMap<&str, (&Commit, i64)> = records
        .iter()
        .map(|r| (r.commit.sha.as_str(), (&r.commit, r.commit.author_time)))
        .collect();
    let mut commits: Vec<Commit> = records
        .iter()
        .filter(|r| r.commit.parents.len() <= 1)
        .map(|r| r.commit.clone())
        .collect();
    commits.sort_by_key(|c| c.time);

    // Pass 2: first-parent merges with their diff against the first parent.
    let merge_log = git(
        repo,
        &[
            "log",
            "--first-parent",
            "--merges",
            "-m",
            &since,
            &until,
            "--format=%x1e%H%x1f%ct%x1f%at%x1f%P%x1f%aN%x1f%s",
            "--numstat",
        ],
    )?;
    let mut merges = Vec::new();
    for r in parse_log(&merge_log) {
        let size_lines = r.commit.files.iter().map(|f| f.added + f.deleted).sum();
        let lead_time_secs =
            oldest_merged_author_time(&graph, &r.commit).map(|oldest| r.commit.time - oldest);
        merges.push(Merge {
            sha: r.commit.sha.clone(),
            time: r.commit.time,
            size_lines,
            lead_time_secs,
        });
    }
    merges.sort_by_key(|m| m.time);

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
            (is_release_tag(name) && t >= window_start && t <= window_end).then_some(t)
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

/// `v` or `V` followed by a digit: `v1.2.0`, `V3`. Not `vendored`, not `v-next`.
pub fn is_release_tag(name: &str) -> bool {
    name.strip_prefix(['v', 'V'])
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
}

/// Author time of the oldest commit reachable from the merge's non-first
/// parents but not from its first parent — within what the window holds.
fn oldest_merged_author_time(graph: &HashMap<&str, (&Commit, i64)>, merge: &Commit) -> Option<i64> {
    let (first, rest) = merge.parents.split_first()?;
    let mut on_mainline: HashSet<&str> = HashSet::new();
    let mut queue: VecDeque<&str> = VecDeque::from([first.as_str()]);
    while let Some(sha) = queue.pop_front() {
        if !on_mainline.insert(sha) {
            continue;
        }
        if let Some((c, _)) = graph.get(sha) {
            queue.extend(c.parents.iter().map(String::as_str));
        }
    }
    let mut oldest: Option<i64> = None;
    let mut seen: HashSet<&str> = HashSet::new();
    let mut queue: VecDeque<&str> = rest.iter().map(String::as_str).collect();
    while let Some(sha) = queue.pop_front() {
        if on_mainline.contains(sha) || !seen.insert(sha) {
            continue;
        }
        if let Some((c, author_time)) = graph.get(sha) {
            oldest = Some(oldest.map_or(*author_time, |o| o.min(*author_time)));
            queue.extend(c.parents.iter().map(String::as_str));
        }
    }
    oldest
}

fn parse_log(text: &str) -> Vec<Record> {
    let mut records = Vec::new();
    for record in text.split('\u{1e}').filter(|r| !r.trim().is_empty()) {
        let mut lines = record.lines();
        let Some(header) = lines.next() else { continue };
        let fields: Vec<&str> = header.split('\u{1f}').collect();
        if fields.len() < 6 {
            continue;
        }
        let files = parse_numstat(&lines.collect::<Vec<_>>().join("\n"));
        records.push(Record {
            commit: Commit {
                sha: fields[0].to_string(),
                time: fields[1].parse().unwrap_or(0),
                author_time: fields[2].parse().unwrap_or(0),
                parents: fields[3].split_whitespace().map(str::to_string).collect(),
                subject: fields[5].to_string(),
                files,
            },
            author: fields[4].to_string(),
        });
    }
    records
}

fn parse_numstat(text: &str) -> Vec<FileChange> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            let added = it.next()?.trim();
            let deleted = it.next()?.trim();
            let raw = it.next()?.trim();
            let (renamed_from, path) = split_rename(raw);
            // Binary files show "-"; they are not lines.
            Some(FileChange {
                added: added.parse().unwrap_or(0),
                deleted: deleted.parse().unwrap_or(0),
                path,
                renamed_from,
            })
        })
        .collect()
}

/// `git log --numstat` writes renames as `old => new` or `dir/{old => new}/f`.
/// Returns (path before, path after); before is `None` when not a rename.
pub fn split_rename(raw: &str) -> (Option<String>, String) {
    if let (Some(open), Some(close)) = (raw.find('{'), raw.find('}')) {
        if open < close {
            if let Some((old, new)) = raw[open + 1..close].split_once(" => ") {
                let prefix = &raw[..open];
                let suffix = &raw[close + 1..];
                let join = |mid: &str| {
                    let joined = format!("{prefix}{mid}{suffix}");
                    joined
                        .replace("//", "/")
                        .trim_start_matches('/')
                        .to_string()
                };
                return (Some(join(old)), join(new));
            }
        }
    }
    if let Some((old, new)) = raw.split_once(" => ") {
        return (Some(old.to_string()), new.to_string());
    }
    (None, raw.to_string())
}

/// Lines added in the window that were deleted again within
/// `CHURN_WINDOW_DAYS`, approximated at file granularity: a later deletion
/// in the same file is attributed to the most recent additions still inside
/// the window, never beyond what they added. A rename carries the file's
/// additions to its new path. An upper bound, and stated as one.
pub fn churn(commits: &[Commit]) -> (u64, u64) {
    let window = CHURN_WINDOW_DAYS * DAY;
    let mut recent: BTreeMap<String, VecDeque<(i64, u64)>> = BTreeMap::new();
    let mut added_total = 0;
    let mut churned = 0;
    for c in commits {
        for f in &c.files {
            if let Some(old) = &f.renamed_from {
                if let Some(carried) = recent.remove(old) {
                    recent.entry(f.path.clone()).or_default().extend(carried);
                }
            }
            let queue = recent.entry(f.path.clone()).or_default();
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
            author_time: time_days * DAY,
            parents: vec![],
            subject: subject.into(),
            files: files
                .iter()
                .map(|(a, d, p)| {
                    let (renamed_from, path) = split_rename(p);
                    FileChange {
                        added: *a,
                        deleted: *d,
                        path,
                        renamed_from,
                    }
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
    fn churn_follows_a_rename() {
        let commits = vec![
            commit(0, &[(100, 0, "a.rs")], "add"),
            commit(1, &[(0, 0, "a.rs => b.rs")], "move"),
            commit(2, &[(0, 30, "b.rs")], "trim after move"),
            commit(3, &[(0, 0, "src/{b.rs => c.rs}")], "move into dir"),
            commit(4, &[(0, 10, "src/c.rs")], "trim again"),
        ];
        // The second rename key is `src/b.rs`, which never held additions, so
        // only the first rename carries: 30 of 100.
        assert_eq!(churn(&commits), (30, 100));
    }

    #[test]
    fn rename_paths_are_normalised() {
        assert_eq!(
            split_rename("a.rs => b.rs"),
            (Some("a.rs".into()), "b.rs".into())
        );
        assert_eq!(
            split_rename("src/{old => new}/f.rs"),
            (Some("src/old/f.rs".into()), "src/new/f.rs".into())
        );
        assert_eq!(
            split_rename("{lib => }/f.rs"),
            (Some("lib/f.rs".into()), "f.rs".into())
        );
        assert_eq!(
            split_rename("plain/path.rs"),
            (None, "plain/path.rs".into())
        );
    }

    #[test]
    fn release_tags_need_a_digit_after_the_v() {
        for yes in ["v1.0", "V3", "v0.0.1-rc1"] {
            assert!(is_release_tag(yes), "{yes}");
        }
        for no in ["vendored", "version-notes", "v-next", "release-1", "1.0"] {
            assert!(!is_release_tag(no), "{no}");
        }
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
    fn oldest_merged_commit_spans_every_non_first_parent_by_author_time() {
        // main: m0 ← M (merge of a1 and b1); a1 authored day 5, b1 authored day 2 but committed day 9 (rebased).
        let mk = |sha: &str, time: i64, author: i64, parents: &[&str]| Commit {
            sha: sha.into(),
            time: time * DAY,
            author_time: author * DAY,
            parents: parents.iter().map(|p| p.to_string()).collect(),
            subject: String::new(),
            files: vec![],
        };
        let m0 = mk("m0", 0, 0, &[]);
        let a1 = mk("a1", 5, 5, &["m0"]);
        let b1 = mk("b1", 9, 2, &["m0"]);
        let merge = mk("M", 10, 10, &["m0", "a1", "b1"]);
        let all = [&m0, &a1, &b1, &merge];
        let graph: HashMap<&str, (&Commit, i64)> = all
            .iter()
            .map(|c| (c.sha.as_str(), (*c, c.author_time)))
            .collect();
        assert_eq!(oldest_merged_author_time(&graph, &merge), Some(2 * DAY));
        assert_eq!(oldest_merged_author_time(&graph, &a1), None, "not a merge");
    }

    #[test]
    fn median_of_even_and_odd() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&mut [4.0, 1.0, 3.0, 2.0]), Some(2.5));
        assert_eq!(median(&mut []), None);
    }
}
