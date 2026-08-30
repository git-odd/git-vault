use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const MANAGED_BLOCK_START: &str = "# >>> git-vault managed patterns >>>";
pub const MANAGED_BLOCK_END: &str = "# <<< git-vault managed patterns <<<";
pub const HOOK_MARKER: &str = "# git-vault managed hook";

pub fn run_git_cmd(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to execute 'git {}': {}", args.join(" "), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let err_msg = if !stderr.trim().is_empty() {
            stderr.trim()
        } else {
            stdout.trim()
        };
        return Err(format!("'git {}' failed: {}", args.join(" "), err_msg));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn get_repo_root() -> Result<PathBuf, String> {
    let current_dir = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?;
    let root_str = run_git_cmd(&current_dir, &["rev-parse", "--show-toplevel"])
        .map_err(|_| "Not a git repository (or any of the parent directories).".to_string())?;
    Ok(PathBuf::from(root_str))
}

pub fn is_worktree_clean(repo_root: &Path) -> Result<bool, String> {
    let diff_working = Command::new("git")
        .current_dir(repo_root)
        .args(["diff", "--quiet"])
        .status()
        .map_err(|e| format!("Failed to run git diff: {}", e))?;

    let diff_cached = Command::new("git")
        .current_dir(repo_root)
        .args(["diff", "--cached", "--quiet"])
        .status()
        .map_err(|e| format!("Failed to run git diff --cached: {}", e))?;

    Ok(diff_working.success() && diff_cached.success())
}

pub fn get_head_sha(repo_root: &Path) -> Result<String, String> {
    run_git_cmd(repo_root, &["rev-parse", "HEAD"])
        .map_err(|e| format!("Failed to get HEAD commit SHA: {}", e))
}

pub fn get_first_parent_ancestors(repo_root: &Path) -> Result<Vec<String>, String> {
    let output = run_git_cmd(repo_root, &["rev-list", "--first-parent", "HEAD"])
        .map_err(|e| format!("Failed to get first-parent commit list: {}", e))?;

    let commits = output
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(commits)
}

pub fn get_origin_url(repo_root: &Path) -> Result<String, String> {
    run_git_cmd(repo_root, &["remote", "get-url", "origin"])
        .map_err(|_| "Remote 'origin' not found. Please ensure the repository has a configured origin remote.".to_string())
}

pub fn parse_project_id(origin_url: &str) -> Result<String, String> {
    let mut url = origin_url.trim().to_string();
    if url.ends_with(".git") {
        url.truncate(url.len() - 4);
    }
    url = url.trim_end_matches('/').to_string();

    // SSH format: git@github.com:user/repo or user@host:port/path
    if let Some(at_idx) = url.find('@') {
        if let Some(colon_idx) = url[at_idx..].find(':') {
            let actual_colon = at_idx + colon_idx;
            // Ensure this is not a URL scheme with port like ssh://user@host:22/path
            if !url.contains("://") {
                let host = &url[at_idx + 1..actual_colon];
                let path = url[actual_colon + 1..].trim_start_matches('/');
                if !host.is_empty() && !path.is_empty() {
                    return Ok(format!("{}/{}", host, path));
                }
            }
        }
    }

    // URL schemes: https://, http://, ssh://, git://
    if let Some(scheme_idx) = url.find("://") {
        let after_scheme = &url[scheme_idx + 3..];
        // Strip user if any (e.g. user@host:port/path)
        let without_user = if let Some(at_idx) = after_scheme.find('@') {
            &after_scheme[at_idx + 1..]
        } else {
            after_scheme
        };

        // Split host and path
        if let Some(slash_idx) = without_user.find('/') {
            let mut host = &without_user[..slash_idx];
            let path = without_user[slash_idx + 1..].trim_start_matches('/');
            // Strip port from host if present
            if let Some(port_idx) = host.find(':') {
                host = &host[..port_idx];
            }
            if !host.is_empty() && !path.is_empty() {
                return Ok(format!("{}/{}", host, path));
            }
        }
    }

    Err(format!(
        "Failed to parse project_id from origin URL: '{}'. Expected format like 'git@host:user/repo.git' or 'https://host/user/repo.git'",
        origin_url
    ))
}

