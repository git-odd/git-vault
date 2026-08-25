use std::fs;
use std::path::Path;
use glob::{MatchOptions, Pattern};

pub const VAULTIGNORE_FILENAME: &str = ".vaultignore";

pub const DEFAULT_PATTERNS: &[&str] = &[
    "SPEC*.md",
    "TODO*.md",
    "AGENTS*.md",
    ".env",
    "*.local.*",
    "docs/private/**",
];

pub const DEFAULT_VAULTIGNORE_CONTENT: &str = r#"# Private assets managed by git-vault
# Changes here are automatically synchronized to .git/info/exclude
SPEC*.md
TODO*.md
AGENTS*.md
.env
*.local.*
docs/private/**
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultRule {
    pub raw: String,
    pub pattern: String,
    pub is_negation: bool,
}

impl VaultRule {
    pub fn parse(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }

        if let Some(stripped) = trimmed.strip_prefix('!') {
            let pat = stripped.trim().to_string();
            if pat.is_empty() {
                return None;
            }
            Some(VaultRule {
                raw: trimmed.to_string(),
                pattern: pat,
                is_negation: true,
            })
        } else {
            Some(VaultRule {
                raw: trimmed.to_string(),
                pattern: trimmed.to_string(),
                is_negation: false,
            })
        }
    }

    pub fn matches(&self, rel_path: &str) -> bool {
        let normalized = rel_path.replace('\\', "/");
        let file_name = normalized.rsplit('/').next().unwrap_or(&normalized);

        let match_opts = MatchOptions {
            case_sensitive: true,
            require_literal_separator: false,
            require_literal_leading_dot: false,
        };

        // If pattern contains a '/', match against the full normalized relative path
        // Otherwise, match against both the full path and the individual filename
        if let Ok(pattern) = Pattern::new(&self.pattern) {
            if pattern.matches_with(&normalized, match_opts) {
                return true;
            }
            if !self.pattern.contains('/') && pattern.matches_with(file_name, match_opts) {
                return true;
            }
        }

        // Support prefix directory matching if pattern ends with '/'
        if self.pattern.ends_with('/') {
            let dir_prefix = &self.pattern;
            if normalized.starts_with(dir_prefix) {
                return true;
            }
        }

        false
    }
}

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

/// Ensures the `.vaultignore` file exists in repo_root.
/// If not present, creates it with default patterns.
/// Returns true if a new file was created.
pub fn ensure_vaultignore_file(repo_root: &Path) -> Result<bool, String> {
    let path = repo_root.join(VAULTIGNORE_FILENAME);
    if !path.exists() {
        fs::write(&path, DEFAULT_VAULTIGNORE_CONTENT)
            .map_err(|e| format!("Failed to create {}: {}", path.display(), e))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Loads and parses vault rules from repo_root/.vaultignore if present,
/// or falls back to DEFAULT_PATTERNS.
/// Returns (rules, raw_lines_for_exclude).
pub fn load_vault_rules(repo_root: &Path) -> (Vec<VaultRule>, Vec<String>) {
    let path = repo_root.join(VAULTIGNORE_FILENAME);
    if path.is_file() {
        if let Ok(content) = fs::read_to_string(&path) {
            let mut rules = Vec::new();
            let mut raw_lines = Vec::new();
            for line in content.lines() {
                if let Some(rule) = VaultRule::parse(line) {
                    rules.push(rule);
                    raw_lines.push(line.trim().to_string());
                }
            }
            if !rules.is_empty() {
                return (rules, raw_lines);
            }
        }
    }

    // Fallback to DEFAULT_PATTERNS
    let rules = DEFAULT_PATTERNS
        .iter()
        .filter_map(|p| VaultRule::parse(p))
        .collect();
    let raw_lines = DEFAULT_PATTERNS.iter().map(|s| s.to_string()).collect();
    (rules, raw_lines)
}

/// Tests whether a relative path matches the list of rules (ordered evaluation with ! overrides).
pub fn matches_vault_rules(rel_path: &str, rules: &[VaultRule]) -> bool {
    let mut matched = false;
    for rule in rules {
        if rule.matches(rel_path) {
            matched = !rule.is_negation;
        }
    }
    matched
}

/// Recursively scans repo_root for files matching the vault rules.
/// Rejects any symlinks encountered along the way.
pub fn scan_matching_files(repo_root: &Path, rules: &[VaultRule]) -> Result<Vec<String>, String> {
    let mut matched_files = Vec::new();
    scan_dir_recursive(repo_root, repo_root, rules, &mut matched_files)?;
    matched_files.sort();
    Ok(matched_files)
}

fn scan_dir_recursive(
    repo_root: &Path,
    current_dir: &Path,
    rules: &[VaultRule],
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
            scan_dir_recursive(repo_root, &path, rules, matched_files)?;
        } else if meta.is_file() {
            let rel_path = path
                .strip_prefix(repo_root)
                .map_err(|e| format!("Failed to strip prefix from {}: {}", path.display(), e))?;
            let normalized = normalize_rel_path(rel_path);

            if !is_safe_relative_path(&normalized) {
                return Err(format!("Unsafe path detected: {}", normalized));
            }

            // Exclude .vaultignore itself from being considered a regular private asset candidate
            if normalized == VAULTIGNORE_FILENAME {
                continue;
            }

            if matches_vault_rules(&normalized, rules) {
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
    fn test_default_pattern_matching() {
        let rules: Vec<VaultRule> = DEFAULT_PATTERNS
            .iter()
            .filter_map(|p| VaultRule::parse(p))
            .collect();

        assert!(matches_vault_rules("SPEC.md", &rules));
        assert!(matches_vault_rules("SPEC_snapping.md", &rules));
        assert!(matches_vault_rules("TODO.md", &rules));
        assert!(matches_vault_rules("AGENTS.md", &rules));
        assert!(matches_vault_rules(".env", &rules));
        assert!(matches_vault_rules("config.local.json", &rules));
        assert!(matches_vault_rules("docs/private/notes.md", &rules));
        assert!(matches_vault_rules("docs/private/sub/deep.md", &rules));

        // Negative patterns (should NOT match)
        assert!(!matches_vault_rules(".env.example", &rules));
        assert!(!matches_vault_rules(".env.sample", &rules));
        assert!(!matches_vault_rules(".env.template", &rules));
        assert!(!matches_vault_rules("src/main.rs", &rules));
        assert!(!matches_vault_rules("Cargo.toml", &rules));
        assert!(!matches_vault_rules("README.md", &rules));
    }

    #[test]
    fn test_negation_rules() {
        let rules = vec![
            VaultRule::parse("SPEC*.md").unwrap(),
            VaultRule::parse("!SPEC_public.md").unwrap(),
            VaultRule::parse("secrets/**").unwrap(),
            VaultRule::parse("!secrets/public.json").unwrap(),
        ];

        assert!(matches_vault_rules("SPEC.md", &rules));
        assert!(matches_vault_rules("SPEC_design.md", &rules));
        assert!(!matches_vault_rules("SPEC_public.md", &rules));

        assert!(matches_vault_rules("secrets/api_key.pem", &rules));
        assert!(!matches_vault_rules("secrets/public.json", &rules));
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
