use std::fs;
use std::path::Path;

use crate::config::{get_vault_repo_dir, load_config, save_config};
use crate::git::{
    DiffFileStat, calculate_diff_stat, check_tracked_files, commit_and_push_vault,
    ensure_vault_repo, format_diff_stat_summary, generate_file_diff, get_first_parent_ancestors,
    get_head_sha, get_origin_url, get_repo_root, install_hooks, is_worktree_clean,
    output_diff_text, parse_project_id, resolve_commit_sha, run_git_cmd, sync_exclude_patterns,
    sync_vault_fetch_rebase, uninstall_hooks,
};
use crate::manifest::VaultManifest;
use crate::pattern::{
    VAULTIGNORE_FILENAME, ensure_vaultignore_file, load_vault_rules, scan_matching_files,
};

pub fn cmd_init(vault_remote: Option<String>) -> Result<(), String> {
    let repo_root = get_repo_root()?;
    let origin_url = get_origin_url(&repo_root)?;
    let project_id = parse_project_id(&origin_url)?;

    // 1. Ensure .vaultignore file exists
    let created_vaultignore = ensure_vaultignore_file(&repo_root)?;
    let (rules, raw_lines) = load_vault_rules(&repo_root);

    // 2. P0 Safety Check: Verify candidate files are not tracked by public Git
    let candidates = scan_matching_files(&repo_root, &rules)?;
    let tracked = check_tracked_files(&repo_root, &candidates)?;
    if !tracked.is_empty() {
        let mut msg = String::from(
            "Error: The following candidate private file(s) are already tracked by public Git:\n",
        );
        for file in &tracked {
            msg.push_str(&format!("  • {}\n", file));
        }
        msg.push_str(
            "\ngit-vault will not protect files that are already part of public Git tracking.\n",
        );
        msg.push_str("Please remove them from Git tracking (e.g. 'git rm --cached <file>') before initializing git-vault.");
        return Err(msg);
    }

    // 3. Register patterns into .git/info/exclude via managed block
    sync_exclude_patterns(&repo_root, &raw_lines)?;

    // 4. Install automated Git hooks (pre-push, post-checkout, post-merge)
    let installed_hooks = install_hooks(&repo_root)?;

    // 5. Update global config if remote provided
    let mut config = load_config()?;
    if let Some(ref remote) = vault_remote {
        config.vault_remote = Some(remote.clone());
        save_config(&config)?;
    }

    // 6. Ensure local vault repo exists
    let vault_repo_dir = get_vault_repo_dir()?;
    ensure_vault_repo(&vault_repo_dir, config.vault_remote.as_deref())?;

    println!("Initialized git-vault for project: {}", project_id);
    if created_vaultignore {
        println!("Created template .vaultignore in repository root.");
    } else {
        println!("Loaded patterns from existing .vaultignore.");
    }
    println!("Patterns synchronized to .git/info/exclude.");
    println!("Git hooks installed: {}", installed_hooks.join(", "));
    if let Some(ref remote) = config.vault_remote {
        println!("Vault remote configured: {}", remote);
    }
    Ok(())
}

