//! The project profile — one reviewed file per project, in the enforcement
//! tier, so a project cannot vary its own constraints. This is the shape
//! from the operating model §3, with an `assessment` block for the facts
//! the qualification needs and nothing else in the profile records.
//!
//! Every field is optional except `id`, because the profile is written by
//! a person over time. What is absent is reported as unknown — and unknown
//! is treated as the most restrictive value, never as fine.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub tenant: Option<String>,
    pub contract: Option<String>,
    pub sector: Option<String>,
    pub annex_iii: Option<bool>,
    pub client_data_class: Option<String>,
    pub egress_profile: Option<String>,
    pub scm_binding: Option<String>,
    pub enforcement_class: Option<String>,
    pub evidence_trust: Option<String>,
    /// The verdict a person recorded. `dok assess` recomputes it and refuses
    /// a profile that claims more than the assessment supports.
    pub substrate: Option<Verdict>,
    /// The mode a person chose. Refused when it exceeds the ceiling the
    /// verdict allows.
    pub default_mode: Option<String>,
    pub model_pin: Option<String>,
    #[serde(default)]
    pub ci: Ci,
    #[serde(default)]
    pub stack: Vec<String>,
    #[serde(default)]
    pub verdict_inputs: VerdictInputs,
    #[serde(default)]
    pub oracles: Vec<Oracle>,
    pub path_registry: Option<String>,
    #[serde(default)]
    pub disqualifiers: Vec<String>,
    pub accountable: Option<String>,
    pub baseline_captured: Option<String>,
    pub requalify_by: Option<String>,
    #[serde(default)]
    pub assessment: Assessment,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ci {
    /// The one command the agent calls as its inner loop.
    pub inner_loop: Option<String>,
    pub inner_loop_p95_minutes: Option<f64>,
    pub full_suite_p95_minutes: Option<f64>,
    /// Share of `main` runs failing without a code cause, last 30 days.
    /// Accepts `0.9%`, `"0.9"` or `0.9`; always a percentage.
    pub flake_rate_30d: Option<Percent>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerdictInputs {
    #[serde(rename = "static")]
    pub static_: Option<String>,
    pub quality: Option<String>,
    /// The mutation tool, or `none`. No credible tool means the lane
    /// ceiling drops: nothing else on the list makes agent-written tests
    /// trustworthy.
    pub mutation: Option<String>,
    pub snippet: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Oracle {
    pub workload: String,
    pub class: OracleClass,
}

/// The oracle independence ranking, most to least independent. Closed:
/// `none` is a class, not an absence, so the report must name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OracleClass {
    Mechanical,
    MachineCheckableSpec,
    Holdout,
    /// Tests the agent wrote. Never an oracle on their own.
    AgentTests,
    /// Never in a gate.
    LlmJudge,
    None,
}

impl OracleClass {
    pub fn as_str(self) -> &'static str {
        match self {
            OracleClass::Mechanical => "mechanical",
            OracleClass::MachineCheckableSpec => "machine-checkable-spec",
            OracleClass::Holdout => "holdout",
            OracleClass::AgentTests => "agent-tests",
            OracleClass::LlmJudge => "llm-judge",
            OracleClass::None => "none",
        }
    }

    /// What this oracle class permits, stated for the person reading it.
    pub fn consequence(self) -> &'static str {
        match self {
            OracleClass::Mechanical => "independent by construction; Lane 4 admissible per workload",
            OracleClass::MachineCheckableSpec => "independent if the agent did not write the spec; Lane 4 admissible per workload",
            OracleClass::Holdout => "independent while the holdout stays behind a credential; Lane 4 admissible per workload",
            OracleClass::AgentTests => "NOT an independent check — never qualifies a change for reduced review",
            OracleClass::LlmJudge => "NOT an independent check — never in a gate; may prioritise attention only",
            OracleClass::None => "no independent check — no lane above 3, regardless of how well the agent performs here",
        }
    }
}

/// Facts the qualification needs that a person measured and recorded.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assessment {
    pub measured_on: Option<String>,
    /// The one documented command a fresh clone builds with.
    pub cold_build_command: Option<String>,
    /// The test entry point is green on `main` today.
    pub test_green_on_main: Option<bool>,
    /// Can required checks be added by someone other than the developers?
    /// If not, there is no F3 boundary on this project.
    pub required_checks_settable_by_non_developers: Option<bool>,
    /// Baseline mutation score, 0–100, from a real run. Not coverage.
    pub mutation_score: Option<Percent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// D2 only. The estate work is the project.
    Red,
    /// D3 available in `m3-staged` only. Restrict change classes.
    Amber,
    /// D3 available, `m3-session` viable, widen change classes normally.
    Green,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Green => "green",
            Verdict::Amber => "amber",
            Verdict::Red => "red",
        }
    }
}

/// A percentage that may arrive as `0.9%`, `"0.9"` or `0.9`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Percent(pub f64);

impl<'de> Deserialize<'de> for Percent {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Num(f64),
            Text(String),
        }
        match Raw::deserialize(d)? {
            Raw::Num(n) => Ok(Percent(n)),
            Raw::Text(s) => s
                .trim()
                .trim_end_matches('%')
                .trim()
                .parse::<f64>()
                .map(Percent)
                .map_err(|_| serde::de::Error::custom(format!("not a percentage: {s:?}"))),
        }
    }
}

impl Profile {
    pub fn from_yaml(text: &str) -> Result<Profile> {
        let profile: Profile =
            serde_yaml::from_str(text).context("profile is not valid YAML for this schema")?;
        if profile.id.trim().is_empty() {
            anyhow::bail!("profile id is empty");
        }
        Ok(profile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_operating_model_example_parses() {
        let text = r#"
id: acme-core-banking
tenant: acme
contract: werkvertrag
sector: finance
annex_iii: false
substrate: amber
default_mode: m3-staged
ci:
  inner_loop: "./gradlew check -x integrationTest"
  inner_loop_p95_minutes: 4
  full_suite_p95_minutes: 26
  flake_rate_30d: 0.9%
stack: [java-21, spring-boot, oracle]
verdict_inputs:
  mutation: pitest
oracles:
  - workload: batch-etl
    class: machine-checkable-spec
  - workload: reporting-ui
    class: none
path_registry: CODEOWNERS
disqualifiers: [payment-authorisation, kyc, crypto]
"#;
        let p = Profile::from_yaml(text).unwrap();
        assert_eq!(p.substrate, Some(Verdict::Amber));
        assert_eq!(p.ci.flake_rate_30d, Some(Percent(0.9)));
        assert_eq!(p.oracles[1].class, OracleClass::None);
    }

    #[test]
    fn percent_accepts_three_spellings() {
        #[derive(Deserialize)]
        struct T {
            p: Percent,
        }
        for text in ["p: 2.5%", "p: '2.5'", "p: 2.5"] {
            assert_eq!(
                serde_yaml::from_str::<T>(text).unwrap().p,
                Percent(2.5),
                "{text}"
            );
        }
        assert!(serde_yaml::from_str::<T>("p: lots").is_err());
    }

    #[test]
    fn unknown_fields_are_refused() {
        assert!(Profile::from_yaml("id: x\nsubstrate_override: green\n").is_err());
    }

    #[test]
    fn verdict_orders_red_below_green() {
        assert!(Verdict::Red < Verdict::Amber && Verdict::Amber < Verdict::Green);
    }
}
