use std::fs;
use std::path::Path;
use glob::{MatchOptions, Pattern};

pub const DEFAULT_PATTERNS: &[&str] = &[
    "SPEC*.md",
    "TODO*.md",
    "AGENTS*.md",
    ".env",
    "*.local.*",
    "docs/private/**",
];

pub fn normalize_rel_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn is_safe_relative_path(rel_path: &str) -> bool {
    if rel_path.is_empty() {
        return false;
    }
    if rel_path.starts_with('/') || rel_path.starts_with('\\') {
        return false;
    }
    // Check components
    for part in rel_path.split(['/', '\\']) {
        if part == ".." || part == "." || part == ".git" {
            return false;
        }
    }
    true
}

pub fn matches_vault_pattern(rel_path: &str, patterns: &[&str]) -> bool {
    let normalized = rel_path.replace('\\', "/");
    let file_name = normalized.rsplit('/').next().unwrap_or(&normalized);

    let match_opts = MatchOptions {
        case_sensitive: true,
        require_literal_separator: false,
        require_literal_leading_dot: false,
    };

    for pat_str in patterns {
        if let Ok(pattern) = Pattern::new(pat_str) {
            // Check matching full relative path (e.g. docs/private/**)
            if pattern.matches_with(&normalized, match_opts) {
                return true;
            }
            // Check matching file name (e.g. SPEC.md against SPEC*.md)
            if pattern.matches_with(file_name, match_opts) {
                return true;
            }
        }
    }
    false
}

/// Recursively scans repo_root for files matching the vault patterns.
/// Rejects any symlinks encountered along the way.
pub fn scan_matching_files(repo_root: &Path) -> Result<Vec<String>, String> {
    let mut matched_files = Vec::new();
    scan_dir_recursive(repo_root, repo_root, &mut matched_files)?;
    matched_files.sort();
    Ok(matched_files)
}

fn scan_dir_recursive(
    repo_root: &Path,
    current_dir: &Path,
    matched_files: &mut Vec<String>,
) -> Result<(), String> {
    let entries = fs::read_dir(current_dir)
        .map_err(|e| format!("Failed to read directory {}: {}", current_dir.display(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();
        
        let meta = fs::symlink_metadata(&path)
            .map_err(|e| format!("Failed to get metadata for {}: {}", path.display(), e))?;
        
        if meta.file_type().is_symlink() {
            return Err(format!(
                "Security error: Symlink detected at {}. Symlinks are strictly forbidden in git-vault.",
                path.display()
            ));
        }

        let file_name = entry.file_name().to_string_lossy().to_string();

        // Skip .git directory
        if file_name == ".git" {
            continue;
        }

        if meta.is_dir() {
            scan_dir_recursive(repo_root, &path, matched_files)?;
        } else if meta.is_file() {
            let rel_path = path.strip_prefix(repo_root)
                .map_err(|e| format!("Failed to strip prefix from {}: {}", path.display(), e))?;
            let normalized = normalize_rel_path(rel_path);
            
            if !is_safe_relative_path(&normalized) {
                return Err(format!("Unsafe path detected: {}", normalized));
            }

            if matches_vault_pattern(&normalized, DEFAULT_PATTERNS) {
                matched_files.push(normalized);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matching() {
        assert!(matches_vault_pattern("SPEC.md", DEFAULT_PATTERNS));
        assert!(matches_vault_pattern("SPEC_snapping.md", DEFAULT_PATTERNS));
        assert!(matches_vault_pattern("TODO.md", DEFAULT_PATTERNS));
        assert!(matches_vault_pattern("AGENTS.md", DEFAULT_PATTERNS));
        assert!(matches_vault_pattern(".env", DEFAULT_PATTERNS));
        assert!(matches_vault_pattern("config.local.json", DEFAULT_PATTERNS));
        assert!(matches_vault_pattern("docs/private/notes.md", DEFAULT_PATTERNS));
        assert!(matches_vault_pattern("docs/private/sub/deep.md", DEFAULT_PATTERNS));

        // Negative patterns (should NOT match)
        assert!(!matches_vault_pattern(".env.example", DEFAULT_PATTERNS));
        assert!(!matches_vault_pattern(".env.sample", DEFAULT_PATTERNS));
        assert!(!matches_vault_pattern(".env.template", DEFAULT_PATTERNS));
        assert!(!matches_vault_pattern("src/main.rs", DEFAULT_PATTERNS));
        assert!(!matches_vault_pattern("Cargo.toml", DEFAULT_PATTERNS));
        assert!(!matches_vault_pattern("README.md", DEFAULT_PATTERNS));
    }

    #[test]
    fn test_safe_relative_path() {
        assert!(is_safe_relative_path("SPEC.md"));
        assert!(is_safe_relative_path("docs/private/note.md"));
        assert!(!is_safe_relative_path("../SPEC.md"));
        assert!(!is_safe_relative_path("docs/../../SPEC.md"));
        assert!(!is_safe_relative_path("/etc/passwd"));
        assert!(!is_safe_relative_path(".git/config"));
        assert!(!is_safe_relative_path("foo/.git/bar"));
    }
}