pub fn cmd_push() -> Result<(), String> {
    let repo_root = get_repo_root()?;

    // 1. Clean public worktree check
    if !is_worktree_clean(&repo_root)? {
        return Err(
            "Error: Public repository is not clean.\n\
             git-vault requires all public code changes to be committed before pushing private assets.\n\
             Please commit or stash your public changes, then retry 'git vault push'."
                .to_string(),
        );
    }

    let (rules, raw_lines) = load_vault_rules(&repo_root);
    sync_exclude_patterns(&repo_root, &raw_lines)?;

    let head_sha = get_head_sha(&repo_root)?;
    let origin_url = get_origin_url(&repo_root)?;
    let project_id = parse_project_id(&origin_url)?;

    // 2. Check tracked files
    let candidates = scan_matching_files(&repo_root, &rules)?;
    let tracked = check_tracked_files(&repo_root, &candidates)?;
    if !tracked.is_empty() {
        let mut msg =
            String::from("Error: The following private file(s) are tracked by public Git:\n");
        for file in &tracked {
            msg.push_str(&format!("  • {}\n", file));
        }
        msg.push_str("Cannot push tracked files to private vault. Untrack them first.");
        return Err(msg);
    }

    // 3. Sync vault repo
    let config = load_config()?;
    let vault_repo_dir = get_vault_repo_dir()?;
    ensure_vault_repo(&vault_repo_dir, config.vault_remote.as_deref())?;
    sync_vault_fetch_rebase(&vault_repo_dir)?;

    // 4. Write snapshot
    let snapshot_dir = vault_repo_dir
        .join("projects")
        .join(&project_id)
        .join("snapshots")
        .join(&head_sha);

    if snapshot_dir.exists() {
        fs::remove_dir_all(&snapshot_dir)
            .map_err(|e| format!("Failed to clear existing snapshot directory: {}", e))?;
    }
    fs::create_dir_all(&snapshot_dir)
        .map_err(|e| format!("Failed to create snapshot directory: {}", e))?;

    // Copy private assets
    for file in &candidates {
        let src_path = repo_root.join(file);
        let dest_path = snapshot_dir.join(file);
        if let Some(parent) = dest_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!("Failed to create directory {}: {}", parent.display(), e)
                })?;
            }
        }
        fs::copy(&src_path, &dest_path)
            .map_err(|e| format!("Failed to copy {} to snapshot: {}", file, e))?;
    }

    // Also snapshot .vaultignore alongside private assets for complete versioning and self-healing
    let vaultignore_src = repo_root.join(VAULTIGNORE_FILENAME);
    if vaultignore_src.exists() {
        let vaultignore_dest = snapshot_dir.join(VAULTIGNORE_FILENAME);
        let _ = fs::copy(&vaultignore_src, &vaultignore_dest);
    }

    // Write manifest
    let manifest = VaultManifest::new(head_sha.clone(), candidates.clone());
    manifest.save_to_dir(&snapshot_dir)?;

    // 5. Commit and push vault repo
    commit_and_push_vault(&vault_repo_dir, &project_id, &head_sha)?;

    let short_sha = if head_sha.len() >= 7 {
        &head_sha[..7]
    } else {
        &head_sha
    };
    println!(
        "Successfully pushed snapshot for {} ({} private files)",
        short_sha,
        candidates.len()
    );
    println!("Project: {}", project_id);
    Ok(())
}

pub fn cmd_pull() -> Result<(), String> {
    let repo_root = get_repo_root()?;

    // 1. Clean public worktree check
    if !is_worktree_clean(&repo_root)? {
        return Err(
            "Error: Public repository is not clean.\n\
             git-vault requires all public code changes to be committed before pulling private assets.\n\
             Please commit or stash your public changes, then retry 'git vault pull'."
                .to_string(),
        );
    }

    let origin_url = get_origin_url(&repo_root)?;
    let project_id = parse_project_id(&origin_url)?;

    // 2. Sync vault repo
    let config = load_config()?;
    let vault_repo_dir = get_vault_repo_dir()?;
    ensure_vault_repo(&vault_repo_dir, config.vault_remote.as_deref())?;
    sync_vault_fetch_rebase(&vault_repo_dir)?;

    // 3. Ancestor lookup along first-parent
    let ancestors = get_first_parent_ancestors(&repo_root)?;
    let mut found_snapshot: Option<(String, VaultManifest, usize)> = None;

    let project_snapshots_dir = vault_repo_dir
        .join("projects")
        .join(&project_id)
        .join("snapshots");

    for (distance, sha) in ancestors.iter().enumerate() {
        let snap_dir = project_snapshots_dir.join(sha);
        if snap_dir.join(".vault-manifest.json").exists() {
            if let Ok(manifest) = VaultManifest::load_from_dir(&snap_dir) {
                found_snapshot = Some((sha.clone(), manifest, distance));
                break;
            }
        }
    }

    let (target_sha, target_manifest, distance) = match found_snapshot {
        Some(res) => res,
        None => {
            return Err(format!(
                "No snapshot found in the first-parent history for project '{}'.",
                project_id
            ));
        }
    };

    let target_snapshot_dir = project_snapshots_dir.join(&target_sha);
    let (rules, raw_lines) = load_vault_rules(&repo_root);
    sync_exclude_patterns(&repo_root, &raw_lines)?;
    let local_files = scan_matching_files(&repo_root, &rules)?;

    // 4. Compare and project
    if local_files.is_empty() {
        // Dehydrated state: safely restore all files from snapshot
        for file in &target_manifest.files {
            let src_path = target_snapshot_dir.join(file);
            let dest_path = repo_root.join(file);
            if let Some(parent) = dest_path.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!("Failed to create directory {}: {}", parent.display(), e)
                    })?;
                }
            }
            fs::copy(&src_path, &dest_path)
                .map_err(|e| format!("Failed to copy {} to workspace: {}", file, e))?;
        }

        // Restore .vaultignore if present in target snapshot and missing locally
        let snap_vaultignore = target_snapshot_dir.join(VAULTIGNORE_FILENAME);
        let local_vaultignore = repo_root.join(VAULTIGNORE_FILENAME);
        if snap_vaultignore.exists() && !local_vaultignore.exists() {
            let _ = fs::copy(&snap_vaultignore, &local_vaultignore);
        }

        let (_, updated_lines) = load_vault_rules(&repo_root);
        sync_exclude_patterns(&repo_root, &updated_lines)?;

        let short_sha = if target_sha.len() >= 7 {
            &target_sha[..7]
        } else {
            &target_sha
        };
        let match_desc = if distance == 0 {
            "exact match".to_string()
        } else {
            format!("ancestor, distance: {}", distance)
        };
        println!(
            "Successfully restored snapshot {} ({}, {} private files).",
            short_sha,
            match_desc,
            target_manifest.files.len()
        );
        return Ok(());
    }

    // Check if local files match target snapshot exactly
    let mut files_match = local_files.len() == target_manifest.files.len();
    if files_match {
        for file in &local_files {
            if !target_manifest.files.contains(file) {
                files_match = false;
                break;
            }
            let local_content = fs::read(repo_root.join(file))
                .map_err(|e| format!("Failed to read {}: {}", file, e))?;
            let target_content = fs::read(target_snapshot_dir.join(file))
                .map_err(|e| format!("Failed to read snapshot file {}: {}", file, e))?;
            if local_content != target_content {
                files_match = false;
                break;
            }
        }
    }

    if files_match {
        let short_sha = if target_sha.len() >= 7 {
            &target_sha[..7]
        } else {
            &target_sha
        };
        println!(
            "Assets already aligned with target snapshot {} (no changes needed).",
            short_sha
        );
        return Ok(());
    }

    // Divergent state: refuse to overwrite
    let short_sha = if target_sha.len() >= 7 {
        &target_sha[..7]
    } else {
        &target_sha
    };
    Err(format!(
        "Error: Local private assets differ from the target snapshot {}.\n\
         Run 'git vault push' to bind them to the current HEAD,\n\
         or run 'git vault clean' before pulling to discard local assets.",
        short_sha
    ))
}

