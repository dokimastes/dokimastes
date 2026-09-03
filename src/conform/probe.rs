//! Executing one probe against a real remote.
//!
//! Attempts are made with `git` and `gh` as subprocesses under the
//! credential the caller already holds. Nothing here reads, stores or
//! generates a key. Every destructive attempt that goes through is undone
//! immediately, and the report says whether that worked.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};

use super::spec::{Assertion, Attempt, Refusal};
use super::verdict::{Mechanism, Outcome, Restoration};

const PROBE_NAME: &str = "dok negative-capability probe";
const PROBE_EMAIL: &str = "probe@dokimastes.invalid";

pub struct Executor {
    /// `owner/name`.
    pub repository: String,
    /// Where git pushes go. A URL, or a path for local rehearsal.
    pub remote: String,
    /// The `gh` binary.
    pub gh: PathBuf,
}

struct Ran {
    ok: bool,
    stdout: String,
    stderr: String,
}

impl Ran {
    fn text(&self) -> String {
        if self.stderr.trim().is_empty() {
            self.stdout.clone()
        } else {
            self.stderr.clone()
        }
    }
}

fn run(cmd: &mut Command, stdin: Option<&[u8]>) -> Result<Ran> {
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut child = cmd
        .spawn()
        .with_context(|| format!("cannot start {program}"))?;
    if let Some(bytes) = stdin {
        child.stdin.take().context("stdin")?.write_all(bytes)?;
    }
    let out = child.wait_with_output()?;
    Ok(Ran {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

fn classify(text: &str, refusals: &[Refusal]) -> Mechanism {
    refusals
        .iter()
        .find(|r| text.contains(&r.pattern))
        .map(|r| Mechanism::Named {
            mechanism: r.mechanism.clone(),
        })
        .unwrap_or_else(|| Mechanism::Unidentified {
            response: text.trim().to_string(),
        })
}

impl Executor {
    fn git(&self, workdir: &Path, signed: bool) -> Command {
        let mut cmd = Command::new("git");
        cmd.arg("-C")
            .arg(workdir)
            .arg("-c")
            .arg(format!("user.name={PROBE_NAME}"))
            .arg("-c")
            .arg(format!("user.email={PROBE_EMAIL}"))
            .arg("-c")
            .arg(format!("commit.gpgsign={signed}"))
            .arg("-c")
            .arg("tag.gpgsign=false")
            .arg("-c")
            .arg("advice.detachedHead=false");
        cmd
    }

    fn clone(&self, branch: Option<&str>) -> Result<(tempfile::TempDir, PathBuf)> {
        let dir = tempfile::tempdir().context("temp dir")?;
        let work = dir.path().join("probe");
        let mut cmd = Command::new("git");
        cmd.args(["clone", "--quiet", "--no-tags"]);
        if let Some(b) = branch {
            cmd.args(["--branch", b]);
        }
        cmd.arg(&self.remote).arg(&work);
        let ran = run(&mut cmd, None)?;
        if !ran.ok {
            return Err(anyhow!(
                "clone of {} failed: {}",
                self.remote,
                ran.text().trim()
            ));
        }
        Ok((dir, work))
    }

    fn rev_parse(&self, work: &Path, what: &str) -> Result<String> {
        let ran = run(self.git(work, false).args(["rev-parse", what]), None)?;
        if !ran.ok {
            return Err(anyhow!("rev-parse {what}: {}", ran.text().trim()));
        }
        Ok(ran.stdout.trim().to_string())
    }

    fn push(&self, work: &Path, refspec: &str, force: bool) -> Result<Ran> {
        let mut cmd = self.git(work, false);
        cmd.args(["push", "--quiet"]);
        if force {
            cmd.arg("--force");
        }
        cmd.arg("origin").arg(refspec);
        run(&mut cmd, None)
    }

    fn restore(&self, work: &Path, refspec: &str, force: bool) -> Restoration {
        match self.push(work, refspec, force) {
            Ok(r) if r.ok => Restoration::Restored,
            Ok(r) => Restoration::Failed {
                detail: r.text().trim().to_string(),
            },
            Err(e) => Restoration::Failed {
                detail: format!("{e:#}"),
            },
        }
    }

    /// A commit with the same tree as HEAD and the same parents — a sibling,
    /// so pushing it is a non-fast-forward update that changes no content.
    fn sibling_of_head(&self, work: &Path) -> Result<String> {
        let tree = self.rev_parse(work, "HEAD^{tree}")?;
        let ran = run(
            self.git(work, false)
                .args(["rev-list", "--parents", "-n", "1", "HEAD"]),
            None,
        )?;
        if !ran.ok {
            return Err(anyhow!("rev-list: {}", ran.text().trim()));
        }
        let parents: Vec<String> = ran
            .stdout
            .split_whitespace()
            .skip(1)
            .map(str::to_string)
            .collect();
        let mut cmd = self.git(work, false);
        cmd.args([
            "commit-tree",
            &tree,
            "-m",
            &format!("{PROBE_NAME}: non-fast-forward sibling"),
        ]);
        for p in &parents {
            cmd.args(["-p", p]);
        }
        let ran = run(&mut cmd, None)?;
        if !ran.ok {
            return Err(anyhow!("commit-tree: {}", ran.text().trim()));
        }
        Ok(ran.stdout.trim().to_string())
    }

    pub fn attempt(&self, attempt: &Attempt, refusals: &[Refusal]) -> Outcome {
        match self.try_attempt(attempt, refusals) {
            Ok(outcome) => outcome,
            Err(e) => Outcome::Errored {
                detail: format!("{e:#}"),
            },
        }
    }

    fn try_attempt(&self, attempt: &Attempt, refusals: &[Refusal]) -> Result<Outcome> {
        match attempt {
            Attempt::ForcePush { branch } => {
                let (_dir, work) = self.clone(Some(branch))?;
                let orig = self.rev_parse(&work, "HEAD")?;
                let sibling = self.sibling_of_head(&work)?;
                let ran = self.push(&work, &format!("{sibling}:refs/heads/{branch}"), true)?;
                if !ran.ok {
                    return Ok(Outcome::Refused { mechanism: classify(&ran.text(), refusals) });
                }
                let restored = self.restore(&work, &format!("{orig}:refs/heads/{branch}"), true);
                Ok(Outcome::Succeeded {
                    detail: format!("force-pushed {} over {} on {branch}", short(&sibling), short(&orig)),
                    restored,
                })
            }
            Attempt::DeleteBranch { branch } => {
                let (_dir, work) = self.clone(Some(branch))?;
                let orig = self.rev_parse(&work, "HEAD")?;
                let ran = self.push(&work, &format!(":refs/heads/{branch}"), false)?;
                if !ran.ok {
                    return Ok(Outcome::Refused { mechanism: classify(&ran.text(), refusals) });
                }
                let restored = self.restore(&work, &format!("{orig}:refs/heads/{branch}"), false);
                Ok(Outcome::Succeeded { detail: format!("deleted {branch} (was {})", short(&orig)), restored })
            }
            Attempt::DirectPush { branch, path, signed } => {
                let (_dir, work) = self.clone(Some(branch))?;
                let orig = self.rev_parse(&work, "HEAD")?;
                let file = work.join(path);
                if let Some(parent) = file.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&file)?;
                writeln!(f, "{PROBE_NAME}: direct push to {branch}")?;
                let ran = run(self.git(&work, false).args(["add", "--", path]), None)?;
                if !ran.ok {
                    return Err(anyhow!("git add: {}", ran.text().trim()));
                }
                let ran = run(
                    self.git(&work, *signed).args(["commit", "--quiet", "-m", &format!("{PROBE_NAME}: touch {path}")]),
                    None,
                )?;
                if !ran.ok {
                    return Err(anyhow!("commit (signed={signed}): {}", ran.text().trim()));
                }
                let ran = self.push(&work, &format!("HEAD:refs/heads/{branch}"), false)?;
                if !ran.ok {
                    return Ok(Outcome::Refused { mechanism: classify(&ran.text(), refusals) });
                }
                let restored = self.restore(&work, &format!("{orig}:refs/heads/{branch}"), true);
                Ok(Outcome::Succeeded {
                    detail: format!("pushed a {} commit touching {path} onto {branch}", if *signed { "signed" } else { "unsigned" }),
                    restored,
                })
            }
            Attempt::PushTag { tag } => {
                let (_dir, work) = self.clone(None)?;
                let ran = run(self.git(&work, false).args(["tag", tag, "HEAD"]), None)?;
                if !ran.ok {
                    return Err(anyhow!("git tag: {}", ran.text().trim()));
                }
                let ran = self.push(&work, &format!("refs/tags/{tag}"), false)?;
                if !ran.ok {
                    return Ok(Outcome::Refused { mechanism: classify(&ran.text(), refusals) });
                }
                let restored = self.restore(&work, &format!(":refs/tags/{tag}"), false);
                Ok(Outcome::Succeeded { detail: format!("pushed tag {tag}"), restored })
            }
            Attempt::ApiCall { method, path, body } => {
                let mut cmd = Command::new(&self.gh);
                cmd.args(["api", "-X", method, path]);
                let payload = body.as_ref().map(serde_json::to_vec).transpose()?;
                if payload.is_some() {
                    cmd.args(["--input", "-"]);
                }
                let ran = run(&mut cmd, payload.as_deref())?;
                if ran.ok {
                    return Ok(Outcome::Succeeded {
                        detail: format!("{method} {path} succeeded"),
                        restored: Restoration::NotNeeded,
                    });
                }
                if !ran.stderr.contains("(HTTP ") {
                    return Err(anyhow!("gh api did not reach the platform: {}", ran.text().trim()));
                }
                // 400/422 mean the platform validated the request body: the
                // credential was *permitted* to make the call. That is not a
                // refusal of the capability, whatever the body was wrong about.
                if ran.stderr.contains("(HTTP 422)") || ran.stderr.contains("(HTTP 400)") {
                    return Ok(Outcome::Succeeded {
                        detail: format!(
                            "{method} {path} reached validation — the credential is permitted to make it: {}",
                            super::verdict::first_line(&ran.stderr)
                        ),
                        restored: Restoration::NotNeeded,
                    });
                }
                Ok(Outcome::Refused { mechanism: classify(&ran.text(), refusals) })
            }
            Attempt::MergeWithFailingCheck
            | Attempt::ApproveGatePathAsNonSteward { .. }
            | Attempt::RemoveRequiredCheckAsAdmin => Ok(Outcome::NotRun {
                reason: "this release of dok cannot perform the attempt; script it, run it by hand, record the result".into(),
            }),
        }
    }

    pub fn assert(&self, assertion: &Assertion) -> Outcome {
        match self.try_assert(assertion) {
            Ok(outcome) => outcome,
            Err(e) => Outcome::Errored {
                detail: format!("{e:#}"),
            },
        }
    }

    fn api_json(&self, path: &str, raw: bool) -> Result<Option<serde_json::Value>> {
        let mut cmd = Command::new(&self.gh);
        cmd.args(["api", path]);
        if raw {
            cmd.args(["-H", "Accept: application/vnd.github.raw+json"]);
        }
        let ran = run(&mut cmd, None)?;
        if !ran.ok {
            if ran.stderr.contains("(HTTP 404)") {
                return Ok(None);
            }
            return Err(anyhow!("GET {path}: {}", ran.text().trim()));
        }
        if raw {
            return Ok(Some(serde_json::Value::String(ran.stdout)));
        }
        Ok(Some(
            serde_json::from_str(&ran.stdout).with_context(|| format!("GET {path}: not JSON"))?,
        ))
    }

    fn try_assert(&self, assertion: &Assertion) -> Result<Outcome> {
        match assertion {
            Assertion::ApiValue {
                path,
                pointer,
                equals,
            } => {
                let Some(doc) = self.api_json(path, false)? else {
                    return Ok(Outcome::Violated {
                        detail: format!("GET {path} → 404"),
                    });
                };
                match doc.pointer(pointer) {
                    Some(actual) if actual == equals => Ok(Outcome::Holds),
                    Some(actual) => Ok(Outcome::Violated {
                        detail: format!("{path}{pointer} is {actual}, expected {equals}"),
                    }),
                    None => Ok(Outcome::Violated {
                        detail: format!("{path} has no value at {pointer}"),
                    }),
                }
            }
            Assertion::CodeownersResolve => {
                let owner = self.repository.split('/').next().unwrap_or_default();
                let mut text = None;
                for candidate in [".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"] {
                    let path = format!("repos/{}/contents/{candidate}", self.repository);
                    if let Some(serde_json::Value::String(s)) = self.api_json(&path, true)? {
                        text = Some((candidate, s));
                        break;
                    }
                }
                let Some((where_, text)) = text else {
                    return Ok(Outcome::Violated {
                        detail: "no CODEOWNERS file in .github/, root or docs/".into(),
                    });
                };
                let teams = codeowner_teams(&text, owner);
                if teams.is_empty() {
                    return Ok(Outcome::Violated {
                        detail: format!("{where_} names no @{owner}/<team> owner"),
                    });
                }
                let mut problems = Vec::new();
                for team in &teams {
                    let members =
                        self.api_json(&format!("orgs/{owner}/teams/{team}/members"), false)?;
                    match members.as_ref().and_then(|v| v.as_array()).map(|a| a.len()) {
                        Some(0) => problems.push(format!("@{owner}/{team} has no members")),
                        Some(_) => {}
                        None => problems.push(format!("@{owner}/{team} does not resolve")),
                    }
                }
                let errors_path = format!("repos/{}/codeowners/errors", self.repository);
                if let Some(doc) = self.api_json(&errors_path, false)? {
                    if let Some(errs) = doc.pointer("/errors").and_then(|e| e.as_array()) {
                        for e in errs {
                            let msg = e
                                .pointer("/message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("?");
                            let line = e
                                .pointer("/line")
                                .map(|l| l.to_string())
                                .unwrap_or_default();
                            problems.push(format!(
                                "platform reports CODEOWNERS error at line {line}: {}",
                                super::verdict::first_line(msg)
                            ));
                        }
                    }
                }
                if problems.is_empty() {
                    Ok(Outcome::Holds)
                } else {
                    Ok(Outcome::Violated {
                        detail: problems.join("; "),
                    })
                }
            }
        }
    }
}

/// Team slugs referenced as `@<owner>/<slug>` in a CODEOWNERS body.
pub fn codeowner_teams(text: &str, owner: &str) -> Vec<String> {
    let prefix = format!("@{owner}/");
    let mut teams: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        for token in line.split_whitespace().skip(1) {
            if let Some(slug) = token.strip_prefix(&prefix) {
                if !slug.is_empty() && !teams.iter().any(|t| t == slug) {
                    teams.push(slug.to_string());
                }
            }
        }
    }
    teams
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(10)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_credits_the_first_matching_mechanism() {
        let refusals = vec![
            Refusal {
                mechanism: "RS-1".into(),
                pattern: "GH013".into(),
            },
            Refusal {
                mechanism: "default-branch guard".into(),
                pattern: "refusing to delete".into(),
            },
        ];
        match classify(
            "remote: error: GH013: Repository rule violations found",
            &refusals,
        ) {
            Mechanism::Named { mechanism } => assert_eq!(mechanism, "RS-1"),
            other => panic!("{other:?}"),
        }
        match classify(
            "! [remote rejected] main (refusing to delete the current branch)",
            &refusals,
        ) {
            Mechanism::Named { mechanism } => assert_eq!(mechanism, "default-branch guard"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            classify("something else entirely", &refusals),
            Mechanism::Unidentified { .. }
        ));
    }

    #[test]
    fn codeowner_teams_are_extracted_and_deduplicated() {
        let text = "# constitutional\n/invariants/  @dokimastes/stewards @other/compliance\n/docs/ @dokimastes/maintainers\n/packs/ @dokimastes/maintainers someone@example.com\n";
        assert_eq!(
            codeowner_teams(text, "dokimastes"),
            vec!["stewards", "maintainers"]
        );
        assert!(codeowner_teams("", "dokimastes").is_empty());
    }
}
