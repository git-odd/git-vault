# git-vault

**English** | [简体中文](README_zh-CN.md)

---

**Zero-trace, Git-anchored out-of-band private asset manager & Git Hook automation CLI for open-source developers.**

`git-vault` keeps your private design specs (`SPEC*.md`), roadmaps (`TODO*.md`), AI agent guidelines (`AGENTS*.md`), and local secrets (`.env`) safely stored in a separate, private Git repository, while automatically aligning them with your public repository's commit history.

Featuring `.vaultignore` custom rules and native Git Hooks integration: **Run `git vault init` once, and let regular `git push` & `git checkout` handle synchronization seamlessly in the background!**

Installed as `git-vault`, it works seamlessly as a native Git subcommand: `git vault <command>`.

---

### 💡 Core Design Philosophy

* **Out-of-band Isolation**: Keep public repositories (MIT/AGPL) purely for code. Private specs and credentials live in your personal vault repo—preventing accidental commits, public exposure, and unwanted crawler scraping.
* **Git-Anchored Time Travel**: The public repository has zero knowledge of the vault. The vault uses the public `HEAD` commit SHA as an anchor. When you check out an older commit, `git-vault` retrieves the corresponding private assets using **first-parent ancestor lookup**.
* **Zero Friction & Hook Automation**:
  * Automatically sets up Git lifecycle hooks (`pre-push`, `post-checkout`, `post-merge`) during `init`.
  * `git push` automatically snapshots and pushes private assets to the vault.
  * `git checkout` automatically restores aligned private snapshots (with auto-rollback defense if local private edits have conflicts).
* **Flexible Rules & Zero Local State (.vaultignore & Managed Block)**:
  * Project-level `.vaultignore` supports full glob and `!` negation rules.
  * Rules are idempotently synchronized to `.git/info/exclude` via a dedicated Managed Block, keeping public history completely untouched.
* **Explicit & Robust Contract**:
  * `push` and `pull` strictly require the public repository to be clean (`git diff --quiet` & `git diff --cached --quiet`), ensuring your assets are bound to reproducible commit states.
  * Re-pushing to the same commit SHA cleanly overwrites the slot.
  * If local private assets diverge from the target snapshot during `pull`, the operation is aborted to protect unsaved edits.

---

### 📦 Installation

```bash
cargo install git-vault
```

---

### 🚀 Quick Start

#### 1. Configure and Initialize

In your public Git repository:

```bash
# Initialize for the current project (optionally provide your central private vault remote)
git vault init git@github.com:yourname/vault.git
```

This will:
* Detect project identity from `origin` (e.g. `github.com/alice/my-project`).
* Generate template `.vaultignore` in the project root (if not present).
* Verify that no candidate private files are currently tracked by public Git.
* Synchronize patterns to `.git/info/exclude` via Managed Block.
* Install automated Git hooks (`pre-push`, `post-checkout`, `post-merge`) in `.git/hooks/`.
* Ensure local vault cache `~/.vault/repo/` is ready.

#### 2. Seamless Daily Workflow

After initialization, you **don't need to manually run `git vault` commands**:
1. Edit code and private assets (e.g. `SPEC.md`, `.env`);
2. Commit public code: `git commit -m "feat: implement feature"`;
3. Push: `git push origin main` (`pre-push` hook snapshots and syncs private assets to your vault automatically);
4. Switch branch: `git checkout dev` (`post-checkout` hook pulls and aligns the matching private snapshot).

#### 3. Manual Inspection & Utilities

```bash
# Snapshot workspace private assets and push to vault
git vault push

# Inspect repository status, anchor match, and private asset alignment
git vault status

# Dehydrate workspace (delete local private files before public demo/screen recording)
git vault clean

# Restore private assets aligned with current HEAD
git vault pull

# View line-by-line diff of private assets (integrated with delta syntax highlight)
git vault diff
git vault diff -s             # Force side-by-side view
git vault diff -P             # Direct scroll (disable paging, mouse wheel scroll)
git vault diff -s -P          # Side-by-side without interactive pager
git vault diff --stat         # Output diffstat summary
git vault diff HEAD~1         # Diff against specific commit snapshot
git vault diff SPEC.md        # Filter by path

# Manage Git hooks
git vault hook install
git vault hook uninstall
```

---

### 🔍 Rule Configuration (`.vaultignore`)

`.vaultignore` in your project root follows `.gitignore`-compatible syntax:

```text
# Private assets managed by git-vault
SPEC*.md          # Design specifications
TODO*.md          # Internal tasks and roadmaps
AGENTS*.md        # AI collaboration instructions
.env              # Local environment secrets
*.local.*         # Machine-specific local config overrides
docs/private/**   # Dedicated private documentation folder

# Negation rule to un-ignore public files
!SPEC_public.md
```

> **Note**: `.vaultignore` is also backed up inside each vault snapshot for 100% self-healing upon fresh clones or dehydration.

---

### 📄 License

Licensed under the [MIT License](LICENSE).