pub fn cmd_clean() -> Result<(), String> {
    let repo_root = get_repo_root()?;
    let (rules, raw_lines) = load_vault_rules(&repo_root);
    sync_exclude_patterns(&repo_root, &raw_lines)?;
    let local_files = scan_matching_files(&repo_root, &rules)?;

    if local_files.is_empty() {
        println!("Working tree is already dehydrated (0 private files).");
        return Ok(());
    }

    for file in &local_files {
        let path = repo_root.join(file);
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|e| format!("Failed to remove {}: {}", path.display(), e))?;
        }
    }

    // Clean up empty parent directories under docs/private if any
    let docs_private = repo_root.join("docs").join("private");
    if docs_private.exists() {
        clean_empty_dirs(&docs_private);
    }

    println!("Cleaned {} private file(s):", local_files.len());
    for file in &local_files {
        println!("  • {}", file);
    }
    println!("Working tree is now dehydrated.");
    Ok(())
}

fn clean_empty_dirs(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        let entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        for entry in &entries {
            if entry.path().is_dir() {
                clean_empty_dirs(&entry.path());
            }
        }
        if let Ok(remaining) = fs::read_dir(dir) {
            if remaining.count() == 0 {
                let _ = fs::remove_dir(dir);
            }
        }
    }
}

