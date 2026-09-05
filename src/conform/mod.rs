//! `dok conform` — run a conformance pack and report every probe.

pub mod probe;
pub mod report;
pub mod spec;
pub mod verdict;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};

use probe::Executor;
use report::{Record, Report};
use spec::{Identity, Pack, Probe};
use verdict::{judge, Expect, Outcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Table,
    Json,
}

#[derive(clap::Args)]
pub struct Args {
    /// The pack to run (YAML).
    #[arg(long)]
    pub pack: PathBuf,

    /// Which identity class the credential in use belongs to. A probe is
    /// only counted when run under an identity it is meaningful for.
    #[arg(long = "as", value_enum)]
    pub identity: Identity,

    /// The state this run expects: red before protection is applied, green
    /// after. A rehearsal that is never seen red proves nothing.
    #[arg(long, value_enum)]
    pub expect: Expect,

    /// Git remote for push attempts. Defaults to the target repository on
    /// github.com over HTTPS; a local path works for rehearsal.
    #[arg(long)]
    pub remote: Option<String>,

    /// The `gh` binary to use for API probes.
    #[arg(long, default_value = "gh")]
    pub gh: PathBuf,

    #[arg(long, value_enum, default_value_t = Format::Table)]
    pub format: Format,

    /// Only run probes with these ids.
    #[arg(long = "only")]
    pub only: Vec<String>,

    /// List what would be attempted, attempt nothing, exit 0.
    #[arg(long)]
    pub dry_run: bool,
}

pub fn run(args: Args) -> Result<ExitCode> {
    let text = std::fs::read_to_string(&args.pack)
        .with_context(|| format!("reading {}", args.pack.display()))?;
    let pack = Pack::from_yaml(&text).with_context(|| format!("{}", args.pack.display()))?;
    let remote = args
        .remote
        .clone()
        .unwrap_or_else(|| format!("https://github.com/{}.git", pack.target.repository));
    let executor = Executor {
        repository: pack.target.repository.clone(),
        remote,
        gh: args.gh.clone(),
    };

    let selected: Vec<&Probe> = pack
        .probes
        .iter()
        .filter(|p| args.only.is_empty() || args.only.iter().any(|id| id == &p.id))
        .collect();
    if selected.is_empty() {
        anyhow::bail!("no probe matches --only {:?}", args.only);
    }

    if args.dry_run {
        println!(
            "pack {} → {} as {}, expecting {:?}",
            pack.name,
            pack.target.repository,
            args.identity.as_str(),
            args.expect
        );
        for p in &selected {
            let gate = identity_gate(p, args.identity);
            println!(
                "  {:<8} {:<10} {}",
                p.id,
                match gate {
                    Some(_) => "not-run",
                    None => "would-run",
                },
                gate.unwrap_or_else(|| describe(p))
            );
        }
        return Ok(ExitCode::SUCCESS);
    }

    let mut records = Vec::with_capacity(selected.len());
    for p in &selected {
        let outcome = match identity_gate(p, args.identity) {
            Some(reason) => Outcome::NotRun { reason },
            None => match (&p.attempt, &p.assert) {
                (Some(attempt), _) => executor.attempt(attempt, &p.refused_by),
                (None, Some(assertion)) => executor.assert(assertion),
                (None, None) => unreachable!("validated at load"),
            },
        };
        let (verdict, note) = judge(args.expect, &outcome);
        records.push(Record {
            id: p.id.clone(),
            claim: p.claim.clone(),
            outcome,
            verdict,
            note,
        });
    }

    let report = Report {
        pack: pack.name.clone(),
        repository: pack.target.repository.clone(),
        identity: args.identity.as_str().to_string(),
        expect: format!("{:?}", args.expect).to_lowercase(),
        records,
    };
    match args.format {
        Format::Table => print!("{}", report.markdown()),
        Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
    }
    Ok(if report.all_pass() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

/// `Some(reason)` when the probe must not be counted under this identity.
pub fn identity_gate(probe: &Probe, identity: Identity) -> Option<String> {
    if probe.run_as.contains(&identity) {
        return None;
    }
    let allowed: Vec<&str> = probe.run_as.iter().map(|i| i.as_str()).collect();
    Some(format!(
        "meaningful only as {}; running as {}",
        allowed.join(" or "),
        identity.as_str()
    ))
}

fn describe(probe: &Probe) -> String {
    match (&probe.attempt, &probe.assert) {
        (Some(a), _) => format!("attempt {a:?}"),
        (None, Some(a)) => format!("assert {a:?}"),
        (None, None) => String::new(),
    }
}
