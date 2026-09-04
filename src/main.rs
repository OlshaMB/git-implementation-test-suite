mod model;
mod pack;
mod runner;
mod validation;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::model::DeltaSelection;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate and validate a pack through an implementation wrapper.
    Run {
        #[arg(long)]
        fixture: PathBuf,
        #[arg(long)]
        implementation: PathBuf,
        #[arg(long, value_enum, default_value_t = DeltaSelection::Both)]
        delta: DeltaSelection,
        /// Preserve generated packs and runner files in this directory.
        #[arg(long)]
        output_dir: Option<PathBuf>,
        /// Write the complete report as JSON.
        #[arg(long)]
        json: Option<PathBuf>,
        /// Do not run canonical Git's index-pack interoperability check.
        #[arg(long)]
        skip_git_validation: bool,
    },
    /// Validate and describe an existing pack.
    Inspect {
        pack: PathBuf,
        /// Write the complete report as JSON.
        #[arg(long)]
        json: Option<PathBuf>,
        /// Do not run canonical Git's index-pack interoperability check.
        #[arg(long)]
        skip_git_validation: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run {
            fixture,
            implementation,
            delta,
            output_dir,
            json,
            skip_git_validation,
        } => runner::run(
            &fixture,
            &implementation,
            delta,
            output_dir.as_deref(),
            json.as_deref(),
            !skip_git_validation,
        ),
        Command::Inspect {
            pack,
            json,
            skip_git_validation,
        } => runner::inspect(&pack, json.as_deref(), !skip_git_validation),
    }
}