pub fn cmd_status() -> Result<(), String> {
    let repo_root = get_repo_root()?;
    let is_clean = is_worktree_clean(&repo_root)?;
    let head_sha = get_head_sha(&repo_root).unwrap_or_else(|_| "unknown".to_string());
    let origin_url = get_origin_url(&repo_root).unwrap_or_else(|_| "unknown".to_string());
    let project_id = parse_project_id(&origin_url).unwrap_or_else(|_| "unknown".to_string());

    let (rules, raw_lines) = load_vault_rules(&repo_root);
    let _ = sync_exclude_patterns(&repo_root, &raw_lines);

    let vault_repo_dir = get_vault_repo_dir()?;
    let ancestors = get_first_parent_ancestors(&repo_root).unwrap_or_default();
    let mut found_snapshot: Option<(String, VaultManifest, usize)> = None;

    let project_snapshots_dir = vault_repo_dir
        .join("projects")
        .join(&project_id)
        .join("snapshots");

    for (distance, sha) in ancestors.iter().enumerate() {
        let snap_dir = project_snapshots_dir.join(sha);
        if snap_dir.join(".vault-manifest.json").exists() {
            if let Ok(manifest) = VaultManifest::load_from_dir(&snap_dir) {
                found_snapshot = Some((sha.clone(), manifest, distance));
                break;
            }
        }
    }

    let local_files = scan_matching_files(&repo_root, &rules)?;

    println!("Project:         {}", project_id);
    println!(
        "Public Worktree: {}",
        if is_clean {
            "clean"
        } else {
            "dirty (uncommitted changes present)"
        }
    );
    println!("HEAD:            {}", head_sha);

    let target_desc = match &found_snapshot {
        Some((sha, _, 0)) => format!("{} (exact match)", &sha[..std::cmp::min(7, sha.len())]),
        Some((sha, _, dist)) => format!(
            "{} (ancestor, distance: {})",
            &sha[..std::cmp::min(7, sha.len())],
            dist
        ),
        None => "none (no snapshot in first-parent history)".to_string(),
    };
    println!("Target Snapshot: {}", target_desc);

    // Compute detailed per-file status tags
    let mut all_files = std::collections::BTreeSet::new();
    for f in &local_files {
        all_files.insert(f.clone());
    }
    if let Some((_, ref manifest, _)) = found_snapshot {
        for f in &manifest.files {
            all_files.insert(f.clone());
        }
    }

    let mut file_statuses = Vec::new();
    let mut modified_count = 0;
    let mut new_count = 0;
    let mut missing_count = 0;

    for file in &all_files {
        let is_in_local = local_files.contains(file);
        let is_in_target = found_snapshot
            .as_ref()
            .map(|(_, m, _)| m.files.contains(file))
            .unwrap_or(false);

        if is_in_local && is_in_target {
            let (target_sha, _, _) = found_snapshot.as_ref().unwrap();
            let snap_dir = project_snapshots_dir.join(target_sha);
            let local_content = fs::read(repo_root.join(file));
            let target_content = fs::read(snap_dir.join(file));

            if let (Ok(loc), Ok(tgt)) = (local_content, target_content) {
                if loc == tgt {
                    file_statuses.push((file.clone(), "unmodified"));
                } else {
                    modified_count += 1;
                    file_statuses.push((file.clone(), "modified"));
                }
            } else {
                modified_count += 1;
                file_statuses.push((file.clone(), "modified"));
            }
        } else if is_in_local {
            new_count += 1;
            file_statuses.push((file.clone(), "new"));
        } else {
            missing_count += 1;
            file_statuses.push((file.clone(), "missing locally"));
        }
    }

    let asset_status_line = if local_files.is_empty() {
        if let Some((_, ref manifest, _)) = found_snapshot {
            if manifest.files.is_empty() {
                "dehydrated (0 files)".to_string()
            } else {
                format!(
                    "dehydrated (0 in workspace, {} in target snapshot)",
                    manifest.files.len()
                )
            }
        } else {
            "dehydrated (0 files)".to_string()
        }
    } else if found_snapshot.is_none() {
        format!("unanchored ({} new file(s))", local_files.len())
    } else if modified_count == 0 && new_count == 0 && missing_count == 0 {
        format!("aligned ({} file(s))", local_files.len())
    } else {
        let mut parts = Vec::new();
        if modified_count > 0 {
            parts.push(format!("{} modified", modified_count));
        }
        if new_count > 0 {
            parts.push(format!("{} new", new_count));
        }
        if missing_count > 0 {
            parts.push(format!("{} missing", missing_count));
        }
        format!("divergent ({})", parts.join(", "))
    };

    println!("Private Assets:  {}", asset_status_line);
    if !file_statuses.is_empty() {
        for (file, tag) in &file_statuses {
            println!("  • {:<30} ({})", file, tag);
        }
    }

    Ok(())
}

pub fn cmd_hook_install() -> Result<(), String> {
    let repo_root = get_repo_root()?;
    let installed = install_hooks(&repo_root)?;
    println!(
        "Successfully installed git-vault hooks: {}",
        installed.join(", ")
    );
    Ok(())
}

pub fn cmd_hook_uninstall() -> Result<(), String> {
    let repo_root = get_repo_root()?;
    let uninstalled = uninstall_hooks(&repo_root)?;
    if uninstalled.is_empty() {
        println!("No git-vault hooks found to uninstall.");
    } else {
        println!(
            "Successfully uninstalled git-vault hooks: {}",
            uninstalled.join(", ")
        );
    }
    Ok(())
}

