use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod parser;
mod sops;
mod sync;

use sync::{check_files, sync_files};

#[derive(Parser)]
#[command(name = "sops-shell")]
#[command(about = "Sync secrets from shell commands to SOPS encrypted files")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Sync {
        #[arg(required = true, help = "SOPS encrypted files to sync")]
        files: Vec<PathBuf>,
    },
    Check {
        #[arg(required = true, help = "SOPS encrypted files to check")]
        files: Vec<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Sync { files } => {
            for file in &files {
                if !file.exists() {
                    return Err(anyhow!("File not found: {}", file.display()));
                }
            }
            sync_files(&files)?
        },
        Commands::Check { files } => {
            for file in &files {
                if !file.exists() {
                    return Err(anyhow!("File not found: {}", file.display()));
                }
            }
            check_files(&files)?
        },
    }

    Ok(())
}
