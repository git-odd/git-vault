use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn run_cmd(cwd: &Path, cmd: &str, args: &[&str], envs: &[(&str, &str)]) -> (bool, String, String) {
    let mut command = Command::new(cmd);
    command.current_dir(cwd).args(args);
    for (k, v) in envs {
        command.env(k, v);
    }
    let output = command.output().expect("failed to run command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
}

struct TestEnv {
    temp_dir: PathBuf,
    public_repo: PathBuf,
    vault_home: PathBuf,
    bin_path: PathBuf,
}

impl TestEnv {
    fn new(name: &str) -> Self {
        let temp_dir = std::env::temp_dir().join(format!(
            "git_vault_test_{}_{}",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let public_repo = temp_dir.join("public_repo");
        let vault_home = temp_dir.join("vault_home");

        fs::create_dir_all(&public_repo).unwrap();
        fs::create_dir_all(&vault_home).unwrap();

        // Init public git repo
        run_cmd(&public_repo, "git", &["init"], &[]);
        run_cmd(&public_repo, "git", &["config", "user.name", "TestUser"], &[]);
        run_cmd(&public_repo, "git", &["config", "user.email", "test@example.com"], &[]);
        run_cmd(&public_repo, "git", &["remote", "add", "origin", "git@github.com:alice/demo-project.git"], &[]);

        // Target debug binary path
        let mut bin_path = std::env::current_exe().unwrap();
        bin_path.pop(); // remove test binary
        if bin_path.ends_with("deps") {
            bin_path.pop();
        }
        bin_path.push(if cfg!(windows) { "git-vault.exe" } else { "git-vault" });

        Self {
            temp_dir,
            public_repo,
            vault_home,
            bin_path,
        }
    }

    fn run_vault(&self, args: &[&str]) -> (bool, String, String) {
        let vault_home_str = self.vault_home.to_string_lossy().to_string();
        let envs = [
            ("USERPROFILE", vault_home_str.as_str()),
            ("HOME", vault_home_str.as_str()),
        ];
        run_cmd(&self.public_repo, self.bin_path.to_str().unwrap(), args, &envs)
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temp_dir);
    }
}

#[test]
fn test_full_lifecycle_flow() {
    let env = TestEnv::new("full_lifecycle");

    // 1. Create public files & commit
    fs::write(env.public_repo.join("main.rs"), "fn main() {}").unwrap();
    run_cmd(&env.public_repo, "git", &["add", "main.rs"], &[]);
    run_cmd(&env.public_repo, "git", &["commit", "-m", "initial commit"], &[]);

    // 2. Initialize git-vault
    let (ok, out, err) = env.run_vault(&["init"]);
    assert!(ok, "init failed: stdout={}, stderr={}", out, err);
    assert!(out.contains("Initialized git-vault for project: github.com/alice/demo-project"));

    // Check .git/info/exclude
    let exclude = fs::read_to_string(env.public_repo.join(".git/info/exclude")).unwrap();
    assert!(exclude.contains(".env"));
    assert!(exclude.contains("SPEC*.md"));

    // 3. Create private assets
    fs::write(env.public_repo.join("SPEC.md"), "# Architecture Spec").unwrap();
    fs::write(env.public_repo.join(".env"), "SECRET_KEY=abc123xyz").unwrap();
    fs::create_dir_all(env.public_repo.join("docs/private")).unwrap();
    fs::write(env.public_repo.join("docs/private/notes.md"), "Private Notes").unwrap();
    // Non-tracked example file that should be ignored by vault
    fs::write(env.public_repo.join(".env.example"), "SECRET_KEY=example").unwrap();

    // 4. Push private assets
    let (ok, out, err) = env.run_vault(&["push"]);
    assert!(ok, "push failed: stdout={}, stderr={}", out, err);
    assert!(out.contains("Successfully pushed snapshot"));
    assert!(out.contains("3 private files")); // SPEC.md, .env, docs/private/notes.md

    // 5. Check status
    let (ok, out, _) = env.run_vault(&["status"]);
    assert!(ok);
    assert!(out.contains("aligned (3 file(s))"));
    assert!(out.contains("exact match"));

    // 6. Test clean (dehydrate)
    let (ok, out, _) = env.run_vault(&["clean"]);
    assert!(ok);
    assert!(out.contains("Cleaned 3 private file(s)"));
    assert!(!env.public_repo.join("SPEC.md").exists());
    assert!(!env.public_repo.join(".env").exists());
    assert!(!env.public_repo.join("docs/private/notes.md").exists());
    assert!(env.public_repo.join(".env.example").exists()); // .env.example must remain!

    // Status in dehydrated mode
    let (ok, out, _) = env.run_vault(&["status"]);
    assert!(ok);
    assert!(out.contains("dehydrated"));

    // 7. Test pull (rehydrate)
    let (ok, out, err) = env.run_vault(&["pull"]);
    assert!(ok, "pull failed: stdout={}, stderr={}", out, err);
    assert!(out.contains("Successfully restored snapshot"));
    assert!(env.public_repo.join("SPEC.md").exists());
    assert!(env.public_repo.join(".env").exists());
    assert_eq!(fs::read_to_string(env.public_repo.join(".env")).unwrap(), "SECRET_KEY=abc123xyz");

    // 8. Modify local private file and test pull conflict rejection
    fs::write(env.public_repo.join(".env"), "SECRET_KEY=modified_unpushed").unwrap();
    let (ok, _, err) = env.run_vault(&["pull"]);
    assert!(!ok, "pull should fail on divergent private files");
    assert!(err.contains("Local private assets differ from the target snapshot"));

    // 9. Advance public git commit (Commit 2) without pushing new private assets
    fs::write(env.public_repo.join("main.rs"), "fn main() { println!(); }").unwrap();
    run_cmd(&env.public_repo, "git", &["add", "main.rs"], &[]);
    run_cmd(&env.public_repo, "git", &["commit", "-m", "commit 2"], &[]);

    // Pull after clean on Commit 2 should find Commit 1 snapshot via first-parent ancestor fallback
    env.run_vault(&["clean"]);
    let (ok, out, _) = env.run_vault(&["pull"]);
    assert!(ok);
    assert!(out.contains("ancestor, distance: 1"));
    assert_eq!(fs::read_to_string(env.public_repo.join(".env")).unwrap(), "SECRET_KEY=abc123xyz");

    // 10. Test empty snapshot (Commit 3 where no private assets are needed)
    fs::write(env.public_repo.join("main.rs"), "fn main() { println!(\"v3\"); }").unwrap();
    run_cmd(&env.public_repo, "git", &["add", "main.rs"], &[]);
    run_cmd(&env.public_repo, "git", &["commit", "-m", "commit 3 - private files decommissioned"], &[]);

    // Dehydrate and push empty snapshot on Commit 3
    env.run_vault(&["clean"]);
    let (ok, out, _) = env.run_vault(&["push"]);
    assert!(ok);
    assert!(out.contains("0 private files"));

    // Now pulling on Commit 3 should restore 0 private files (empty snapshot stops ancestor fallback)
    let (ok, out, _) = env.run_vault(&["pull"]);
    assert!(ok);
    assert!(out.contains("0 private files"));
    assert!(!env.public_repo.join(".env").exists());
}

#[test]
fn test_dirty_worktree_rejection() {
    let env = TestEnv::new("dirty_worktree");

    fs::write(env.public_repo.join("main.rs"), "fn main() {}").unwrap();
    run_cmd(&env.public_repo, "git", &["add", "main.rs"], &[]);
    run_cmd(&env.public_repo, "git", &["commit", "-m", "init"], &[]);
    env.run_vault(&["init"]);

    // Dirty public file without committing
    fs::write(env.public_repo.join("main.rs"), "fn main() { dirty }").unwrap();
    fs::write(env.public_repo.join("SPEC.md"), "# Spec").unwrap();

    let (ok_push, _, err_push) = env.run_vault(&["push"]);
    assert!(!ok_push);
    assert!(err_push.contains("Public repository is not clean"));

    let (ok_pull, _, err_pull) = env.run_vault(&["pull"]);
    assert!(!ok_pull);
    assert!(err_pull.contains("Public repository is not clean"));
}

#[test]
fn test_tracked_file_rejection_on_init() {
    let env = TestEnv::new("tracked_init");

    // Track a .env file directly in public git
    fs::write(env.public_repo.join(".env"), "LEAKED_KEY=123").unwrap();
    run_cmd(&env.public_repo, "git", &["add", ".env"], &[]);
    run_cmd(&env.public_repo, "git", &["commit", "-m", "mistaken commit with env"], &[]);

    // Init should catch it and abort!
    let (ok, _, err) = env.run_vault(&["init"]);
    assert!(!ok);
    assert!(err.contains("already tracked by public Git"));
    assert!(err.contains(".env"));
}
