use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultManifest {
    pub commit_sha: String,
    pub files: Vec<String>,
}

impl VaultManifest {
    pub fn new(commit_sha: String, files: Vec<String>) -> Self {
        Self { commit_sha, files }
    }

    pub fn load_from_dir(snapshot_dir: &Path) -> Result<Self, String> {
        let manifest_path = snapshot_dir.join(".vault-manifest.json");
        if !manifest_path.exists() {
            return Err(format!(
                "Manifest file not found in {}",
                snapshot_dir.display()
            ));
        }
        let content = fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Failed to read manifest {}: {}", manifest_path.display(), e))?;
        let manifest: VaultManifest = serde_json::from_str(&content).map_err(|e| {
            format!(
                "Failed to parse manifest {}: {}",
                manifest_path.display(),
                e
            )
        })?;
        Ok(manifest)
    }

    pub fn save_to_dir(&self, snapshot_dir: &Path) -> Result<(), String> {
        let manifest_path = snapshot_dir.join(".vault-manifest.json");
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize manifest: {}", e))?;
        fs::write(&manifest_path, content).map_err(|e| {
            format!(
                "Failed to write manifest {}: {}",
                manifest_path.display(),
                e
            )
        })?;
        Ok(())
    }
}
