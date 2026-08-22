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
#[command(
    version,
    about = "Zero-trace, Git-anchored private asset manager for open-source repositories",
    long_about = "git-vault is an out-of-band private asset synchronizer.\n\
                  It binds private design docs (SPEC*.md, TODO*.md, AGENTS*.md) and local secrets (.env)\n\
                  to public Git commit SHAs in a separate central vault repo, using first-parent ancestor fallback.\n\n\
                  CORE RULES FOR AGENTS & SCRIPTS:\n\
                  - Public git repo MUST be clean before running 'push' or 'pull'. Commit public code first.\n\
                  - Private assets are ignored via .git/info/exclude (zero traces in public Git history).\n\
                  - 'push' always overwrites the snapshot slot for current HEAD SHA.\n\
                  - 'pull' will FAIL if local private files have unpushed changes. To overwrite, run 'clean' then 'pull'.\n\
                  - 'clean' deletes all local private files, putting the workspace into clean/dehydrated mode."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize git-vault for current repository and register exclude rules
    #[command(
        about = "Initialize git-vault for current repository",
        long_about = "Initializes git-vault in the current public Git repository.\n\n\
                      ACTIONS PERFORMED:\n\
                      1. Derives project_id from 'git remote get-url origin' (e.g. github.com/user/repo).\n\
                      2. Checks if any candidate private files are already tracked by public Git. ABORTS if tracked.\n\
                      3. Appends private file patterns to .git/info/exclude.\n\
                      4. Clones or initializes the local central vault repository (~/.vault/repo/)."
    )]
    Init {
        /// Optional central private vault remote URL (e.g. git@github.com:user/vault.git or http://...)
        vault_remote: Option<String>,
    },

    /// Snapshot workspace private assets anchored to current HEAD and push to vault
    #[command(
        about = "Snapshot workspace private assets anchored to current HEAD commit",
        long_about = "Snapshots all workspace private assets (SPEC*.md, TODO*.md, AGENTS*.md, .env, *.local.*, docs/private/**)\n\
                      anchored to the current public HEAD commit SHA and pushes them to the central vault repository.\n\n\
                      PRECONDITIONS:\n\
                      - Public repository working tree and index MUST be clean (no uncommitted public changes).\n\
                      - No private candidate files may be tracked in public Git history.\n\n\
                      BEHAVIOR:\n\
                      - Overwrites any previous snapshot bound to the same HEAD SHA.\n\
                      - If no private assets exist in workspace, writes an empty snapshot (files: []) to halt ancestor fallback."
    )]
    Push,

    /// Restore private assets aligned with current HEAD (via first-parent ancestor lookup)
    #[command(
        about = "Restore private assets aligned with current HEAD commit",
        long_about = "Finds the nearest ancestor snapshot along 'git rev-list --first-parent HEAD' and injects\n\
                      the private assets into the workspace.\n\n\
                      PRECONDITIONS:\n\
                      - Public repository working tree and index MUST be clean.\n\
                      - Local private assets must either be empty (dehydrated) or identical to target snapshot.\n\n\
                      BEHAVIOR:\n\
                      - If workspace has NO private files (dehydrated): Restores all snapshot files directly.\n\
                      - If workspace private files match target snapshot: Exits with success (no-op).\n\
                      - If workspace private files differ: ABORTS to protect local modifications.\n\
                        To discard local private changes and force pull: Run 'git vault clean' first, then 'git vault pull'."
    )]
    Pull,

    /// Delete all private assets from workspace to enter clean/dehydrated mode
    #[command(
        about = "Delete all private assets from workspace to enter clean/dehydrated mode",
        long_about = "Scans and deletes all workspace files matching private vault patterns (SPEC*.md, TODO*.md, AGENTS*.md, .env, *.local.*, docs/private/**).\n\n\
                      USE CASES:\n\
                      - Preparing for screen-recording, live demos, or public open-source inspection.\n\
                      - Resetting local private state before pulling a different snapshot version.\n\n\
                      SAFETY:\n\
                      - Only deletes files matching vault patterns.\n\
                      - Vault snapshots in ~/.vault/repo/ remain completely safe and intact."
    )]
    Clean,

    /// Check public worktree clean state, snapshot alignment, and private assets
    #[command(
        about = "Check public worktree state, snapshot alignment, and private assets",
        long_about = "Inspects and outputs the operational status of the workspace.\n\n\
                      OUTPUT FIELDS:\n\
                      - Public Worktree: 'clean' or 'dirty'\n\
                      - HEAD: Current 40-character public commit SHA\n\
                      - Target Snapshot: Matched SHA and distance along the first-parent chain\n\
                      - Private Assets:\n\
                          * 'dehydrated' (0 private files in workspace)\n\
                          * 'aligned' (local private assets match target snapshot exactly)\n\
                          * 'divergent' (local private assets differ from target snapshot)"
    )]
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
