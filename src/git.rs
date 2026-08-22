use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

pub fn ensure_exclude_patterns(repo_root: &Path, patterns: &[&str]) -> Result<(), String> {
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

    let existing_lines: Vec<&str> = existing.lines().map(|s| s.trim()).collect();
    let mut to_append = Vec::new();

    for pat in patterns {
        if !existing_lines.contains(pat) {
            to_append.push(*pat);
        }
    }

    if !to_append.is_empty() {
        let mut new_content = existing;
        if !new_content.is_empty() && !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str("# git-vault managed patterns\n");
        for pat in to_append {
            new_content.push_str(pat);
            new_content.push('\n');
        }
        fs::write(&exclude_path, new_content)
            .map_err(|e| format!("Failed to write {}: {}", exclude_path.display(), e))?;
    }

    Ok(())
}

pub fn ensure_vault_repo(vault_repo_dir: &Path, vault_remote: Option<&str>) -> Result<(), String> {
    if vault_repo_dir.join(".git").exists() {
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
                return Ok(());
            }
        }
    }

    // Fallback or local initialisation
    fs::create_dir_all(vault_repo_dir)
        .map_err(|e| format!("Failed to create directory {}: {}", vault_repo_dir.display(), e))?;
    run_git_cmd(vault_repo_dir, &["init"])?;

    // Ensure git author identity is set in vault repo if unset globally
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

    if let Some(remote) = vault_remote {
        let _ = run_git_cmd(vault_repo_dir, &["remote", "add", "origin", remote]);
    }

    Ok(())
}

pub fn sync_vault_fetch_rebase(vault_repo_dir: &Path) -> Result<(), String> {
    if !vault_repo_dir.join(".git").exists() {
        return Ok(());
    }

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
    run_git_cmd(vault_repo_dir, &["add", "-A"])?;

    let status = run_git_cmd(vault_repo_dir, &["status", "--porcelain"])?;
    if status.trim().is_empty() {
        return Ok(());
    }

    let short_sha = if head_sha.len() >= 7 {
        &head_sha[..7]
    } else {
        head_sha
    };

    let msg = format!("vault({}): snapshot at {}", project_id, short_sha);
    run_git_cmd(vault_repo_dir, &["commit", "-m", &msg])?;

    let remotes = run_git_cmd(vault_repo_dir, &["remote"])?;
    if remotes.lines().any(|r| r.trim() == "origin") {
        let push_out = Command::new("git")
            .current_dir(vault_repo_dir)
            .args(["push"])
            .output()
            .map_err(|e| format!("Failed to push vault to remote: {}", e))?;

        if !push_out.status.success() {
            let err = String::from_utf8_lossy(&push_out.stderr);
            return Err(format!(
                "Failed to push vault to remote: {}\nChanges committed locally in {}",
                err.trim(),
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
}
