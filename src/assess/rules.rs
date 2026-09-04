//! The qualification table of the operating model §2, as rules.
//!
//! Each check yields a finding with what was observed, where that came
//! from (measured here, declared in the profile, or unknown), a rating,
//! and — when not fine — what would have to change. Unknown is rated as
//! the most restrictive value the check can take: anything incomplete
//! starts at the most restrictive treatment or waits.
//!
//! Thresholds are the design's, cited inline. They are the judgement in
//! this file; the rest is bookkeeping.

use serde::Serialize;

use super::measure::Measured;
use super::profile::{Profile, Verdict};

/// Flake above this makes the inner loop noise for the agent (§2: "above ~2%").
pub const FLAKE_CEILING_PERCENT: f64 = 2.0;
/// The inner loop must come back under this or the mode is `m3-staged` (§2).
pub const INNER_LOOP_CEILING_MINUTES: f64 = 5.0;
/// More distinct build systems than this is the Spotify finding in numbers.
pub const BUILD_SYSTEMS_CEILING: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Check {
    ColdBuild,
    TestEntryPoint,
    FlakeRate,
    CiClock,
    EnvironmentDeterminism,
    OwnershipMap,
    MutationScore,
    Heterogeneity,
    BranchProtection,
}

impl Check {
    pub fn title(self) -> &'static str {
        match self {
            Check::ColdBuild => "Cold build",
            Check::TestEntryPoint => "Test entry point",
            Check::FlakeRate => "Flake rate",
            Check::CiClock => "CI clock",
            Check::EnvironmentDeterminism => "Environment determinism",
            Check::OwnershipMap => "Ownership map",
            Check::MutationScore => "Mutation score",
            Check::Heterogeneity => "Heterogeneity",
            Check::BranchProtection => "Branch protection",
        }
    }

    /// Why this check decides the mode (§2, verbatim in spirit).
    pub fn why(self) -> &'static str {
        match self {
            Check::ColdBuild => "without one documented command, every agent session begins by rediscovering the build — cost per session, not per project",
            Check::TestEntryPoint => "an agent cannot use a suite it cannot invoke",
            Check::FlakeRate => "flake is indistinguishable from regression to the agent; above ~2% the inner loop is noise",
            Check::CiClock => "in D3 the agent calls CI as an inner-loop tool, so CI latency is the dominant cost driver",
            Check::EnvironmentDeterminism => "determines whether isolation is available at all",
            Check::OwnershipMap => "the path registry has nothing to bind to otherwise",
            Check::MutationScore => "the only test-quality signal that survives agents writing tests — not coverage",
            Check::Heterogeneity => "factories are significantly better on standardised codebases, measurably worse on fragmented repos",
            Check::BranchProtection => "if required checks cannot be added by someone other than the developers, there is no F3 boundary on this project",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Measured,
    Declared,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rating {
    Ok,
    /// Caps the verdict at amber.
    Concern,
    /// Caps the verdict at red.
    Blocking,
}

#[derive(Debug, Serialize)]
pub struct Finding {
    pub check: Check,
    pub observed: String,
    pub source: Source,
    pub rating: Rating,
    /// What would have to change. Present whenever the rating is not ok.
    pub to_change: Option<String>,
}

/// The most delegation the substrate supports. Ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModeCeiling {
    /// Red: D2 only.
    D2Only,
    /// Amber, or a slow inner loop: `m3-staged`.
    M3Staged,
    /// Green with a fast inner loop: `m3-session`.
    M3Session,
}

impl ModeCeiling {
    pub fn as_str(self) -> &'static str {
        match self {
            ModeCeiling::D2Only => "D2 only (m2-*)",
            ModeCeiling::M3Staged => "m3-staged",
            ModeCeiling::M3Session => "m3-session",
        }
    }
}