pub fn cmd_diff(
    revision: Option<String>,
    revision_b: Option<String>,
    mut paths: Vec<String>,
    side_by_side: bool,
    unified: bool,
    stat: bool,
    name_only: bool,
    name_status: bool,
    no_pager: bool,
    no_paging: bool,
    paging: Option<String>,
    _color: Option<String>,
) -> Result<(), String> {
    let repo_root = get_repo_root()?;
    let origin_url = get_origin_url(&repo_root)?;
    let project_id = parse_project_id(&origin_url)?;

    let config = load_config()?;
    let vault_repo_dir = get_vault_repo_dir()?;
    ensure_vault_repo(&vault_repo_dir, config.vault_remote.as_deref())?;
    sync_vault_fetch_rebase(&vault_repo_dir)?;

    let project_snapshots_dir = vault_repo_dir
        .join("projects")
        .join(&project_id)
        .join("snapshots");

    let mut target_rev_a: Option<String> = None;
    let mut target_rev_b: Option<String> = None;

    if let (Some(r_a), Some(r_b)) = (&revision, &revision_b) {
        let res_a = resolve_commit_sha(&repo_root, r_a);
        let res_b = resolve_commit_sha(&repo_root, r_b);
        match (res_a, res_b) {
            (Ok(sha_a), Ok(sha_b)) => {
                target_rev_a = Some(sha_a);
                target_rev_b = Some(sha_b);
            }
            (Ok(sha_a), Err(_)) => {
                target_rev_a = Some(sha_a);
                paths.push(r_b.clone());
            }
            (Err(_), Ok(sha_b)) => {
                target_rev_a = Some(sha_b);
                paths.push(r_a.clone());
            }
            (Err(_), Err(_)) => {
                paths.push(r_a.clone());
                paths.push(r_b.clone());
            }
        }
    } else if let Some(ref r_a) = revision {
        match resolve_commit_sha(&repo_root, r_a) {
            Ok(sha_a) => {
                target_rev_a = Some(sha_a);
            }
            Err(_) => {
                paths.push(r_a.clone());
            }
        }
    }

    let mut file_pairs = Vec::new();

    if let (Some(sha_a), Some(sha_b)) = (&target_rev_a, &target_rev_b) {
        let snap_dir_a = project_snapshots_dir.join(sha_a);
        let snap_dir_b = project_snapshots_dir.join(sha_b);

        if !snap_dir_a.exists() {
            return Err(format!(
                "Snapshot for revision '{}' ({}) not found in vault.",
                revision.unwrap_or_default(),
                &sha_a[..std::cmp::min(7, sha_a.len())]
            ));
        }
        if !snap_dir_b.exists() {
            return Err(format!(
                "Snapshot for revision '{}' ({}) not found in vault.",
                revision_b.unwrap_or_default(),
                &sha_b[..std::cmp::min(7, sha_b.len())]
            ));
        }

        let manifest_a = VaultManifest::load_from_dir(&snap_dir_a)
            .unwrap_or_else(|_| VaultManifest::new(sha_a.clone(), Vec::new()));
        let manifest_b = VaultManifest::load_from_dir(&snap_dir_b)
            .unwrap_or_else(|_| VaultManifest::new(sha_b.clone(), Vec::new()));

        let mut all_files = std::collections::BTreeSet::new();
        for f in &manifest_a.files {
            all_files.insert(f.clone());
        }
        for f in &manifest_b.files {
            all_files.insert(f.clone());
        }
        if snap_dir_a.join(VAULTIGNORE_FILENAME).exists()
            || snap_dir_b.join(VAULTIGNORE_FILENAME).exists()
        {
            all_files.insert(VAULTIGNORE_FILENAME.to_string());
        }

        for file in all_files {
            let old_path = if snap_dir_a.join(&file).exists() {
                Some(snap_dir_a.join(&file))
            } else {
                None
            };
            let new_path = if snap_dir_b.join(&file).exists() {
                Some(snap_dir_b.join(&file))
            } else {
                None
            };
            file_pairs.push((file, old_path, new_path));
        }
    } else {
        let found_snapshot = if let Some(ref sha) = target_rev_a {
            let output = run_git_cmd(&repo_root, &["rev-list", "--first-parent", sha])
                .map_err(|e| format!("Failed to resolve first-parent list for '{}': {}", sha, e))?;
            let ancestors: Vec<String> = output
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let mut snap = None;
            for (dist, ancestor_sha) in ancestors.iter().enumerate() {
                let snap_dir = project_snapshots_dir.join(ancestor_sha);
                if snap_dir.join(".vault-manifest.json").exists() {
                    if let Ok(m) = VaultManifest::load_from_dir(&snap_dir) {
                        snap = Some((ancestor_sha.clone(), m, dist));
                        break;
                    }
                }
            }
            snap
        } else {
            let ancestors = get_first_parent_ancestors(&repo_root).unwrap_or_default();
            let mut snap = None;
            for (dist, sha) in ancestors.iter().enumerate() {
                let snap_dir = project_snapshots_dir.join(sha);
                if snap_dir.join(".vault-manifest.json").exists() {
                    if let Ok(m) = VaultManifest::load_from_dir(&snap_dir) {
                        snap = Some((sha.clone(), m, dist));
                        break;
                    }
                }
            }
            snap
        };

        let (rules, raw_lines) = load_vault_rules(&repo_root);
        let _ = sync_exclude_patterns(&repo_root, &raw_lines);
        let local_files = scan_matching_files(&repo_root, &rules)?;

        let mut all_files = std::collections::BTreeSet::new();
        for f in &local_files {
            all_files.insert(f.clone());
        }
        if let Some((_, ref manifest, _)) = found_snapshot {
            for f in &manifest.files {
                all_files.insert(f.clone());
            }
        }
        let local_vaultignore = repo_root.join(VAULTIGNORE_FILENAME);
        let snap_vaultignore = found_snapshot
            .as_ref()
            .map(|(sha, _, _)| project_snapshots_dir.join(sha).join(VAULTIGNORE_FILENAME));
        if local_vaultignore.exists()
            || snap_vaultignore
                .as_ref()
                .map(|p| p.exists())
                .unwrap_or(false)
        {
            all_files.insert(VAULTIGNORE_FILENAME.to_string());
        }

        for file in all_files {
            let old_path = if let Some((ref target_sha, _, _)) = found_snapshot {
                let p = project_snapshots_dir.join(target_sha).join(&file);
                if p.exists() { Some(p) } else { None }
            } else {
                None
            };
            let new_path = {
                let p = repo_root.join(&file);
                if p.exists() { Some(p) } else { None }
            };
            file_pairs.push((file, old_path, new_path));
        }
    }

    if !paths.is_empty() {
        let normalized_filters: Vec<String> = paths
            .iter()
            .map(|p| p.replace('\\', "/").trim_end_matches('/').to_string())
            .collect();
        file_pairs.retain(|(file, _, _)| {
            let norm_file = file.replace('\\', "/");
            normalized_filters.iter().any(|filter| {
                norm_file == *filter || norm_file.starts_with(&format!("{}/", filter))
            })
        });
    }

    let mut diff_results = Vec::new();
    for (rel_path, old_path, new_path) in file_pairs {
        if let Ok(Some(diff_content)) =
            generate_file_diff(&rel_path, old_path.as_deref(), new_path.as_deref())
        {
            let status_tag = match (old_path.is_some(), new_path.is_some()) {
                (true, true) => "M",
                (false, true) => "A",
                (true, false) => "D",
                (false, false) => "M",
            };
            let stat = calculate_diff_stat(&diff_content, &rel_path);
            diff_results.push((rel_path, diff_content, status_tag, stat));
        }
    }

    if diff_results.is_empty() {
        return Ok(());
    }

    if name_only {
        for (rel_path, _, _, _) in &diff_results {
            println!("{}", rel_path);
        }
        return Ok(());
    }

    if name_status {
        for (rel_path, _, tag, _) in &diff_results {
            println!("{}\t{}", tag, rel_path);
        }
        return Ok(());
    }

    if stat {
        let stats: Vec<DiffFileStat> = diff_results.iter().map(|(_, _, _, s)| s.clone()).collect();
        let stat_text = format_diff_stat_summary(&stats);
        print!("{}", stat_text);
        return Ok(());
    }

    let mut combined_diff = String::new();
    for (_, diff_content, _, _) in diff_results {
        combined_diff.push_str(&diff_content);
        if !diff_content.ends_with('\n') {
            combined_diff.push('\n');
        }
    }

    output_diff_text(
        &combined_diff,
        side_by_side,
        unified,
        no_pager,
        no_paging,
        paging.as_deref(),
        config.diff_side_by_side,
        config.diff_pager.as_deref(),
        config.diff_paging.as_deref(),
    )?;

    Ok(())
}