pub fn check_tracked_files(repo_root: &Path, candidates: &[String]) -> Result<Vec<String>, String> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let mut tracked = Vec::new();
    for file in candidates {
        let output = run_git_cmd(repo_root, &["ls-files", "--", file])?;
        if !output.trim().is_empty() {
            tracked.push(file.clone());
        }
    }
    Ok(tracked)
}

pub fn resolve_commit_sha(repo_root: &Path, rev: &str) -> Result<String, String> {
    run_git_cmd(repo_root, &["rev-parse", "--verify", rev])
        .map_err(|_| format!("Unknown revision or commit: '{}'", rev))
}

pub fn is_command_in_path(cmd: &str) -> bool {
    let check_cmd = if cfg!(windows) { "where" } else { "which" };
    Command::new(check_cmd)
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn generate_file_diff(
    rel_path: &str,
    old_file: Option<&Path>,
    new_file: Option<&Path>,
) -> Result<Option<String>, String> {
    let null_target = "/dev/null";

    let (old_arg, new_arg, mode) = match (old_file, new_file) {
        (Some(old_p), Some(new_p)) => (old_p.to_str().unwrap_or(""), new_p.to_str().unwrap_or(""), "modified"),
        (None, Some(new_p)) => (null_target, new_p.to_str().unwrap_or(""), "new"),
        (Some(old_p), None) => (old_p.to_str().unwrap_or(""), null_target, "deleted"),
        (None, None) => return Ok(None),
    };

    let output = Command::new("git")
        .args(["diff", "--no-index", "--color=never", "--", old_arg, new_arg])
        .output()
        .map_err(|e| format!("Failed to run git diff: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok(None);
    }

    let norm_path = rel_path.replace('\\', "/");

    // Format header
    let hunk_start_idx = stdout.find("@@").or_else(|| stdout.find("Binary files"));
    let hunks = match hunk_start_idx {
        Some(idx) => &stdout[idx..],
        None => stdout.as_ref(),
    };

    let formatted_diff = match mode {
        "new" => format!(
            "diff --git a/{rel} b/{rel}\nnew file mode 100644\n--- /dev/null\n+++ b/{rel}\n{}",
            hunks,
            rel = norm_path
        ),
        "deleted" => format!(
            "diff --git a/{rel} b/{rel}\ndeleted file mode 100644\n--- a/{rel}\n+++ /dev/null\n{}",
            hunks,
            rel = norm_path
        ),
        _ => format!(
            "diff --git a/{rel} b/{rel}\n--- a/{rel}\n+++ b/{rel}\n{}",
            hunks,
            rel = norm_path
        ),
    };

    Ok(Some(formatted_diff))
}

#[derive(Debug, Clone)]
pub struct DiffFileStat {
    pub file_path: String,
    pub insertions: usize,
    pub deletions: usize,
    pub is_binary: bool,
}

pub fn calculate_diff_stat(diff_text: &str, file_path: &str) -> DiffFileStat {
    if diff_text.contains("Binary files") {
        return DiffFileStat {
            file_path: file_path.to_string(),
            insertions: 0,
            deletions: 0,
            is_binary: true,
        };
    }

    let mut insertions = 0;
    let mut deletions = 0;
    let mut in_hunk = false;

    for line in diff_text.lines() {
        if line.starts_with("@@ ") {
            in_hunk = true;
            continue;
        }
        if in_hunk {
            if line.starts_with('+') && !line.starts_with("+++") {
                insertions += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                deletions += 1;
            }
        }
    }

    DiffFileStat {
        file_path: file_path.to_string(),
        insertions,
        deletions,
        is_binary: false,
    }
}

pub fn format_diff_stat_summary(stats: &[DiffFileStat]) -> String {
    if stats.is_empty() {
        return String::new();
    }

    let max_len = stats.iter().map(|s| s.file_path.len()).max().unwrap_or(10);
    let max_changes = stats.iter().map(|s| s.insertions + s.deletions).max().unwrap_or(1);
    let bar_width = 30usize;

    let mut out = String::new();
    let mut total_ins = 0;
    let mut total_del = 0;

    for stat in stats {
        total_ins += stat.insertions;
        total_del += stat.deletions;

        if stat.is_binary {
            out.push_str(&format!(" {:<width$} | Bin\n", stat.file_path, width = max_len));
            continue;
        }

        let changes = stat.insertions + stat.deletions;
        let (plus_count, minus_count) = if max_changes > 0 {
            let total_bar = ((changes as f64 / max_changes as f64) * (bar_width as f64)).ceil() as usize;
            let total_bar = std::cmp::max(1, std::cmp::min(bar_width, total_bar));
            let plus = ((stat.insertions as f64 / changes.max(1) as f64) * (total_bar as f64)).round() as usize;
            let minus = total_bar.saturating_sub(plus);
            (plus, minus)
        } else {
            (0, 0)
        };

        out.push_str(&format!(
            " {:<width$} | {:>4} {}{}\n",
            stat.file_path,
            changes,
            "+".repeat(plus_count),
            "-".repeat(minus_count),
            width = max_len
        ));
    }

    let file_count = stats.len();
    let file_suffix = if file_count == 1 { "file" } else { "files" };
    let mut summary_parts = vec![format!("{} {} changed", file_count, file_suffix)];

    if total_ins > 0 {
        summary_parts.push(format!("{} insertion{}(+)", total_ins, if total_ins == 1 { "" } else { "s" }));
    }
    if total_del > 0 {
        summary_parts.push(format!("{} deletion{}(-)", total_del, if total_del == 1 { "" } else { "s" }));
    }

    out.push_str(&format!(" {}\n", summary_parts.join(", ")));
    out
}

pub fn colorize_unified_diff(diff: &str) -> String {
    let mut out = String::with_capacity(diff.len() * 12 / 10);
    for line in diff.lines() {
        if line.starts_with("diff --git") || line.starts_with("index ") {
            out.push_str("\x1b[1m");
            out.push_str(line);
            out.push_str("\x1b[0m\n");
        } else if line.starts_with("--- ") || line.starts_with("+++ ") || line.starts_with("new file") || line.starts_with("deleted file") {
            out.push_str("\x1b[1m");
            out.push_str(line);
            out.push_str("\x1b[0m\n");
        } else if line.starts_with("@@ ") {
            out.push_str("\x1b[36m");
            out.push_str(line);
            out.push_str("\x1b[0m\n");
        } else if line.starts_with('+') {
            out.push_str("\x1b[32m");
            out.push_str(line);
            out.push_str("\x1b[0m\n");
        } else if line.starts_with('-') {
            out.push_str("\x1b[31m");
            out.push_str(line);
            out.push_str("\x1b[0m\n");
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn pipe_to_command(cmd: &str, args: &[&str], input: &str) -> Result<(), String> {
    use std::io::Write;
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", cmd, e))?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }

    let _ = child.wait();
    Ok(())
}

fn pipe_to_custom_pager(pager_cmd_str: &str, input: &str) -> Result<(), String> {
    let parts: Vec<&str> = pager_cmd_str.split_whitespace().collect();
    if parts.is_empty() {
        return Err("Empty pager command".to_string());
    }
    let cmd = parts[0];
    let args = &parts[1..];
    pipe_to_command(cmd, args, input)
}

pub fn output_diff_text(
    diff_text: &str,
    force_side_by_side: bool,
    force_unified: bool,
    no_pager: bool,
    no_paging: bool,
    paging: Option<&str>,
    config_side_by_side: Option<bool>,
    config_pager: Option<&str>,
    config_paging: Option<&str>,
) -> Result<(), String> {
    use std::io::IsTerminal;
    if diff_text.is_empty() {
        return Ok(());
    }

    let is_tty = std::io::stdout().is_terminal();

    if no_pager || !is_tty {
        print!("{}", diff_text);
        return Ok(());
    }

    let disable_paging = no_paging
        || paging == Some("never")
        || (paging.is_none() && config_paging == Some("never"));

    let delta_available = is_command_in_path("delta");

    let use_side_by_side = if force_unified {
        false
    } else if force_side_by_side {
        true
    } else if let Some(pref) = config_side_by_side {
        pref
    } else {
        false
    };

    if delta_available {
        let mut delta_args = Vec::new();
        if use_side_by_side {
            delta_args.push("--side-by-side".to_string());
        }
        if disable_paging {
            delta_args.push("--paging=never".to_string());
        } else if let Some(p) = paging {
            delta_args.push(format!("--paging={}", p));
        }
        let delta_args_refs: Vec<&str> = delta_args.iter().map(|s| s.as_str()).collect();
        if pipe_to_command("delta", &delta_args_refs, diff_text).is_ok() {
            return Ok(());
        }
    }

    if disable_paging {
        let colored = colorize_unified_diff(diff_text);
        print!("{}", colored);
        return Ok(());
    }

    if let Some(custom_pager) = config_pager {
        if !custom_pager.is_empty() && pipe_to_custom_pager(custom_pager, diff_text).is_ok() {
            return Ok(());
        }
    }

    if let Ok(git_pager) = std::env::var("GIT_PAGER") {
        if !git_pager.is_empty() && pipe_to_custom_pager(&git_pager, diff_text).is_ok() {
            return Ok(());
        }
    }

    if let Ok(git_core_pager) = run_git_cmd(Path::new("."), &["config", "core.pager"]) {
        if !git_core_pager.trim().is_empty() && pipe_to_custom_pager(&git_core_pager, diff_text).is_ok() {
            return Ok(());
        }
    }

    if let Ok(env_pager) = std::env::var("PAGER") {
        if !env_pager.is_empty() && pipe_to_custom_pager(&env_pager, diff_text).is_ok() {
            return Ok(());
        }
    }

    if is_command_in_path("less") {
        let colored = colorize_unified_diff(diff_text);
        if pipe_to_command("less", &["-RFX"], &colored).is_ok() {
            return Ok(());
        }
    }

    let colored = colorize_unified_diff(diff_text);
    print!("{}", colored);
    Ok(())
}

/// Idempotently synchronizes the managed block in `.git/info/exclude`.
/// Preserves any user-defined lines outside the managed block.
pub fn sync_exclude_patterns(repo_root: &Path, raw_lines: &[String]) -> Result<(), String> {
    let git_dir = repo_root.join(".git");
    if !git_dir.exists() {
        return Err(format!("Not a git repository: {}", repo_root.display()));
    }

    let info_dir = git_dir.join("info");
    if !info_dir.exists() {
        fs::create_dir_all(&info_dir)
            .map_err(|e| format!("Failed to create directory {}: {}", info_dir.display(), e))?;
    }

    let exclude_path = info_dir.join("exclude");
    let existing = if exclude_path.exists() {
        fs::read_to_string(&exclude_path)
            .map_err(|e| format!("Failed to read {}: {}", exclude_path.display(), e))?
    } else {
        String::new()
    };

    // Build the new managed block content
    let mut block = String::new();
    block.push_str(MANAGED_BLOCK_START);
    block.push_str("\n# Auto-generated by git-vault. Do not edit this block manually.\n");
    block.push_str(crate::pattern::VAULTIGNORE_FILENAME);
    block.push('\n');
    for line in raw_lines {
        let trimmed = line.trim();
        if !trimmed.is_empty() && trimmed != crate::pattern::VAULTIGNORE_FILENAME {
            block.push_str(trimmed);
            block.push('\n');
        }
    }
    block.push_str(MANAGED_BLOCK_END);

    let new_content = if let (Some(start_idx), Some(end_idx)) = (
        existing.find(MANAGED_BLOCK_START),
        existing.find(MANAGED_BLOCK_END),
    ) {
        if start_idx <= end_idx {
            let before = &existing[..start_idx];
            let after_end = end_idx + MANAGED_BLOCK_END.len();
            let after = &existing[after_end..];
            format!("{}{}{}", before, block, after)
        } else {
            // Malformed blocks, append at the end
            format!("{}\n\n{}\n", existing.trim_end(), block)
        }
    } else {
        // No existing block, append at the end
        if existing.trim().is_empty() {
            format!("{}\n", block)
        } else {
            format!("{}\n\n{}\n", existing.trim_end(), block)
        }
    };

    fs::write(&exclude_path, new_content)
        .map_err(|e| format!("Failed to write {}: {}", exclude_path.display(), e))?;

    Ok(())
}


pub fn install_hooks(repo_root: &Path) -> Result<Vec<String>, String> {
    let git_dir = repo_root.join(".git");
    if !git_dir.exists() {
        return Err(format!("Not a git repository: {}", repo_root.display()));
    }

    let hooks_dir = git_dir.join("hooks");
    if !hooks_dir.exists() {
        fs::create_dir_all(&hooks_dir)
            .map_err(|e| format!("Failed to create directory {}: {}", hooks_dir.display(), e))?;
    }

    let pre_push_script = format!(
        r#"#!/bin/sh
{}
if [ "$GIT_VAULT_SKIP_HOOK" = "1" ]; then
    exit 0
fi
command -v git-vault >/dev/null 2>&1 || exit 0

echo "[git-vault] Syncing private assets to vault before push..."
git vault push
exit $?
"#,
        HOOK_MARKER
    );

    let post_checkout_script = format!(
        r#"#!/bin/sh
{}
# $1: previous HEAD, $2: new HEAD, $3: flag (1 = branch checkout, 0 = file checkout)
if [ "$GIT_VAULT_SKIP_HOOK" = "1" ] || [ "$3" != "1" ]; then
    exit 0
fi
command -v git-vault >/dev/null 2>&1 || exit 0

if ! git vault pull; then
    echo "[git-vault] Aborting branch switch. Rolling back to previous commit $1..."
    GIT_VAULT_SKIP_HOOK=1 git checkout "$1" >/dev/null 2>&1
    exit 1
fi
"#,
        HOOK_MARKER
    );

    let post_merge_script = format!(
        r#"#!/bin/sh
{}
if [ "$GIT_VAULT_SKIP_HOOK" = "1" ]; then
    exit 0
fi
command -v git-vault >/dev/null 2>&1 || exit 0

git vault pull || true
"#,
        HOOK_MARKER
    );

    let hooks = [
        ("pre-push", pre_push_script),
        ("post-checkout", post_checkout_script),
        ("post-merge", post_merge_script),
    ];

    let mut installed = Vec::new();

    for (name, content) in &hooks {
        let hook_path = hooks_dir.join(name);
        fs::write(&hook_path, content)
            .map_err(|e| format!("Failed to write hook {}: {}", hook_path.display(), e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = fs::metadata(&hook_path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o755);
                let _ = fs::set_permissions(&hook_path, perms);
            }
        }

        installed.push(name.to_string());
    }

    Ok(installed)
}

pub fn uninstall_hooks(repo_root: &Path) -> Result<Vec<String>, String> {
    let hooks_dir = repo_root.join(".git").join("hooks");
    if !hooks_dir.exists() {
        return Ok(Vec::new());
    }

    let hook_names = ["pre-push", "post-checkout", "post-merge"];
    let mut uninstalled = Vec::new();

    for name in &hook_names {
        let hook_path = hooks_dir.join(name);
        if hook_path.exists() {
            if let Ok(content) = fs::read_to_string(&hook_path) {
                if content.contains(HOOK_MARKER) {
                    let _ = fs::remove_file(&hook_path);
                    uninstalled.push(name.to_string());
                }
            }
        }
    }

    Ok(uninstalled)
}

pub fn ensure_vault_readme(vault_repo_dir: &Path) {
    let readme_path = vault_repo_dir.join("README.md");
    if !readme_path.exists() {
        let content = r#"# git-vault Storage Backend

> **Note**: This repository is automatically maintained by [git-vault](https://github.com/git-odd/git-vault).
> Manual editing is generally discouraged, as assets are synchronized and anchored by public Git commit SHAs.

## Storage Structure

```text
projects/
└── <host>/<username>/<repo>/
    └── snapshots/
        └── <commit_sha>/
            ├── .vault-manifest.json
            └── [private assets...]
```

## Emergency Manual Recovery

If you need to retrieve private files without the `git-vault` CLI:
1. Navigate to `projects/<host>/<username>/<repo>/snapshots/`.
2. Locate the folder matching your desired public commit SHA (or its nearest ancestor).
3. Copy the required private files directly back into your project workspace.
"#;
        let _ = fs::write(&readme_path, content);
    }
}

pub fn ensure_vault_author_identity(vault_repo_dir: &Path) {
    ensure_vault_readme(vault_repo_dir);
    let has_author = Command::new("git")
        .current_dir(vault_repo_dir)
        .args(["config", "user.name"])
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false);

    if !has_author {
        let _ = run_git_cmd(vault_repo_dir, &["config", "user.name", "git-vault"]);
        let _ = run_git_cmd(vault_repo_dir, &["config", "user.email", "vault@local"]);
    }
}

pub fn ensure_vault_repo(vault_repo_dir: &Path, vault_remote: Option<&str>) -> Result<(), String> {
    if vault_repo_dir.join(".git").exists() {
        ensure_vault_author_identity(vault_repo_dir);
        ensure_vault_readme(vault_repo_dir);
        return Ok(());
    }

    if let Some(remote) = vault_remote {
        let parent = vault_repo_dir.parent()
            .ok_or_else(|| "Invalid vault repo directory path".to_string())?;
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
        }

        let clone_res = Command::new("git")
            .current_dir(parent)
            .args(["clone", remote, "repo"])
            .output();

        if let Ok(out) = clone_res {
            if out.status.success() {
                ensure_vault_author_identity(vault_repo_dir);
                return Ok(());
            }
        }
    }

    // Fallback or local initialisation
    fs::create_dir_all(vault_repo_dir)
        .map_err(|e| format!("Failed to create directory {}: {}", vault_repo_dir.display(), e))?;
    run_git_cmd(vault_repo_dir, &["init"])?;
    ensure_vault_author_identity(vault_repo_dir);

    if let Some(remote) = vault_remote {
        let _ = run_git_cmd(vault_repo_dir, &["remote", "add", "origin", remote]);
    }

    Ok(())
}

pub fn sync_vault_fetch_rebase(vault_repo_dir: &Path) -> Result<(), String> {
    if !vault_repo_dir.join(".git").exists() {
        return Ok(());
    }

    ensure_vault_author_identity(vault_repo_dir);

    let remotes = run_git_cmd(vault_repo_dir, &["remote"])?;
    if !remotes.lines().any(|r| r.trim() == "origin") {
        return Ok(());
    }

    // Fetch from origin
    let fetch_out = Command::new("git")
        .current_dir(vault_repo_dir)
        .args(["fetch", "origin"])
        .output()
        .map_err(|e| format!("Failed to fetch vault remote: {}", e))?;

    if !fetch_out.status.success() {
        let err = String::from_utf8_lossy(&fetch_out.stderr);
        return Err(format!("Failed to fetch private vault from remote: {}", err.trim()));
    }

    // Check if HEAD exists in vault repo
    let head_exists = Command::new("git")
        .current_dir(vault_repo_dir)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if head_exists {
        // Try rebase on origin/main or origin/master if remote branch exists
        for branch in &["origin/main", "origin/master"] {
            let verify = Command::new("git")
                .current_dir(vault_repo_dir)
                .args(["rev-parse", "--verify", branch])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if verify {
                let rebase_out = Command::new("git")
                    .current_dir(vault_repo_dir)
                    .args(["rebase", branch])
                    .output()
                    .map_err(|e| format!("Failed to rebase vault: {}", e))?;

                if !rebase_out.status.success() {
                    let _ = Command::new("git").current_dir(vault_repo_dir).args(["rebase", "--abort"]).output();
                    let err = String::from_utf8_lossy(&rebase_out.stderr);
                    return Err(format!(
                        "Conflict or failure while rebasing vault with {}: {}\nResolve manually at {}",
                        branch,
                        err.trim(),
                        vault_repo_dir.display()
                    ));
                }
                break;
            }
        }
    }

    Ok(())
}

pub fn commit_and_push_vault(
    vault_repo_dir: &Path,
    project_id: &str,
    head_sha: &str,
) -> Result<(), String> {
    ensure_vault_author_identity(vault_repo_dir);

    run_git_cmd(vault_repo_dir, &["add", "-A"])?;

    let status = run_git_cmd(vault_repo_dir, &["status", "--porcelain"])?;
    if !status.trim().is_empty() {
        let short_sha = if head_sha.len() >= 7 {
            &head_sha[..7]
        } else {
            head_sha
        };

        let msg = format!("vault({}): snapshot at {}", project_id, short_sha);
        run_git_cmd(
            vault_repo_dir,
            &[
                "-c",
                "user.name=git-vault",
                "-c",
                "user.email=vault@local",
                "commit",
                "-m",
                &msg,
            ],
        )?;
    }

    let remotes = run_git_cmd(vault_repo_dir, &["remote"])?;
    if remotes.lines().any(|r| r.trim() == "origin") {
        let push_out = Command::new("git")
            .current_dir(vault_repo_dir)
            .args(["push", "-u", "origin", "HEAD"])
            .output()
            .map_err(|e| format!("Failed to push vault to remote: {}", e))?;

        if !push_out.status.success() {
            let err = String::from_utf8_lossy(&push_out.stderr);
            let out = String::from_utf8_lossy(&push_out.stdout);
            let err_msg = if !err.trim().is_empty() {
                err.trim()
            } else {
                out.trim()
            };
            return Err(format!(
                "Failed to push vault to remote: {}\nChanges committed locally in {}",
                err_msg,
                vault_repo_dir.display()
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_project_id() {
        assert_eq!(
            parse_project_id("git@github.com:dango-canvas/dango.git").unwrap(),
            "github.com/dango-canvas/dango"
        );
        assert_eq!(
            parse_project_id("https://github.com/dango-canvas/dango.git").unwrap(),
            "github.com/dango-canvas/dango"
        );
        assert_eq!(
            parse_project_id("https://gitlab.com/group/subgroup/project.git").unwrap(),
            "gitlab.com/group/subgroup/project"
        );
        assert_eq!(
            parse_project_id("ssh://git@git.mycompany.com:2222/org/repo.git").unwrap(),
            "git.mycompany.com/org/repo"
        );
        assert_eq!(
            parse_project_id("git://gitea.local/alice/test").unwrap(),
            "gitea.local/alice/test"
        );
    }

    #[test]
    fn test_managed_block_sync() {
        let temp_dir = std::env::temp_dir().join(format!("test_vault_exclude_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join(".git/info")).unwrap();

        let initial_exclude = "# User custom line 1\nnode_modules/\n# User custom line 2\n";
        fs::write(temp_dir.join(".git/info/exclude"), initial_exclude).unwrap();

        let patterns = vec!["SPEC*.md".to_string(), ".env".to_string()];
        sync_exclude_patterns(&temp_dir, &patterns).unwrap();

        let content = fs::read_to_string(temp_dir.join(".git/info/exclude")).unwrap();
        assert!(content.contains("node_modules/"));
        assert!(content.contains(MANAGED_BLOCK_START));
        assert!(content.contains(".vaultignore"));
        assert!(content.contains("SPEC*.md"));
        assert!(content.contains(".env"));
        assert!(content.contains(MANAGED_BLOCK_END));

        // Update patterns
        let updated_patterns = vec!["AGENTS*.md".to_string()];
        sync_exclude_patterns(&temp_dir, &updated_patterns).unwrap();

        let updated_content = fs::read_to_string(temp_dir.join(".git/info/exclude")).unwrap();
        assert!(updated_content.contains("node_modules/"));
        assert!(updated_content.contains(".vaultignore"));
        assert!(updated_content.contains("AGENTS*.md"));
        assert!(!updated_content.contains("SPEC*.md"));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