/// A declared `default_mode`, ranked against the ceiling. Closed on the
/// mode families the framework defines; anything else is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclaredMode {
    M2,
    M3Staged,
    M3Session,
    /// D4. Never admitted by substrate assessment — admission is per
    /// workload, by dokimasia, and needs an independent oracle.
    M4,
}

impl DeclaredMode {
    pub fn parse(text: &str) -> Option<DeclaredMode> {
        match text.trim() {
            t if t.starts_with("m2-") => Some(DeclaredMode::M2),
            "m3-staged" => Some(DeclaredMode::M3Staged),
            "m3-session" => Some(DeclaredMode::M3Session),
            t if t.starts_with("m4-") => Some(DeclaredMode::M4),
            _ => None,
        }
    }

    pub fn within(self, ceiling: ModeCeiling) -> bool {
        match self {
            DeclaredMode::M2 => true,
            DeclaredMode::M3Staged => ceiling >= ModeCeiling::M3Staged,
            DeclaredMode::M3Session => ceiling >= ModeCeiling::M3Session,
            DeclaredMode::M4 => false,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Assessment {
    pub findings: Vec<Finding>,
    pub verdict: Verdict,
    pub ceiling: ModeCeiling,
    /// The full-suite wait, reported separately from the inner loop (S1.2).
    pub full_suite_p95_minutes: Option<f64>,
    /// Why the profile is refused, if it is. A refusal is an exit code, not a warning.
    pub refusals: Vec<String>,
}

fn finding(
    check: Check,
    observed: impl Into<String>,
    source: Source,
    rating: Rating,
    to_change: Option<&str>,
) -> Finding {
    Finding {
        check,
        observed: observed.into(),
        source,
        rating,
        to_change: to_change.map(str::to_string),
    }
}

pub fn assess(profile: &Profile, measured: &Measured) -> Assessment {
    let a = &profile.assessment;
    let mut findings = Vec::with_capacity(9);

    findings.push(match &a.cold_build_command {
        Some(cmd) => finding(Check::ColdBuild, format!("`{cmd}`"), Source::Declared, Rating::Ok, None),
        None if !measured.build_systems.is_empty() => finding(
            Check::ColdBuild,
            format!("build system(s) detected: {}; no single documented command declared", join(&measured.build_systems)),
            Source::Measured,
            Rating::Concern,
            Some("document the one command a fresh clone builds with, and record it as assessment.cold_build_command"),
        ),
        None => finding(
            Check::ColdBuild,
            "no build system marker found and no command declared",
            Source::Unknown,
            Rating::Concern,
            Some("record assessment.cold_build_command, or accept that every agent session starts by discovering the build"),
        ),
    });

    findings.push(match (&profile.ci.inner_loop, a.test_green_on_main) {
        (Some(cmd), Some(true)) => finding(
            Check::TestEntryPoint,
            format!("`{cmd}`, green on main"),
            Source::Declared,
            Rating::Ok,
            None,
        ),
        (Some(cmd), Some(false)) => finding(
            Check::TestEntryPoint,
            format!("`{cmd}` is declared but NOT green on main"),
            Source::Declared,
            Rating::Blocking,
            Some("make main green before any agent runs; a red suite teaches the agent nothing"),
        ),
        (Some(cmd), None) => finding(
            Check::TestEntryPoint,
            format!("`{cmd}` is declared; whether it is green on main today is not recorded"),
            Source::Unknown,
            Rating::Blocking,
            Some("run it on main and record assessment.test_green_on_main"),
        ),
        (None, _) => finding(
            Check::TestEntryPoint,
            "no test entry point declared (ci.inner_loop)",
            Source::Unknown,
            Rating::Blocking,
            Some("declare the one command that runs the tests as ci.inner_loop"),
        ),
    });

    findings.push(match profile.ci.flake_rate_30d {
        Some(p) if p.0 <= FLAKE_CEILING_PERCENT => finding(Check::FlakeRate, format!("{}% over 30 days", p.0), Source::Declared, Rating::Ok, None),
        Some(p) => finding(
            Check::FlakeRate,
            format!("{}% over 30 days, above the {FLAKE_CEILING_PERCENT}% ceiling", p.0),
            Source::Declared,
            Rating::Concern,
            Some("quarantine or fix the flaky tests until main fails only for code causes"),
        ),
        None => finding(
            Check::FlakeRate,
            "not recorded",
            Source::Unknown,
            Rating::Concern,
            Some("measure the share of main runs failing without a code cause over the last 30 days and record ci.flake_rate_30d"),
        ),
    });

    findings.push(match profile.ci.inner_loop_p95_minutes {
        Some(m) if m < INNER_LOOP_CEILING_MINUTES => finding(Check::CiClock, format!("inner loop p95 {m} min"), Source::Declared, Rating::Ok, None),
        Some(m) => finding(
            Check::CiClock,
            format!("inner loop p95 {m} min, at or above the {INNER_LOOP_CEILING_MINUTES} min ceiling"),
            Source::Declared,
            Rating::Concern,
            Some("split the suite: a sub-five-minute inner loop (compile, unit, lint, type, static) and the full suite pre-PR; until then the mode is m3-staged"),
        ),
        None => finding(
            Check::CiClock,
            "inner loop p95 not recorded",
            Source::Unknown,
            Rating::Concern,
            Some("measure p95 commit-to-verdict for the inner loop and record ci.inner_loop_p95_minutes; until then the mode is m3-staged"),
        ),
    });

    findings.push(if measured.determinism_markers.is_empty() {
        finding(
            Check::EnvironmentDeterminism,
            "no container, devbox or toolchain pin found in the tree",
            Source::Measured,
            Rating::Concern,
            Some("pin the environment (container image, devbox, or toolchain files) so a sealed runner can reproduce it"),
        )
    } else {
        finding(Check::EnvironmentDeterminism, format!("pinned by {}", measured.determinism_markers.join(", ")), Source::Measured, Rating::Ok, None)
    });

    findings.push(match (&measured.codeowners, &profile.path_registry) {
        (Some(path), _) => finding(Check::OwnershipMap, format!("{path} present"), Source::Measured, Rating::Ok, None),
        (None, Some(registry)) => finding(
            Check::OwnershipMap,
            format!("path_registry declared as {registry}, but no CODEOWNERS found in the tree"),
            Source::Declared,
            Rating::Concern,
            Some("commit the ownership map the profile points at, or point the profile at the one that exists"),
        ),
        (None, None) => finding(
            Check::OwnershipMap,
            "no CODEOWNERS and no path_registry",
            Source::Unknown,
            Rating::Concern,
            Some("create an ownership map; the path registry binds to it"),
        ),
    });

    findings.push(match (profile.verdict_inputs.mutation.as_deref(), a.mutation_score) {
        (Some("none"), _) => finding(
            Check::MutationScore,
            "verdict_inputs.mutation is none",
            Source::Declared,
            Rating::Concern,
            Some("no credible mutation tool for this stack: the lane ceiling drops, because nothing else makes agent-written tests trustworthy"),
        ),
        (Some(tool), Some(score)) => finding(Check::MutationScore, format!("{tool}, baseline {}%", score.0), Source::Declared, Rating::Ok, None),
        (Some(tool), None) => finding(
            Check::MutationScore,
            format!("{tool} declared, no baseline score recorded"),
            Source::Unknown,
            Rating::Concern,
            Some("run the mutation tool once on main and record assessment.mutation_score — the baseline cannot be reconstructed later"),
        ),
        (None, _) => finding(
            Check::MutationScore,
            "no mutation tool declared",
            Source::Unknown,
            Rating::Concern,
            Some("name the mutation tool for this stack in verdict_inputs.mutation, or `none` if there is no credible one"),
        ),
    });

    findings.push({
        let n = measured.build_systems.len();
        let langs = measured.languages.len();
        let observed = format!("{n} build system(s): {}; {langs} language(s)", if n == 0 { "none".to_string() } else { join(&measured.build_systems) });
        if n > BUILD_SYSTEMS_CEILING {
            finding(
                Check::Heterogeneity,
                observed,
                Source::Measured,
                Rating::Concern,
                Some("standardise the estate first, or scope the project to one build system; codebase standardisation is a prerequisite, not a parallel workstream"),
            )
        } else {
            finding(Check::Heterogeneity, observed, Source::Measured, Rating::Ok, None)
        }
    });

    findings.push(match a.required_checks_settable_by_non_developers {
        Some(true) => finding(Check::BranchProtection, "required checks settable by someone other than the developers", Source::Declared, Rating::Ok, None),
        Some(false) => finding(
            Check::BranchProtection,
            "required checks are NOT settable by anyone other than the developers",
            Source::Declared,
            Rating::Blocking,
            Some("move branch protection to a level the project developers cannot edit (organisation rulesets or an equivalent); without it there is no F3 boundary and no autonomous mode"),
        ),
        None => finding(
            Check::BranchProtection,
            "not recorded",
            Source::Unknown,
            Rating::Blocking,
            Some("establish who can add a required check and record assessment.required_checks_settable_by_non_developers; unknown is treated as no boundary"),
        ),
    });

    let worst = findings
        .iter()
        .map(|f| f.rating)
        .max()
        .unwrap_or(Rating::Ok);
    let verdict = match worst {
        Rating::Blocking => Verdict::Red,
        Rating::Concern => Verdict::Amber,
        Rating::Ok => Verdict::Green,
    };
    let inner_loop_ok = findings
        .iter()
        .any(|f| f.check == Check::CiClock && f.rating == Rating::Ok);
    let ceiling = match verdict {
        Verdict::Red => ModeCeiling::D2Only,
        Verdict::Amber => ModeCeiling::M3Staged,
        Verdict::Green if inner_loop_ok => ModeCeiling::M3Session,
        Verdict::Green => ModeCeiling::M3Staged,
    };

    let mut refusals = Vec::new();
    if let Some(declared) = profile.substrate {
        if declared > verdict {
            refusals.push(format!(
                "profile declares substrate {} but the assessment supports {} — a profile may not claim more than the measurement",
                declared.as_str(),
                verdict.as_str()
            ));
        }
    }
    if let Some(mode) = &profile.default_mode {
        match DeclaredMode::parse(mode) {
            None => refusals.push(format!("default_mode {mode:?} is not a mode this framework defines (m2-*, m3-staged, m3-session, m4-*)")),
            Some(DeclaredMode::M4) => refusals.push(format!(
                "default_mode {mode} is D4; substrate assessment never admits D4 — admission is per workload, by dokimasia, on an independent oracle"
            )),
            Some(m) if !m.within(ceiling) => refusals.push(format!(
                "default_mode {mode} exceeds the ceiling this substrate supports ({}) — the mode follows from the substrate, not from preference",
                ceiling.as_str()
            )),
            Some(_) => {}
        }
    }

    Assessment {
        findings,
        verdict,
        ceiling,
        full_suite_p95_minutes: profile.ci.full_suite_p95_minutes,
        refusals,
    }
}

fn join(set: &std::collections::BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assess::profile::Percent;

    fn measured_good() -> Measured {
        let mut m = Measured::default();
        m.build_systems.insert("gradle".into());
        m.determinism_markers.push("Dockerfile".into());
        m.codeowners = Some("CODEOWNERS".into());
        m
    }

    fn profile_good() -> Profile {
        let mut p = Profile {
            id: "p".into(),
            ..Default::default()
        };
        p.assessment.cold_build_command = Some("./gradlew build".into());
        p.assessment.test_green_on_main = Some(true);
        p.assessment.required_checks_settable_by_non_developers = Some(true);
        p.assessment.mutation_score = Some(Percent(61.0));
        p.ci.inner_loop = Some("./gradlew check".into());
        p.ci.inner_loop_p95_minutes = Some(4.0);
        p.ci.flake_rate_30d = Some(Percent(0.9));
        p.verdict_inputs.mutation = Some("pitest".into());
        p
    }

    #[test]
    fn everything_in_order_is_green_with_m3_session() {
        let a = assess(&profile_good(), &measured_good());
        assert_eq!(a.verdict, Verdict::Green, "{:#?}", a.findings);
        assert_eq!(a.ceiling, ModeCeiling::M3Session);
        assert!(a.refusals.is_empty());
        assert!(a.findings.iter().all(|f| f.to_change.is_none()));
    }

    #[test]
    fn nothing_known_is_red_because_unknown_is_restrictive() {
        let p = Profile {
            id: "p".into(),
            ..Default::default()
        };
        let a = assess(&p, &Measured::default());
        assert_eq!(a.verdict, Verdict::Red);
        assert_eq!(a.ceiling, ModeCeiling::D2Only);
        assert!(
            a.findings
                .iter()
                .filter(|f| f.rating != Rating::Ok)
                .all(|f| f.to_change.is_some()),
            "every non-ok finding names what must change"
        );
        assert!(a
            .findings
            .iter()
            .any(|f| f.source == Source::Unknown && f.rating == Rating::Blocking));
    }

    #[test]
    fn a_slow_inner_loop_caps_green_at_m3_staged() {
        let mut p = profile_good();
        p.ci.inner_loop_p95_minutes = Some(12.0);
        let a = assess(&p, &measured_good());
        assert_eq!(a.verdict, Verdict::Amber);
        assert_eq!(a.ceiling, ModeCeiling::M3Staged);
    }

    #[test]
    fn no_f3_boundary_is_red_however_good_the_rest() {
        let mut p = profile_good();
        p.assessment.required_checks_settable_by_non_developers = Some(false);
        let a = assess(&p, &measured_good());
        assert_eq!(a.verdict, Verdict::Red);
    }

    #[test]
    fn three_build_systems_is_amber() {
        let mut m = measured_good();
        m.build_systems.insert("npm".into());
        m.build_systems.insert("maven".into());
        assert_eq!(assess(&profile_good(), &m).verdict, Verdict::Amber);
    }

    #[test]
    fn a_profile_claiming_more_than_measured_is_refused() {
        let mut p = profile_good();
        p.ci.inner_loop_p95_minutes = Some(12.0); // amber
        p.substrate = Some(Verdict::Green);
        p.default_mode = Some("m3-session".into());
        let a = assess(&p, &measured_good());
        assert_eq!(a.refusals.len(), 2, "{:?}", a.refusals);
        assert!(a.refusals[0].contains("substrate green"));
        assert!(a.refusals[1].contains("exceeds the ceiling"));
    }

    #[test]
    fn a_profile_claiming_less_is_fine() {
        let mut p = profile_good();
        p.substrate = Some(Verdict::Amber);
        p.default_mode = Some("m2-review".into());
        assert!(assess(&p, &measured_good()).refusals.is_empty());
    }

    #[test]
    fn d4_is_never_admitted_by_assessment() {
        let mut p = profile_good();
        p.default_mode = Some("m4-flagged-rollout".into());
        let a = assess(&p, &measured_good());
        assert_eq!(a.verdict, Verdict::Green);
        assert_eq!(a.refusals.len(), 1);
        assert!(a.refusals[0].contains("D4"));
    }

    #[test]
    fn an_unknown_mode_is_refused() {
        let mut p = profile_good();
        p.default_mode = Some("yolo".into());
        assert!(assess(&p, &measured_good()).refusals[0].contains("not a mode"));
    }
}
