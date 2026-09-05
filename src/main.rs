//! `dok` — the Dokimastes command line.
//!
//! Release 0 does not exist yet. Three verbs are implemented: `assess`, the
//! substrate verdict on a codebase, `baseline`, the before number from the
//! repository's own history, and `conform`, which runs the
//! negative-capability suite — the set of things the framework claims an
//! agent *cannot* do, each tried for real and each refusal attributed to
//! the mechanism that produced it.
//!
//! Everything else the build list assigns to this binary (`classify`,
//! `validate`, `register`, `approve`, …) is deliberately absent
//! rather than stubbed. A verb that exists and decides nothing would be read
//! as a gate.

use dok::{assess, baseline, conform};

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dok", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Assess whether a codebase can support agentic delivery, and in which mode.
    Assess(assess::Args),
    /// Capture the before number: the retrospective metric contract from the repository's history.
    Baseline(baseline::Args),
    /// Run a conformance pack against a repository and report every probe.
    Conform(conform::Args),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Assess(args) => assess::run(args),
        Command::Baseline(args) => baseline::run(args),
        Command::Conform(args) => conform::run(args),
    };
    match result {
        Ok(status) => status,
        Err(err) => {
            eprintln!("dok: {err:#}");
            ExitCode::from(2)
        }
    }
}
