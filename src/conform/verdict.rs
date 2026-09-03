//! What happened when a probe ran, and what that means under the
//! expectation the run was started with.

use serde::Serialize;

/// What a probe observed. Closed: every arm is judged explicitly below.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum Outcome {
    /// The attempt was refused.
    Refused { mechanism: Mechanism },
    /// The attempt went through. `restored` says whether the probe undid it.
    Succeeded {
        detail: String,
        restored: Restoration,
    },
    /// The assertion holds.
    Holds,
    /// The assertion does not hold.
    Violated { detail: String },
    /// The probe was not attempted, for a stated reason. Never silent.
    NotRun { reason: String },
    /// The probe could not complete for a reason unrelated to the control
    /// under test (tooling missing, network down, …).
    Errored { detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Mechanism {
    /// One of the mechanisms the pack named.
    Named { mechanism: String },
    /// Refused, but by nothing the pack named. That is a finding: "blocked"
    /// and "blocked by the control you think" are different results.
    Unidentified { response: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Restoration {
    Restored,
    NotNeeded,
    Failed { detail: String },
}

/// The state the run expects to find.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Expect {
    /// Before protection: every attempt must succeed. A refusal here means
    /// something other than the control under test fired.
    Red,
    /// After protection: every attempt must be refused, every assertion hold.
    Green,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    Pass,
    Fail,
    /// No attempt was made, so the claim stands unproven — reported as such,
    /// never as satisfied.
    Unproven,
}

pub fn judge(expect: Expect, outcome: &Outcome) -> (Verdict, String) {
    match (expect, outcome) {
        (Expect::Green, Outcome::Refused { mechanism: Mechanism::Named { mechanism } }) => {
            (Verdict::Pass, format!("refused by {mechanism}"))
        }
        (Expect::Green, Outcome::Refused { mechanism: Mechanism::Unidentified { response } }) => (
            Verdict::Pass,
            format!("refused, but by no mechanism the pack names — finding: {}", first_line(response)),
        ),
        (Expect::Green, Outcome::Succeeded { detail, restored }) => (
            Verdict::Fail,
            format!("the attempt went through ({detail}); {}", describe_restoration(restored)),
        ),
        (Expect::Green, Outcome::Holds) => (Verdict::Pass, "holds".to_string()),
        (Expect::Green, Outcome::Violated { detail }) => (Verdict::Fail, format!("does not hold: {detail}")),

        (Expect::Red, Outcome::Succeeded { detail, restored }) => (
            Verdict::Pass,
            format!("control absent, as expected before protection ({detail}); {}", describe_restoration(restored)),
        ),
        (Expect::Red, Outcome::Violated { detail }) => {
            (Verdict::Pass, format!("does not hold yet, as expected before protection: {detail}"))
        }
        (Expect::Red, Outcome::Refused { mechanism }) => (
            Verdict::Fail,
            format!(
                "failed rehearsal: refused before protection was applied, by {}",
                describe_mechanism(mechanism)
            ),
        ),
        (Expect::Red, Outcome::Holds) => (
            Verdict::Fail,
            "failed rehearsal: already holds before protection, so the after-run cannot demonstrate it".to_string(),
        ),

        (_, Outcome::NotRun { reason }) => (Verdict::Unproven, format!("not run: {reason}")),
        (_, Outcome::Errored { detail }) => (Verdict::Unproven, format!("probe errored: {detail}")),
    }
}

fn describe_mechanism(m: &Mechanism) -> String {
    match m {
        Mechanism::Named { mechanism } => mechanism.clone(),
        Mechanism::Unidentified { response } => {
            format!("an unidentified mechanism: {}", first_line(response))
        }
    }
}

fn describe_restoration(r: &Restoration) -> String {
    match r {
        Restoration::Restored => "the previous state was restored".to_string(),
        Restoration::NotNeeded => "nothing to restore".to_string(),
        Restoration::Failed { detail } => {
            format!("RESTORATION FAILED, manual repair needed: {detail}")
        }
    }
}

pub fn first_line(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named() -> Outcome {
        Outcome::Refused {
            mechanism: Mechanism::Named {
                mechanism: "RS-1".into(),
            },
        }
    }
    fn unidentified() -> Outcome {
        Outcome::Refused {
            mechanism: Mechanism::Unidentified {
                response: "remote: nope".into(),
            },
        }
    }
    fn succeeded() -> Outcome {
        Outcome::Succeeded {
            detail: "pushed".into(),
            restored: Restoration::Restored,
        }
    }

    #[test]
    fn green_wants_refusals_and_holds() {
        assert_eq!(judge(Expect::Green, &named()).0, Verdict::Pass);
        assert_eq!(judge(Expect::Green, &unidentified()).0, Verdict::Pass);
        assert!(judge(Expect::Green, &unidentified()).1.contains("finding"));
        assert_eq!(judge(Expect::Green, &succeeded()).0, Verdict::Fail);
        assert_eq!(judge(Expect::Green, &Outcome::Holds).0, Verdict::Pass);
        assert_eq!(
            judge(Expect::Green, &Outcome::Violated { detail: "x".into() }).0,
            Verdict::Fail
        );
    }

    #[test]
    fn red_wants_the_control_absent() {
        assert_eq!(judge(Expect::Red, &succeeded()).0, Verdict::Pass);
        assert_eq!(
            judge(Expect::Red, &Outcome::Violated { detail: "x".into() }).0,
            Verdict::Pass
        );
        assert_eq!(judge(Expect::Red, &named()).0, Verdict::Fail);
        assert_eq!(judge(Expect::Red, &unidentified()).0, Verdict::Fail);
        assert_eq!(judge(Expect::Red, &Outcome::Holds).0, Verdict::Fail);
    }

    #[test]
    fn not_run_is_unproven_under_either_expectation() {
        let nr = Outcome::NotRun {
            reason: "needs a second identity".into(),
        };
        let er = Outcome::Errored {
            detail: "git missing".into(),
        };
        for e in [Expect::Red, Expect::Green] {
            assert_eq!(judge(e, &nr).0, Verdict::Unproven);
            assert_eq!(judge(e, &er).0, Verdict::Unproven);
        }
    }

    #[test]
    fn failed_restoration_is_shouted() {
        let o = Outcome::Succeeded {
            detail: "d".into(),
            restored: Restoration::Failed { detail: "x".into() },
        };
        assert!(judge(Expect::Green, &o).1.contains("RESTORATION FAILED"));
        assert!(judge(Expect::Red, &o).1.contains("RESTORATION FAILED"));
    }
}
