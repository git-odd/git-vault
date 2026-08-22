mod config;
mod git;
mod manifest;
mod pattern;
mod commands;

use clap::{Parser, Subcommand};
use std::process::exit;

#[derive(Parser)]
#[command(name = "git-vault")]
#[command(bin_name = "git-vault")]
#[command(version, about = "Zero-trace, Git-anchored private asset manager for open source developers")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize git-vault for current repository and register exclude rules
    Init {
        /// Optional central private vault remote URL (e.g. git@github.com:user/vault.git)
        vault_remote: Option<String>,
    },
    /// Snapshot workspace private assets anchored to HEAD commit and push to vault
    Push,
    /// Restore private assets aligned with current HEAD (via first-parent ancestor lookup)
    Pull,
    /// Delete all private assets from workspace to enter clean/dehydrated mode
    Clean,
    /// Check public worktree clean state, snapshot alignment, and private assets
    Status,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { vault_remote } => commands::cmd_init(vault_remote),
        Commands::Push => commands::cmd_push(),
        Commands::Pull => commands::cmd_pull(),
        Commands::Clean => commands::cmd_clean(),
        Commands::Status => commands::cmd_status(),
    };

    if let Err(err) = result {
        eprintln!("{}", err);
        exit(1);
    }
}
