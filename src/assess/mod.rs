//! `dok assess` — the substrate verdict: can this codebase support agentic
//! delivery at all, in which mode, and what would have to change.

pub mod measure;
pub mod profile;
pub mod rules;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use serde::Serialize;

use profile::Profile;
use rules::{Assessment, Rating, Source};

#[derive(clap::Args)]
pub struct Args {
    /// Working tree to measure.
    #[arg(long, default_value = ".")]
    pub repo: PathBuf,

    /// The project profile. Without one, every declared fact is unknown,
    /// and unknown is treated as the most restrictive value.
    #[arg(long)]
    pub profile: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = crate::conform::Format::Table)]
    pub format: crate::conform::Format,
}

#[derive(Serialize)]
struct Output<'a> {
    repo: String,
    profile: Option<&'a str>,
    measured: &'a measure::Measured,
    #[serde(flatten)]
    assessment: &'a Assessment,
    oracles: Vec<OracleRow>,
}

#[derive(Serialize)]
struct OracleRow {
    workload: String,
    class: &'static str,
    consequence: &'static str,
}

pub fn run(args: Args) -> Result<ExitCode> {
    let profile = match &args.profile {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            Profile::from_yaml(&text).with_context(|| format!("{}", path.display()))?
        }
        None => Profile {
            id: "(no profile)".into(),
            ..Default::default()
        },
    };
    let measured = measure::measure(&args.repo)
        .with_context(|| format!("measuring {}", args.repo.display()))?;
    let assessment = rules::assess(&profile, &measured);
    let oracles: Vec<OracleRow> = profile
        .oracles
        .iter()
        .map(|o| OracleRow {
            workload: o.workload.clone(),
            class: o.class.as_str(),
            consequence: o.class.consequence(),
        })
        .collect();

    let output = Output {
        repo: args.repo.display().to_string(),
        profile: args.profile.as_ref().map(|_| profile.id.as_str()),
        measured: &measured,
        assessment: &assessment,
        oracles,
    };
    match args.format {
        crate::conform::Format::Json => println!("{}", serde_json::to_string_pretty(&output)?),
        crate::conform::Format::Table => print!("{}", markdown(&output)),
    }
    Ok(if assessment.refusals.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn markdown(o: &Output) -> String {
    let a = o.assessment;
    let mut out = String::new();
    out.push_str(&format!(
        "## Substrate assessment — `{}`{}\n\n",
        o.repo,
        o.profile
            .map(|id| format!(" · profile `{id}`"))
            .unwrap_or_else(|| " · no profile, every declared fact unknown".into())
    ));
    out.push_str(&format!(
        "**Verdict: {}.** Mode ceiling: **{}**.",
        a.verdict.as_str().to_uppercase(),
        a.ceiling.as_str()
    ));
    match a.full_suite_p95_minutes {
        Some(m) => out.push_str(&format!(
            " Full suite p95: {m} min, reported separately from the inner loop.\n\n"
        )),
        None => out.push_str(" Full suite p95: not recorded.\n\n"),
    }
    out.push_str("| Check | Observed | Source | Rating |\n|---|---|---|---|\n");
    for f in &a.findings {
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            f.check.title(),
            cell(&f.observed),
            source(f.source),
            rating(f.rating)
        ));
    }
    let changes: Vec<&rules::Finding> = a
        .findings
        .iter()
        .filter(|f| f.to_change.is_some())
        .collect();
    if !changes.is_empty() {
        out.push_str("\n### What would have to change\n\n");
        for f in changes {
            out.push_str(&format!(
                "- **{}** — {}. *Why it decides the mode:* {}.\n",
                f.check.title(),
                f.to_change.as_deref().unwrap_or(""),
                f.check.why()
            ));
        }
    }
    out.push_str("\n### Oracle map\n\n");
    if o.oracles.is_empty() {
        out.push_str("No oracles declared. Every area is therefore an area with no independent check: no lane above 3 anywhere.\n");
    } else {
        out.push_str("| Workload | Oracle class | Consequence |\n|---|---|---|\n");
        for r in &o.oracles {
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                r.workload, r.class, r.consequence
            ));
        }
    }
    if !a.refusals.is_empty() {
        out.push_str("\n### Refused\n\n");
        for r in &a.refusals {
            out.push_str(&format!("- {r}\n"));
        }
        out.push_str(
            "\nThe profile is refused, not warned about. Fix the profile or fix the substrate.\n",
        );
    }
    out
}

fn source(s: Source) -> &'static str {
    match s {
        Source::Measured => "measured",
        Source::Declared => "declared",
        Source::Unknown => "**unknown**",
    }
}

fn rating(r: Rating) -> &'static str {
    match r {
        Rating::Ok => "ok",
        Rating::Concern => "concern",
        Rating::Blocking => "**blocking**",
    }
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}
