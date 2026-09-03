//! The declarative shape of a conformance pack.
//!
//! A pack is data. It names claims, the identity each claim must be tried
//! under, the attempt or assertion that tries it, and — for attempts — the
//! mechanisms that are allowed to refuse it. Every enum here is closed on
//! purpose: a new kind of probe is a compile error at every site that
//! judges one, never a silent fall-through.

use anyhow::{bail, Context, Result};
use serde::Deserialize;

pub const API_VERSION: &str = "dokimastes/v1";
pub const KIND: &str = "NegativeCapabilityPack";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub target: Target,
    pub probes: Vec<Probe>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// `owner/name` on the hosting platform.
    pub repository: String,
}

/// Who is holding the credential the probe runs under.
///
/// A refusal only demonstrates something when the identity had no standing
/// bypass. An owner force-pushing `main` and being allowed to proves nothing
/// about the ruleset — which is why every probe names the identities it is
/// meaningful for, and the runner refuses to count a probe run under any
/// other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Identity {
    /// The agent-class token: push to `agent/**`, nothing else.
    Agent,
    /// A member of `@dokimastes/maintainers`; no ruleset bypass.
    Maintainer,
    /// A member of `@dokimastes/stewards`; on the bypass list, audited.
    Steward,
    /// Repository admin who is *not* an organisation owner.
    RepoAdmin,
}

impl Identity {
    pub fn as_str(self) -> &'static str {
        match self {
            Identity::Agent => "agent",
            Identity::Maintainer => "maintainer",
            Identity::Steward => "steward",
            Identity::RepoAdmin => "repo-admin",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Probe {
    pub id: String,
    /// The sentence the framework makes to a client, an auditor or a
    /// Betriebsrat. Untested, it is a sentence in a document.
    pub claim: String,
    #[serde(default)]
    pub stories: Vec<String>,
    /// Identities under which a result is meaningful.
    pub run_as: Vec<Identity>,
    #[serde(default)]
    pub attempt: Option<Attempt>,
    /// Mechanisms allowed to refuse the attempt, matched against the
    /// server's response text. An attempt refused by none of them is still
    /// refused — but reported as refused by an *unidentified* mechanism,
    /// which is a finding rather than a pass.
    #[serde(default)]
    pub refused_by: Vec<Refusal>,
    #[serde(default)]
    pub assert: Option<Assertion>,
}

/// Something that must be refused.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Attempt {
    /// Non-fast-forward update of `branch`. Restored on success.
    ForcePush { branch: String },
    /// Delete `branch`. Restored on success.
    DeleteBranch { branch: String },
    /// Commit a change to `path` directly onto `branch`, no pull request.
    /// Restored on success.
    DirectPush {
        branch: String,
        path: String,
        #[serde(default = "default_true")]
        signed: bool,
    },
    /// Push a lightweight tag. Deleted on success.
    PushTag { tag: String },
    /// A REST call that must fail.
    ApiCall {
        method: String,
        path: String,
        #[serde(default)]
        body: Option<serde_json::Value>,
    },
    /// Merge a pull request while a required check is failing.
    MergeWithFailingCheck,
    /// A non-steward approval on a constitutional path satisfies review.
    ApproveGatePathAsNonSteward { path: String },
    /// A repository admin removes a required status check.
    RemoveRequiredCheckAsAdmin,
}

/// Something that must hold.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Assertion {
    /// Every owner named in `CODEOWNERS` resolves to a populated team, and
    /// the platform reports no CODEOWNERS errors. Verified via the API,
    /// never by reading the file alone — an unresolvable owner fails open.
    CodeownersResolve,
    /// `GET path`, then the JSON value at RFC 6901 `pointer` equals `equals`.
    ApiValue {
        path: String,
        pointer: String,
        equals: serde_json::Value,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refusal {
    /// The control, named as the deployment names it (`RS-1 …`).
    pub mechanism: String,
    /// Substring that must appear in the response text for this mechanism
    /// to be credited.
    pub pattern: String,
}

fn default_true() -> bool {
    true
}

impl Pack {
    pub fn from_yaml(text: &str) -> Result<Pack> {
        let pack: Pack =
            serde_yaml::from_str(text).context("pack is not valid YAML for this schema")?;
        pack.validate()?;
        Ok(pack)
    }

    fn validate(&self) -> Result<()> {
        if self.api_version != API_VERSION {
            bail!(
                "apiVersion is {:?}, expected {:?}",
                self.api_version,
                API_VERSION
            );
        }
        if self.kind != KIND {
            bail!("kind is {:?}, expected {:?}", self.kind, KIND);
        }
        let (owner, name) = self
            .target
            .repository
            .split_once('/')
            .context("target.repository must be owner/name")?;
        if owner.is_empty() || name.is_empty() || name.contains('/') {
            bail!(
                "target.repository must be owner/name, got {:?}",
                self.target.repository
            );
        }
        if self.probes.is_empty() {
            bail!("a pack with no probes claims nothing");
        }
        let mut seen = std::collections::BTreeSet::new();
        for probe in &self.probes {
            if !seen.insert(&probe.id) {
                bail!("probe id {:?} appears twice", probe.id);
            }
            if probe.run_as.is_empty() {
                bail!("probe {}: run_as must name at least one identity", probe.id);
            }
            match (&probe.attempt, &probe.assert) {
                (Some(_), Some(_)) => bail!("probe {}: attempt and assert are exclusive", probe.id),
                (None, None) => bail!("probe {}: needs an attempt or an assert", probe.id),
                (Some(_), None) if probe.refused_by.is_empty() => {
                    bail!(
                        "probe {}: an attempt must name at least one refusing mechanism",
                        probe.id
                    )
                }
                (None, Some(_)) if !probe.refused_by.is_empty() => {
                    bail!("probe {}: refused_by only applies to an attempt", probe.id)
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn owner(&self) -> &str {
        self.target.repository.split('/').next().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
apiVersion: dokimastes/v1
kind: NegativeCapabilityPack
name: t
target: { repository: o/r }
probes:
  - id: P1
    claim: c
    run_as: [agent]
    attempt: { kind: force-push, branch: main }
    refused_by: [{ mechanism: m, pattern: GH013 }]
"#;

    #[test]
    fn minimal_pack_parses() {
        let pack = Pack::from_yaml(MINIMAL).unwrap();
        assert_eq!(pack.owner(), "o");
        assert_eq!(pack.probes.len(), 1);
    }

    #[test]
    fn attempt_without_mechanism_is_refused() {
        let text = MINIMAL.replace("    refused_by: [{ mechanism: m, pattern: GH013 }]\n", "");
        let err = Pack::from_yaml(&text).unwrap_err().to_string();
        assert!(err.contains("refusing mechanism"), "{err}");
    }

    #[test]
    fn unknown_attempt_kind_is_a_parse_error() {
        let text = MINIMAL.replace("force-push", "wish-really-hard");
        assert!(Pack::from_yaml(&text).is_err());
    }

    #[test]
    fn wrong_api_version_is_refused() {
        let text = MINIMAL.replace("dokimastes/v1", "factory/v1");
        let err = Pack::from_yaml(&text).unwrap_err().to_string();
        assert!(err.contains("apiVersion"), "{err}");
    }

    #[test]
    fn duplicate_ids_are_refused() {
        let dup = MINIMAL.to_string()
            + r#"  - id: P1
    claim: c
    run_as: [agent]
    assert: { kind: codeowners-resolve }
"#;
        let err = Pack::from_yaml(&dup).unwrap_err().to_string();
        assert!(err.contains("twice"), "{err}");
    }
}
